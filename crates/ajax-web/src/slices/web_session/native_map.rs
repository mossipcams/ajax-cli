//! Pure mapping from the native adapter event vocabulary ([`NativeEvent`] in
//! `adapters::web_session_acp::agent_event`) to [`SessionServerEvent`]. Every
//! variant produces exactly what `acp_map` produces for the equivalent ACP
//! session update, so wire output stays identical once the Claude SDK / Pi RPC
//! adapters stop translating through ACP.

use crate::adapters::web_session_acp::NativeEvent;

use super::acp_map::{derive_tool_locations, message_event, target_from_raw_input};
use super::{PlanEntry, SessionServerEvent, ToolContent};
use serde_json::{Map, Value};

/// Map one native event to the [`SessionServerEvent`] values `acp_map` would
/// produce for the equivalent ACP session update. Empty chunks and zero-sized
/// usage produce no events, matching `acp_map`.
pub fn map_native_event(event: &NativeEvent) -> Vec<SessionServerEvent> {
    match event {
        NativeEvent::AgentText(text) => native_message("agent", text),
        NativeEvent::AgentThought(text) => native_message("thought", text),
        NativeEvent::ToolCall {
            id,
            title,
            kind,
            status,
            raw_input,
        } => vec![native_tool_event(
            id,
            title,
            kind,
            status,
            raw_input.as_ref(),
            edit_diffs(raw_input.as_ref()),
        )],
        NativeEvent::ToolCallUpdate {
            id,
            status,
            content,
            raw_input,
        } => {
            let mut blocks = edit_diffs(raw_input.as_ref());
            blocks.extend(native_tool_content(content));
            vec![native_tool_event(
                id,
                "",
                "",
                status,
                raw_input.as_ref(),
                blocks,
            )]
        }
        NativeEvent::Plan(entries) => vec![SessionServerEvent::Plan {
            entries: native_plan_entries(entries),
        }],
        NativeEvent::Usage { used, size } => (size > &0)
            .then(|| SessionServerEvent::Usage {
                used: *used,
                size: *size,
            })
            .into_iter()
            .collect(),
        NativeEvent::ElicitationRequest {
            request_id,
            message,
            schema,
        } => vec![SessionServerEvent::ElicitationRequest {
            request_id: request_id.clone(),
            message: message.clone(),
            schema: schema.clone(),
        }],
        // Lifecycle events are handled by the drain loop, because they need
        // request and host state.
        NativeEvent::TurnFinished { .. } | NativeEvent::Error(_) | NativeEvent::Exited => {
            Vec::new()
        }
    }
}

/// The [`SessionServerEvent::Message`] an ACP text/thought chunk update maps to
/// (`acp_map`'s `typed_message_event` without content blocks or a message id).
/// Empty chunks produce no event.
fn native_message(role: &str, text: &str) -> Vec<SessionServerEvent> {
    message_event(role, text.to_string(), Vec::new(), None)
}

/// The [`SessionServerEvent::ToolCall`] for one tool-call-shaped native event,
/// mirroring `acp_map`'s `typed_tool_call_update_event`: titles and kinds fall
/// back to the empty string, locations come from `raw_input`, content is
/// pre-mapped.
fn native_tool_event(
    call_id: &str,
    title: &str,
    kind: &str,
    status: &str,
    raw_input: Option<&Value>,
    content: Vec<ToolContent>,
) -> SessionServerEvent {
    let locations = if !content.is_empty() {
        derive_tool_locations(Vec::new(), raw_input, &content)
    } else {
        target_from_raw_input(raw_input)
            .map(|target| vec![target])
            .unwrap_or_default()
    };
    SessionServerEvent::ToolCall {
        call_id: call_id.to_string(),
        title: title.to_string(),
        kind: kind.to_string(),
        status: status.to_string(),
        locations,
        content,
    }
}

/// A native tool-call text payload as [`ToolContent`]: empty/whitespace-only
/// text produces no blocks, mirroring `acp_map`'s whitespace filtering.
fn native_tool_content(content: &Option<String>) -> Vec<ToolContent> {
    content
        .as_deref()
        .filter(|text| !text.trim().is_empty())
        .map(|text| ToolContent::Text {
            text: text.to_string(),
        })
        .into_iter()
        .collect()
}

/// [`ToolContent::Diff`] blocks derived from a native edit/write tool's raw
/// input, like ACP harnesses send. Single edits (Claude `Edit`, Pi `edit`) and
/// multi-edits (`edits` arrays) produce one diff per new text; write inputs
/// (`content` with no old/new keys) produce a diff without an old text. Any
/// other shape produces nothing, and odd shapes never panic.
fn edit_diffs(raw_input: Option<&Value>) -> Vec<ToolContent> {
    const NEW_KEYS: [&str; 4] = ["new_string", "newText", "new_text", "newString"];
    const OLD_KEYS: [&str; 4] = ["old_string", "oldText", "old_text", "oldString"];

    let input = match raw_input.and_then(Value::as_object) {
        Some(input) => input,
        None => return Vec::new(),
    };
    let path = match first_string(input, &["file_path", "path", "filePath"]) {
        Some(path) => path.to_string(),
        None => return Vec::new(),
    };

    let mut diffs = Vec::new();
    if let Some(edits) = input.get("edits").and_then(Value::as_array) {
        for edit in edits.iter().filter_map(Value::as_object) {
            if let Some(new_text) = first_string(edit, &NEW_KEYS).map(str::to_string) {
                diffs.push(ToolContent::Diff {
                    path: path.clone(),
                    old_text: first_string(edit, &OLD_KEYS).map(str::to_string),
                    new_text,
                });
            }
        }
    } else if let Some(new_text) = first_string(input, &NEW_KEYS).map(str::to_string) {
        diffs.push(ToolContent::Diff {
            path,
            old_text: first_string(input, &OLD_KEYS).map(str::to_string),
            new_text,
        });
    } else if let Some(content) = input.get("content").and_then(Value::as_str) {
        diffs.push(ToolContent::Diff {
            path,
            old_text: None,
            new_text: content.to_string(),
        });
    }
    diffs
}

/// The first string value among `keys` in a JSON object, if any.
fn first_string<'a>(input: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| input.get(*key).and_then(Value::as_str))
}
/// `(content, status)` pairs as [`PlanEntry`] values with `acp_map`'s empty-
/// entry filtering and content trimming.
fn native_plan_entries(entries: &[(String, String)]) -> Vec<PlanEntry> {
    entries
        .iter()
        .filter(|(content, _)| !content.trim().is_empty())
        .map(|(content, status)| PlanEntry {
            content: content.trim().to_string(),
            status: status.clone(),
        })
        .collect()
}

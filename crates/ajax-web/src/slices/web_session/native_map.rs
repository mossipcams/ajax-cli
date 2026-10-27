use crate::adapters::web_session_acp::NativeEvent;

use super::acp_map::{derive_tool_locations, message_event, target_from_raw_input};
use super::{PlanEntry, SessionServerEvent, ToolContent};
use serde_json::{Map, Value};

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
        NativeEvent::TurnFinished { .. } | NativeEvent::Error(_) | NativeEvent::Exited => {
            Vec::new()
        }
    }
}

fn native_message(role: &str, text: &str) -> Vec<SessionServerEvent> {
    message_event(role, text.to_string(), Vec::new(), None)
}

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

fn first_string<'a>(input: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| input.get(*key).and_then(Value::as_str))
}
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

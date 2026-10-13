//! Pure mapping from the native adapter event vocabulary ([`NativeEvent`] in
//! `adapters::web_session_acp::agent_event`) to [`SessionServerEvent`]. Every
//! variant produces exactly what `acp_map` produces for the equivalent ACP
//! session update, so wire output stays identical once the Claude SDK / Pi RPC
//! adapters stop translating through ACP.

use crate::adapters::web_session_acp::NativeEvent;

use super::acp_map::{derive_tool_locations, message_event, target_from_raw_input};
use super::{PlanEntry, SessionServerEvent, ToolContent};
use serde_json::Value;

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
            Vec::new(),
        )],
        NativeEvent::ToolCallUpdate {
            id,
            status,
            content,
        } => vec![native_tool_event(
            id,
            "",
            "",
            status,
            None,
            native_tool_content(content),
        )],
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

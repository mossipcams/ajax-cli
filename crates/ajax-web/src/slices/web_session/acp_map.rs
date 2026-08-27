//! ACP v1 `session/update` → browser wire event mapping.
//!
//! Two entry points on purpose. `map_acp_session_notification` takes the typed
//! SDK notification and is the path every conforming harness uses.
//! `map_acp_session_update` takes the raw JSON and covers updates the SDK could
//! not type (`UnknownSessionUpdate`), where field names still vary by harness.
//!
//! What crosses this boundary decides what the chat surface can render. ACP
//! separates message, thought, tool call, tool content, plan, permission, and
//! usage; anything flattened here is unrecoverable in the browser.

use super::output_content::{
    extract_output_block, extract_tool_content_item, map_output_block, map_output_block_to_tool,
    OutputContentBlockWire,
};
use super::{PlanEntry, SessionServerEvent, ToolContent};
use agent_client_protocol::schema::v1::{
    ContentBlock, ContentChunk, SessionNotification, SessionUpdate, ToolCall, ToolCallContent,
    ToolCallStatus, ToolCallUpdate, ToolKind, UsageUpdate,
};
use serde_json::Value;

pub fn map_acp_session_update(update: &Value) -> Vec<SessionServerEvent> {
    let Some(update_body) = update.get("update") else {
        return Vec::new();
    };
    let session_update = update_body
        .get("sessionUpdate")
        .or_else(|| update_body.get("session_update"))
        .and_then(Value::as_str)
        .unwrap_or_default();

    match session_update {
        "agent_message" | "agent_message_chunk" => {
            let (text, blocks) = extract_message_payload(update_body);
            message_event("agent", text, blocks, extract_message_id(update_body))
        }
        "user_message" | "user_message_chunk" => {
            let (text, blocks) = extract_message_payload(update_body);
            message_event("user", text, blocks, extract_message_id(update_body))
        }
        "thought" | "thought_chunk" => {
            let (text, blocks) = extract_message_payload(update_body);
            message_event("thought", text, blocks, extract_message_id(update_body))
        }
        "tool_call" | "tool_call_update" => tool_call_event(update_body),
        "plan" | "plan_update" => vec![SessionServerEvent::Plan {
            entries: extract_plan_entries(update_body),
        }],
        "state_update" | "status" => status_event(update_body),
        "usage_update" => extract_usage(update_body),
        // Capability announcements, not conversation. Slash commands are live
        // session state (see drain); mode/config updates are also non-transcript.
        "available_commands_update"
        | "current_mode_update"
        | "config_option_update"
        | "session_info_update" => Vec::new(),
        other if !other.is_empty() => vec![SessionServerEvent::Artifact {
            kind: other.to_string(),
            title: None,
            body: serde_json::to_string_pretty(update_body).ok(),
        }],
        _ => Vec::new(),
    }
}

/// Map the official ACP v1 notification without erasing its typed update first.
pub fn map_acp_session_notification(update: &SessionNotification) -> Vec<SessionServerEvent> {
    match &update.update {
        SessionUpdate::UserMessageChunk(chunk) => typed_message_event("user", chunk),
        SessionUpdate::AgentMessageChunk(chunk) => typed_message_event("agent", chunk),
        SessionUpdate::AgentThoughtChunk(chunk) => typed_message_event("thought", chunk),
        SessionUpdate::ToolCall(call) => typed_tool_call_event(call),
        SessionUpdate::ToolCallUpdate(call) => typed_tool_call_update_event(call),
        SessionUpdate::Plan(plan) => vec![SessionServerEvent::Plan {
            entries: plan
                .entries
                .iter()
                .filter(|entry| !entry.content.trim().is_empty())
                .map(|entry| PlanEntry {
                    content: entry.content.trim().to_string(),
                    status: plan_status(&entry.status).to_string(),
                })
                .collect(),
        }],
        SessionUpdate::CurrentModeUpdate(_) => Vec::new(),
        SessionUpdate::ConfigOptionUpdate(_) => Vec::new(),
        SessionUpdate::SessionInfoUpdate(_) => Vec::new(),
        SessionUpdate::UsageUpdate(update) => typed_usage_event(update),
        SessionUpdate::AvailableCommandsUpdate(_) => Vec::new(),
        _ => Vec::new(),
    }
}

fn typed_message_event(role: &str, chunk: &ContentChunk) -> Vec<SessionServerEvent> {
    let mut text = String::new();
    let mut blocks = Vec::new();
    match &chunk.content {
        ContentBlock::Text(value) => text = value.text.clone(),
        other => {
            if let Some(block) = map_output_block(other) {
                blocks.push(block);
            }
        }
    }
    message_event(
        role,
        text,
        blocks,
        chunk.message_id.as_ref().map(ToString::to_string),
    )
}

/// Context pressure is the one number an operator steers by mid-turn, so it is
/// a first-class event rather than an artifact blob. Cumulative cost is omitted:
/// `ajax cost` already owns spend reporting.
fn typed_usage_event(update: &UsageUpdate) -> Vec<SessionServerEvent> {
    if update.size == 0 {
        return Vec::new();
    }
    vec![SessionServerEvent::Usage {
        used: update.used,
        size: update.size,
    }]
}

fn typed_tool_call_event(call: &ToolCall) -> Vec<SessionServerEvent> {
    let content = map_tool_content(&call.content);
    vec![SessionServerEvent::ToolCall {
        call_id: call.tool_call_id.to_string(),
        title: call.title.clone(),
        kind: tool_kind(call.kind).to_string(),
        status: tool_status(call.status).to_string(),
        locations: derive_tool_locations(
            call.locations
                .iter()
                .map(|location| location.path.display().to_string())
                .collect(),
            call.raw_input.as_ref(),
            &content,
        ),
        content,
    }]
}

fn typed_tool_call_update_event(call: &ToolCallUpdate) -> Vec<SessionServerEvent> {
    let content = map_tool_content(call.fields.content.as_deref().unwrap_or_default());
    vec![SessionServerEvent::ToolCall {
        call_id: call.tool_call_id.to_string(),
        title: call.fields.title.clone().unwrap_or_default(),
        kind: call
            .fields
            .kind
            .map(tool_kind)
            .unwrap_or_default()
            .to_string(),
        status: call
            .fields
            .status
            .map(tool_status)
            .unwrap_or_default()
            .to_string(),
        locations: derive_tool_locations(
            call.fields
                .locations
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(|location| location.path.display().to_string())
                .collect(),
            call.fields.raw_input.as_ref(),
            &content,
        ),
        content,
    }]
}

/// A tool call's output is the substance of a turn — the diff it wrote, the text
/// a command printed. ACP carries it in `content`; dropping it left the browser
/// able to say only that an edit happened.
///
/// `ToolCallContent::Terminal` is skipped: Ajax advertises no `terminal/*`
/// client capability, so no agent can create a terminal to embed here.
fn map_tool_content(content: &[ToolCallContent]) -> Vec<ToolContent> {
    content
        .iter()
        .filter_map(|item| match item {
            ToolCallContent::Content(block) => match &block.content {
                ContentBlock::Text(text) if !text.text.trim().is_empty() => {
                    Some(ToolContent::Text {
                        text: text.text.clone(),
                    })
                }
                other => map_output_block_to_tool(other),
            },
            ToolCallContent::Diff(diff) => Some(ToolContent::Diff {
                path: diff.path.display().to_string(),
                old_text: diff.old_text.clone(),
                new_text: diff.new_text.clone(),
            }),
            ToolCallContent::Terminal(_) => None,
            _ => None,
        })
        .collect()
}

fn tool_kind(kind: ToolKind) -> &'static str {
    match kind {
        ToolKind::Read => "read",
        ToolKind::Edit => "edit",
        ToolKind::Delete => "delete",
        ToolKind::Move => "move",
        ToolKind::Search => "search",
        ToolKind::Execute => "execute",
        ToolKind::Think => "think",
        ToolKind::Fetch => "fetch",
        ToolKind::SwitchMode => "switch_mode",
        ToolKind::Other => "other",
        _ => "other",
    }
}

fn tool_status(status: ToolCallStatus) -> &'static str {
    match status {
        ToolCallStatus::Pending => "pending",
        ToolCallStatus::InProgress => "in_progress",
        ToolCallStatus::Completed => "completed",
        ToolCallStatus::Failed => "failed",
        _ => "pending",
    }
}

fn plan_status(status: &agent_client_protocol::schema::v1::PlanEntryStatus) -> &'static str {
    use agent_client_protocol::schema::v1::PlanEntryStatus;
    match status {
        PlanEntryStatus::Pending => "pending",
        PlanEntryStatus::InProgress => "in_progress",
        PlanEntryStatus::Completed => "completed",
        _ => "pending",
    }
}

/// ACP tool calls are the bulk of a turn. `tool_call` opens one and
/// `tool_call_update` revises it, so both map to the same event keyed by
/// `toolCallId`; the browser merges by that key and keeps the fields an update
/// omits.
fn tool_call_event(update_body: &Value) -> Vec<SessionServerEvent> {
    let call_id = update_body
        .get("toolCallId")
        .or_else(|| update_body.get("tool_call_id"))
        .or_else(|| update_body.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if call_id.is_empty() {
        return Vec::new();
    }
    let content = extract_tool_content(update_body);
    vec![SessionServerEvent::ToolCall {
        call_id,
        title: update_body
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        kind: update_body
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        status: update_body
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        locations: derive_tool_locations(
            extract_tool_locations(update_body),
            raw_input_from_body(update_body),
            &content,
        ),
        content,
    }]
}

fn raw_input_from_body(update_body: &Value) -> Option<&Value> {
    update_body
        .get("rawInput")
        .or_else(|| update_body.get("raw_input"))
}

/// Cursor often sends the path on `rawInput` while leaving `locations` empty.
/// Derive one follow-along target so the browser row can name the file.
fn derive_tool_locations(
    explicit: Vec<String>,
    raw_input: Option<&Value>,
    content: &[ToolContent],
) -> Vec<String> {
    if !explicit.is_empty() {
        return explicit;
    }
    if let Some(target) = target_from_raw_input(raw_input) {
        return vec![target];
    }
    if let Some(path) = target_from_diff_content(content) {
        return vec![path];
    }
    Vec::new()
}

fn target_from_raw_input(raw_input: Option<&Value>) -> Option<String> {
    let Value::Object(map) = raw_input? else {
        return None;
    };
    for key in ["path"] {
        if let Some(value) = map.get(key).and_then(non_empty_str) {
            return Some(value.to_string());
        }
    }
    for key in ["query", "pattern", "glob"] {
        if let Some(value) = map.get(key).and_then(non_empty_str) {
            return Some(value.to_string());
        }
    }
    map.get("command")
        .and_then(non_empty_str)
        .map(str::to_string)
}

fn target_from_diff_content(content: &[ToolContent]) -> Option<String> {
    content.iter().find_map(|item| match item {
        ToolContent::Diff { path, .. } if !path.is_empty() => Some(path.clone()),
        _ => None,
    })
}

fn non_empty_str(value: &Value) -> Option<&str> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn extract_tool_locations(update_body: &Value) -> Vec<String> {
    update_body
        .get("locations")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| match item {
                    Value::String(path) => Some(path.clone()),
                    _ => item.get("path").and_then(Value::as_str).map(str::to_string),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Untyped mirror of [`map_tool_content`], for harnesses whose update the SDK
/// could not type. Same two shapes, same omissions.
fn extract_tool_content(update_body: &Value) -> Vec<ToolContent> {
    update_body
        .get("content")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| match item.get("type").and_then(Value::as_str) {
                    Some("diff") => Some(ToolContent::Diff {
                        path: item
                            .get("path")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        old_text: item
                            .get("oldText")
                            .or_else(|| item.get("old_text"))
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        new_text: item
                            .get("newText")
                            .or_else(|| item.get("new_text"))
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    }),
                    Some("terminal") => None,
                    _ => extract_tool_content_item(item),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// ACP status-like updates carry a machine `state` and sometimes a human
/// `detail`/`label`/`title`. The browser prefers the human line in the head.
fn status_event(update_body: &Value) -> Vec<SessionServerEvent> {
    let state = update_body
        .get("state")
        .or_else(|| update_body.pointer("/status/state"))
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    vec![SessionServerEvent::Status {
        state,
        detail: human_status_detail(update_body),
    }]
}

fn human_status_detail(update_body: &Value) -> Option<String> {
    ["detail", "label", "title", "message", "stopReason"]
        .iter()
        .find_map(|key| {
            update_body
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
        })
}

fn extract_usage(update_body: &Value) -> Vec<SessionServerEvent> {
    let used = update_body.get("used").and_then(Value::as_u64);
    let size = update_body.get("size").and_then(Value::as_u64);
    match (used, size) {
        (Some(used), Some(size)) if size > 0 => vec![SessionServerEvent::Usage { used, size }],
        _ => Vec::new(),
    }
}

fn extract_plan_entries(update_body: &Value) -> Vec<PlanEntry> {
    update_body
        .get("entries")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let content = item
                        .get("content")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                    if content.is_empty() {
                        return None;
                    }
                    Some(PlanEntry {
                        content,
                        status: item
                            .get("status")
                            .and_then(Value::as_str)
                            .unwrap_or("pending")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn map_acp_client_request(method: &str, params: &Value) -> Option<SessionServerEvent> {
    match method {
        "client/requestPermission" | "session/request_permission" => {
            let request_id = params
                .get("requestId")
                .or_else(|| params.get("request_id"))
                .or_else(|| params.get("id"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let title = params
                .get("title")
                .or_else(|| params.pointer("/toolCall/title"))
                .or_else(|| params.pointer("/permission/title"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let detail = params
                .get("message")
                .or_else(|| params.get("detail"))
                .or_else(|| params.pointer("/permission/description"))
                .and_then(Value::as_str)
                .map(str::to_string);
            Some(SessionServerEvent::PermissionRequest {
                request_id,
                title,
                detail,
            })
        }
        _ => None,
    }
}

pub(crate) fn message_event(
    role: &str,
    text: String,
    content_blocks: Vec<OutputContentBlockWire>,
    message_id: Option<String>,
) -> Vec<SessionServerEvent> {
    if text.is_empty() && content_blocks.is_empty() {
        return Vec::new();
    }
    vec![SessionServerEvent::Message {
        role: role.to_string(),
        text,
        content_blocks,
        item_id: String::new(),
        message_id,
    }]
}

fn extract_message_payload(update_body: &Value) -> (String, Vec<OutputContentBlockWire>) {
    if let Some(text) = update_body.get("text").and_then(Value::as_str) {
        return (text.to_string(), Vec::new());
    }
    if let Some(content) = update_body.get("content") {
        return extract_content_payload(content);
    }
    if let Some(message) = update_body.get("message") {
        return extract_content_payload(message);
    }
    (String::new(), Vec::new())
}

fn extract_content_payload(content: &Value) -> (String, Vec<OutputContentBlockWire>) {
    match content {
        Value::String(text) => (text.clone(), Vec::new()),
        Value::Array(items) => {
            let mut text = String::new();
            let mut blocks = Vec::new();
            for item in items {
                if item.get("type").and_then(Value::as_str) == Some("text") {
                    if let Some(chunk) = item.get("text").and_then(Value::as_str) {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(chunk);
                    }
                } else if let Some(block) = extract_output_block(item) {
                    blocks.push(block);
                }
            }
            (text, blocks)
        }
        Value::Object(_) => {
            if content.get("type").and_then(Value::as_str) == Some("text") {
                (
                    content
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    Vec::new(),
                )
            } else if let Some(block) = extract_output_block(content) {
                (String::new(), vec![block])
            } else {
                (extract_content_text(content), Vec::new())
            }
        }
        _ => (String::new(), Vec::new()),
    }
}

fn extract_message_id(update_body: &Value) -> Option<String> {
    update_body
        .get("messageId")
        .or_else(|| update_body.get("message_id"))
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .map(str::to_string)
}

fn extract_content_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| {
                if item.get("type").and_then(Value::as_str) == Some("text") {
                    item.get("text").and_then(Value::as_str)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(map) => map
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        _ => String::new(),
    }
}

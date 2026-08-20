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

use super::{PlanEntry, SessionServerEvent, ToolContent};
use agent_client_protocol::schema::v1::{
    ContentBlock, ContentChunk, SessionNotification, SessionUpdate, ToolCall, ToolCallContent,
    ToolCallStatus, ToolCallUpdate, ToolKind, UsageUpdate,
};
use serde::Serialize;
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
        "agent_message" | "agent_message_chunk" => message_event(
            "agent",
            extract_message_text(update_body),
            extract_message_id(update_body),
        ),
        "user_message" | "user_message_chunk" => message_event(
            "user",
            extract_message_text(update_body),
            extract_message_id(update_body),
        ),
        "thought" | "thought_chunk" => message_event(
            "thought",
            extract_message_text(update_body),
            extract_message_id(update_body),
        ),
        "tool_call" | "tool_call_update" => tool_call_event(update_body),
        "plan" | "plan_update" => vec![SessionServerEvent::Plan {
            entries: extract_plan_entries(update_body),
        }],
        "state_update" | "status" => status_event(update_body),
        "usage_update" => extract_usage(update_body),
        // Capability announcements, not conversation: Cursor emits these on
        // every session/new and they carry nothing an operator can act on.
        "available_commands_update" | "current_mode_update" | "config_option_update" => Vec::new(),
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
        SessionUpdate::SessionInfoUpdate(update) => typed_artifact("session_info", update),
        SessionUpdate::UsageUpdate(update) => typed_usage_event(update),
        SessionUpdate::AvailableCommandsUpdate(_) => Vec::new(),
        _ => Vec::new(),
    }
}

fn typed_message_event(role: &str, chunk: &ContentChunk) -> Vec<SessionServerEvent> {
    let ContentBlock::Text(text) = &chunk.content else {
        return Vec::new();
    };
    // `messageId` is optional in ACP v1, so it refines the browser's grouping
    // when a harness sends it and changes nothing when it does not.
    message_event(
        role,
        text.text.clone(),
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
    vec![SessionServerEvent::ToolCall {
        call_id: call.tool_call_id.to_string(),
        title: call.title.clone(),
        kind: tool_kind(call.kind).to_string(),
        status: tool_status(call.status).to_string(),
        locations: call
            .locations
            .iter()
            .map(|location| location.path.display().to_string())
            .collect(),
        content: map_tool_content(&call.content),
    }]
}

fn typed_tool_call_update_event(call: &ToolCallUpdate) -> Vec<SessionServerEvent> {
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
        locations: call
            .fields
            .locations
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|location| location.path.display().to_string())
            .collect(),
        content: map_tool_content(call.fields.content.as_deref().unwrap_or_default()),
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
                _ => None,
            },
            ToolCallContent::Diff(diff) => Some(ToolContent::Diff {
                path: diff.path.display().to_string(),
                old_text: diff.old_text.clone(),
                new_text: diff.new_text.clone(),
            }),
            _ => None,
        })
        .collect()
}

fn typed_artifact<T: Serialize>(kind: &str, update: &T) -> Vec<SessionServerEvent> {
    vec![SessionServerEvent::Artifact {
        kind: kind.to_string(),
        title: None,
        body: serde_json::to_string_pretty(update).ok(),
    }]
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
        locations: extract_tool_locations(update_body),
        content: extract_tool_content(update_body),
    }]
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
                    _ => {
                        let text = extract_content_text(item.get("content").unwrap_or(item));
                        (!text.trim().is_empty()).then_some(ToolContent::Text { text })
                    }
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
    message_id: Option<String>,
) -> Vec<SessionServerEvent> {
    if text.is_empty() {
        return Vec::new();
    }
    vec![SessionServerEvent::Message {
        role: role.to_string(),
        text,
        item_id: String::new(),
        message_id,
    }]
}

fn extract_message_id(update_body: &Value) -> Option<String> {
    update_body
        .get("messageId")
        .or_else(|| update_body.get("message_id"))
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .map(str::to_string)
}

fn extract_message_text(update_body: &Value) -> String {
    if let Some(text) = update_body.get("text").and_then(Value::as_str) {
        return text.to_string();
    }
    if let Some(content) = update_body.get("content") {
        return extract_content_text(content);
    }
    if let Some(message) = update_body.get("message") {
        return extract_content_text(message);
    }
    String::new()
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

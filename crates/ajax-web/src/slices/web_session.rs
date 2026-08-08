//! Browser orchestration-chat session attach planning and wire protocol.

use ajax_core::{commands::CommandContext, models::AgentClient, registry::Registry};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

pub const SESSION_PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionClientMessage {
    #[serde(rename = "prompt")]
    Prompt { text: String },
    #[serde(rename = "cancel")]
    Cancel,
    #[serde(rename = "permission")]
    Permission {
        #[serde(rename = "requestId")]
        request_id: String,
        approved: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionServerEvent {
    #[serde(rename = "ready")]
    Ready,
    #[serde(rename = "message")]
    Message { role: String, text: String },
    #[serde(rename = "artifact")]
    Artifact {
        kind: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<String>,
    },
    #[serde(rename = "permission_request")]
    PermissionRequest {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    #[serde(rename = "tool_call")]
    ToolCall {
        #[serde(rename = "callId")]
        call_id: String,
        title: String,
        kind: String,
        status: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        locations: Vec<String>,
    },
    #[serde(rename = "plan")]
    Plan { entries: Vec<PlanEntry> },
    #[serde(rename = "status")]
    Status {
        state: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    #[serde(rename = "turn_end")]
    TurnEnd {
        #[serde(
            rename = "stopReason",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        stop_reason: Option<String>,
    },
    #[serde(rename = "error")]
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanEntry {
    pub content: String,
    pub status: String,
}

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
            message_event("agent", extract_message_text(update_body))
        }
        "user_message" | "user_message_chunk" => {
            message_event("user", extract_message_text(update_body))
        }
        "thought" | "thought_chunk" => message_event("thought", extract_message_text(update_body)),
        "tool_call" | "tool_call_update" => tool_call_event(update_body),
        "plan" | "plan_update" => vec![SessionServerEvent::Plan {
            entries: extract_plan_entries(update_body),
        }],
        "state_update" => {
            let state = update_body
                .get("state")
                .or_else(|| update_body.pointer("/status/state"))
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let detail = update_body
                .get("stopReason")
                .or_else(|| update_body.get("detail"))
                .and_then(Value::as_str)
                .map(str::to_string);
            vec![SessionServerEvent::Status { state, detail }]
        }
        other if !other.is_empty() => vec![SessionServerEvent::Artifact {
            kind: other.to_string(),
            title: None,
            body: serde_json::to_string_pretty(update_body).ok(),
        }],
        _ => Vec::new(),
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
                .and_then(Value::as_str)?
                .to_string();
            let title = params
                .get("title")
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

fn message_event(role: &str, text: String) -> Vec<SessionServerEvent> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    vec![SessionServerEvent::Message {
        role: role.to_string(),
        text,
    }]
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionAttachPlan {
    pub qualified_handle: String,
    pub worktree_path: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionRouteError {
    TaskNotFound,
    WorktreeMissing,
    NotOrchestrationChat,
}

pub fn prepare_task_session<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<SessionAttachPlan, SessionRouteError> {
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
        .ok_or(SessionRouteError::TaskNotFound)?;

    if task.selected_agent != AgentClient::Cursor {
        return Err(SessionRouteError::NotOrchestrationChat);
    }
    if !task.worktree_path.exists() {
        return Err(SessionRouteError::WorktreeMissing);
    }

    Ok(SessionAttachPlan {
        qualified_handle: qualified_handle.to_string(),
        worktree_path: task.worktree_path.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;

    #[test]
    fn prepare_task_session_returns_worktree_for_cursor_task() {
        let mut task = test_support::fix_login_task();
        task.selected_agent = AgentClient::Cursor;
        let worktree = std::env::temp_dir().join("ajax-web-session-test-fix-login");
        let _ = std::fs::remove_dir_all(&worktree);
        std::fs::create_dir_all(&worktree).expect("worktree dir");
        task.worktree_path = worktree;
        let context = test_support::context_with_tasks(&["web"], vec![task]);
        let plan = prepare_task_session(&context, "web/fix-login").expect("plan");
        assert_eq!(plan.qualified_handle, "web/fix-login");
        assert!(plan
            .worktree_path
            .ends_with("ajax-web-session-test-fix-login"));
    }

    #[test]
    fn prepare_task_session_rejects_non_cursor_agent() {
        let mut task = test_support::fix_login_task();
        task.selected_agent = ajax_core::models::AgentClient::Codex;
        let context = test_support::context_with_tasks(&["web"], vec![task]);
        let error = prepare_task_session(&context, "web/fix-login").unwrap_err();
        assert_eq!(error, SessionRouteError::NotOrchestrationChat);
    }

    #[test]
    fn map_tool_call_to_structured_event_not_raw_json() {
        let update = serde_json::json!({
            "sessionId": "sess_1",
            "update": {
                "sessionUpdate": "tool_call",
                "toolCallId": "call_001",
                "title": "Read configuration",
                "kind": "read",
                "status": "pending",
                "locations": [{ "path": "/repo/config.json" }]
            }
        });
        assert_eq!(
            map_acp_session_update(&update),
            vec![SessionServerEvent::ToolCall {
                call_id: "call_001".to_string(),
                title: "Read configuration".to_string(),
                kind: "read".to_string(),
                status: "pending".to_string(),
                locations: vec!["/repo/config.json".to_string()],
            }]
        );
    }

    #[test]
    fn map_tool_call_update_keeps_call_id_when_title_absent() {
        let update = serde_json::json!({
            "update": {
                "sessionUpdate": "tool_call_update",
                "toolCallId": "call_001",
                "status": "completed"
            }
        });
        let events = map_acp_session_update(&update);
        let SessionServerEvent::ToolCall {
            call_id,
            title,
            status,
            ..
        } = &events[0]
        else {
            panic!("expected tool call, got {events:?}");
        };
        assert_eq!(call_id, "call_001");
        assert_eq!(title, "");
        assert_eq!(status, "completed");
    }

    #[test]
    fn map_tool_call_without_id_is_dropped() {
        let update = serde_json::json!({
            "update": { "sessionUpdate": "tool_call", "title": "Nameless" }
        });
        assert!(map_acp_session_update(&update).is_empty());
    }

    #[test]
    fn map_thought_uses_its_own_role_so_chat_can_separate_reasoning() {
        let update = serde_json::json!({
            "update": {
                "sessionUpdate": "thought_chunk",
                "content": { "type": "text", "text": "Checking the router" }
            }
        });
        assert_eq!(
            map_acp_session_update(&update),
            vec![SessionServerEvent::Message {
                role: "thought".to_string(),
                text: "Checking the router".to_string(),
            }]
        );
    }

    #[test]
    fn map_plan_to_structured_entries() {
        let update = serde_json::json!({
            "update": {
                "sessionUpdate": "plan",
                "entries": [
                    { "content": "Read the router", "status": "completed" },
                    { "content": "Patch the guard", "status": "in_progress" },
                    { "content": "   ", "status": "pending" }
                ]
            }
        });
        assert_eq!(
            map_acp_session_update(&update),
            vec![SessionServerEvent::Plan {
                entries: vec![
                    PlanEntry {
                        content: "Read the router".to_string(),
                        status: "completed".to_string(),
                    },
                    PlanEntry {
                        content: "Patch the guard".to_string(),
                        status: "in_progress".to_string(),
                    },
                ],
            }]
        );
    }

    #[test]
    fn unknown_update_body_is_pretty_printed_not_a_single_line_dump() {
        let update = serde_json::json!({
            "update": { "sessionUpdate": "available_commands_update", "commands": ["a"] }
        });
        let events = map_acp_session_update(&update);
        let SessionServerEvent::Artifact { body, .. } = &events[0] else {
            panic!("expected artifact, got {events:?}");
        };
        assert!(body.as_deref().unwrap_or_default().contains('\n'));
    }

    #[test]
    fn map_agent_message_chunk_to_browser_message() {
        let update = serde_json::json!({
            "sessionId": "sess_1",
            "update": {
                "sessionUpdate": "agent_message_chunk",
                "content": { "type": "text", "text": "Working on it" }
            }
        });
        let events = map_acp_session_update(&update);
        assert_eq!(
            events,
            vec![SessionServerEvent::Message {
                role: "agent".to_string(),
                text: "Working on it".to_string(),
            }]
        );
    }
}

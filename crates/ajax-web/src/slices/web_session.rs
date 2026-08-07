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
    #[serde(rename = "status")]
    Status {
        state: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    #[serde(rename = "error")]
    Error { message: String },
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
        "thought" | "thought_chunk" => message_event("system", extract_message_text(update_body)),
        "plan" | "plan_update" => vec![SessionServerEvent::Artifact {
            kind: "plan".to_string(),
            title: update_body
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_string),
            body: Some(update_body.to_string()),
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
            body: Some(update_body.to_string()),
        }],
        _ => Vec::new(),
    }
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

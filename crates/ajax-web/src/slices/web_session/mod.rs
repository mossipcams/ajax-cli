//! Browser orchestration-chat wire protocol and ACP update mapping.

use ajax_core::{
    adapters::acp_launch_for_agent, commands::CommandContext, models::AgentClient,
    registry::Registry,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::VecDeque, path::PathBuf};

pub const SESSION_PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionClientMessage {
    #[serde(rename = "prompt")]
    Prompt { text: String },
    #[serde(rename = "cancel")]
    Cancel {
        #[serde(default, rename = "keepQueue")]
        keep_queue: bool,
    },
    #[serde(rename = "set_model")]
    SetModel { model: String },
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
    Ready {
        #[serde(default = "default_session_model")]
        model: String,
        /// Whether a turn is actually in flight. The transcript alone cannot say:
        /// replayed history has no turn-start marker, so a trailing host note
        /// would otherwise leave the browser reading "Working" forever.
        #[serde(default)]
        busy: bool,
    },
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
    /// Operator answered a permission request. Recorded so reconnect/reload
    /// replay does not resurrect an already-decided prompt.
    #[serde(rename = "permission_resolved")]
    PermissionResolved {
        #[serde(rename = "requestId")]
        request_id: String,
        approved: bool,
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

fn default_session_model() -> String {
    "auto".to_string()
}

/// Model a harness runs when neither the socket nor the task pins one. Cursor
/// gets the Ajax default (the same one an interactive Cursor task launches
/// with); a bridge harness has none here and picks for itself.
fn harness_default_model(agent: AgentClient) -> Option<&'static str> {
    acp_launch_for_agent(agent).and_then(|launch| launch.default_model)
}

/// Normalize a client-supplied model id for ACP spawn.
/// Empty / whitespace → `auto`. Rejects control chars, spaces, and oversized ids.
pub fn normalize_session_model(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(default_session_model());
    }
    if ajax_core::adapters::parse_model_selection(trimmed).is_none() {
        return Err("model id must not contain whitespace or exceed 128 chars".to_string());
    }
    Ok(trimmed.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanEntry {
    pub content: String,
    pub status: String,
}

/// Cursor ACP allows one in-flight `session/prompt`; additional prompts queue here.
pub const MAX_QUEUED_PROMPTS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptDispatch {
    StartNow,
    Queued,
}

/// Decide whether to start a prompt now or enqueue it behind the in-flight turn.
pub fn dispatch_prompt(
    prompt_in_flight: bool,
    queued: &mut VecDeque<String>,
    text: String,
) -> PromptDispatch {
    if prompt_in_flight {
        // ponytail: cap at 8 queued prompts; upgrade path is block + error event to the operator.
        if queued.len() >= MAX_QUEUED_PROMPTS {
            queued.pop_front();
        }
        queued.push_back(text);
        PromptDispatch::Queued
    } else {
        PromptDispatch::StartNow
    }
}

pub fn clear_prompt_queue(queued: &mut VecDeque<String>) {
    queued.clear();
}

/// Cancel clears queued prompts unless the operator asked to keep them.
pub fn apply_cancel_to_queue(queued: &mut VecDeque<String>, keep_queue: bool) {
    if !keep_queue {
        clear_prompt_queue(queued);
    }
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
        // Capability announcements, not conversation: Cursor emits these on
        // every session/new and they carry nothing an operator can act on.
        "available_commands_update" | "current_mode_update" => Vec::new(),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionAttachPlan {
    pub qualified_handle: String,
    pub worktree_path: PathBuf,
    /// Normalized Cursor model id (`auto` for CLI default).
    pub model: String,
    /// Harness whose ACP process backs this session.
    pub agent: AgentClient,
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
    model: &str,
) -> Result<SessionAttachPlan, SessionRouteError> {
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
        .ok_or(SessionRouteError::TaskNotFound)?;

    // Any harness Ajax can start over ACP qualifies; the durable provisioned bit
    // still decides, so an interactive tmux task never gets a second agent.
    if acp_launch_for_agent(task.selected_agent).is_none() || !task.skip_interactive_agent() {
        return Err(SessionRouteError::NotOrchestrationChat);
    }
    if !task.worktree_path.exists() {
        return Err(SessionRouteError::WorktreeMissing);
    }

    // The browser may pin a model per socket; otherwise the task's own choice
    // (made when it was created) wins, then the harness default.
    let model = match normalize_session_model(model) {
        Ok(model) if model != default_session_model() => model,
        _ => task
            .session_model()
            .map(str::to_string)
            .or_else(|| harness_default_model(task.selected_agent).map(str::to_string))
            .unwrap_or_default(),
    };

    Ok(SessionAttachPlan {
        qualified_handle: qualified_handle.to_string(),
        worktree_path: task.worktree_path.clone(),
        model,
        agent: task.selected_agent,
    })
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod fake_acp_tests;

//! Browser orchestration-chat wire protocol. ACP update mapping lives in
//! `acp_map`; this module owns only the shapes both ends agree on.

mod acp_map;

pub use acp_map::{map_acp_client_request, map_acp_session_notification, map_acp_session_update};

use ajax_core::{
    adapters::acp_launch_for_agent, commands::CommandContext, models::AgentClient,
    registry::Registry,
};
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, path::PathBuf};

pub const SESSION_PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionClientMessage {
    #[serde(rename = "prompt")]
    Prompt {
        text: String,
        #[serde(rename = "clientMessageId")]
        client_message_id: String,
    },
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
    Message {
        role: String,
        text: String,
        /// ACP v1 message identity. Chunks sharing one id are one message; a
        /// change starts a new one. Optional in the protocol, so the browser
        /// keeps its role-adjacency fallback for harnesses that omit it.
        #[serde(rename = "messageId", default, skip_serializing_if = "Option::is_none")]
        message_id: Option<String>,
    },
    #[serde(rename = "prompt_accepted")]
    PromptAccepted {
        #[serde(rename = "clientMessageId")]
        client_message_id: String,
    },
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
        /// What the call produced: printed output, a file diff. Carried through
        /// so the browser can render a diff as a diff instead of announcing
        /// that an unnamed edit happened.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        content: Vec<ToolContent>,
    },
    #[serde(rename = "plan")]
    Plan { entries: Vec<PlanEntry> },
    /// Context window pressure, from ACP `usage_update`.
    #[serde(rename = "usage")]
    Usage { used: u64, size: u64 },
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

/// Output attached to a tool call. Mirrors the two `ToolCallContent` variants
/// Ajax can receive; `terminal` is absent because Ajax advertises no
/// `terminal/*` client capability for an agent to create one with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "diff")]
    Diff {
        path: String,
        #[serde(rename = "oldText", default, skip_serializing_if = "Option::is_none")]
        old_text: Option<String>,
        #[serde(rename = "newText")]
        new_text: String,
    },
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

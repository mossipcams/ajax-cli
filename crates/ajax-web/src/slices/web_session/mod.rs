//! Browser orchestration-chat wire protocol and per-task session runtime.

mod acp_drain;
mod acp_map;
pub(crate) mod acp_usage;
#[path = "../ci_agent_delivery.rs"]
mod ci_agent_delivery;
pub(crate) mod model_change;
mod normalize;
mod output_content;
mod prompt_content;
mod protocol;
mod replay;
mod session_cleanup;
mod task_session;
mod task_session_directory;
mod task_session_spawn;
mod transcript;
mod ws_bridge;

pub use acp_map::{map_acp_client_request, map_acp_session_notification, map_acp_session_update};
pub(crate) use ci_agent_delivery::deliver as deliver_agent_notification;
pub use protocol::{
    parse_client_cursor, SessionEventEnvelope, SessionSnapshot, SESSION_PROTOCOL_VERSION,
};
pub(crate) use session_cleanup::owned_session_handles;
pub(crate) use task_session_directory::TaskSessionDirectory;
pub(crate) use task_session_directory::{apply_client_message, ApplyClientMessageOutcome};
pub(crate) use ws_bridge::bridge_task_session_socket;

use ajax_core::{
    adapters::acp_launch_for_agent, commands::CommandContext, models::AgentClient,
    registry::Registry,
};

use crate::adapters::web_session_acp::is_unspecified_model;
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, path::PathBuf, sync::Arc};

pub(crate) type PersistSessionModel = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionClientMessage {
    #[serde(rename = "prompt")]
    Prompt {
        text: String,
        #[serde(default, rename = "contentBlocks")]
        content_blocks: Vec<prompt_content::PromptContentBlockWire>,
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
    #[serde(rename = "set_config_option")]
    SetConfigOption {
        #[serde(rename = "configId")]
        config_id: String,
        value: crate::adapters::web_session_acp::SessionConfigValue,
    },
    #[serde(rename = "permission")]
    Permission {
        #[serde(rename = "requestId")]
        request_id: String,
        approved: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    #[serde(rename = "elicitation")]
    Elicitation {
        #[serde(rename = "requestId")]
        request_id: String,
        action: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content: Option<serde_json::Value>,
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
        /// Non-text ACP output blocks (image, resource_link, embedded resource).
        #[serde(
            default,
            rename = "contentBlocks",
            skip_serializing_if = "Vec::is_empty"
        )]
        content_blocks: Vec<output_content::OutputContentBlockWire>,
        /// Stable host-generated identity for replace-by-id replay in the browser.
        #[serde(rename = "itemId", default)]
        item_id: String,
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
    #[serde(rename = "elicitation_request")]
    ElicitationRequest {
        #[serde(rename = "requestId")]
        request_id: String,
        message: String,
        schema: serde_json::Value,
    },
    /// Operator answered an agent elicitation. Recorded so reconnect/reload
    /// replay does not resurrect an already-decided prompt.
    #[serde(rename = "elicitation_resolved")]
    ElicitationResolved {
        #[serde(rename = "requestId")]
        request_id: String,
        action: String,
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
    /// Per-turn token usage, from ACP `session/prompt` result.usage.
    #[serde(rename = "turn_usage")]
    TurnUsage {
        #[serde(rename = "requestId", default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
        #[serde(
            rename = "inputTokens",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        input_tokens: Option<u64>,
        #[serde(
            rename = "outputTokens",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        output_tokens: Option<u64>,
        #[serde(
            rename = "cacheReadTokens",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        cache_read_tokens: Option<u64>,
        #[serde(
            rename = "cacheWriteTokens",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        cache_write_tokens: Option<u64>,
        #[serde(
            rename = "totalTokens",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        total_tokens: Option<u64>,
    },
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

/// Output attached to a tool call. Mirrors ACP `ToolCallContent` minus
/// `terminal`: Ajax advertises no `terminal/*` client capability.
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
    #[serde(rename = "image")]
    Image {
        #[serde(rename = "mimeType")]
        mime_type: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        uri: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<String>,
    },
    #[serde(rename = "resource_link")]
    ResourceLink {
        name: String,
        uri: String,
        #[serde(default, rename = "mimeType", skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    #[serde(rename = "resource")]
    Resource {
        uri: String,
        #[serde(default, rename = "mimeType", skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        blob: Option<String>,
    },
}

/// Cursor ACP allows one in-flight `session/prompt`; additional prompts queue here.
pub const MAX_QUEUED_PROMPTS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptDispatch {
    StartNow,
    Queued,
}

/// One validated prompt waiting behind an in-flight turn.
#[derive(Debug, Clone)]
pub struct QueuedPrompt {
    pub transcript_text: String,
    pub blocks: Vec<agent_client_protocol::schema::v1::ContentBlock>,
}

/// Decide whether to start a prompt now or enqueue it behind the in-flight turn.
pub fn dispatch_prompt(
    prompt_in_flight: bool,
    queued: &mut VecDeque<QueuedPrompt>,
    payload: QueuedPrompt,
) -> PromptDispatch {
    if prompt_in_flight {
        // ponytail: cap at 8 queued prompts; upgrade path is block + error event to the operator.
        if queued.len() >= MAX_QUEUED_PROMPTS {
            queued.pop_front();
        }
        queued.push_back(payload);
        PromptDispatch::Queued
    } else {
        PromptDispatch::StartNow
    }
}

pub fn clear_prompt_queue(queued: &mut VecDeque<QueuedPrompt>) {
    queued.clear();
}

/// Cancel clears queued prompts unless the operator asked to keep them.
pub fn apply_cancel_to_queue(queued: &mut VecDeque<QueuedPrompt>, keep_queue: bool) {
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

    // Task metadata wins over a socket ?model= pin (#910). The URL fallback is
    // only for tasks with no stored model, then the harness default. Legacy
    // stored `auto` is unspecified ([#952](https://github.com/mossipcams/ajax-cli/issues/952)).
    let url_model = normalize_session_model(model).ok();
    let model = task
        .session_model()
        .filter(|stored| !is_unspecified_model(Some(stored)))
        .map(str::to_string)
        .or_else(|| {
            url_model
                .filter(|model| *model != default_session_model())
                .map(|model| model.to_string())
        })
        .or_else(|| harness_default_model(task.selected_agent).map(str::to_string))
        .unwrap_or_default();

    Ok(SessionAttachPlan {
        qualified_handle: qualified_handle.to_string(),
        worktree_path: task.worktree_path.clone(),
        model,
        agent: task.selected_agent,
    })
}

#[cfg(test)]
mod normalize_tests;

#[cfg(test)]
mod protocol_tests;

#[cfg(test)]
mod replay_tests;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod fake_acp_tests;

#[cfg(test)]
mod task_session_tests;

#[cfg(test)]
mod task_session_idle_eviction_tests;

#[cfg(test)]
mod transcript_tests;

#[cfg(test)]
mod available_commands_tests;
#[cfg(test)]
mod elicitation_tests;
#[cfg(test)]
mod output_content_tests;
#[cfg(test)]
mod prompt_capabilities_tests;
#[cfg(test)]
mod prompt_content_tests;
#[cfg(test)]
mod session_info_tests;

#[cfg(test)]
mod acp_drain_tests;

#[cfg(test)]
mod acp_usage_tests;

#[cfg(test)]
mod ws_bridge_tests;

#[cfg(test)]
mod session_cleanup_tests;

#[cfg(test)]
mod session_close_tests;

#[cfg(test)]
pub(crate) mod test_support;

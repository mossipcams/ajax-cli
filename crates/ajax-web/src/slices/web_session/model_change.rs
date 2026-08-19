//! Same-harness model apply for Switch and WebSocket `set_model`.

use super::normalize_session_model;
use super::task_session_directory::TaskSessionDirectory;
use ajax_core::models::AgentClient;
use std::path::Path;

/// Cross-harness swap resets backend context; same harness keeps the ACP child.
pub(crate) fn swap_resets_harness_context(current: AgentClient, requested_agent: &str) -> bool {
    agent_client_from_name(requested_agent) != current
}

pub(crate) fn agent_client_from_name(agent: &str) -> AgentClient {
    match agent.trim().to_ascii_lowercase().as_str() {
        "codex" => AgentClient::Codex,
        "claude" => AgentClient::Claude,
        "cursor" => AgentClient::Cursor,
        "pi" => AgentClient::Pi,
        _ => AgentClient::Other,
    }
}

/// Apply persisted `session_model` on a live slot when present; no-op without one.
pub(crate) async fn apply_persisted_model(
    directory: &TaskSessionDirectory,
    handle: &str,
    worktree_path: &Path,
    model: Option<&str>,
) -> Result<(), String> {
    if !directory.has_live_entry(handle) {
        return Ok(());
    }
    let model = normalize_session_model(model.unwrap_or("auto"))?;
    directory
        .apply_model(handle, worktree_path, &model)
        .await
        .map(|_| ())
}

/// After a cross-harness swap: reset the live slot or clear stored resume id.
pub(crate) async fn apply_cross_harness_reset(
    directory: &TaskSessionDirectory,
    handle: &str,
    worktree_path: &Path,
    agent: AgentClient,
    model: Option<&str>,
) -> Result<(), String> {
    let model = normalize_session_model(model.unwrap_or("auto"))?;
    directory
        .reset_harness_context(handle, worktree_path, agent, &model)
        .await
}

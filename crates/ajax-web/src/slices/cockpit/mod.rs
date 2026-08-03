//! Browser Cockpit read experience.

use ajax_core::{
    commands::{self, CommandContext},
    models::{AgentAttempt, GitStatus, TmuxStatus},
    output::{InboxResponse, ReposResponse, TaskCard},
    registry::Registry,
};
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::slices::actions::{browser_actions, WebAction};

#[derive(Serialize)]
pub struct BrowserCockpitView {
    pub backend: BrowserBackend,
    pub repos: ReposResponse,
    pub cards: Vec<BrowserTaskCard>,
    pub inbox: InboxResponse,
}

#[derive(Serialize)]
pub struct BrowserBackend {
    pub authority: &'static str,
    pub control_enabled: bool,
    pub warning: Option<&'static str>,
}

#[derive(Serialize)]
pub struct BrowserTaskCard {
    pub id: String,
    pub qualified_handle: String,
    pub repo: String,
    pub title: String,
    pub status: ajax_core::ui_state::TaskStatus,
    pub status_explanation: Option<String>,
    pub attention: ajax_core::ui_state::AttentionBand,
    pub last_activity_unix_secs: u64,
    pub actions: Vec<WebAction>,
}

pub fn browser_cockpit_json<R: Registry>(
    context: &CommandContext<R>,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&browser_cockpit_view(context))
}

pub fn browser_cockpit_view<R: Registry>(context: &CommandContext<R>) -> BrowserCockpitView {
    let view = commands::cockpit_view(context);
    BrowserCockpitView {
        backend: host_native_backend(),
        repos: view.repos,
        cards: view
            .cards
            .iter()
            .map(|card| browser_task_card(context, card))
            .collect(),
        inbox: view.inbox,
    }
}

fn host_native_backend() -> BrowserBackend {
    BrowserBackend {
        authority: "host-native",
        control_enabled: true,
        warning: None,
    }
}

fn browser_task_card<R: Registry>(context: &CommandContext<R>, card: &TaskCard) -> BrowserTaskCard {
    BrowserTaskCard {
        id: card.id.as_str().to_string(),
        qualified_handle: card.qualified_handle.clone(),
        repo: repo_of_handle(&card.qualified_handle),
        title: card.title.clone(),
        status: card.status,
        status_explanation: card.status_explanation.clone(),
        attention: card.attention,
        last_activity_unix_secs: unix_secs(card.last_activity_at),
        actions: browser_actions(context, card),
    }
}

/// Explicit repository identity for browser DTOs. Splitting `qualified_handle`
/// is a policy the browser must not own; Rust derives it once here.
fn repo_of_handle(qualified_handle: &str) -> String {
    qualified_handle
        .split_once('/')
        .map(|(repo, _)| repo.to_string())
        .unwrap_or_else(|| qualified_handle.to_string())
}

#[derive(Serialize)]
pub struct BrowserTaskDetail {
    pub qualified_handle: String,
    pub repo: String,
    pub title: String,
    pub branch: String,
    pub base_branch: String,
    pub worktree_path: String,
    pub tmux_session: String,
    pub lifecycle: String,
    pub agent: String,
    pub agent_status: String,
    pub status: ajax_core::ui_state::TaskStatus,
    pub status_explanation: Option<String>,
    pub runtime_observation_error: Option<String>,
    pub actions: Vec<WebAction>,
    pub live_status_kind: Option<String>,
    pub live_status_summary: Option<String>,
    pub agent_activity: Option<String>,
    pub git: Option<GitStatus>,
    pub tmux: Option<TmuxStatus>,
    pub annotations: Vec<String>,
    pub created_unix_secs: u64,
    pub last_activity_unix_secs: u64,
    pub agent_attempts: Vec<BrowserAgentAttempt>,
}

#[derive(Serialize)]
pub struct BrowserAgentAttempt {
    pub started_unix_secs: u64,
    pub completed_unix_secs: Option<u64>,
    pub outcome: String,
}

pub fn browser_task_detail_view<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Option<BrowserTaskDetail> {
    let view = commands::cockpit_view(context);
    let card = view
        .cards
        .iter()
        .find(|card| card.qualified_handle == qualified_handle)?;
    let task = context.registry.get_task(&card.id)?.clone();
    let actions = browser_actions(context, card);
    let live_status_kind = task
        .live_status
        .as_ref()
        .map(|live| format!("{:?}", live.kind));
    let live_status_summary = task.live_status.as_ref().map(|live| live.summary.clone());
    let agent_activity = live_status_summary.clone();

    Some(BrowserTaskDetail {
        qualified_handle: task.qualified_handle(),
        repo: task.repo.clone(),
        title: task.title.clone(),
        branch: task.branch.clone(),
        base_branch: task.base_branch.clone(),
        worktree_path: task.worktree_path.display().to_string(),
        tmux_session: task.tmux_session.clone(),
        lifecycle: format!("{:?}", task.lifecycle_status),
        agent: format!("{:?}", task.selected_agent),
        agent_status: format!("{:?}", task.agent_status),
        status: card.status,
        status_explanation: card.status_explanation.clone(),
        runtime_observation_error: task.runtime_projection.observation_error.clone(),
        actions,
        live_status_kind,
        live_status_summary,
        agent_activity,
        git: task.git_status.clone(),
        tmux: task.tmux_status.clone(),
        annotations: task
            .annotations
            .iter()
            .map(|annotation| format!("{annotation:?}"))
            .collect(),
        created_unix_secs: unix_secs(task.created_at),
        last_activity_unix_secs: unix_secs(task.last_activity_at),
        agent_attempts: task
            .agent_attempts
            .iter()
            .map(browser_agent_attempt)
            .collect(),
    })
}

fn unix_secs(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn browser_agent_attempt(attempt: &AgentAttempt) -> BrowserAgentAttempt {
    BrowserAgentAttempt {
        started_unix_secs: unix_secs(attempt.started_at),
        completed_unix_secs: attempt.finished_at.map(unix_secs),
        outcome: format!("{:?}", attempt.status),
    }
}

#[cfg(test)]
pub(crate) mod tests;

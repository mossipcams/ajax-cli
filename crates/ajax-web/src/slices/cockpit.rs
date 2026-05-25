//! Browser Cockpit read experience.

use ajax_core::{
    commands::{self, CommandContext},
    models::{AgentAttempt, GitStatus, LifecycleStatus, OperatorAction, TmuxStatus},
    output::{InboxResponse, ReposResponse, TaskCard},
    registry::Registry,
};
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize)]
pub struct BrowserCockpitView {
    pub backend: BrowserBackend,
    pub repos: ReposResponse,
    pub cards: Vec<BrowserTaskCard>,
    pub inbox: InboxResponse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendAuthority {
    HostNative,
    SnapshotOnly,
}

impl BackendAuthority {
    pub fn control_enabled(self) -> bool {
        matches!(self, Self::HostNative)
    }
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
    pub title: String,
    pub ui_state: String,
    pub status_label: String,
    pub lifecycle: String,
    pub primary_action: String,
    pub available_actions: Vec<String>,
    pub live_summary: Option<String>,
}

pub fn browser_cockpit_json<R: Registry>(
    context: &CommandContext<R>,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&browser_cockpit_view(context))
}

pub fn browser_cockpit_view<R: Registry>(context: &CommandContext<R>) -> BrowserCockpitView {
    browser_cockpit_view_with_backend(context, BackendAuthority::HostNative)
}

pub fn browser_cockpit_view_with_backend<R: Registry>(
    context: &CommandContext<R>,
    backend: BackendAuthority,
) -> BrowserCockpitView {
    let view = commands::rebuild_cockpit_view(context);
    BrowserCockpitView {
        backend: browser_backend(backend),
        repos: view.repos,
        cards: view.cards.iter().map(browser_task_card).collect(),
        inbox: view.inbox,
    }
}

fn browser_backend(backend: BackendAuthority) -> BrowserBackend {
    match backend {
        BackendAuthority::HostNative => BrowserBackend {
            authority: "host-native",
            control_enabled: true,
            warning: None,
        },
        BackendAuthority::SnapshotOnly => BrowserBackend {
            authority: "snapshot-only",
            control_enabled: false,
            warning: Some(
                "Live PWA control requires the host-native Ajax web backend with access to SQLite, repo paths, worktrees, tmux sessions, agent CLIs, and host process state.",
            ),
        },
    }
}

fn browser_task_card(card: &TaskCard) -> BrowserTaskCard {
    let available: Vec<OperatorAction> = card
        .available_actions
        .iter()
        .copied()
        .filter(|action| is_web_supported(*action))
        .collect();
    let primary = if is_web_supported(card.primary_action) {
        card.primary_action
    } else {
        available.first().copied().unwrap_or(card.primary_action)
    };

    BrowserTaskCard {
        id: card.id.as_str().to_string(),
        qualified_handle: card.qualified_handle.clone(),
        title: card.title.clone(),
        ui_state: card.ui_state.as_str().to_string(),
        status_label: card.status_label.clone(),
        lifecycle: format!("{:?}", card.lifecycle),
        primary_action: primary.as_str().to_string(),
        available_actions: available
            .iter()
            .map(|action| action.as_str().to_string())
            .collect(),
        live_summary: card.live_summary.clone(),
    }
}

// Resume drops the operator into a native tmux pane and Start needs an
// interactive title prompt; both are rejected by web action handling, so the
// browser Cockpit should not surface them as buttons.
fn is_web_supported(action: OperatorAction) -> bool {
    !matches!(action, OperatorAction::Resume | OperatorAction::Start)
}

#[derive(Serialize)]
pub struct BrowserTaskDetail {
    pub qualified_handle: String,
    pub title: String,
    pub branch: String,
    pub base_branch: String,
    pub worktree_path: String,
    pub tmux_session: String,
    pub lifecycle: String,
    pub agent: String,
    pub agent_status: String,
    pub ui_state: String,
    pub status_label: String,
    pub primary_action: String,
    pub available_actions: Vec<String>,
    pub live_status_kind: Option<String>,
    pub live_status_summary: Option<String>,
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

pub fn browser_task_detail_json<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Option<Result<String, serde_json::Error>> {
    browser_task_detail_view(context, qualified_handle).map(|detail| serde_json::to_string(&detail))
}

pub fn browser_task_detail_view<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Option<BrowserTaskDetail> {
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
        .cloned()?;

    let view = commands::rebuild_cockpit_view(context);
    let card = view
        .cards
        .iter()
        .find(|card| card.qualified_handle == qualified_handle);
    let card_clone = card.cloned();
    let available_actions: Vec<String> = card_clone
        .as_ref()
        .map(|card| {
            card.available_actions
                .iter()
                .copied()
                .filter(|action| is_web_supported(*action))
                .map(|action| action.as_str().to_string())
                .collect()
        })
        .unwrap_or_default();
    let primary_action = card_clone
        .as_ref()
        .map(|card| card.primary_action.as_str().to_string())
        .unwrap_or_else(|| OperatorAction::Resume.as_str().to_string());

    Some(BrowserTaskDetail {
        qualified_handle: task.qualified_handle(),
        title: task.title.clone(),
        branch: task.branch.clone(),
        base_branch: task.base_branch.clone(),
        worktree_path: task.worktree_path.display().to_string(),
        tmux_session: task.tmux_session.clone(),
        lifecycle: lifecycle_label(task.lifecycle_status),
        agent: format!("{:?}", task.selected_agent),
        agent_status: format!("{:?}", task.agent_status),
        ui_state: card_clone
            .as_ref()
            .map(|card| card.ui_state.as_str().to_string())
            .unwrap_or_default(),
        status_label: card_clone
            .as_ref()
            .map(|card| card.status_label.clone())
            .unwrap_or_default(),
        primary_action,
        available_actions,
        live_status_kind: task
            .live_status
            .as_ref()
            .map(|live| format!("{:?}", live.kind)),
        live_status_summary: task.live_status.as_ref().map(|live| live.summary.clone()),
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

fn lifecycle_label(status: LifecycleStatus) -> String {
    format!("{status:?}")
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
mod tests {
    use super::{browser_cockpit_json, browser_task_card};
    use ajax_core::{
        commands::CommandContext,
        config::Config,
        models::{LifecycleStatus, OperatorAction, TaskId},
        output::TaskCard,
        registry::InMemoryRegistry,
        ui_state::UiState,
    };

    #[test]
    fn cockpit_slice_serializes_empty_projection() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let json = browser_cockpit_json(&context).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["repos"]["repos"], serde_json::json!([]));
        assert_eq!(value["cards"], serde_json::json!([]));
        assert_eq!(value["inbox"]["items"], serde_json::json!([]));
    }

    #[test]
    fn task_detail_returns_none_for_unknown_handle() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let detail = super::browser_task_detail_view(&context, "web/missing");
        assert!(detail.is_none());
    }

    #[test]
    fn task_detail_surfaces_structured_live_state_for_a_task() {
        use ajax_core::config::ManagedRepo;
        use ajax_core::models::{AgentClient, GitStatus, LiveObservation, LiveStatusKind, Task};
        use ajax_core::registry::Registry as _;

        let config = Config {
            repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
            ..Config::default()
        };
        let mut registry = InMemoryRegistry::default();
        let mut task = Task::new(
            TaskId::new("web/fix-login"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            "/repo/web__worktrees/ajax-fix-login",
            "ajax-web-fix-login",
            "worktrunk",
            AgentClient::Codex,
        );
        task.lifecycle_status = LifecycleStatus::Reviewable;
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::WaitingForApproval,
            "waiting for review",
        ));
        task.git_status = Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: false,
            ahead: 3,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        });
        registry.create_task(task).unwrap();
        let context = CommandContext::new(config, registry);

        let detail = super::browser_task_detail_view(&context, "web/fix-login").unwrap();

        assert_eq!(detail.qualified_handle, "web/fix-login");
        assert_eq!(detail.title, "Fix login");
        assert_eq!(detail.branch, "ajax/fix-login");
        assert_eq!(detail.base_branch, "main");
        assert_eq!(detail.lifecycle, "Reviewable");
        assert_eq!(
            detail.live_status_summary.as_deref(),
            Some("waiting for review")
        );
        assert_eq!(
            detail.live_status_kind.as_deref(),
            Some("WaitingForApproval")
        );
        assert_eq!(detail.git.as_ref().map(|g| g.ahead), Some(3));
        assert!(detail.worktree_path.contains("ajax-fix-login"));
    }

    #[test]
    fn cockpit_slice_shapes_cards_for_the_mobile_pwa() {
        let card = TaskCard {
            id: TaskId::new("web/fix-login"),
            qualified_handle: "web/fix-login".to_string(),
            title: "Fix login".to_string(),
            ui_state: UiState::ReviewReady,
            status_label: "review ready".to_string(),
            lifecycle: LifecycleStatus::Reviewable,
            annotations: Vec::new(),
            primary_action: OperatorAction::Resume,
            available_actions: vec![
                OperatorAction::Start,
                OperatorAction::Resume,
                OperatorAction::Review,
                OperatorAction::Ship,
            ],
            live_summary: Some("waiting for review".to_string()),
        };

        let browser = browser_task_card(&card);

        assert_eq!(browser.qualified_handle, "web/fix-login");
        assert_eq!(browser.ui_state, "review ready");
        assert_eq!(browser.status_label, "review ready");
        assert_eq!(browser.lifecycle, "Reviewable");
        assert_eq!(browser.live_summary.as_deref(), Some("waiting for review"));
        assert_eq!(browser.primary_action, "review");
        assert_eq!(browser.available_actions, ["review", "ship"]);
    }
}

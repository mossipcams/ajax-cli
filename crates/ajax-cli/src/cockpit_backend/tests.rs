use super::{build_cockpit_snapshot, mobile_web_port_for_command, refresh_cockpit_snapshot};
use ajax_core::{
    adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec},
    agent_status::{
        ActivityKind, Confidence, ObservationSource, ProcessLiveness, StatusObservation,
    },
    commands::CommandContext,
    config::{Config, ManagedRepo},
    models::{
        AgentClient, AgentRuntimeStatus, GitStatus, LifecycleStatus, LiveObservation,
        LiveStatusKind, OperatorAction, RuntimeHealth, RuntimeObservationSource, SideFlag, Task,
        TaskId, TaskWindowStatus, TmuxStatus,
    },
    output::TaskCard,
    registry::{InMemoryRegistry, Registry},
    runtime_refresh::{refresh_runtime_context_with_tier, AgentStatusSource, RefreshTier},
};
use ajax_tui::CockpitSnapshot;

#[derive(Default)]
struct LiveRefreshRunner;

impl CommandRunner for LiveRefreshRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        let stdout = match command.args.as_slice() {
            [command, ..] if command == "list-sessions" => "ajax-web-fix-login\n",
            [_, repo, subcommand, action, flag]
                if repo == "/Users/matt/projects/web"
                    && subcommand == "worktree"
                    && action == "list"
                    && flag == "--porcelain" =>
            {
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n"
            }
            [_, repo, subcommand, format]
                if repo == "/Users/matt/projects/web"
                    && subcommand == "branch"
                    && format == "--format=%(refname:short)" =>
            {
                "main\najax/fix-login\n"
            }
            [command, ..] if command == "list-windows" => {
                "ajax-web-fix-login\ttask\t/tmp/worktrees/web-fix-login\n"
            }
            [command, ..] if command == "capture-pane" => {
                // No wait chrome — running reconcile must not invent Waiting.
                "agent working\nesc to interrupt\n"
            }
            _ => "",
        };

        Ok(CommandOutput {
            status_code: 0,
            stdout: stdout.to_string(),
            stderr: String::new(),
        })
    }
}

fn context_with_active_task() -> CommandContext<InMemoryRegistry> {
    let config = Config {
        repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
        ..Config::default()
    };
    let mut registry = InMemoryRegistry::default();
    let mut task = Task::new(
        TaskId::new("task-1"),
        "web",
        "fix-login",
        "Fix login",
        "ajax/fix-login",
        "main",
        "/tmp/worktrees/web-fix-login",
        "ajax-web-fix-login",
        "task",
        AgentClient::Codex,
    );
    task.lifecycle_status = LifecycleStatus::Active;
    task.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/fix-login".to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: None,
    });
    registry.create_task(task).unwrap();

    CommandContext::new(config, registry)
}

#[test]
fn mobile_web_ports_are_separate_for_stable_and_dev() {
    assert_eq!(mobile_web_port_for_command("stable"), 8787);
    assert_eq!(mobile_web_port_for_command("cockpit"), 8787);
    assert_eq!(mobile_web_port_for_command("dev"), 8788);
}

#[derive(Default)]
struct EmptyTmuxRunner;

impl CommandRunner for EmptyTmuxRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        let stdout = match command.args.as_slice() {
            [command, ..] if command == "list-sessions" => "",
            _ => "",
        };

        Ok(CommandOutput {
            status_code: 0,
            stdout: stdout.to_string(),
            stderr: String::new(),
        })
    }
}

fn context_with_cached_running_task() -> CommandContext<InMemoryRegistry> {
    let mut context = context_with_active_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.agent_status = AgentRuntimeStatus::Running;
    task.add_side_flag(SideFlag::AgentRunning);
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::AgentRunning,
        "working on task",
    ));
    task.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/fix-login".to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: None,
    });
    task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
    task.task_window_status = Some(TaskWindowStatus::present(
        "task",
        "/tmp/worktrees/web-fix-login",
    ));
    context
}

struct StaticAgentStatusSource {
    observations: Vec<StatusObservation>,
    liveness: Option<ProcessLiveness>,
}

impl StaticAgentStatusSource {
    fn lifecycle(kind: ActivityKind) -> Self {
        let now = std::time::SystemTime::now();
        Self {
            observations: vec![StatusObservation {
                source: ObservationSource::ProviderLifecycle,
                observed_at: now,
                expires_at: now + std::time::Duration::from_secs(1800),
                confidence: Confidence::High,
                run_id: "primary".to_string(),
                parent_run_id: None,
                kind,
            }],
            liveness: None,
        }
    }
}

impl AgentStatusSource for StaticAgentStatusSource {
    fn observations_for_task(&self, _task_id: &TaskId) -> Vec<StatusObservation> {
        self.observations.clone()
    }

    fn process_liveness_for_task(&self, _task_id: &TaskId) -> Option<ProcessLiveness> {
        self.liveness
    }
}

#[test]
fn live_refresh_updates_cached_annotations_for_cockpit_inbox() {
    let mut context = context_with_active_task();
    let mut runner = LiveRefreshRunner;
    let cache = StaticAgentStatusSource::lifecycle(ActivityKind::WaitingApproval);
    let mut state_changed = false;

    state_changed |=
        refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Full)
            .unwrap();
    let snapshot = build_cockpit_snapshot(&context);

    assert!(state_changed);
    assert_eq!(
        snapshot.cards[0].status_explanation.as_deref(),
        Some("Waiting for approval")
    );
    assert!(snapshot.inbox.items.iter().any(|item| {
        item.reason == "waiting_for_approval" && item.task_handle == "web/fix-login"
    }));
    assert!(context
        .registry
        .get_task(&TaskId::new("task-1"))
        .unwrap()
        .annotations
        .iter()
        .any(|annotation| annotation.evidence.label() == "waiting for approval"));
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_eq!(task.runtime_projection.health, RuntimeHealth::Healthy);
    assert_eq!(
        task.runtime_projection.source,
        RuntimeObservationSource::TmuxProbe
    );
}

#[test]
fn cockpit_refresh_uses_hook_backed_agent_status_cache() {
    let mut context = context_with_active_task();
    let mut runner = LiveRefreshRunner;
    let cache = StaticAgentStatusSource::lifecycle(ActivityKind::Working);
    let mut state_changed = false;

    state_changed |=
        refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Full)
            .unwrap();
    let snapshot = build_cockpit_snapshot(&context);

    assert!(state_changed);
    let card = snapshot
        .cards
        .iter()
        .find(|card| card.qualified_handle == "web/fix-login")
        .expect("task should stay visible in cockpit");
    assert_eq!(card.status, ajax_core::ui_state::TaskStatus::Running);
    assert_eq!(card.status_explanation.as_deref(), Some("Agent working"));
}

#[test]
fn live_refresh_clears_stale_input_when_hook_reports_working() {
    let mut context = context_with_active_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .expect("fixture task should exist");
    task.lifecycle_status = LifecycleStatus::Waiting;
    task.agent_status = AgentRuntimeStatus::Waiting;
    task.add_side_flag(SideFlag::NeedsInput);
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::WaitingForInput,
        "waiting for input",
    ));
    task.annotations = ajax_core::attention::annotate(task);
    let mut runner = LiveRefreshRunner;
    let cache = StaticAgentStatusSource::lifecycle(ActivityKind::Working);
    let mut state_changed = false;

    state_changed |=
        refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Full)
            .unwrap();
    let snapshot = build_cockpit_snapshot(&context);

    assert!(state_changed);
    let card = snapshot
        .cards
        .iter()
        .find(|card| card.qualified_handle == "web/fix-login")
        .expect("task should stay visible in cockpit");
    assert_eq!(card.status, ajax_core::ui_state::TaskStatus::Running);
    assert_eq!(card.status_explanation.as_deref(), Some("Agent working"));
    assert!(card.annotations.is_empty(), "{:?}", card.annotations);
    assert!(!snapshot
        .inbox
        .items
        .iter()
        .any(|item| item.task_handle == "web/fix-login"));

    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_eq!(task.agent_status, AgentRuntimeStatus::Running);
    assert!(!task.has_side_flag(SideFlag::NeedsInput));
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::AgentRunning)
    );
}

#[test]
fn live_refresh_marks_cached_running_task_invalid_when_tmux_sessions_are_empty() {
    let mut context = context_with_cached_running_task();
    let mut runner = EmptyTmuxRunner;
    let mut state_changed = false;

    let snapshot =
        refresh_cockpit_snapshot(&mut context, &mut runner, &mut state_changed, &mut None).unwrap();

    assert!(state_changed);
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(task.has_side_flag(SideFlag::TmuxMissing));
    assert!(!task.has_side_flag(SideFlag::AgentRunning));
    assert_eq!(task.agent_status, AgentRuntimeStatus::Dead);
    assert_eq!(
        task.tmux_status.as_ref().map(|status| status.exists),
        Some(false)
    );
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::TmuxMissing)
    );
    let card = snapshot
        .cards
        .iter()
        .find(|card| card.qualified_handle == "web/fix-login")
        .expect("invalid task should stay visible in cockpit");
    assert_eq!(card.primary_action, OperatorAction::Drop);
    assert_eq!(card.available_actions, vec![OperatorAction::Drop]);
    assert!(snapshot
        .inbox
        .items
        .iter()
        .any(|item| item.task_handle == "web/fix-login" && item.action == OperatorAction::Drop));
    assert!(ajax_core::commands::inbox(&context)
        .items
        .iter()
        .any(|item| {
            item.task_handle == "web/fix-login" && item.action == OperatorAction::Drop
        }));
}

#[test]
fn live_refresh_marks_cached_present_tmux_missing_even_after_fresh_command_result() {
    let mut context = context_with_active_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .expect("fixture task should exist");
    task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
    task.runtime_projection = ajax_core::models::RuntimeProjection::new(
        RuntimeHealth::Healthy,
        std::time::SystemTime::now(),
        RuntimeObservationSource::CommandResult,
    );
    let mut runner = EmptyTmuxRunner;

    let changed = super::refresh_live_context(&mut context, &mut runner).unwrap();
    let task = context
        .registry
        .get_task(&TaskId::new("task-1"))
        .expect("fixture task should remain registered");

    assert!(changed);
    assert!(task.has_side_flag(SideFlag::TmuxMissing));
    assert_eq!(
        task.tmux_status.as_ref().map(|status| status.exists),
        Some(false)
    );
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::TmuxMissing)
    );
}

#[derive(Default)]
struct SubstrateRecoveryRunner {
    commands: Vec<CommandSpec>,
}

impl CommandRunner for SubstrateRecoveryRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        self.commands.push(command.clone());
        let stdout = match command.args.as_slice() {
            [_, repo, subcommand, action, flag]
                if repo == "/Users/matt/projects/web"
                    && subcommand == "worktree"
                    && action == "list"
                    && flag == "--porcelain" =>
            {
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /Users/matt/projects/web__worktrees/ajax-code\nHEAD 2222222\nbranch refs/heads/ajax/code\n\n"
            }
            [command, ..] if command == "list-sessions" => {
                "ajax-web-existing\najax-web-code\n"
            }
            [command, ..] if command == "list-windows" => {
                "ajax-web-code\ttask\t/Users/matt/projects/web__worktrees/ajax-code\n"
            }
            [command, ..] if command == "capture-pane" => "codex is working\n",
            _ => "",
        };

        Ok(CommandOutput {
            status_code: 0,
            stdout: stdout.to_string(),
            stderr: String::new(),
        })
    }
}

#[test]
fn refresh_recovers_missing_registry_task_from_existing_ajax_worktree_and_tmux() {
    let config = Config {
        repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
        ..Config::default()
    };
    let mut registry = InMemoryRegistry::default();
    let mut existing = Task::new(
        TaskId::new("task-1"),
        "web",
        "existing",
        "existing",
        "ajax/existing",
        "main",
        "/Users/matt/projects/web__worktrees/ajax-existing",
        "ajax-web-existing",
        "task",
        AgentClient::Codex,
    );
    existing.lifecycle_status = LifecycleStatus::Active;
    registry.create_task(existing).unwrap();
    let mut context = CommandContext::new(config, registry);
    let mut runner = SubstrateRecoveryRunner::default();
    let mut state_changed = false;

    let snapshot =
        refresh_cockpit_snapshot(&mut context, &mut runner, &mut state_changed, &mut None).unwrap();

    assert!(state_changed);
    assert!(snapshot
        .cards
        .iter()
        .any(|card| card.qualified_handle == "web/code"));
    let task = context
        .registry
        .get_task(&TaskId::new("web/code"))
        .expect("missing Ajax worktree should be recovered into the registry");
    assert_eq!(task.branch, "ajax/code");
    assert_eq!(
        task.worktree_path.to_string_lossy(),
        "/Users/matt/projects/web__worktrees/ajax-code"
    );
    assert_eq!(task.tmux_session, "ajax-web-code");
    assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
}

#[derive(Default)]
struct OrphanWorktreeRecoveryRunner {
    commands: Vec<CommandSpec>,
}

impl CommandRunner for OrphanWorktreeRecoveryRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        self.commands.push(command.clone());
        let stdout = match command.args.as_slice() {
            [_, repo, subcommand, action, flag]
                if repo == "/Users/matt/projects/web"
                    && subcommand == "worktree"
                    && action == "list"
                    && flag == "--porcelain" =>
            {
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /Users/matt/projects/web__worktrees/ajax-orphan\nHEAD 2222222\nbranch refs/heads/ajax/orphan\n\n"
            }
            [command, ..] if command == "list-sessions" => "ajax-web-existing\n",
            [command, ..] if command == "list-windows" => "",
            _ => "",
        };

        Ok(CommandOutput {
            status_code: 0,
            stdout: stdout.to_string(),
            stderr: String::new(),
        })
    }
}

#[test]
fn refresh_recovers_missing_registry_task_from_orphaned_ajax_worktree_without_tmux() {
    let config = Config {
        repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
        ..Config::default()
    };
    let mut registry = InMemoryRegistry::default();
    let mut existing = Task::new(
        TaskId::new("task-1"),
        "web",
        "existing",
        "existing",
        "ajax/existing",
        "main",
        "/Users/matt/projects/web__worktrees/ajax-existing",
        "ajax-web-existing",
        "task",
        AgentClient::Codex,
    );
    existing.lifecycle_status = LifecycleStatus::Active;
    registry.create_task(existing).unwrap();
    let mut context = CommandContext::new(config, registry);
    let mut runner = OrphanWorktreeRecoveryRunner::default();

    let changed = super::refresh_live_context(&mut context, &mut runner).unwrap();

    assert!(changed);
    let task = context
        .registry
        .get_task(&TaskId::new("web/orphan"))
        .expect("orphaned Ajax worktree should be recovered into the registry");
    assert_eq!(task.branch, "ajax/orphan");
    assert_eq!(
        task.worktree_path.to_string_lossy(),
        "/Users/matt/projects/web__worktrees/ajax-orphan"
    );
    assert!(task
        .git_status
        .as_ref()
        .is_some_and(|status| status.worktree_exists && status.branch_exists));
    assert_eq!(
        task.tmux_status.as_ref().map(|status| status.exists),
        Some(false)
    );
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::TmuxMissing)
    );
    assert!(task.has_side_flag(SideFlag::TmuxMissing));
}

#[derive(Default)]
struct CountingLiveRefreshRunner {
    commands: Vec<CommandSpec>,
}

impl CommandRunner for CountingLiveRefreshRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        self.commands.push(command.clone());
        let stdout = match command.args.as_slice() {
            [command, ..] if command == "list-sessions" => "ajax-web-fix-login\n",
            [command, ..] if command == "list-windows" => "task\t/tmp/worktrees/web-fix-login\n",
            [command, ..] if command == "capture-pane" => "codex is working\n",
            _ => "",
        };

        Ok(CommandOutput {
            status_code: 0,
            stdout: stdout.to_string(),
            stderr: String::new(),
        })
    }
}

#[test]
fn live_refresh_skips_window_and_pane_probes_for_non_live_tasks() {
    let mut context = context_with_active_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .expect("fixture task should exist");
    task.lifecycle_status = LifecycleStatus::Cleanable;
    task.tmux_status = Some(ajax_core::models::TmuxStatus::present(
        task.tmux_session.clone(),
    ));
    task.task_window_status = Some(ajax_core::models::TaskWindowStatus {
        exists: true,
        window_name: task.task_window.clone(),
        current_path: task.worktree_path.clone(),
        points_at_expected_path: true,
    });

    let mut runner = CountingLiveRefreshRunner::default();

    let changed = super::refresh_live_context(&mut context, &mut runner).unwrap();

    assert!(!changed);
    assert!(!runner.commands.iter().any(
        |command| matches!(command.args.as_slice(), [command, ..] if command == "list-sessions")
    ));
    assert!(!runner.commands.iter().any(
        |command| matches!(command.args.as_slice(), [command, ..] if command == "list-windows")
    ));
    assert!(!runner.commands.iter().any(
        |command| matches!(command.args.as_slice(), [command, ..] if command == "capture-pane")
    ));
}

#[test]
fn cockpit_snapshot_rebuilds_after_cached_task_is_removed() {
    let mut context = context_with_active_task();
    let initial_snapshot = build_cockpit_snapshot(&context);
    assert_eq!(initial_snapshot.cards.len(), 1);
    assert_eq!(initial_snapshot.cards[0].qualified_handle, "web/fix-login");

    let mut cached_snapshot = Some(initial_snapshot);
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .expect("fixture task should exist");
    task.lifecycle_status = LifecycleStatus::Removed;
    // Fully dropped ghosts have no remaining git substrate; Removed rows that
    // still report a worktree/branch stay visible so Drop can finish teardown.
    if let Some(git_status) = task.git_status.as_mut() {
        git_status.worktree_exists = false;
        git_status.branch_exists = false;
        git_status.current_branch = None;
    }

    let mut runner = EmptyTmuxRunner;
    let mut state_changed = false;
    let snapshot = refresh_cockpit_snapshot(
        &mut context,
        &mut runner,
        &mut state_changed,
        &mut cached_snapshot,
    )
    .unwrap();

    assert!(snapshot.cards.is_empty());
    assert!(cached_snapshot.as_ref().unwrap().cards.is_empty());
    assert!(snapshot
        .repos
        .repos
        .iter()
        .all(|repo| repo.active_tasks == 0));
    assert!(snapshot.inbox.items.is_empty());
}

#[test]
fn cockpit_snapshot_reuses_cache_when_visible_tasks_are_unchanged() {
    let mut context = context_with_active_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .expect("fixture task should exist");
    task.lifecycle_status = LifecycleStatus::Cleanable;
    task.tmux_status = Some(TmuxStatus::present(task.tmux_session.clone()));
    task.task_window_status = Some(TaskWindowStatus {
        exists: true,
        window_name: task.task_window.clone(),
        current_path: task.worktree_path.clone(),
        points_at_expected_path: true,
    });
    let fresh_snapshot = build_cockpit_snapshot(&context);
    let mut cached_snapshot = Some(CockpitSnapshot {
        repos: fresh_snapshot.repos,
        cards: vec![TaskCard {
            status_explanation: Some("cached-only summary".to_string()),
            ..fresh_snapshot.cards[0].clone()
        }],
        inbox: fresh_snapshot.inbox,
    });
    let mut runner = EmptyTmuxRunner;
    let mut state_changed = false;

    let snapshot = refresh_cockpit_snapshot(
        &mut context,
        &mut runner,
        &mut state_changed,
        &mut cached_snapshot,
    )
    .unwrap();

    assert!(!state_changed);
    assert_eq!(
        snapshot.cards[0].status_explanation.as_deref(),
        Some("cached-only summary")
    );
    assert_eq!(
        cached_snapshot.as_ref().unwrap().cards[0]
            .status_explanation
            .as_deref(),
        Some("cached-only summary")
    );
}

#[test]
fn live_refresh_does_not_probe_generic_error_task_without_live_attention() {
    let mut context = context_with_active_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .expect("fixture task should exist");
    task.lifecycle_status = LifecycleStatus::Error;
    task.agent_status = AgentRuntimeStatus::Blocked;
    task.tmux_status = Some(ajax_core::models::TmuxStatus::present(
        task.tmux_session.clone(),
    ));
    task.task_window_status = Some(ajax_core::models::TaskWindowStatus {
        exists: true,
        window_name: task.task_window.clone(),
        current_path: task.worktree_path.clone(),
        points_at_expected_path: true,
    });

    let mut runner = CountingLiveRefreshRunner::default();

    let changed = super::refresh_live_context(&mut context, &mut runner).unwrap();

    assert!(!changed);
    assert!(!runner.commands.iter().any(
        |command| matches!(command.args.as_slice(), [command, ..] if command == "list-sessions")
    ));
    assert!(!runner.commands.iter().any(
        |command| matches!(command.args.as_slice(), [command, ..] if command == "list-windows")
    ));
    assert!(!runner.commands.iter().any(
        |command| matches!(command.args.as_slice(), [command, ..] if command == "capture-pane")
    ));
}

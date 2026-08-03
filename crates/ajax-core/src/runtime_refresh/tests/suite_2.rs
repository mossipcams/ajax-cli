use super::super::*;
use super::*;

#[test]
fn pane_wait_observed_only_when_no_lifecycle_evidence() {
    let mut context = context_with_active_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new(TASK_ID))
        .unwrap();
    task.selected_agent = AgentClient::Other;
    let mut runner = PermissionMenuRunner::default();
    let cache = ObsSource::new(vec![]).with_liveness(ProcessLiveness {
        alive: true,
        observed_at: SystemTime::now(),
    });

    refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Full)
        .unwrap();

    let capture_panes = runner
        .commands
        .iter()
        .filter(
            |command| matches!(command.args.as_slice(), [command, ..] if command == "capture-pane"),
        )
        .count();
    assert_eq!(
        capture_panes, 1,
        "expected exactly one capture-pane, got {:?}",
        runner.commands
    );
    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::WaitingForApproval)
    );
}

#[test]
fn running_lifecycle_reconciles_to_waiting_on_permission_pane() {
    let mut context = context_with_unchanged_running_task();
    let mut runner = PermissionMenuRunner::default();
    let cache = ObsSource::new(vec![lifecycle_obs(ActivityKind::Working, 1, 120)]);

    refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Full)
        .unwrap();

    let capture_panes = runner
        .commands
        .iter()
        .filter(
            |command| matches!(command.args.as_slice(), [command, ..] if command == "capture-pane"),
        )
        .count();
    assert_eq!(capture_panes, 1);
    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::WaitingForApproval)
    );
}

const CURSOR_PERMISSION_MENU: &str =
    "Run this command?\n\n> Allow this command\n  Deny\n\nenter to select · esc to cancel";

#[derive(Default)]
struct CursorPermissionMenuRunner {
    commands: Vec<CommandSpec>,
}

impl CommandRunner for CursorPermissionMenuRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        self.commands.push(command.clone());
        let stdout = match command.args.as_slice() {
            [command, ..] if command == "capture-pane" => CURSOR_PERMISSION_MENU,
            _ => runtime_stdout(&command.args),
        };

        Ok(CommandOutput {
            status_code: 0,
            stdout: stdout.to_string(),
            stderr: String::new(),
        })
    }
}

#[test]
fn cursor_running_reconciles_on_cursor_permission_chrome() {
    let mut context = context_with_unchanged_running_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new(TASK_ID))
        .unwrap();
    task.selected_agent = AgentClient::Cursor;
    let mut runner = CursorPermissionMenuRunner::default();
    let cache = ObsSource::new(vec![lifecycle_obs(ActivityKind::Working, 1, 120)]);

    refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Full)
        .unwrap();

    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::WaitingForApproval)
    );
}

#[test]
fn ack_blocked_pane_wait_does_not_fall_through_to_agent_running() {
    let mut context = context_with_unchanged_running_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new(TASK_ID))
        .unwrap();
    task.selected_agent = AgentClient::Claude;
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::WaitingForApproval,
        "waiting for approval",
    ));
    task.live_status_observed_at = Some(SystemTime::now() - Duration::from_secs(30));
    // Future ack makes pane Waiting blocked_by_ack (`now <= ack`).
    task.record_attention_acknowledgment(SystemTime::now() + Duration::from_secs(3600));
    let mut runner = PermissionMenuRunner::default();
    let cache = ObsSource::new(vec![lifecycle_obs(ActivityKind::Working, 1, 120)]);

    refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Full)
        .unwrap();

    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::WaitingForApproval),
        "ack-blocked pane wait chrome must not fall through to AgentRunning"
    );
}

#[test]
fn claude_running_reconciles_despite_native_wait_capability() {
    let mut context = context_with_unchanged_running_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new(TASK_ID))
        .unwrap();
    task.selected_agent = AgentClient::Claude;
    let mut runner = PermissionMenuRunner::default();
    let cache = ObsSource::new(vec![lifecycle_obs(ActivityKind::Working, 1, 120)]);

    refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Full)
        .unwrap();

    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::WaitingForApproval)
    );
}

#[test]
fn fully_completed_claude_idle_reconciles_to_waiting_on_permission_pane() {
    let mut context = context_with_unchanged_running_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new(TASK_ID))
        .unwrap();
    task.selected_agent = AgentClient::Claude;
    let mut runner = PermissionMenuRunner::default();
    let cache = ObsSource::new(vec![lifecycle_obs(ActivityKind::Done, 1, 120)]);

    refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Full)
        .unwrap();

    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::WaitingForApproval)
    );
}

#[test]
fn fully_completed_codex_idle_reconciles_to_waiting_on_permission_pane() {
    let mut context = context_with_unchanged_running_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new(TASK_ID))
        .unwrap();
    task.selected_agent = AgentClient::Codex;
    let mut runner = PermissionMenuRunner::default();
    let cache = ObsSource::new(vec![lifecycle_obs(ActivityKind::Done, 1, 120)]);

    refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Full)
        .unwrap();

    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::WaitingForApproval)
    );
}

#[test]
fn fully_completed_cursor_idle_reconciles_to_waiting_on_permission_pane() {
    let mut context = context_with_unchanged_running_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new(TASK_ID))
        .unwrap();
    task.selected_agent = AgentClient::Cursor;
    let mut runner = CursorPermissionMenuRunner::default();
    let cache = ObsSource::new(vec![lifecycle_obs(ActivityKind::Done, 1, 120)]);

    refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Full)
        .unwrap();

    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::WaitingForApproval)
    );
}

const PI_PARKED_INPUT_PANE: &str = "complete\npi>";

#[derive(Default)]
struct PiParkedInputRunner {
    commands: Vec<CommandSpec>,
}

impl CommandRunner for PiParkedInputRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        self.commands.push(command.clone());
        let stdout = match command.args.as_slice() {
            [command, ..] if command == "capture-pane" => PI_PARKED_INPUT_PANE,
            _ => runtime_stdout(&command.args),
        };

        Ok(CommandOutput {
            status_code: 0,
            stdout: stdout.to_string(),
            stderr: String::new(),
        })
    }
}

#[test]
fn pi_running_reconciles_to_waiting_on_parked_input_pane() {
    let mut context = context_with_unchanged_running_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new(TASK_ID))
        .unwrap();
    task.selected_agent = AgentClient::Pi;
    let mut runner = PiParkedInputRunner::default();
    let cache = ObsSource::new(vec![lifecycle_obs(ActivityKind::Working, 1, 120)]);

    refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Full)
        .unwrap();

    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::WaitingForInput)
    );
}

#[test]
fn steady_state_refresh_skips_capture_pane_when_agent_cache_is_stable() {
    let mut context = context_with_unchanged_running_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new(TASK_ID))
        .unwrap();
    task.live_status = Some(LiveObservation::new(LiveStatusKind::Done, "done"));
    task.agent_status = AgentRuntimeStatus::Done;
    task.remove_side_flag(SideFlag::AgentRunning);
    let mut runner = GitSkippingRunner::default();
    let cache = ObsSource::new(vec![lifecycle_obs(ActivityKind::Done, 1, 120)]);

    let _changed =
        refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Full)
            .unwrap();

    assert!(
        !runner.commands.iter().any(|command| {
            matches!(command.args.as_slice(), [command, ..] if command == "capture-pane")
        }),
        "stable FullyCompleted task with no reconcile gate should skip capture-pane"
    );
}

#[test]
fn claude_unknown_phase_preserves_prior_waiting_live_status() {
    let mut context = context_with_unchanged_running_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new(TASK_ID))
        .unwrap();
    task.selected_agent = AgentClient::Claude;
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::WaitingForApproval,
        "waiting for approval",
    ));
    task.agent_status = AgentRuntimeStatus::Waiting;
    task.add_side_flag(SideFlag::NeedsInput);
    task.remove_side_flag(SideFlag::AgentRunning);
    let mut runner = PermissionMenuRunner::default();
    // Liveness without lifecycle observations → reducer Unknown. Claude has no
    // capability-gated unknown fallback; must not apply Unknown / clear waiting.
    let cache = ObsSource::new(vec![]).with_liveness(ProcessLiveness {
        alive: true,
        observed_at: SystemTime::now(),
    });

    refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Full)
        .unwrap();

    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::WaitingForApproval),
        "Unknown must not clear prior waiting live status for Claude"
    );
    assert!(task.has_side_flag(SideFlag::NeedsInput));
    assert_eq!(
        runner
            .commands
            .iter()
            .filter(|command| {
                matches!(command.args.as_slice(), [command, ..] if command == "capture-pane")
            })
            .count(),
        0,
        "Claude Unknown without fallback must not capture-pane"
    );
}

#[test]
fn claude_done_fully_completed_does_not_idle_reconcile_on_permission_pane() {
    let mut context = context_with_unchanged_running_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new(TASK_ID))
        .unwrap();
    task.selected_agent = AgentClient::Claude;
    task.live_status = Some(LiveObservation::new(LiveStatusKind::Done, "done"));
    task.agent_status = AgentRuntimeStatus::Done;
    task.remove_side_flag(SideFlag::AgentRunning);
    let mut runner = PermissionMenuRunner::default();
    let cache = ObsSource::new(vec![lifecycle_obs(ActivityKind::Done, 1, 120)]);

    refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Full)
        .unwrap();

    let capture_panes = runner
        .commands
        .iter()
        .filter(
            |command| matches!(command.args.as_slice(), [command, ..] if command == "capture-pane"),
        )
        .count();
    assert_eq!(
        capture_panes, 0,
        "Done (soft Waiting-class) must not open idle reconcile"
    );
    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::Done)
    );
}

#[test]
fn steady_state_refresh_skips_global_list_windows_when_no_probed_session_exists() {
    let mut context = context_with_active_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new(TASK_ID))
        .unwrap();
    task.tmux_session = "ajax-web-missing-session".to_string();
    let mut runner = MissingSessionRunner::default();

    let _changed = refresh_runtime_context(&mut context, &mut runner).unwrap();

    assert!(
        !runner.commands.iter().any(|command| {
            matches!(command.args.as_slice(), [command, ..] if command == "list-windows")
        }),
        "missing probed session should not list all windows: {:?}",
        runner.commands
    );
}

#[test]
fn steady_state_refresh_operation_budget() {
    let mut context = context_with_unchanged_running_task();
    let mut runner = GitSkippingRunner::default();
    let cache = ObsSource::new(vec![lifecycle_obs(ActivityKind::Working, 1, 120)]);

    let _changed =
        refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Live)
            .unwrap();

    let git_worktree_lists = runner
        .commands
        .iter()
        .filter(|command| git_worktree_list(&command.args))
        .count();
    let capture_panes = runner
        .commands
        .iter()
        .filter(
            |command| matches!(command.args.as_slice(), [command, ..] if command == "capture-pane"),
        )
        .count();
    let tmux_commands = runner
        .commands
        .iter()
        .filter(|command| {
            matches!(
                command.args.first().map(String::as_str),
                Some("list-sessions" | "list-windows")
            )
        })
        .count();

    assert_eq!(git_worktree_lists, 0);
    // Working Codex opens the running-reconcile capture gate once.
    assert_eq!(capture_panes, 1);
    assert!(
        tmux_commands <= 2,
        "expected at most list-sessions + list-windows, got {tmux_commands}"
    );
}

#[test]
fn steady_state_refresh_skips_branch_refresh_when_git_state_is_fresh() {
    let mut context = context_with_unchanged_running_task();
    seed_fresh_ci_probe(&mut context);
    let mut runner = GitSkippingRunner::default();

    let changed = refresh_runtime_context(&mut context, &mut runner).unwrap();

    assert!(!changed);
    assert!(
        !runner
            .commands
            .iter()
            .any(|command| git_branch_list(&command.args)),
        "fresh runtime refresh should not list repo branches: {:?}",
        runner.commands
    );
}

#[test]
fn missing_git_status_with_missing_flags_still_refreshes_git_substrate() {
    let mut context = context_with_active_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new(TASK_ID))
        .unwrap();
    task.git_status = None;
    task.add_side_flag(SideFlag::WorktreeMissing);
    task.add_side_flag(SideFlag::BranchMissing);
    let mut runner = HealthyRefreshRunner::default();

    let changed = refresh_runtime_context(&mut context, &mut runner).unwrap();

    assert!(changed);
    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert_eq!(task.git_status.as_ref(), Some(&clean_git_status()));
    assert!(!task.has_side_flag(SideFlag::WorktreeMissing));
    assert!(!task.has_side_flag(SideFlag::BranchMissing));
    assert!(runner
        .commands
        .iter()
        .any(|command| git_worktree_list(&command.args)));
    assert!(runner
        .commands
        .iter()
        .any(|command| git_branch_list(&command.args)));
}

#[test]
fn tmux_probe_failure_preserves_session_and_records_probe_error() {
    struct FailingTmuxRunner {
        inner: MissingSessionRunner,
    }

    impl CommandRunner for FailingTmuxRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            if command
                .args
                .first()
                .is_some_and(|arg| arg == "list-sessions")
            {
                return Err(CommandRunError::SpawnFailed("tmux unavailable".to_string()));
            }
            self.inner.run(command)
        }
    }

    let mut context = context_with_task_for_missing_session();
    let mut runner = FailingTmuxRunner {
        inner: MissingSessionRunner::default(),
    };

    let changed = refresh_runtime_context(&mut context, &mut runner).unwrap();

    assert!(changed);
    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert!(task
        .tmux_status
        .as_ref()
        .is_some_and(|status| status.exists));
    assert!(!task.has_side_flag(SideFlag::TmuxMissing));
    assert_eq!(
        task.runtime_projection.observation_error.as_deref(),
        Some("tmux list-sessions probe failed: failed to start command: tmux unavailable")
    );
}

#[test]
fn window_probe_failure_preserves_task_window_evidence() {
    struct FailingWindowRunner;

    impl CommandRunner for FailingWindowRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            match command.args.first().map(String::as_str) {
                Some("list-sessions") => Ok(CommandOutput {
                    status_code: 0,
                    stdout: format!("{TASK_SESSION}\n"),
                    stderr: String::new(),
                }),
                Some("list-windows") => Err(CommandRunError::SpawnFailed(
                    "tmux windows unavailable".to_string(),
                )),
                _ => Ok(CommandOutput {
                    status_code: 0,
                    stdout: runtime_stdout(&command.args).to_string(),
                    stderr: String::new(),
                }),
            }
        }
    }

    let mut context = context_with_task_for_missing_session();
    let mut runner = FailingWindowRunner;

    let changed = refresh_runtime_context(&mut context, &mut runner).unwrap();

    assert!(changed);
    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert!(task
        .task_window_status
        .as_ref()
        .is_some_and(|status| status.exists && status.points_at_expected_path));
    assert!(!task.has_side_flag(SideFlag::TaskWindowMissing));
    assert_eq!(
        task.runtime_projection.observation_error.as_deref(),
        Some("tmux list-windows probe failed: failed to start command: tmux windows unavailable")
    );
}

#[test]
fn orphan_recovery_deletes_stale_same_worktree_task_before_insert() {
    let config = Config {
        repos: vec![ManagedRepo::new(REPO_NAME, REPO_PATH, BASE_BRANCH)],
        ..Config::default()
    };
    let mut registry = InMemoryRegistry::default();
    let mut stale = Task::new(
        TaskId::new("web/stale-task"),
        REPO_NAME,
        "stale-task",
        "Stale task",
        "ajax/stale-task",
        BASE_BRANCH,
        TASK_WORKTREE,
        "ajax-web-stale-task",
        TASK_WINDOW,
        AgentClient::Codex,
    );
    stale.lifecycle_status = LifecycleStatus::Active;
    stale.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/stale-task".to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: None,
    });
    registry.create_task(stale).unwrap();
    let mut context = CommandContext::new(config, registry);
    let mut runner = OrphanRecoveryRunner::default();

    let changed = refresh_runtime_context(&mut context, &mut runner).unwrap();

    assert!(changed);
    assert!(context
        .registry
        .get_task(&TaskId::new("web/stale-task"))
        .is_none());
    assert!(context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .is_some());
}

#[test]
fn branch_rename_preserves_live_session_for_same_worktree() {
    struct RenamedBranchRunner;

    impl CommandRunner for RenamedBranchRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            let stdout = match command.args.as_slice() {
                [command, ..] if command == "list-sessions" => "ajax-web-stale-task\n",
                [command, ..] if command == "list-windows" => {
                    "ajax-web-stale-task\ttask\t/tmp/worktrees/web-fix-login\n"
                }
                [_, repo, subcommand, action, flag]
                    if repo == REPO_PATH
                        && subcommand == "worktree"
                        && action == "list"
                        && flag == "--porcelain" =>
                {
                    "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/renamed-task\n\n"
                }
                [_, repo, subcommand, format]
                    if repo == REPO_PATH
                        && subcommand == "branch"
                        && format == "--format=%(refname:short)" =>
                {
                    "main\najax/renamed-task\n"
                }
                [command, ..] if command == "capture-pane" => {
                    "› Continue implementation\n\n  gpt-5.5 high · ~/repo\n"
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

    let config = Config {
        repos: vec![ManagedRepo::new(REPO_NAME, REPO_PATH, BASE_BRANCH)],
        ..Config::default()
    };
    let mut registry = InMemoryRegistry::default();
    let mut stale = Task::new(
        TaskId::new("web/stale-task"),
        REPO_NAME,
        "stale-task",
        "Stale task",
        "ajax/stale-task",
        BASE_BRANCH,
        TASK_WORKTREE,
        "ajax-web-stale-task",
        TASK_WINDOW,
        AgentClient::Codex,
    );
    stale.lifecycle_status = LifecycleStatus::Active;
    registry.create_task(stale).unwrap();
    let mut context = CommandContext::new(config, registry);
    let mut runner = RenamedBranchRunner;

    let changed = refresh_runtime_context(&mut context, &mut runner).unwrap();

    assert!(changed);
    assert!(context
        .registry
        .get_task(&TaskId::new("web/stale-task"))
        .is_none());
    let renamed = context
        .registry
        .get_task(&TaskId::new("web/renamed-task"))
        .unwrap();
    assert_eq!(renamed.tmux_session, "ajax-web-stale-task");
    assert_eq!(
        renamed.tmux_status.as_ref().map(|status| status.exists),
        Some(true)
    );
    assert!(!renamed.has_side_flag(SideFlag::TmuxMissing));
}

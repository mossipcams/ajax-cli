use super::super::*;
use super::*;

#[test]
fn orphan_recovery_adopts_session_whose_window_points_at_worktree() {
    struct MatchingWindowRunner;

    impl CommandRunner for MatchingWindowRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            let stdout = match command.args.as_slice() {
                [command, ..] if command == "list-sessions" => "ajax-web-old-name\n",
                [command, ..] if command == "list-panes" => {
                    "ajax-web-old-name\ttask\t/tmp/worktrees/web-fix-login\n"
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
    let mut renamed = Task::new(
        TaskId::new("web/renamed-task"),
        REPO_NAME,
        "renamed-task",
        "Renamed task",
        "ajax/renamed-task",
        BASE_BRANCH,
        TASK_WORKTREE,
        "ajax-web-renamed-task",
        TASK_WINDOW,
        AgentClient::Codex,
    );
    renamed.lifecycle_status = LifecycleStatus::Active;
    registry.create_task(renamed).unwrap();
    let mut context = CommandContext::new(config, registry);
    let mut runner = MatchingWindowRunner;

    let changed = refresh_runtime_context(&mut context, &mut runner).unwrap();

    assert!(changed);
    let renamed = context
        .registry
        .get_task(&TaskId::new("web/renamed-task"))
        .unwrap();
    assert_eq!(renamed.tmux_session, "ajax-web-old-name");
    assert_eq!(
        renamed.tmux_status.as_ref().map(|status| status.exists),
        Some(true)
    );
    assert!(!renamed.has_side_flag(SideFlag::TmuxMissing));
}

fn context_with_many_active_tasks(count: usize) -> CommandContext<InMemoryRegistry> {
    let config = Config {
        repos: vec![
            ManagedRepo::new(REPO_NAME, REPO_PATH, BASE_BRANCH),
            ManagedRepo::new("api", "/Users/matt/projects/api", BASE_BRANCH),
        ],
        ..Config::default()
    };
    let mut registry = InMemoryRegistry::default();
    for index in 0..count {
        let repo = if index % 2 == 0 { REPO_NAME } else { "api" };
        let handle = format!("task-{index}");
        let branch = format!("ajax/{handle}");
        let session = format!("ajax-{repo}-{handle}");
        let worktree = format!("/tmp/worktrees/{repo}-{handle}");
        let mut task = Task::new(
            TaskId::new(format!("{repo}/{handle}")),
            repo,
            &handle,
            format!("Task {index}"),
            &branch,
            BASE_BRANCH,
            &worktree,
            &session,
            TASK_WINDOW,
            AgentClient::Codex,
        );
        task.lifecycle_status = LifecycleStatus::Active;
        task.git_status = Some(clean_git_status());
        task.tmux_status = Some(TmuxStatus::present(&session));
        task.task_window_status = Some(TaskWindowStatus::present(TASK_WINDOW, &worktree));
        registry.create_task(task).unwrap();
    }
    CommandContext::new(config, registry)
}

#[test]
fn live_refresh_many_active_tasks_use_bounded_tmux_commands() {
    let mut context = context_with_many_active_tasks(24);
    let mut runner = GitSkippingRunner::default();
    let cache = ObsSource::new(vec![lifecycle_obs(ActivityKind::Working, 1, 120)]);

    refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Live)
        .unwrap();

    let list_sessions = runner
        .commands
        .iter()
        .filter(|command| command.args.first().map(String::as_str) == Some("list-sessions"))
        .count();
    let list_all_windows = runner
        .commands
        .iter()
        .filter(|command| {
            command.args.first().map(String::as_str) == Some("list-windows")
                && command.args.contains(&"-a".to_string())
        })
        .count();

    assert_eq!(list_sessions, 1);
    assert!(list_all_windows <= 1);
}

#[test]
fn hyphenated_repo_registered_session_does_not_trigger_orphan_recovery() {
    let config = Config {
        repos: vec![ManagedRepo::new("api-v2", "/repo/api-v2", BASE_BRANCH)],
        ..Config::default()
    };
    let mut registry = InMemoryRegistry::default();
    let mut task = Task::new(
        TaskId::new("api-v2/fix-login"),
        "api-v2",
        "fix-login",
        "Fix login",
        "ajax/fix-login",
        BASE_BRANCH,
        "/repo/api-v2__worktrees/ajax-fix-login",
        "ajax-api-v2-fix-login",
        TASK_WINDOW,
        AgentClient::Codex,
    );
    task.lifecycle_status = LifecycleStatus::Active;
    task.git_status = Some(clean_git_status());
    task.runtime_projection = RuntimeProjection::new(
        RuntimeHealth::Healthy,
        SystemTime::now(),
        RuntimeObservationSource::TmuxProbe,
    );
    registry.create_task(task).unwrap();
    let mut context = CommandContext::new(config, registry);
    let mut runner = OrphanRecoveryRunner {
        sessions_output: Some("ajax-api-v2-fix-login\n".to_string()),
        ..Default::default()
    };

    refresh_runtime_context_with_tier(
        &mut context,
        &mut runner,
        &NoAgentStatusSource,
        RefreshTier::Live,
    )
    .unwrap();

    assert_eq!(context.registry.list_tasks().len(), 1);
    assert!(
        !runner
            .commands
            .iter()
            .any(|command| git_worktree_list(&command.args)),
        "registered hyphenated sessions must not trigger orphan git discovery"
    );
}

#[test]
fn exact_registered_session_names_gate_orphan_recovery() {
    let base = context_with_unchanged_running_task();
    let mut context = CommandContext::new(base.config, base.registry);
    let mut runner = OrphanRecoveryRunner {
        sessions_output: Some("ajax-web-fix-login\najax-web-a\n".to_string()),
        ..Default::default()
    };

    let changed = refresh_runtime_context_with_tier(
        &mut context,
        &mut runner,
        &NoAgentStatusSource,
        RefreshTier::Live,
    )
    .unwrap();

    assert!(changed);
    assert!(context.registry.get_task(&TaskId::new("web/a")).is_some());
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new(TASK_ID))
            .expect("registered session should remain")
            .tmux_session,
        TASK_SESSION
    );
}

#[test]
fn rooted_runtime_recovery_ignores_legacy_sibling_worktrees() {
    let base = context_with_unchanged_running_task();
    let runtime_paths = RuntimePathRequest::new("/Users/matt")
        .with_cli_profile("dev")
        .resolve();
    let mut context = CommandContext::with_runtime_paths(
        base.config,
        CountingRegistry::from_registry(base.registry),
        runtime_paths,
    );
    let mut runner = OrphanRecoveryRunner::default();

    let _changed = refresh_runtime_context(&mut context, &mut runner).unwrap();

    assert!(context.registry.get_task(&TaskId::new("web/a")).is_none());
    assert!(context.registry.get_task(&TaskId::new("web/b")).is_none());
    assert!(context.registry.get_task(&TaskId::new("web/c")).is_none());
}

/// A task provisioned with `AgentRunning` whose agent then went away without
/// leaving a single trace — no native observation, no wrapper liveness, no
/// alive-process marker — must not keep reporting "Agent working". Every
/// source that could retract the claim is silent, so the refresh itself has to
/// retract it or the operator surfaces lie indefinitely.
#[test]
fn exited_agent_is_not_projected_as_running_after_refresh() {
    let mut context = context_with_active_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new(TASK_ID))
        .unwrap();
    task.add_side_flag(SideFlag::AgentRunning);
    task.agent_status = AgentRuntimeStatus::NotStarted;
    task.live_status = None;
    task.tmux_status = Some(TmuxStatus::present(TASK_SESSION));
    task.task_window_status = Some(TaskWindowStatus::present(TASK_WINDOW, TASK_WORKTREE));
    task.runtime_projection = RuntimeProjection::new(
        RuntimeHealth::Healthy,
        SystemTime::now(),
        RuntimeObservationSource::TmuxProbe,
    );
    assert_eq!(
        derive_operator_status(context.registry.get_task(&TaskId::new(TASK_ID)).unwrap())
            .explanation
            .as_deref(),
        Some("Agent working"),
        "precondition: the stale flag alone drives the running status"
    );
    let mut runner = HealthyRefreshRunner::default();

    // NoAgentStatusSource: no observations and no process liveness, matching a
    // pane whose agent exited long ago.
    refresh_runtime_context_with_tier(
        &mut context,
        &mut runner,
        &NoAgentStatusSource,
        RefreshTier::Full,
    )
    .unwrap();

    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert!(
        !task.has_side_flag(SideFlag::AgentRunning),
        "stale running flag must be retracted, got {:?}",
        task.side_flags().collect::<Vec<_>>()
    );
    assert_ne!(
        derive_operator_status(task).status,
        TaskStatus::Running,
        "operator status must stop claiming the agent is working"
    );
}

/// The retraction above must not fire while the wrapper still reports the
/// process alive: that is real evidence, and clearing on it would flap a
/// live-but-quiet agent back to idle every refresh.
#[test]
fn live_process_evidence_preserves_running_projection() {
    let mut context = context_with_active_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new(TASK_ID))
        .unwrap();
    task.add_side_flag(SideFlag::AgentRunning);
    task.tmux_status = Some(TmuxStatus::present(TASK_SESSION));
    task.task_window_status = Some(TaskWindowStatus::present(TASK_WINDOW, TASK_WORKTREE));
    let cache = ObsSource::new(vec![]).with_liveness(ProcessLiveness {
        alive: true,
        observed_at: SystemTime::now(),
    });
    let mut runner = HealthyRefreshRunner::default();

    refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Full)
        .unwrap();

    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert!(
        task.has_side_flag(SideFlag::AgentRunning),
        "a live process is real evidence; the claim must stand"
    );
    assert!(
        agent_process_is_alive(task),
        "fresh liveness must stamp the alive marker"
    );
}

/// A live status is a newer observation than provisioning, so the retraction
/// must defer to the live-status machinery that owns it. Without this the
/// steady-state running task flaps to idle on every hook-silent refresh.
#[test]
fn fresh_agent_status_preserves_running_projection() {
    let mut context = context_with_unchanged_running_task();
    seed_fresh_ci_probe(&mut context);
    let mut runner = GitSkippingRunner::default();

    refresh_runtime_context(&mut context, &mut runner).unwrap();

    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert!(
        task.has_side_flag(SideFlag::AgentRunning),
        "live status corroborates the claim; it must stand"
    );
    assert_eq!(task.agent_status, AgentRuntimeStatus::Running);
}

/// A provisioned ACP task has no agent pane, so the pane classifier has nothing
/// true to say about it. Its run state comes from the ACP host as authoritative
/// evidence; a refresh must not overwrite that with a shell reading.
#[test]
fn provisioned_task_uses_acp_run_state_instead_of_shell_guess() {
    use crate::live;
    use crate::models::{LiveObservation, LiveStatusKind};
    use crate::ui_state::derive_operator_status;

    struct IdleShellRunner;

    impl CommandRunner for IdleShellRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            let args = command.args.join(" ");
            let stdout = if args.contains("capture-pane") {
                "matt@host ajax-cli % ".to_string()
            } else {
                runtime_stdout(&command.args).to_string()
            };
            Ok(CommandOutput {
                status_code: 0,
                stdout,
                stderr: String::new(),
            })
        }
    }

    let config = Config {
        repos: vec![ManagedRepo::new(REPO_NAME, REPO_PATH, BASE_BRANCH)],
        ..Config::default()
    };
    let mut registry = InMemoryRegistry::default();
    let mut task = task_with_live(LiveStatusKind::AgentRunning, "Agent working");
    task.set_skip_interactive_agent(true);
    let task_id = task.id.clone();
    // The ACP host's report, applied the way the session slice applies it.
    live::apply_authoritative_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::AgentRunning, "Agent working"),
        SystemTime::now(),
    );
    registry.create_task(task).unwrap();
    let mut context = CommandContext::new(config, registry);
    let mut runner = IdleShellRunner;

    let _ = refresh_runtime_context(&mut context, &mut runner);

    let task = context.registry.get_task(&task_id).unwrap();
    let status = derive_operator_status(task);
    assert_eq!(
        status.status,
        crate::ui_state::TaskStatus::Running,
        "provisioned ACP task lost its run state to the pane classifier: {status:?}"
    );
}

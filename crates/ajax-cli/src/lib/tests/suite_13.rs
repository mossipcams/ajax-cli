#[test]
fn pending_cockpit_unknown_action_does_not_open_or_mutate_task() {
    let mut context = sample_context();
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: "mystery action".to_string(),
        task_title: None,
    };
    let mut runner = RecordingCommandRunner::default();
    let mut state_changed = false;
    let error = execute_pending_cockpit_action(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
    )
    .unwrap_err();
    assert_eq!(
        error,
        CliError::CommandFailed("unknown cockpit action: mystery action".to_string())
    );
    assert!(runner.commands().is_empty());
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::Reviewable
    );
    assert!(!state_changed);
}
#[test]
fn pending_cockpit_risky_merge_requires_confirmation_without_running() {
    let mut context = sample_context();
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: "ship".to_string(),
        task_title: None,
    };
    let mut runner = RecordingCommandRunner::default();
    let mut state_changed = false;
    let error = execute_pending_cockpit_action(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        CliError::CommandFailed(message)
            if message == "confirmation required; pass --yes"
    ));
    assert!(runner.commands().is_empty());
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::Reviewable
    );
    assert!(!state_changed);
}
#[test]
fn pending_cockpit_failed_external_command_does_not_mutate_state() {
    let mut context = safe_merge_context();
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: "ship".to_string(),
        task_title: None,
    };
    let mut runner = QueuedRunner::new(vec![CommandOutput {
        status_code: 42,
        stdout: String::new(),
        stderr: "merge failed".to_string(),
    }]);
    let mut state_changed = false;
    let error = execute_pending_cockpit_action(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        CliError::CommandFailed(message)
            if message == "command failed: git exited with status 42: merge failed"
    ));
    assert_eq!(
        runner.commands,
        &[CommandSpec::new(
            "git",
            ["-C", "/Users/matt/projects/web", "switch", "main"]
        )]
    );
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::Reviewable
    );
    assert!(!state_changed);
}
#[test]
fn pending_cockpit_errors_return_to_ajax_with_flash_message() {
    let mut cockpit_flash = None;
    let outcome = handle_pending_cockpit_result(
        Err(CliError::CommandFailed(
            "git exited with status 42".to_string(),
        )),
        &mut cockpit_flash,
    );
    assert!(!outcome);
    assert_eq!(cockpit_flash.as_deref(), Some("git exited with status 42"));
}
#[test]
fn pending_cockpit_open_action_runs_task_without_lifecycle_change() {
    let mut context = sample_context();
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: "resume".to_string(),
        task_title: None,
    };
    let mut runner = RecordingCommandRunner::default();
    let mut state_changed = false;
    cockpit_actions::execute_pending_cockpit_action_with_open_mode(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
        OpenMode::Attach,
    )
    .unwrap();
    assert_eq!(
        runner.commands(),
        &[
            CommandSpec::new("tmux", ["select-window", "-t", "ajax-web-fix-login:task"]),
            CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        ]
    );
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::Reviewable
    );
    assert!(state_changed);
}
#[test]
fn pending_cockpit_open_action_switches_client_when_inside_tmux() {
    let mut context = sample_context();
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: "resume".to_string(),
        task_title: None,
    };
    let mut runner = RecordingCommandRunner::default();
    let mut state_changed = false;
    cockpit_actions::execute_pending_cockpit_action_with_open_mode(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
        OpenMode::SwitchClient,
    )
    .unwrap();
    assert_eq!(
        runner.commands(),
        &[
            CommandSpec::new("tmux", ["select-window", "-t", "ajax-web-fix-login:task"]),
            CommandSpec::new("tmux", ["switch-client", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        ]
    );
    assert!(state_changed);
}
#[test]
fn pending_cockpit_merge_action_runs_task_and_marks_merged() {
    let mut context = safe_merge_context();
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: "ship".to_string(),
        task_title: None,
    };
    let mut runner = RecordingCommandRunner::default();
    let mut state_changed = false;
    execute_pending_cockpit_action(&pending, &mut context, &mut runner, &mut state_changed)
        .unwrap();
    assert_eq!(
        runner.commands(),
        &[
            CommandSpec::new("git", ["-C", "/Users/matt/projects/web", "switch", "main"]),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "merge",
                    "--ff-only",
                    "ajax/fix-login"
                ]
            )
        ]
    );
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::Merged
    );
    assert!(state_changed);
}
#[test]
fn pending_cockpit_clean_action_runs_task_and_marks_removed() {
    let mut context = cleanable_context();
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: "drop".to_string(),
        task_title: None,
    };
    let mut runner = QueuedRunner::new(present_cleanable_drop_outputs());
    let mut state_changed = false;
    let output = execute_pending_cockpit_action(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
    )
    .unwrap();
    assert_eq!(output, None);
    assert_present_cleanable_force_drop_commands(&runner.commands);
    assert!(context.registry.get_task(&TaskId::new("task-1")).is_none());
    assert!(state_changed);
}
#[test]
fn failed_deferred_drop_restores_task_in_next_cockpit_snapshot() {
    let mut context = cleanable_context();
    let item = ajax_core::models::CockpitActionItem {
        task_id: TaskId::new("__task_action__web_fix_login__clean"),
        task_handle: "web/fix-login".to_string(),
        reason: "Clean task".to_string(),
        priority: 0,
        action: "drop".to_string(),
    };
    let mut state_changed = false;
    let outcome = tui_cockpit_confirmed_action(&item, &mut context).unwrap();
    let ajax_tui::ActionOutcome::RefreshAndDefer(optimistic, pending) = outcome else {
        panic!("confirmed drop should optimistically refresh and defer cleanup");
    };
    assert!(optimistic.cards.is_empty());
    let mut failing_runner = QueuedRunner::new(vec![
            output(0, ""),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n",
            ),
            output(0, "main\najax/fix-login\n"),
            output(0, ""),
            CommandOutput {
                status_code: 2,
                stdout: String::new(),
                stderr: "branch delete failed".to_string(),
            },
            output(0, ""),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
            ),
            output(0, "main\najax/fix-login\n"),
        ]);
    let error = execute_pending_cockpit_action(
        &pending,
        &mut context,
        &mut failing_runner,
        &mut state_changed,
    )
    .unwrap_err();
    let mut flash = None;
    let handled = handle_pending_cockpit_result(Err(error), &mut flash);
    let restored = crate::cockpit_backend::build_cockpit_snapshot(&context);
    assert!(!handled);
    assert_eq!(
        flash.as_deref(),
        Some("drop incomplete for web/fix-login at delete branch: git exited with status 2: branch delete failed; retry with `ajax drop web/fix-login --execute`")
    );
    assert!(restored
        .cards
        .iter()
        .any(|card| card.qualified_handle == "web/fix-login"));
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::TeardownIncomplete
    );
}
#[test]
fn cockpit_reconcile_action_is_unknown() {
    let mut context = sample_context();
    let item = ajax_core::models::CockpitActionItem {
        task_id: TaskId::new("__project_action__web__reconcile"),
        task_handle: "web".to_string(),
        reason: "Reconcile".to_string(),
        priority: 0,
        action: "reconcile".to_string(),
    };
    let outcome = tui_cockpit_action(&item, &mut context).unwrap();
    assert!(matches!(outcome, ajax_tui::ActionOutcome::Message(message)
            if message == "cockpit action is not configured: reconcile"));
}
#[test]
fn cockpit_clean_action_requires_confirmation_before_running() {
    let mut context = cleanable_context();
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .add_side_flag(SideFlag::Dirty);
    let item = ajax_core::models::CockpitActionItem {
        task_id: TaskId::new("__task_action__web_fix_login__clean"),
        task_handle: "web/fix-login".to_string(),
        reason: "Clean task".to_string(),
        priority: 0,
        action: "drop".to_string(),
    };
    let outcome = tui_cockpit_action(&item, &mut context).unwrap();
    match outcome {
        ajax_tui::ActionOutcome::Confirm(message) => {
            assert_eq!(message, "press enter again to confirm drop");
        }
        _ => panic!("drop should confirm before running"),
    }
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::Cleanable
    );
}
#[test]
fn confirmed_cockpit_clean_action_removes_from_snapshot_and_defers_cleanup() {
    let mut context = cleanable_context();
    let item = ajax_core::models::CockpitActionItem {
        task_id: TaskId::new("__task_action__web_fix_login__clean"),
        task_handle: "web/fix-login".to_string(),
        reason: "Clean task".to_string(),
        priority: 0,
        action: "drop".to_string(),
    };
    let outcome = tui_cockpit_confirmed_action(&item, &mut context).unwrap();
    match outcome {
        ajax_tui::ActionOutcome::RefreshAndDefer(snapshot, pending) => {
            assert_eq!(snapshot.repos.repos.len(), 1);
            assert!(snapshot.cards.is_empty());
            assert!(snapshot.inbox.items.is_empty());
            assert_eq!(pending.task_handle, "web/fix-login");
            assert_eq!(pending.action, "drop");
        }
        ajax_tui::ActionOutcome::Defer(_) => {
            panic!("drop task should optimistically refresh before deferring")
        }
        ajax_tui::ActionOutcome::Refresh(_) => {
            panic!("drop task should defer backend cleanup after refresh")
        }
        ajax_tui::ActionOutcome::Message(message) => {
            panic!("drop task should run instead of showing message: {message}")
        }
        ajax_tui::ActionOutcome::Confirm(message) => {
            panic!("confirmed drop task should run instead of confirming: {message}")
        }
    }
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::Cleanable
    );
}
#[test]
fn removed_reconcile_command_does_not_touch_registry_snapshot() {
    let directory = std::env::temp_dir().join(format!(
        "ajax-cli-removed-reconcile-{}-{}",
        std::process::id(),
        "state"
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let config_file = directory.join("config.toml");
    let state_file = directory.join("state.db");
    std::fs::write(
        &config_file,
        r#"
            [[repos]]
            name = "web"
            path = "/Users/matt/projects/web"
            default_branch = "main"
            "#,
    )
    .unwrap();
    SqliteRegistryStore::new(&state_file)
        .save(&sample_context().registry)
        .unwrap();
    let mut runner = QueuedRunner::new(vec![
        output(0, "other-session\n"),
        output(128, "fatal: not a git repository\n"),
    ]);
    let error = run_with_context_paths_and_runner(
        ["ajax", "reconcile", "--json"],
        &CliContextPaths::new(&config_file, &state_file),
        &mut runner,
    )
    .unwrap_err();
    let restored = SqliteRegistryStore::new(&state_file).load().unwrap();
    std::fs::remove_dir_all(Path::new(&directory)).unwrap();
    assert_eq!(
        error,
        CliError::CommandFailed(
            "error: unrecognized subcommand 'reconcile'\n\nUsage: ajax [OPTIONS] [COMMAND]\n\nFor more information, try '--help'.\n".to_string()
        )
    );
    assert!(runner.commands.is_empty());
    let restored_task = restored.get_task(&TaskId::new("task-1")).unwrap();
    assert!(!restored_task.has_side_flag(SideFlag::WorktreeMissing));
    assert_eq!(
        restored.list_tasks().len(),
        sample_context().registry.list_tasks().len()
    );
}
#[test]
fn agent_runtime_command_runs_without_loading_ajax_context() {
    let directory = std::env::temp_dir().join(format!(
        "ajax-cli-agent-runtime-command-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let output = run_with_args([
        "ajax",
        "--config",
        "/definitely/missing/ajax-config.toml",
        "__agent-runtime",
        "--task-id",
        "web/fix-login",
        "--state-root",
        directory.to_str().unwrap(),
        "--",
        "/bin/sh",
        "-c",
        "exit 0",
    ])
    .unwrap();
    assert_eq!(output, "");
    let latest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(directory.join("web__fix-login.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(latest["state"], "exited_success");
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn cockpit_refresh_does_not_mark_agent_running_from_wrapper_liveness_alone() {
    let directory = runtime_snapshot_directory("running-liveness");
    let now_millis = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    write_runtime_snapshot(&directory, "running", now_millis);
    let mut context = active_runtime_context(&directory);
    let mut runner = QueuedRunner::new(tmux_live_outputs());
    crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    // Wrapper "running" is process liveness only; idle pane has no activity.
    assert_ne!(task.agent_status, AgentRuntimeStatus::Running);
    assert_ne!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::AgentRunning)
    );
    assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn cockpit_refresh_marks_killed_agent_failed_instead_of_unknown() {
    let directory = runtime_snapshot_directory("failed");
    let now_millis = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    write_runtime_snapshot(&directory, "exited_failure", now_millis);
    let mut context = active_runtime_context(&directory);
    let mut runner = QueuedRunner::new(tmux_live_outputs());
    crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_ne!(task.agent_status, AgentRuntimeStatus::Unknown);
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::CommandFailed)
    );
    assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn cockpit_refresh_promotes_wrapper_completion_to_reviewable() {
    let directory = runtime_snapshot_directory("completed");
    let now_millis = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    write_runtime_snapshot(&directory, "exited_success", now_millis);
    let mut context = active_runtime_context(&directory);
    let mut runner = QueuedRunner::new(tmux_live_outputs());
    crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_eq!(task.agent_status, AgentRuntimeStatus::Done);
    assert_eq!(task.lifecycle_status, LifecycleStatus::Reviewable);
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn stale_wrapper_running_snapshot_cannot_keep_task_running() {
    let directory = runtime_snapshot_directory("stale");
    write_runtime_snapshot(&directory, "running", 1);
    let mut context = active_runtime_context(&directory);
    let mut runner = QueuedRunner::new(tmux_live_outputs());
    crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_ne!(task.agent_status, AgentRuntimeStatus::Running);
    assert_ne!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::AgentRunning)
    );
    // A stale wrapper-running snapshot is no longer a probe failure; core falls
    // through to a successful agent-aware pane observation instead.
    assert_ne!(
        task.runtime_projection.observation_error.as_deref(),
        Some("agent status stale")
    );
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn tmux_probe_failure_renders_unavailable_without_marking_session_missing() {
    struct FailingTmuxRunner;
    impl CommandRunner for FailingTmuxRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            assert_eq!(
                command.args.first().map(String::as_str),
                Some("list-sessions")
            );
            Err(CommandRunError::SpawnFailed("tmux unavailable".to_string()))
        }
    }
    let directory = runtime_snapshot_directory("tmux-failed");
    let mut context = active_runtime_context(&directory);
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
    let mut runner = FailingTmuxRunner;
    crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(!task.has_side_flag(SideFlag::TmuxMissing));
    assert!(task
        .tmux_status
        .as_ref()
        .is_some_and(|status| status.exists));
    assert_eq!(
        ajax_core::commands::cockpit_view(&context).cards[0]
            .status_explanation
            .as_deref(),
        Some("Status unavailable")
    );
}

#[test]
fn declined_cockpit_mismatch_confirmation_does_not_mutate_intent() {
    let mut context = sample_context_with_named_checkout_mismatch();
    let item = cockpit_item("web/fix-login", "repair");
    let event_count_before = context
        .registry
        .events_for_task(&TaskId::new("task-1"))
        .len();
    let mut retained_plan = None;
    let outcome = cockpit_actions::cockpit_action_outcome(
        &item,
        &mut context,
        false,
        &mut retained_plan,
    )
    .unwrap();
    assert!(matches!(
        outcome,
        ajax_tui::ActionOutcome::Confirm(ref message)
            if message == "press enter again to adopt branch fix/pane-stuck (expected ajax/fix-login)"
    ));
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .branch,
        "ajax/fix-login"
    );
    assert_eq!(
        context
            .registry
            .events_for_task(&TaskId::new("task-1"))
            .len(),
        event_count_before
    );
}
#[test]
fn pending_cockpit_drop_reconciles_missing_substrate_before_registry_removal() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.tmux_status = Some(TmuxStatus {
        exists: false,
        session_name: "ajax-web-fix-login".to_string(),
    });
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
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: "drop".to_string(),
        task_title: None,
    };
    let mut runner = QueuedRunner::new(missing_drop_observation_outputs());
    let mut state_changed = false;
    let outcome = execute_pending_cockpit_action(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
    )
    .unwrap();
    assert_eq!(outcome, None);
    assert_eq!(runner.commands, missing_drop_observation_commands());
    assert!(context.registry.get_task(&TaskId::new("task-1")).is_none());
    assert!(state_changed);
}
#[test]
fn task_session_pending_drop_uses_observed_drop_semantics() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.tmux_status = Some(TmuxStatus {
        exists: false,
        session_name: "ajax-web-fix-login".to_string(),
    });
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
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: "drop".to_string(),
        task_title: None,
    };
    let mut runner = QueuedRunner::new(missing_drop_observation_outputs());
    let mut task_session = RecordingTaskSessionRunner::default();
    let mut state_changed = false;
    execute_pending_cockpit_action_with_task_session(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
        &mut task_session,
        None,
    )
    .unwrap();
    assert_eq!(runner.commands, missing_drop_observation_commands());
    assert!(task_session.commands.is_empty());
    assert!(context.registry.get_task(&TaskId::new("task-1")).is_none());
    assert!(state_changed);
}
#[test]
fn pending_cockpit_reconcile_is_unknown() {
    let mut context = CommandContext::new(
        Config {
            repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    );
    let mut runner = PanicRunner;
    let mut state_changed = true;
    let pending = ajax_tui::PendingAction {
        task_handle: "web".to_string(),
        action: "reconcile".to_string(),
        task_title: None,
    };
    let error = execute_pending_cockpit_action(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
    )
    .unwrap_err();
    assert!(matches!(error, CliError::CommandFailed(message)
            if message == "unknown cockpit action: reconcile"));
    assert!(state_changed);
}
#[test]
fn pending_cockpit_open_and_create_actions_return_to_ajax_after_task_session() {
    let action = "resume";
    let mut context = sample_context();
    let mut runner = RecordingCommandRunner::default();
    let mut task_session = RecordingTaskSessionRunner::default();
    let mut state_changed = false;
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: action.to_string(),
        task_title: None,
    };
    cockpit_actions::execute_pending_cockpit_action_with_task_session(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
        &mut task_session,
        None,
    )
    .unwrap();
    assert_eq!(
        runner.commands(),
        &[CommandSpec::new(
            "tmux",
            ["select-window", "-t", "ajax-web-fix-login:task"]
        )]
    );
    assert_eq!(
        task_session.commands,
        vec![
            CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        ]
    );
    assert!(matches!(
        cockpit_actions::execute_pending_cockpit_action_with_task_session(
            &pending,
            &mut context,
            &mut runner,
            &mut state_changed,
            &mut OpenNewTaskTaskSessionRunner,
            None,
        )
        .unwrap(),
        cockpit_actions::PendingCockpitExecution::OpenNewTask { repo } if repo == "web"
    ));
    let mut context = CommandContext::new(
        Config {
            repos: vec![ManagedRepo::new("api", "/Users/matt/projects/api", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    );
    let pending = ajax_tui::PendingAction {
        task_handle: "api".to_string(),
        action: "start".to_string(),
        task_title: Some("Fix login".to_string()),
    };
    let mut runner = RecordingCommandRunner::default();
    let mut task_session = RecordingTaskSessionRunner::default();
    let mut state_changed = false;
    cockpit_actions::execute_pending_cockpit_action_with_task_session(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
        &mut task_session,
        None,
    )
    .unwrap();
    assert_eq!(
        task_session.commands,
        vec![
            CommandSpec::new("tmux", ["attach-session", "-t", "ajax-api-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        ]
    );
    assert!(!runner.commands().contains(&CommandSpec::new(
        "tmux",
        ["bind-key", "-n", "C-q", "detach-client"]
    )));
    assert!(runner.commands().iter().any(|command| {
        command.program == "tmux"
            && command.args.starts_with(&[
                "new-session".to_string(),
                "-d".to_string(),
                "-s".to_string(),
                "ajax-api-fix-login".to_string(),
            ])
    }));
    assert!(state_changed);
}
#[test]
fn pending_cockpit_resume_task_session_failure_stays_in_cockpit_without_lifecycle_change() {
    let mut context = sample_context();
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: "resume".to_string(),
        task_title: None,
    };
    let mut runner = RecordingCommandRunner::default();
    let mut task_session = FailingTaskSessionRunner {
        message: "tmux attach failed: session gone",
    };
    let mut state_changed = false;
    let error = cockpit_actions::execute_pending_cockpit_action_with_task_session(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
        &mut task_session,
        None,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        CliError::CommandFailed(message) if message == "tmux attach failed: session gone"
    ));
    assert_eq!(
        runner.commands(),
        &[CommandSpec::new(
            "tmux",
            ["select-window", "-t", "ajax-web-fix-login:task"]
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
fn pending_cockpit_create_task_session_failure_requests_cockpit_reload() {
    let mut context = CommandContext::new(
        Config {
            repos: vec![ManagedRepo::new("api", "/Users/matt/projects/api", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    );
    let pending = ajax_tui::PendingAction {
        task_handle: "api".to_string(),
        action: "start".to_string(),
        task_title: Some("Fix login".to_string()),
    };
    let mut runner = RecordingCommandRunner::default();
    let mut task_session = FailingTaskSessionRunner {
        message: "tmux missing",
    };
    let mut state_changed = false;
    let error = cockpit_actions::execute_pending_cockpit_action_with_task_session(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
        &mut task_session,
        None,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        CliError::CommandFailedAfterStateChange(message) if message == "tmux missing"
    ));
    assert!(state_changed);
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == "api/fix-login")
        .expect("failed start should still record the task");
    assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
}
#[test]
fn pending_cockpit_removed_actions_are_rejected() {
    for action in [
        "inspect agent",
        "inspect test output",
        "monitor task",
        "review branch",
        "review diff",
    ] {
        let mut context = sample_context();
        let pending = ajax_tui::PendingAction {
            task_handle: "web/fix-login".to_string(),
            action: action.to_string(),
            task_title: None,
        };
        let mut runner = PanicRunner;
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
            CliError::CommandFailed(format!("unknown cockpit action: {action}"))
        );
        assert!(!state_changed, "{action}");
    }
}
#[test]
fn pending_cockpit_repair_runs_task_window_plan() {
    let mut context = sample_context();
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: "repair".to_string(),
        task_title: None,
    };
    let mut runner = RecordingCommandRunner::default();
    let mut state_changed = false;
    let outcome = cockpit_actions::execute_pending_cockpit_action_with_open_mode(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
        OpenMode::Attach,
    )
    .unwrap();
    assert_eq!(outcome, None);
    assert_eq!(
        runner.commands(),
        &[
            CommandSpec::new(
                "tmux",
                [
                    "new-session",
                    "-d",
                    "-s",
                    "ajax-web-fix-login",
                    "-n",
                    "task",
                    "-c",
                    "/tmp/worktrees/web-fix-login"
                ]
            ),
            CommandSpec::new("tmux", ["select-window", "-t", "ajax-web-fix-login:task"]),
            CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        ]
    );
    assert!(state_changed);
}
#[test]
fn pending_cockpit_repair_switches_client_when_inside_tmux() {
    let mut context = sample_context();
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: "repair".to_string(),
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
        runner.commands().last(),
        Some(
            &CommandSpec::new("tmux", ["switch-client", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        )
    );
    assert!(state_changed);
}
#[test]
fn pending_cockpit_open_alias_actions_run_open_plan_without_lifecycle_change() {
    let action = "resume";
    let mut context = sample_context();
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: action.to_string(),
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
        ],
        "{action}"
    );
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::Reviewable,
        "{action} should preserve the task lifecycle"
    );
    assert!(state_changed, "{action}");
}

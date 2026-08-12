#[test]
fn cockpit_known_actions_never_return_command_hints() {
    for (handle, action) in [("web/fix-login", "resume"), ("web/fix-login", "ship")] {
        let mut context = sample_context();
        let item = ajax_core::models::CockpitActionItem {
            task_id: TaskId::new(format!("__cockpit_action__{action}")),
            task_handle: handle.to_string(),
            reason: action.to_string(),
            priority: 0,
            action: action.to_string(),
        };
        let outcome = tui_cockpit_action(&item, &mut context).unwrap();
        assert!(
            matches!(
                outcome,
                ajax_tui::ActionOutcome::Defer(pending)
                    if pending.task_handle == handle && pending.action == action
            ),
            "{action} should defer for execution"
        );
    }
    let mut context = sample_context();
    let start_item = ajax_core::models::CockpitActionItem {
        task_id: TaskId::new("__cockpit_action__start"),
        task_handle: "web".to_string(),
        reason: "start".to_string(),
        priority: 0,
        action: "start".to_string(),
    };
    let start_outcome = tui_cockpit_action(&start_item, &mut context).unwrap();
    assert!(matches!(
        start_outcome,
        ajax_tui::ActionOutcome::Message(message)
            if message == "select a project, then choose start task to enter a task name"
    ));
    let mut context = sample_context();
    let status_item = ajax_core::models::CockpitActionItem {
        task_id: TaskId::new("__cockpit_action__status"),
        task_handle: "web".to_string(),
        reason: "status".to_string(),
        priority: 0,
        action: "status".to_string(),
    };
    let status_outcome = tui_cockpit_action(&status_item, &mut context).unwrap();
    assert!(matches!(
        status_outcome,
        ajax_tui::ActionOutcome::Message(message) if message == "web: 1 task(s)"
    ));
    let mut context = cleanable_context();
    let item = cockpit_item("web/fix-login", "drop");
    let outcome = tui_cockpit_action(&item, &mut context).unwrap();
    match &outcome {
        ajax_tui::ActionOutcome::Confirm(message) => {
            assert_eq!(message, "press enter again to confirm drop");
        }
        ajax_tui::ActionOutcome::RefreshAndDefer(_, pending) => {
            assert_eq!(pending.action, "drop");
        }
        _ => panic!("drop task should confirm or refresh-and-defer"),
    }
}
#[test]
fn removed_cockpit_task_actions_are_unknown() {
    let mut context = sample_context();
    for action in [
        "inspect task",
        "inspect agent",
        "inspect test output",
        "monitor task",
        "review branch",
        "review diff",
    ] {
        let item = cockpit_item("web/fix-login", action);
        let outcome = tui_cockpit_action(&item, &mut context).unwrap();
        match outcome {
            ajax_tui::ActionOutcome::Message(message) => {
                assert_eq!(
                    message,
                    format!("cockpit action is not configured: {action}")
                );
            }
            _ => panic!("{action} should be an unknown cockpit action"),
        }
    }
}
#[test]
fn cockpit_unknown_action_does_not_suggest_shell_command() {
    let mut context = sample_context();
    let item = cockpit_item("web/fix-login", "mystery action");
    let outcome = tui_cockpit_action(&item, &mut context).unwrap();
    match outcome {
        ajax_tui::ActionOutcome::Message(message) => {
            assert_eq!(message, "cockpit action is not configured: mystery action");
        }
        _ => panic!("unknown cockpit action should stay in cockpit"),
    }
}
#[test]
fn cockpit_action_contract_covers_all_current_actions() {
    enum Expected<'a> {
        Defer,
        Message(&'a str),
        RefreshAndDefer,
    }
    let cases = [
        (
            "start",
            "web",
            Expected::Message("select a project, then choose start task to enter a task name"),
        ),
        ("resume", "web/fix-login", Expected::Defer),
        ("review", "web/fix-login", Expected::Defer),
        ("ship", "web/fix-login", Expected::Defer),
        ("drop", "web/fix-login", Expected::RefreshAndDefer),
        ("repair", "web/fix-login", Expected::Defer),
        ("status", "web", Expected::Message("web: 1 task(s)")),
    ];
    let covered_actions = cases
        .iter()
        .map(|(action, _, _)| *action)
        .collect::<std::collections::BTreeSet<_>>();
    let product_actions = OperatorAction::all()
        .iter()
        .map(|action| action.as_str())
        .chain(std::iter::once("status"))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(covered_actions, product_actions);
    for (action, handle, expected) in cases {
        let mut context = if action == "drop" {
            cleanable_context()
        } else {
            sample_context()
        };
        let item = cockpit_item(handle, action);
        let outcome = tui_cockpit_action(&item, &mut context).unwrap();
        match expected {
            Expected::Defer => match outcome {
                ajax_tui::ActionOutcome::Defer(pending) => {
                    assert_eq!(pending.task_handle, handle, "{action}");
                    assert_eq!(pending.action, action);
                    assert!(pending.task_title.is_none(), "{action}");
                }
                ajax_tui::ActionOutcome::Message(message) => {
                    panic!("{action} should defer, got message: {message}");
                }
                ajax_tui::ActionOutcome::Confirm(message) => {
                    panic!("{action} should defer, got confirm: {message}");
                }
                ajax_tui::ActionOutcome::Refresh { .. } => {
                    panic!("{action} should defer, got refresh");
                }
                ajax_tui::ActionOutcome::RefreshAndDefer(_, _) => {
                    panic!("{action} should defer without refreshing first");
                }
            },
            Expected::Message(expected_message) => match outcome {
                ajax_tui::ActionOutcome::Message(message) => {
                    assert_eq!(message, expected_message, "{action}");
                }
                ajax_tui::ActionOutcome::Defer(_) => {
                    panic!("{action} should render in cockpit, got defer");
                }
                ajax_tui::ActionOutcome::Confirm(message) => {
                    panic!("{action} should render in cockpit, got confirm: {message}");
                }
                ajax_tui::ActionOutcome::Refresh(_) => {
                    panic!("{action} should render in cockpit, got refresh");
                }
                ajax_tui::ActionOutcome::RefreshAndDefer(_, _) => {
                    panic!("{action} should render in cockpit, got refresh and defer");
                }
            },
            Expected::RefreshAndDefer => match outcome {
                ajax_tui::ActionOutcome::RefreshAndDefer(snapshot, pending) => {
                    assert_eq!(snapshot.repos.repos.len(), 1, "{action}");
                    assert!(snapshot.cards.is_empty(), "{action}");
                    assert!(snapshot.inbox.items.is_empty(), "{action}");
                    assert_eq!(pending.task_handle, handle, "{action}");
                    assert_eq!(pending.action, action, "{action}");
                }
                ajax_tui::ActionOutcome::Defer(_) => {
                    panic!("{action} should refresh before deferring, got defer");
                }
                ajax_tui::ActionOutcome::Message(message) => {
                    panic!("{action} should refresh before deferring, got message: {message}");
                }
                ajax_tui::ActionOutcome::Confirm(message) => {
                    panic!("{action} should refresh before deferring, got confirm: {message}");
                }
                ajax_tui::ActionOutcome::Refresh(_) => {
                    panic!("{action} should defer backend cleanup after refresh");
                }
            },
        }
    }
}
#[test]
fn cockpit_merge_task_action_stays_inside_ajax() {
    let mut context = sample_context();
    let item = ajax_core::models::CockpitActionItem {
        task_id: TaskId::new("__task_action__web_fix_login__merge"),
        task_handle: "web/fix-login".to_string(),
        reason: "Merge task".to_string(),
        priority: 0,
        action: "ship".to_string(),
    };
    let outcome = tui_cockpit_action(&item, &mut context).unwrap();
    match outcome {
        ajax_tui::ActionOutcome::Defer(pending) => {
            assert_eq!(pending.task_handle, "web/fix-login");
            assert_eq!(pending.action, "ship");
            assert!(pending.task_title.is_none());
        }
        _ => panic!("completed task action should defer for execution"),
    }
}
#[test]
fn cockpit_task_action_return_stays_inside_ajax() {
    let mut context = sample_context();
    let item = ajax_core::models::CockpitActionItem {
        task_id: TaskId::new("__task_action__web_fix_login__open"),
        task_handle: "web/fix-login".to_string(),
        reason: "Open task".to_string(),
        priority: 0,
        action: "resume".to_string(),
    };
    let outcome = tui_cockpit_action(&item, &mut context).unwrap();
    match outcome {
        ajax_tui::ActionOutcome::Defer(pending) => {
            assert_eq!(pending.task_handle, "web/fix-login");
            assert_eq!(pending.action, "resume");
            assert!(pending.task_title.is_none());
        }
        _ => panic!("task action should defer for execution"),
    }
}
#[test]
fn pending_new_task_action_requires_completed_title() {
    let mut context = CommandContext::new(
        Config {
            repos: vec![
                ManagedRepo::new("web", "/Users/matt/projects/web", "main"),
                ManagedRepo::new("api", "/Users/matt/projects/api", "main"),
            ],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    );
    let pending = ajax_tui::PendingAction {
        task_handle: "api".to_string(),
        action: "start".to_string(),
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
        CliError::CommandFailed(
            "start task title is required before cockpit can create the task".to_string()
        )
    );
    assert!(context.registry.list_tasks().is_empty());
    assert!(!state_changed);
}
#[test]
fn pending_new_task_action_does_not_run_without_title() {
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
        task_title: None,
    };
    let mut runner = QueuedRunner::new(vec![output(1, "")]);
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
        CliError::CommandFailed(
            "start task title is required before cockpit can create the task".to_string()
        )
    );
    assert!(runner.commands.is_empty());
    assert!(context.registry.list_tasks().is_empty());
    assert!(!state_changed);
}
#[test]
fn failed_pending_new_task_action_marks_state_changed_for_cockpit_recovery() {
    let mut context = CommandContext::new(
        Config {
            repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    );
    let pending = ajax_tui::PendingAction {
        task_handle: "web".to_string(),
        action: "start".to_string(),
        task_title: Some("Fix login".to_string()),
    };
    let mut runner = QueuedRunner::new(vec![CommandOutput {
        status_code: 42,
        stdout: String::new(),
        stderr: "git failed".to_string(),
    }]);
    let mut state_changed = false;
    let error = crate::cockpit_actions::execute_pending_cockpit_action_with_open_mode(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
        OpenMode::Attach,
    )
    .unwrap_err();
    assert!(
        matches!(error, CliError::CommandFailedAfterStateChange(message)
                if message == "command failed: git exited with status 42: git failed")
    );
    assert!(state_changed);
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == "web/fix-login")
        .expect("failed cockpit create should leave a visible task");
    assert_eq!(task.lifecycle_status, LifecycleStatus::Error);
    let tasks = ajax_core::commands::list_tasks(&context, None);
    assert_eq!(
        tasks.tasks[0].actions,
        vec![
            OperatorAction::Resume.as_str().to_string(),
            OperatorAction::Drop.as_str().to_string(),
        ]
    );
    let inbox = ajax_core::commands::inbox(&context);
    assert_eq!(inbox.items.len(), 1);
    assert_eq!(inbox.items[0].action, OperatorAction::Resume);
}
#[test]
fn pending_new_task_action_runs_after_title_is_collected() {
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
    let mut state_changed = false;
    let outcome = crate::cockpit_actions::execute_pending_cockpit_action_with_open_mode(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
        OpenMode::Attach,
    )
    .unwrap();
    assert_eq!(
        outcome.as_deref().and_then(|output| output
            .lines()
            .find(|line| line.starts_with("recorded task:"))),
        Some("recorded task: api/fix-login")
    );
    let mut expected_commands =
        expected_sync_default_branch_commands("/Users/matt/projects/api", "main");
    expected_commands.extend([
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/api",
                "worktree",
                "add",
                "-b",
                "ajax/fix-login",
                "/Users/matt/projects/api__worktrees/ajax-fix-login",
                "origin/main",
            ],
        ),
        CommandSpec::new(
            "tmux",
            [
                "new-session",
                "-d",
                "-s",
                "ajax-api-fix-login",
                "-n",
                "task",
                "-c",
                "/Users/matt/projects/api__worktrees/ajax-fix-login",
            ],
        ),
        expected_task_launch_command(
            "ajax-api-fix-login",
            "api/fix-login",
            "/Users/matt/projects/api__worktrees/ajax-fix-login",
            None,
        ),
        CommandSpec::new("tmux", ["select-window", "-t", "ajax-api-fix-login:task"]),
        expected_new_task_open_command("ajax-api-fix-login"),
    ]);
    assert_eq!(runner.commands(), expected_commands.as_slice());
    let task = context
        .registry
        .list_tasks()
        .iter()
        .find(|task| task.qualified_handle() == "api/fix-login")
        .cloned()
        .expect("start task should be recorded");
    assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
    assert!(state_changed);
}
#[test]
fn cockpit_start_persists_task_before_first_external_command() {
    struct PreExternalStateRunner {
        state_file: PathBuf,
        checked: bool,
        outputs: std::collections::VecDeque<CommandOutput>,
    }
    impl CommandRunner for PreExternalStateRunner {
        fn run(&mut self, _command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            if !self.checked {
                self.checked = true;
                let restored = SqliteRegistryStore::new(&self.state_file)
                    .load()
                    .expect("state should be readable before first external command");
                assert!(
                    restored
                        .list_tasks()
                        .iter()
                        .any(|task| task.qualified_handle() == "api/fix-login"),
                    "cockpit start task should be durable before the first external command"
                );
            }
            self.outputs
                .pop_front()
                .ok_or_else(|| CommandRunError::SpawnFailed("missing queued output".to_string()))
        }
    }
    let directory = std::env::temp_dir().join(format!(
        "ajax-cli-cockpit-start-{}-{}",
        std::process::id(),
        "pre-external"
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let config_file = directory.join("config.toml");
    let state_file = directory.join("state.db");
    let paths = CliContextPaths::new(&config_file, &state_file);
    let mut context = CommandContext::with_runtime_paths(
        Config {
            repos: vec![ManagedRepo::new("api", "/Users/matt/projects/api", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
        paths.runtime_paths.clone(),
    );
    let mut save_state = crate::context::context_save_state_from_registry(&context.registry);
    let pending = ajax_tui::PendingAction {
        task_handle: "api".to_string(),
        action: "start".to_string(),
        task_title: Some("Fix login".to_string()),
    };
    let mut runner = PreExternalStateRunner {
        state_file: state_file.clone(),
        checked: false,
        outputs: vec![
            output(0, ""),
            output(0, ""),
            output(0, ""),
            output(0, ""),
            output(0, ""),
            output(0, ""),
            output(0, ""),
            output(0, ""),
        ]
        .into(),
    };
    let mut task_session = RecordingTaskSessionRunner::default();
    let mut state_changed = false;
    cockpit_actions::execute_pending_cockpit_action_with_task_session_and_checkpoint(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
        &mut task_session,
        None,
        |checkpoint_context| {
            crate::context::save_context_with_state(&paths, checkpoint_context, &mut save_state)
                .map_err(|error| {
                    ajax_core::commands::CommandError::CommandRun(CommandRunError::SpawnFailed(
                        format!("persist test checkpoint: {error}"),
                    ))
                })
        },
    )
    .unwrap();
    std::fs::remove_dir_all(Path::new(&directory)).unwrap();
    assert!(runner.checked);
}
#[test]
fn task_verbs_render_core_operation_titles() {
    let context = sample_context();
    let resume = run_with_context(["ajax", "resume", "web/fix-login"], &context).unwrap();
    let repair = run_with_context(["ajax", "repair", "web/fix-login"], &context).unwrap();
    let review = run_with_context(["ajax", "review", "web/fix-login"], &context).unwrap();
    let ship = run_with_context(["ajax", "ship", "web/fix-login"], &context).unwrap();
    assert_eq!(resume.lines().next().unwrap(), "open task: web/fix-login");
    assert_eq!(repair.lines().next().unwrap(), "repair task: web/fix-login");
    assert_eq!(review.lines().next().unwrap(), "diff task: web/fix-login");
    assert_eq!(ship.lines().next().unwrap(), "merge task: web/fix-login");
}
#[test]
fn reconcile_is_not_an_operator_action() {
    assert_eq!(OperatorAction::from_label("reconcile"), None);
}
#[test]
fn drop_plan_refreshes_stale_git_evidence_before_rendering_commands() {
    let mut context = sample_context();
    let task_id = TaskId::new("task-1");
    context
        .registry
        .update_git_status(
            &task_id,
            GitStatus {
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
            },
        )
        .unwrap();
    context
        .registry
        .update_tmux_status(
            &task_id,
            Some(TmuxStatus {
                exists: false,
                session_name: "ajax-web-fix-login".to_string(),
            }),
        )
        .unwrap();
    let matches = build_cli()
        .try_get_matches_from(["ajax", "drop", "web/fix-login", "--json"])
        .unwrap();
    let Some((_, subcommand)) = matches.subcommand() else {
        panic!("drop should parse as a subcommand");
    };
    let mut runner = QueuedRunner::new(vec![
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
        ),
        output(0, "main\n"),
        output(0, "origin/main\n"),
        ]);
    let rendered = render_drop_command(subcommand, &mut context, &mut runner).unwrap();
    assert!(rendered.state_changed);
    assert_eq!(
        runner.commands,
        vec![
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "worktree",
                    "list",
                    "--porcelain"
                ]
            ),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "branch",
                    "--format=%(refname:short)"
                ]
            )
        ]
    );
    let drop_plan: serde_json::Value = serde_json::from_str(&rendered.output).unwrap();
    assert_eq!(drop_plan["title"], "remove task: web/fix-login");
    assert_eq!(drop_plan["blocked_reasons"], serde_json::json!([]));
    assert_eq!(drop_plan["title"].as_str().unwrap().find("worktree"), None);
    assert_eq!(drop_plan["title"].as_str().unwrap().find("branch"), None);
    let task = context.registry.get_task(&task_id).unwrap();
    let git_status = task.git_status.as_ref().unwrap();
    assert!(!git_status.worktree_exists);
    assert!(!git_status.branch_exists);
}

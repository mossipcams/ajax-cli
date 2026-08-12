#[test]
fn drop_execute_keeps_task_when_worktree_remove_fails_before_tmux_session_kill() {
    let mut context = cleanable_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
    task.task_window_status = Some(TaskWindowStatus::present(
        "task",
        "/tmp/worktrees/web-fix-login",
    ));
    let mut runner = QueuedRunner::new(vec![
        output(0, "ajax-web-fix-login\n"),
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n",
        ),
        output(0, "main\najax/fix-login\n"),
        output(0, "origin/main\norigin/ajax/fix-login\n"),
        CommandOutput {
            status_code: 2,
            stdout: String::new(),
            stderr: "error: failed to remove worktree: permission denied".to_string(),
        },
        output(0, "ajax-web-fix-login\n"),
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n",
        ),
        output(0, "main\najax/fix-login\n"),
        output(0, "origin/main\norigin/ajax/fix-login\n"),
        ]);
    run_with_context_and_runner(
        ["ajax", "drop", "web/fix-login", "--execute"],
        &mut context,
        &mut runner,
    )
    .unwrap_err();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_eq!(task.lifecycle_status, LifecycleStatus::TeardownIncomplete);
    assert_eq!(
        task.metadata.get("drop_failed_step").map(String::as_str),
        Some("remove worktree")
    );
    assert_eq!(
        task.metadata.get("drop_failed_detail").map(String::as_str),
        Some("sh exited with status 2: error: failed to remove worktree: permission denied")
    );
    assert!(task
        .tmux_status
        .as_ref()
        .is_some_and(|status| status.exists));
    assert!(!runner.commands.iter().any(|command| {
        command.program == "tmux" && command.args.iter().any(|arg| arg == "kill-session")
    }));
}
#[test]
fn drop_execute_branch_failure_after_worktree_remove_marks_teardown_incomplete() {
    let mut context = cleanable_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
    task.task_window_status = Some(TaskWindowStatus::present(
        "task",
        "/tmp/worktrees/web-fix-login",
    ));
    let mut runner = QueuedRunner::new(vec![
            output(0, "ajax-web-fix-login\n"),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n",
            ),
            output(0, "main\najax/fix-login\n"),
        output(0, "origin/main\norigin/ajax/fix-login\n"),
        output(0, ""),
            CommandOutput {
                status_code: 2,
                stdout: String::new(),
                stderr: "branch delete failed".to_string(),
            },
            output(0, "ajax-web-fix-login\n"),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
            ),
            output(0, "main\najax/fix-login\n"),
        output(0, "origin/main\norigin/ajax/fix-login\n"),
        ]);
    let error = run_with_context_and_runner(
        ["ajax", "drop", "web/fix-login", "--execute"],
        &mut context,
        &mut runner,
    )
    .unwrap_err();
    assert_eq!(
        error,
        CliError::CommandFailedAfterStateChange(
            "drop incomplete for web/fix-login at delete branch: sh exited with status 2: branch delete failed; retry with `ajax drop web/fix-login --execute`".to_string()
        )
    );
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_eq!(task.lifecycle_status, LifecycleStatus::TeardownIncomplete);
    assert_eq!(
        task.metadata.get("drop_failed_step").map(String::as_str),
        Some("delete branch")
    );
    assert!(task
        .tmux_status
        .as_ref()
        .is_some_and(|status| status.exists));
    assert!(task
        .git_status
        .as_ref()
        .is_some_and(|status| { !status.worktree_exists && status.branch_exists }));
    assert!(!runner.commands.iter().any(|command| {
        command.program == "tmux" && command.args.iter().any(|arg| arg == "kill-session")
    }));
    assert!(!ajax_core::commands::list_tasks(&context, None)
        .tasks
        .is_empty());
}
#[test]
fn drop_execute_second_run_after_partial_failure_resumes_and_removes_task() {
    let mut context = cleanable_context();
    let mut failing_runner = QueuedRunner::new(vec![
            output(0, ""),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n",
            ),
            output(0, "main\najax/fix-login\n"),
        output(0, "origin/main\norigin/ajax/fix-login\n"),
        output(0, ""),
            CommandOutput {
                status_code: 2,
                stdout: String::new(),
                stderr: "branch delete failed".to_string(),
            },
            output(0, "ajax-web-fix-login\n"),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
            ),
            output(0, "main\najax/fix-login\n"),
        output(0, "origin/main\norigin/ajax/fix-login\n"),
        ]);
    run_with_context_and_runner(
        ["ajax", "drop", "web/fix-login", "--execute"],
        &mut context,
        &mut failing_runner,
    )
    .unwrap_err();
    let mut resume_runner = QueuedRunner::new(vec![
        output(0, "ajax-web-fix-login\n"),
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
        ),
        output(0, "main\najax/fix-login\n"),
        output(0, "origin/main\norigin/ajax/fix-login\n"),
        output(0, ""),
        output(0, ""),
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
        ),
        output(0, "main\n"),
        output(0, "origin/main\n"),
        ]);
    run_with_context_and_runner(
        ["ajax", "drop", "web/fix-login", "--execute"],
        &mut context,
        &mut resume_runner,
    )
    .unwrap();
    assert!(context.registry.get_task(&TaskId::new("task-1")).is_none());
}
#[test]
fn repair_execute_repairs_task_window_with_injected_runner() {
    let mut context = sample_context();
    let mut runner = RecordingCommandRunner::default();
    let matches = build_cli()
        .try_get_matches_from(["ajax", "repair", "web/fix-login", "--execute"])
        .unwrap();
    let (_, subcommand) = matches.subcommand().unwrap();
    render_task_command(
        TaskCommandKind::Repair,
        subcommand,
        &mut context,
        &mut runner,
        OpenMode::Attach,
    )
    .unwrap();
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
}
#[test]
fn repair_execute_switches_client_when_inside_tmux() {
    let mut context = sample_context();
    let mut runner = RecordingCommandRunner::default();
    let matches = build_cli()
        .try_get_matches_from(["ajax", "repair", "web/fix-login", "--execute"])
        .unwrap();
    let (_, subcommand) = matches.subcommand().unwrap();
    render_task_command(
        TaskCommandKind::Repair,
        subcommand,
        &mut context,
        &mut runner,
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
}
#[test]
fn repair_execute_clears_missing_tmux_and_task_flags() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.add_side_flag(SideFlag::TmuxMissing);
    task.add_side_flag(SideFlag::TaskWindowMissing);
    task.tmux_status = Some(TmuxStatus {
        exists: false,
        session_name: "ajax-web-fix-login".to_string(),
    });
    task.task_window_status = Some(TaskWindowStatus {
        exists: false,
        window_name: "task".to_string(),
        current_path: "/tmp/worktrees/web-fix-login".into(),
        points_at_expected_path: false,
    });
    let mut runner = RecordingCommandRunner::default();
    run_with_context_and_runner(
        ["ajax", "repair", "web/fix-login", "--execute"],
        &mut context,
        &mut runner,
    )
    .unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(!task.has_side_flag(SideFlag::TmuxMissing));
    assert!(!task.has_side_flag(SideFlag::TaskWindowMissing));
    assert_eq!(
        task.tmux_status,
        Some(TmuxStatus::present("ajax-web-fix-login"))
    );
    assert_eq!(
        task.task_window_status,
        Some(TaskWindowStatus::present(
            "task",
            "/tmp/worktrees/web-fix-login"
        ))
    );
}
#[test]
fn repair_execute_recreated_worktree_is_marked_present() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.git_status = Some(GitStatus {
        worktree_exists: false,
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
    task.add_side_flag(SideFlag::WorktreeMissing);
    let mut runner = RecordingCommandRunner::default();
    run_with_context_and_runner(
        ["ajax", "repair", "web/fix-login", "--execute"],
        &mut context,
        &mut runner,
    )
    .unwrap();
    assert!(runner.commands().iter().any(|command| {
        command
            == &CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "worktree",
                    "add",
                    "/tmp/worktrees/web-fix-login",
                    "ajax/fix-login",
                ],
            )
    }));
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(task
        .git_status
        .as_ref()
        .is_some_and(|status| status.worktree_exists));
    assert!(!task.has_side_flag(SideFlag::WorktreeMissing));
}
#[test]
fn repair_execute_uses_injected_runner() {
    let mut context = sample_context();
    context.config.test_commands = vec![ajax_core::config::TestCommand::new("web", "cargo test")];
    let mut runner = RecordingCommandRunner::default();
    let matches = build_cli()
        .try_get_matches_from(["ajax", "repair", "web/fix-login", "--execute"])
        .unwrap();
    let (_, subcommand) = matches.subcommand().unwrap();
    // Inject the open mode explicitly. The `run_with_context_and_runner`
    // dispatch path resolves it from the ambient `$TMUX` env var, which
    // makes this assertion non-deterministic across environments (passing
    // inside tmux, failing in CI). Pin the env-independent `Attach`
    // default so the full command sequence is asserted deterministically.
    render_task_command(
        TaskCommandKind::Repair,
        subcommand,
        &mut context,
        &mut runner,
        OpenMode::Attach,
    )
    .unwrap();
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
                    "/tmp/worktrees/web-fix-login",
                ],
            ),
            CommandSpec::new("tmux", ["select-window", "-t", "ajax-web-fix-login:task"],),
            CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio),
            CommandSpec::new("sh", ["-lc", "cargo test"]).with_cwd("/tmp/worktrees/web-fix-login")
        ]
    );
}
#[test]
fn repair_execute_failure_records_tests_failed_attention_without_lifecycle_corruption() {
    let mut context = sample_context();
    context.config.test_commands = vec![ajax_core::config::TestCommand::new("web", "cargo test")];
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .lifecycle_status = LifecycleStatus::Active;
    let mut runner = QueuedRunner::new(vec![
        output(0, ""),
        output(0, ""),
        output(0, ""),
        CommandOutput {
            status_code: 42,
            stdout: String::new(),
            stderr: "tests failed".to_string(),
        },
    ]);
    let error = run_with_context_and_runner(
        ["ajax", "repair", "web/fix-login", "--execute"],
        &mut context,
        &mut runner,
    )
    .unwrap_err();
    assert!(
        matches!(error, CliError::CommandFailedAfterStateChange(message)
                if message == "command failed: sh exited with status 42 in /tmp/worktrees/web-fix-login: tests failed")
    );
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
    assert!(task.has_side_flag(SideFlag::TestsFailed));
    assert_eq!(
        task.live_status
            .as_ref()
            .map(|status| (status.kind, status.summary.as_str())),
        Some((LiveStatusKind::CiFailed, "check failed"))
    );
}
#[test]
fn check_execute_success_promotes_active_task_to_reviewable() {
    let mut context = sample_context();
    context.config.test_commands = vec![ajax_core::config::TestCommand::new("web", "cargo test")];
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.add_side_flag(SideFlag::TestsFailed);
    let mut runner = RecordingCommandRunner::default();
    run_with_context_and_runner(
        ["ajax", "repair", "web/fix-login", "--execute"],
        &mut context,
        &mut runner,
    )
    .unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_eq!(task.lifecycle_status, LifecycleStatus::Reviewable);
    assert!(!task.has_side_flag(SideFlag::TestsFailed));
    assert!(task.live_status.is_none());
}
#[test]
fn diff_execute_uses_injected_runner() {
    let mut context = sample_context();
    let mut runner = RecordingCommandRunner::default();
    run_with_context_and_runner(
        ["ajax", "review", "web/fix-login", "--execute"],
        &mut context,
        &mut runner,
    )
    .unwrap();
    assert_eq!(
        runner.commands(),
        &[CommandSpec::new("git", ["diff", "--stat", "main...HEAD"])
            .with_cwd("/tmp/worktrees/web-fix-login")]
    );
}
#[test]
fn sweep_execute_uses_injected_runner_and_marks_safe_tasks_removed() {
    let mut context = cleanable_context();
    let mut runner = QueuedRunner::new(present_cleanable_drop_outputs());
    run_with_context_and_runner(["ajax", "tidy", "--execute"], &mut context, &mut runner).unwrap();
    assert!(context.registry.get_task(&TaskId::new("task-1")).is_none());
}
#[test]
fn sweep_execute_persists_completed_removals_when_later_command_fails() {
    let directory = std::env::temp_dir().join(format!(
        "ajax-cli-sweep-partial-{}-{}",
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
        .save(&two_cleanable_tasks_context().registry)
        .unwrap();
    let plan_context = two_cleanable_tasks_context();
    let candidates = ajax_core::commands::sweep_cleanup_candidates(&plan_context);
    let total_plan_commands: usize = candidates
        .iter()
        .map(|candidate| {
            ajax_core::commands::clean_task_plan(&plan_context, candidate)
                .unwrap()
                .commands
                .len()
        })
        .sum();
    let trash_sweeps = ajax_core::commands::sweep_trash_commands(&plan_context);
    let mut runner_outputs = trash_sweeps
        .iter()
        .map(|_| output(0, ""))
        .collect::<Vec<_>>();
    runner_outputs.push(output(0, "ajax-web-fix-login\n"));
    runner_outputs.extend((0..=total_plan_commands + 1).map(|_| output(0, "")));
    *runner_outputs
        .last_mut()
        .expect("sweep should queue commands") = CommandOutput {
        status_code: 2,
        stdout: String::new(),
        stderr: "worktree remove failed".to_string(),
    };
    let mut runner = QueuedRunner::new(runner_outputs);
    let error = run_with_context_paths_and_runner(
        ["ajax", "tidy", "--execute"],
        &CliContextPaths::new(&config_file, &state_file),
        &mut runner,
    )
    .unwrap_err();
    let restored = SqliteRegistryStore::new(&state_file).load().unwrap();
    std::fs::remove_dir_all(Path::new(&directory)).unwrap();
    assert_eq!(
        error.to_string(),
        "command failed: git exited with status 2: worktree remove failed"
    );
    assert_eq!(
        restored
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::TeardownIncomplete
    );
    assert_eq!(
        restored
            .get_task(&TaskId::new("task-2"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::Cleanable
    );
}
#[test]
fn cockpit_new_task_action_guides_operator_to_project_input() {
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
    let item = ajax_core::models::CockpitActionItem {
        task_id: TaskId::new("__project_action__api__new_task"),
        task_handle: "api".to_string(),
        reason: "+ New task".to_string(),
        priority: 0,
        action: "start".to_string(),
    };
    let outcome = tui_cockpit_action(&item, &mut context).unwrap();
    match outcome {
        ajax_tui::ActionOutcome::Message(message) => {
            assert_eq!(
                message,
                "select a project, then choose start task to enter a task name"
            );
        }
        _ => panic!("start task should remain inside Ajax cockpit"),
    }
    assert!(context.registry.list_tasks().is_empty());
}
#[test]
fn cockpit_actions_defer_to_executable_ajax_commands() {
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
        match outcome {
            ajax_tui::ActionOutcome::Defer(pending) => {
                assert_eq!(pending.task_handle, handle);
                assert_eq!(pending.action, action);
                assert!(pending.task_title.is_none());
            }
            ajax_tui::ActionOutcome::Message(message) => {
                panic!("{action} should defer for execution instead of showing message: {message}")
            }
            ajax_tui::ActionOutcome::Refresh { .. } => {
                panic!("{action} should defer for execution instead of refreshing")
            }
            ajax_tui::ActionOutcome::RefreshAndDefer(_, _) => {
                panic!("{action} should defer without refreshing first")
            }
            ajax_tui::ActionOutcome::Confirm(message) => {
                panic!("{action} should defer for execution instead of confirming: {message}")
            }
        }
    }
}

#[test]
fn resume_plan_refreshes_stale_git_evidence_before_rendering_commands() {
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
    let matches = build_cli()
        .try_get_matches_from(["ajax", "resume", "web/fix-login", "--json"])
        .unwrap();
    let Some((_, subcommand)) = matches.subcommand() else {
        panic!("resume should parse as a subcommand");
    };
    let mut runner = QueuedRunner::new(vec![
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
        ),
        output(0, "main\n"),
    ]);
    let rendered = render_task_command(
        TaskCommandKind::Resume,
        subcommand,
        &mut context,
        &mut runner,
        OpenMode::Attach,
    )
    .unwrap();
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
    let resume_plan: serde_json::Value = serde_json::from_str(&rendered.output).unwrap();
    assert_eq!(
        resume_plan["blocked_reasons"],
        serde_json::json!(["task has missing substrate"])
    );
    let task = context.registry.get_task(&task_id).unwrap();
    let git_status = task.git_status.as_ref().unwrap();
    assert!(!git_status.worktree_exists);
    assert!(!git_status.branch_exists);
}
#[test]
fn drop_execute_does_not_mark_removed_when_final_observation_is_unavailable() {
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
        .try_get_matches_from(["ajax", "drop", "web/fix-login", "--execute", "--yes"])
        .unwrap();
    let Some((_, subcommand)) = matches.subcommand() else {
        panic!("drop should parse as a subcommand");
    };
    let mut runner = QueuedRunner::new(vec![
        output(0, ""),
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
        ),
        output(0, "main\n"),
        output(0, ""),
        CommandOutput {
            status_code: 128,
            stdout: String::new(),
            stderr: "fatal: not a git repository".to_string(),
        },
        CommandOutput {
            status_code: 128,
            stdout: String::new(),
            stderr: "fatal: not a git repository".to_string(),
        },
    ]);
    let error = render_drop_command(subcommand, &mut context, &mut runner).unwrap_err();
    assert_eq!(
        error,
        CliError::CommandFailedAfterStateChange(
            "drop incomplete for web/fix-login at remove worktree: external resources still present after teardown attempt; retry with `ajax drop web/fix-login --execute`".to_string()
        )
    );
    assert_eq!(
        runner.commands,
        vec![
            CommandSpec::new("tmux", ["list-sessions", "-F", "#{session_name}"])
                .with_timeout(std::time::Duration::from_secs(8)),
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
            ),
            CommandSpec::new("tmux", ["list-sessions", "-F", "#{session_name}"])
                .with_timeout(std::time::Duration::from_secs(8)),
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
    assert_eq!(
        context
            .registry
            .get_task(&task_id)
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::TeardownIncomplete
    );
}
#[test]
fn drop_execute_reports_registry_removal_when_no_external_resources_remain() {
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
        .try_get_matches_from(["ajax", "drop", "web/fix-login", "--execute", "--yes"])
        .unwrap();
    let Some((_, subcommand)) = matches.subcommand() else {
        panic!("drop should parse as a subcommand");
    };
    let mut runner = QueuedRunner::new(vec![
        output(0, ""),
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
        ),
        output(0, "main\n"),
        output(0, ""),
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
        ),
        output(0, "main\n"),
    ]);
    let rendered = render_drop_command(subcommand, &mut context, &mut runner).unwrap();
    assert_eq!(rendered.output, "removed task: web/fix-login");
    assert!(context.registry.get_task(&task_id).is_none());
}
#[test]
fn drop_execute_hard_removes_task_from_sqlite_state_file() {
    let directory = std::env::temp_dir().join(format!(
        "ajax-cli-drop-execute-{}-{}",
        std::process::id(),
        "hard-delete"
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
    SqliteRegistryStore::new(&state_file)
        .save(&context.registry)
        .unwrap();
    let mut runner = QueuedRunner::new(vec![
        output(0, ""),
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
        ),
        output(0, "main\n"),
        output(0, ""),
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
        ),
        output(0, "main\n"),
    ]);
    let output = run_with_context_paths_and_runner(
        ["ajax", "drop", "web/fix-login", "--execute", "--yes"],
        &CliContextPaths::new(&config_file, &state_file),
        &mut runner,
    )
    .unwrap();
    let restored = SqliteRegistryStore::new(&state_file).load().unwrap();
    std::fs::remove_dir_all(Path::new(&directory)).unwrap();
    assert_eq!(output, "removed task: web/fix-login");
    assert!(restored.get_task(&task_id).is_none());
}
#[test]
fn drop_execute_hard_remove_survives_subsequent_tasks_read() {
    let directory = std::env::temp_dir().join(format!(
        "ajax-cli-drop-tasks-read-{}-{}",
        std::process::id(),
        "hard-delete"
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
    SqliteRegistryStore::new(&state_file)
        .save(&context.registry)
        .unwrap();
    let paths = CliContextPaths::new(&config_file, &state_file);
    let mut drop_runner = QueuedRunner::new(vec![
        output(0, ""),
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
        ),
        output(0, "main\n"),
        output(0, ""),
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
        ),
        output(0, "main\n"),
    ]);
    run_with_context_paths_and_runner(
        ["ajax", "drop", "web/fix-login", "--execute", "--yes"],
        &paths,
        &mut drop_runner,
    )
    .unwrap();
    let tasks_output = run_with_context_paths_and_runner(
        ["ajax", "tasks", "--json"],
        &paths,
        &mut QueuedRunner::new(vec![]),
    )
    .unwrap();
    let restored = SqliteRegistryStore::new(&state_file).load().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&tasks_output).unwrap();
    std::fs::remove_dir_all(Path::new(&directory)).unwrap();
    assert_eq!(parsed["tasks"].as_array().unwrap().len(), 0);
    assert!(restored.get_task(&task_id).is_none());
    assert!(restored.list_tasks().is_empty());
}
#[test]
fn drop_parses_as_executable_task_command() {
    let matches = build_cli()
        .try_get_matches_from(["ajax", "drop", "web/fix-login", "--execute", "--yes"])
        .unwrap_or_else(|error| panic!("drop should parse: {error}"));
    let Some((name, subcommand)) = matches.subcommand() else {
        panic!("drop should parse as a subcommand");
    };
    assert_eq!(name, "drop");
    assert_eq!(
        subcommand.get_one::<String>("task").map(String::as_str),
        Some("web/fix-login")
    );
    assert!(subcommand.get_flag("execute"));
    assert!(subcommand.get_flag("yes"));
}
#[test]
fn pending_cockpit_merge_returns_to_ajax() {
    let mut merge_context = safe_merge_context();
    let mut merge_runner = QueuedRunner::new(vec![output(0, ""), output(0, "merged\n")]);
    let mut state_changed = false;
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: "ship".to_string(),
        task_title: None,
    };
    let outcome = execute_pending_cockpit_action(
        &pending,
        &mut merge_context,
        &mut merge_runner,
        &mut state_changed,
    )
    .unwrap();
    assert_eq!(outcome, None);
    assert_eq!(
        merge_context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::Merged
    );
    assert!(state_changed);
}
#[test]
fn cockpit_remove_action_requires_confirmation_before_running() {
    let mut context = sample_context();
    let item = ajax_core::models::CockpitActionItem {
        task_id: TaskId::new("__task_action__web_fix_login__remove"),
        task_handle: "web/fix-login".to_string(),
        reason: "Remove task".to_string(),
        priority: 0,
        action: "drop".to_string(),
    };
    let outcome = tui_cockpit_action(&item, &mut context).unwrap();
    assert!(matches!(
        outcome,
        ajax_tui::ActionOutcome::Confirm(message)
            if message == "press enter again to confirm drop"
    ));
}
#[test]
fn confirmed_cockpit_remove_action_optimistically_removes_and_defers_cleanup() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
    task.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/fix-login".to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: true,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: None,
    });
    let item = ajax_core::models::CockpitActionItem {
        task_id: TaskId::new("__task_action__web_fix_login__remove"),
        task_handle: "web/fix-login".to_string(),
        reason: "Remove task".to_string(),
        priority: 0,
        action: "drop".to_string(),
    };
    let outcome = tui_cockpit_confirmed_action(&item, &mut context).unwrap();
    let ajax_tui::ActionOutcome::RefreshAndDefer(snapshot, pending) = outcome else {
        panic!("confirmed force drop should optimistically refresh and defer cleanup");
    };
    assert!(snapshot.cards.is_empty());
    assert!(snapshot.inbox.items.is_empty());
    assert_eq!(pending.task_handle, "web/fix-login");
    assert_eq!(pending.action, "drop");
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::Reviewable
    );
}
#[test]
fn cockpit_mismatch_repair_prompts_for_exact_branch_adoption() {
    let mut context = sample_context_with_named_checkout_mismatch();
    let item = cockpit_item("web/fix-login", "repair");
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
    let plan = retained_plan.expect("first repair activation should retain the core plan");
    assert!(plan.commands.is_empty());
    assert!(plan.requires_confirmation);
    let adoption = plan
        .branch_adoption
        .as_ref()
        .expect("repair confirmation should retain typed adoption");
    assert_eq!(adoption.expected_branch, "ajax/fix-login");
    assert_eq!(adoption.observed_branch, "fix/pane-stuck");
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .branch,
        "ajax/fix-login"
    );
}
#[test]
fn confirmed_cockpit_mismatch_repair_adopts_original_plan_without_commands() {
    let mut context = sample_context_with_named_checkout_mismatch();
    let task_before = context
        .registry
        .get_task(&TaskId::new("task-1"))
        .unwrap()
        .clone();
    let item = cockpit_item("web/fix-login", "repair");
    let mut retained_plan = None;
    cockpit_actions::cockpit_action_outcome(&item, &mut context, false, &mut retained_plan)
        .unwrap();
    let outcome = cockpit_actions::cockpit_action_outcome(
        &item,
        &mut context,
        true,
        &mut retained_plan,
    )
    .unwrap();
    let ajax_tui::ActionOutcome::Defer(pending) = outcome else {
        panic!("confirmed mismatch repair should defer the retained plan");
    };
    assert_eq!(pending.action, "repair");
    assert_eq!(pending.task_handle, "web/fix-login");
    let mut runner = RecordingCommandRunner::default();
    let mut task_session = RecordingTaskSessionRunner::default();
    let mut state_changed = false;
    let result = cockpit_actions::execute_pending_cockpit_action_with_task_session(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
        &mut task_session,
        retained_plan.as_ref(),
    )
    .unwrap();
    assert_eq!(
        result,
        cockpit_actions::PendingCockpitExecution::Continue(None)
    );
    assert!(runner.commands().is_empty());
    assert!(task_session.commands.is_empty());
    assert!(state_changed);
    let task_after = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_eq!(task_after.branch, "fix/pane-stuck");
    assert!(!task_after.has_checkout_mismatch());
    assert_eq!(task_after.id, task_before.id);
    assert_eq!(task_after.worktree_path, task_before.worktree_path);
    assert_eq!(task_after.tmux_session, task_before.tmux_session);
}
#[test]
fn stale_cockpit_mismatch_confirmation_does_not_adopt_changed_checkout() {
    let mut context = sample_context_with_named_checkout_mismatch();
    let item = cockpit_item("web/fix-login", "repair");
    let mut retained_plan = None;
    cockpit_actions::cockpit_action_outcome(&item, &mut context, false, &mut retained_plan)
        .unwrap();
    let outcome = cockpit_actions::cockpit_action_outcome(
        &item,
        &mut context,
        true,
        &mut retained_plan,
    )
    .unwrap();
    let ajax_tui::ActionOutcome::Defer(pending) = outcome else {
        panic!("confirmed mismatch repair should defer execution");
    };
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .git_status
        .as_mut()
        .unwrap()
        .current_branch = Some("other/branch".to_string());
    let event_count_before = context
        .registry
        .events_for_task(&TaskId::new("task-1"))
        .len();
    let mut runner = RecordingCommandRunner::default();
    let mut task_session = RecordingTaskSessionRunner::default();
    let mut state_changed = false;
    let error = cockpit_actions::execute_pending_cockpit_action_with_task_session(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
        &mut task_session,
        retained_plan.as_ref(),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("checkout changed since repair was planned; refresh and retry"),
        "unexpected error: {error}"
    );
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
    assert!(runner.commands().is_empty());
    assert!(task_session.commands.is_empty());
    assert!(!state_changed);
}

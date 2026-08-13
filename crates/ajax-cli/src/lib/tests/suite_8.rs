#[test]
fn remove_execute_force_removes_task_resources() {
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
    let mut runner = QueuedRunner::new(vec![
            output(0, "ajax-web-fix-login\n"),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n",
            ),
            output(0, "main\najax/fix-login\n"),
        output(0, "origin/main\norigin/ajax/fix-login\n"),
        output(0, ""),
            output(0, ""),
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
        ["ajax", "drop", "web/fix-login", "--execute", "--yes"],
        &mut context,
        &mut runner,
    )
    .unwrap();
    assert_eq!(runner.commands.len(), 11);
    assert_eq!(
        runner.commands.iter().filter(|command| **command == git_list_remote_branches_command()).count(),
        2
    );
    assert_eq!(
        runner.commands[6],
        CommandSpec::new("tmux", ["kill-session", "-t", "ajax-web-fix-login"])
    );
    assert!(runner.commands.iter().any(|command| {
        command.program == "sh" && command.args.get(2) == Some(&"ajax-delete-branch".to_string())
    }));
    assert!(context.registry.get_task(&TaskId::new("task-1")).is_none());
}
#[test]
fn clean_execute_requires_yes_for_risky_task_without_running() {
    let mut context = cleanable_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    let git_status = task.git_status.as_mut().unwrap();
    git_status.dirty = true;
    git_status.merged = false;
    git_status.unpushed_commits = 1;
    let mut runner = RecordingCommandRunner::default();
    let error = run_with_context_and_runner(
        ["ajax", "drop", "web/fix-login", "--execute"],
        &mut context,
        &mut runner,
    )
    .unwrap_err();
    assert_eq!(
        error,
        CliError::CommandFailed("confirmation required; pass --yes".to_string())
    );
    assert!(runner.commands().is_empty());
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
fn clean_execute_removes_risky_task_with_yes() {
    let mut context = cleanable_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    let git_status = task.git_status.as_mut().unwrap();
    git_status.dirty = true;
    git_status.merged = false;
    git_status.unpushed_commits = 1;
    let mut runner = QueuedRunner::new(vec![
            output(0, ""),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n",
            ),
            output(0, "main\najax/fix-login\n"),
        output(0, "origin/main\norigin/ajax/fix-login\n"),
        output(0, ""),
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
        ["ajax", "drop", "web/fix-login", "--execute", "--yes"],
        &mut context,
        &mut runner,
    )
    .unwrap();
    assert_present_cleanable_force_drop_commands(&runner.commands);
    assert!(context.registry.get_task(&TaskId::new("task-1")).is_none());
}
#[test]
fn drop_execute_continues_when_tmux_session_is_already_missing() {
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
            output(0, ""),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n",
            ),
            output(0, "main\najax/fix-login\n"),
        output(0, "origin/main\norigin/ajax/fix-login\n"),
        output(0, ""),
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
        &mut runner,
    )
    .unwrap();
    assert_present_cleanable_force_drop_commands(&runner.commands);
    assert!(context.registry.get_task(&TaskId::new("task-1")).is_none());
}
#[test]
fn drop_execute_kills_live_tmux_when_registry_cache_says_absent() {
    let mut context = cleanable_context();
    let task_id = TaskId::new("task-1");
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
    context
        .registry
        .update_git_status(
            &task_id,
            GitStatus {
                worktree_exists: false,
                branch_exists: false,
                current_branch: None,
                dirty: false,
                ahead: 0,
                behind: 0,
                merged: true,
                untracked_files: 0,
                unpushed_commits: 0,
                conflicted: false,
                last_commit: None,
            },
        )
        .unwrap();
    let mut runner = QueuedRunner::new(vec![
        output(0, "ajax-web-fix-login\n"),
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
        ),
        output(0, "main\n"),
        output(0, "origin/main\n"),
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
        ["ajax", "drop", "web/fix-login", "--execute", "--yes"],
        &mut context,
        &mut runner,
    )
    .unwrap();
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
            git_list_remote_branches_command(),
            CommandSpec::new("tmux", ["kill-session", "-t", "ajax-web-fix-login"]),
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
            git_list_remote_branches_command(),
        ]
    );
    assert!(context.registry.get_task(&task_id).is_none());
}
#[test]
fn drop_execute_continues_when_worktree_is_already_missing() {
    let mut context = cleanable_context();
    let mut runner = QueuedRunner::new(vec![
        output(0, ""),
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
        &mut runner,
    )
    .unwrap();
    assert_eq!(runner.commands.len(), 9);
    assert_eq!(runner.commands[3], git_list_remote_branches_command());
    assert_eq!(runner.commands[4].program, "sh");
    assert_eq!(runner.commands[4].args[2], "ajax-delete-branch");
    assert_eq!(runner.commands[8], git_list_remote_branches_command());
    assert!(context.registry.get_task(&TaskId::new("task-1")).is_none());
}
#[test]
fn drop_execute_completes_when_branch_is_already_missing() {
    let mut context = cleanable_context();
    let mut runner = QueuedRunner::new(vec![
            output(0, ""),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/main\n\n",
            ),
            output(0, "main\n"),
        output(0, "origin/main\n"),
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
        &mut runner,
    )
    .unwrap();
    // Path-only Present still force-removes a drifted checkout even when the
    // ajax/* branch is already gone.
    assert_eq!(runner.commands.len(), 9);
    assert_eq!(runner.commands[4].program, "sh");
    assert_eq!(runner.commands[4].args[2], "ajax-fast-worktree-remove");
    assert!(!runner.commands.iter().any(|command| {
        command.program == "sh" && command.args.get(2) == Some(&"ajax-delete-branch".to_string())
    }));
    assert!(context.registry.get_task(&TaskId::new("task-1")).is_none());
}
#[test]
fn drop_execute_treats_missing_resource_stderr_variants_as_already_absent() {
    let mut context = cleanable_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
    let mut runner = QueuedRunner::new(vec![
            output(0, "ajax-web-fix-login\n"),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n",
            ),
            output(0, "main\najax/fix-login\n"),
        output(0, "origin/main\norigin/ajax/fix-login\n"),
        CommandOutput {
                status_code: 128,
                stdout: String::new(),
                stderr: "fatal: '/tmp/worktrees/web-fix-login' is not a worktree".to_string(),
            },
            CommandOutput {
                status_code: 1,
                stdout: String::new(),
                stderr: "error: branch 'ajax/fix-login' not found.".to_string(),
            },
            CommandOutput {
                status_code: 1,
                stdout: String::new(),
                stderr: "no server running on /tmp/tmux-501/default".to_string(),
            },
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
        &mut runner,
    )
    .unwrap();
    assert!(context.registry.get_task(&TaskId::new("task-1")).is_none());
}
#[test]
fn drop_execute_treats_no_such_branch_as_already_absent() {
    let mut context = cleanable_context();
    let mut runner = QueuedRunner::new(vec![
            output(0, ""),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n",
            ),
            output(0, "main\najax/fix-login\n"),
        output(0, "origin/main\norigin/ajax/fix-login\n"),
        output(0, ""),
            CommandOutput {
                status_code: 1,
                stdout: String::new(),
                stderr: "error: no such branch 'ajax/fix-login'".to_string(),
            },
            output(0, ""),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
            ),
            output(0, "main\n"),
        output(0, "origin/main\n"),
        ]);
    run_with_context_and_runner(
        ["ajax", "drop", "web/fix-login", "--execute", "--yes"],
        &mut context,
        &mut runner,
    )
    .unwrap();
    assert!(context.registry.get_task(&TaskId::new("task-1")).is_none());
}

#[test]
fn start_execute_persists_task_before_first_external_command() {
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
                        .any(|task| task.qualified_handle() == "web/fix-login"),
                    "start task should be durable before the first external command"
                );
            }
            self.outputs
                .pop_front()
                .ok_or_else(|| CommandRunError::SpawnFailed("missing queued output".to_string()))
        }
    }
    let directory = std::env::temp_dir().join(format!(
        "ajax-cli-new-execute-{}-{}",
        std::process::id(),
        "pre-external"
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
    run_with_context_paths_and_runner(
        [
            "ajax",
            "start",
            "--repo",
            "web",
            "--title",
            "Fix login",
            "--execute",
            "--yes",
        ],
        &CliContextPaths::new(&config_file, &state_file),
        &mut runner,
    )
    .unwrap();
    std::fs::remove_dir_all(Path::new(&directory)).unwrap();
    assert!(runner.checked);
}
#[test]
fn new_execute_persists_state_when_open_after_create_fails() {
    let directory = std::env::temp_dir().join(format!(
        "ajax-cli-new-execute-{}-{}",
        std::process::id(),
        "open-failure"
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
    let mut runner = QueuedRunner::new(vec![
        output(0, ""),
        output(0, ""),
        output(0, ""),
        output(0, ""),
        output(0, ""),
        CommandOutput {
            status_code: 42,
            stdout: String::new(),
            stderr: "attach failed".to_string(),
        },
    ]);
    let error = run_with_context_paths_and_runner(
        [
            "ajax",
            "start",
            "--repo",
            "web",
            "--title",
            "Fix login",
            "--execute",
        ],
        &CliContextPaths::new(&config_file, &state_file),
        &mut runner,
    )
    .unwrap_err();
    let restored = SqliteRegistryStore::new(&state_file).load().unwrap();
    std::fs::remove_dir_all(Path::new(&directory)).unwrap();
    assert!(
        matches!(error, CliError::CommandFailedAfterStateChange(message)
                if message == "command failed: tmux exited with status 42: attach failed")
    );
    let task = restored
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == "web/fix-login")
        .expect("state-changing create error should persist task");
    assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
    assert_eq!(task.agent_attempts.len(), 1);
}
#[test]
fn open_execute_marks_task_active() {
    let mut context = sample_context();
    let mut runner = RecordingCommandRunner::default();
    run_with_context_and_runner(
        ["ajax", "resume", "web/fix-login", "--execute"],
        &mut context,
        &mut runner,
    )
    .unwrap();
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
fn merge_execute_requires_yes_before_marking_merged() {
    let mut context = sample_context();
    let mut runner = RecordingCommandRunner::default();
    let error = run_with_context_and_runner(
        ["ajax", "ship", "web/fix-login", "--execute"],
        &mut context,
        &mut runner,
    )
    .unwrap_err();
    assert_eq!(
        error,
        CliError::CommandFailed("confirmation required; pass --yes".to_string())
    );
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
fn failed_merge_records_attention_without_lifecycle_change() {
    let mut context = sample_context();
    let mut runner = QueuedRunner::new(vec![output(0, ""), output(42, "")]);
    let error = run_with_context_and_runner(
        ["ajax", "ship", "web/fix-login", "--execute", "--yes"],
        &mut context,
        &mut runner,
    )
    .unwrap_err();
    assert!(
        matches!(error, CliError::CommandFailedAfterStateChange(message)
                if message == "command failed: git exited with status 42")
    );
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_eq!(task.lifecycle_status, LifecycleStatus::Reviewable);
    assert_eq!(
        task.live_status
            .as_ref()
            .map(|status| (status.kind, status.summary.as_str())),
        Some((LiveStatusKind::CommandFailed, "merge failed"))
    );
}
#[test]
fn external_command_failure_uses_operator_facing_message() {
    let mut context = sample_context();
    let mut runner = QueuedRunner::new(vec![
        output(0, ""),
        CommandOutput {
            status_code: 42,
            stdout: String::new(),
            stderr: "merge failed".to_string(),
        },
    ]);
    let error = run_with_context_and_runner(
        ["ajax", "ship", "web/fix-login", "--execute", "--yes"],
        &mut context,
        &mut runner,
    )
    .unwrap_err();
    assert_eq!(
        error,
        CliError::CommandFailedAfterStateChange(
            "command failed: git exited with status 42: merge failed".to_string()
        )
    );
}
#[test]
fn merge_execute_with_yes_marks_task_merged() {
    let mut context = sample_context();
    let mut runner = RecordingCommandRunner::default();
    run_with_context_and_runner(
        ["ajax", "ship", "web/fix-login", "--execute", "--yes"],
        &mut context,
        &mut runner,
    )
    .unwrap();
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::Merged
    );
}
#[test]
fn merge_execute_refreshes_git_evidence_before_merge_commands() {
    let mut context = sample_context();
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .git_status = Some(GitStatus {
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
    let mut runner = RecordingCommandRunner::default();
    run_with_context_and_runner(
        ["ajax", "ship", "web/fix-login", "--execute", "--yes"],
        &mut context,
        &mut runner,
    )
    .unwrap();
    assert_eq!(
        runner.commands().first(),
        Some(&CommandSpec::new(
            "git",
            [
                "-C",
                "/tmp/worktrees/web-fix-login",
                "status",
                "--porcelain=v1",
                "--branch"
            ]
        ))
    );
}
#[test]
fn clean_execute_hard_removes_task() {
    let mut context = cleanable_context();
    let mut runner = RecordingCommandRunner::default();
    run_with_context_and_runner(
        ["ajax", "drop", "web/fix-login", "--execute", "--yes"],
        &mut context,
        &mut runner,
    )
    .unwrap();
    assert!(context.registry.get_task(&TaskId::new("task-1")).is_none());
}
#[test]
fn clean_execute_collects_git_status_when_bookkeeping_is_missing() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Merged;
    task.git_status = None;
    task.remove_side_flag(SideFlag::NeedsInput);
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
    assert_eq!(
        runner.commands[0],
        CommandSpec::new("tmux", ["list-sessions", "-F", "#{session_name}"])
            .with_timeout(std::time::Duration::from_secs(8))
    );
    assert_eq!(
        runner.commands[1],
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "worktree",
                "list",
                "--porcelain"
            ]
        )
    );
    assert_eq!(
        runner.commands[2],
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "branch",
                "--format=%(refname:short)"
            ]
        )
    );
    assert_eq!(runner.commands[3], git_list_remote_branches_command());
    assert_eq!(runner.commands[4].program, "sh");
    assert_eq!(runner.commands[4].args[0], "-c");
    assert_eq!(
        runner.commands[4].args[1],
        "mkdir -p \"$(dirname \"$3\")\" && { [ ! -e \"$2\" ] || mv \"$2\" \"$3\"; } && { git -C \"$1\" worktree prune || git -C \"$1\" worktree remove --force \"$2\"; } && { rm -rf \"$3\" >/dev/null 2>&1 & }"
    );
    assert_eq!(runner.commands[4].args[2], "ajax-fast-worktree-remove");
    assert_eq!(runner.commands[4].args[3], "/Users/matt/projects/web");
    assert_eq!(runner.commands[4].args[4], "/tmp/worktrees/web-fix-login");
    assert!(runner.commands[4].args[5].starts_with("/tmp/worktrees/.ajax-trash/fix-login-"));
    assert_eq!(runner.commands[5].program, "sh");
    assert_eq!(runner.commands[5].args[2], "ajax-delete-branch");
    assert_eq!(runner.commands[5].args[4], "ajax/fix-login");
    assert_eq!(
        runner.commands[6],
        CommandSpec::new("tmux", ["list-sessions", "-F", "#{session_name}"])
            .with_timeout(std::time::Duration::from_secs(8))
    );
    assert_eq!(
        runner.commands[7],
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "worktree",
                "list",
                "--porcelain"
            ]
        )
    );
    assert_eq!(
        runner.commands[8],
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "branch",
                "--format=%(refname:short)"
            ]
        )
    );
    assert_eq!(runner.commands[9], git_list_remote_branches_command());
    assert!(context.registry.get_task(&TaskId::new("task-1")).is_none());
}
#[test]
fn clean_execute_force_removes_when_refresh_finds_missing_worktree() {
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
        ["ajax", "drop", "web/fix-login", "--execute", "--yes"],
        &mut context,
        &mut runner,
    )
    .unwrap();
    assert_eq!(runner.commands.len(), 9);
    assert_eq!(
        runner.commands[4].program,
        "sh"
    );
    assert_eq!(runner.commands[4].args[2], "ajax-delete-branch");
    assert_eq!(runner.commands[4].args[4], "ajax/fix-login");
    assert_eq!(runner.commands[3], git_list_remote_branches_command());
    assert_eq!(runner.commands[4].program, "sh");
    assert_eq!(runner.commands[4].args[2], "ajax-delete-branch");
    assert_eq!(runner.commands[8], git_list_remote_branches_command());
    assert!(context.registry.get_task(&TaskId::new("task-1")).is_none());
}
#[test]
fn cleanup_execute_force_removes_cleanable_task() {
    let mut context = cleanable_context();
    let mut runner = QueuedRunner::new(present_cleanable_drop_outputs());
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
fn remove_execute_requires_yes_before_running() {
    let mut context = sample_context();
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
        LifecycleStatus::Reviewable
    );
}

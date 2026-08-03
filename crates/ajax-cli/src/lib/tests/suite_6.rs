#[test]
fn cli_rejects_legacy_json_state_without_migration() {
    let directory = std::env::temp_dir().join(format!(
        "ajax-cli-context-{}-{}",
        std::process::id(),
        "legacy-json"
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let config_file = directory.join("config.toml");
    let state_file = directory.join("state.db");
    std::fs::write(&state_file, r#"{"tasks":[],"events":[]}"#).unwrap();
    let error = run_with_context_paths(
        ["ajax", "tasks", "--json"],
        &CliContextPaths::new(&config_file, &state_file),
    )
    .unwrap_err();
    std::fs::remove_dir_all(Path::new(&directory)).unwrap();
    assert_eq!(
        error,
        CliError::ContextLoad(format!(
            "legacy JSON state is unsupported after the SQLite rewrite; remove {} to start with fresh state",
            state_file.display()
        ))
    );
}
#[test]
fn cli_context_load_errors_do_not_expose_debug_variants() {
    let directory = std::env::temp_dir().join(format!(
        "ajax-cli-context-{}-{}",
        std::process::id(),
        "invalid-sqlite"
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let config_file = directory.join("config.toml");
    let state_file = directory.join("state.db");
    std::fs::write(&state_file, "not sqlite").unwrap();
    let error = run_with_context_paths(
        ["ajax", "tasks", "--json"],
        &CliContextPaths::new(&config_file, &state_file),
    )
    .unwrap_err();
    std::fs::remove_dir_all(Path::new(&directory)).unwrap();
    let message = match error {
        CliError::ContextLoad(message) => message,
        other => panic!("expected ContextLoad, got {other:?}"),
    };
    assert!(
        message.starts_with("state load failed: database error:"),
        "{message}"
    );
    assert_eq!(message.find("Database("), None, "{message}");
}
#[test]
fn new_execute_records_task_in_registry_after_runner_succeeds() {
    let mut context = CommandContext::new(
        Config {
            repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    );
    let mut runner = RecordingCommandRunner::default();
    let output = run_start_with_attach_mode(
        [
            "ajax",
            "start",
            "--repo",
            "web",
            "--title",
            "Fix login",
            "--agent",
            "codex",
            "--execute",
        ],
        &mut context,
        &mut runner,
    )
    .unwrap();
    assert_eq!(
        output
            .lines()
            .find(|line| line.starts_with("recorded task:")),
        Some("recorded task: web/fix-login")
    );
    let mut expected_commands =
        expected_sync_default_branch_commands("/Users/matt/projects/web", "main");
    expected_commands.extend([
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "worktree",
                "add",
                "-b",
                "ajax/fix-login",
                "/Users/matt/projects/web__worktrees/ajax-fix-login",
                "origin/main",
            ],
        ),
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
                "/Users/matt/projects/web__worktrees/ajax-fix-login",
            ],
        ),
        expected_task_setup_command(
            "/Users/matt/projects/web",
            "/Users/matt/projects/web__worktrees/ajax-fix-login",
            None,
        ),
        expected_task_launch_command(
            "ajax-web-fix-login",
            "web/fix-login",
            "/Users/matt/projects/web__worktrees/ajax-fix-login",
        ),
        CommandSpec::new("tmux", ["select-window", "-t", "ajax-web-fix-login:task"]),
        expected_new_task_open_command("ajax-web-fix-login"),
    ]);
    assert_eq!(runner.commands(), expected_commands.as_slice());
    let recorded = context
        .registry
        .list_tasks()
        .iter()
        .find(|task| task.qualified_handle() == "web/fix-login")
        .cloned()
        .expect("start task should be recorded");
    assert_eq!(
        recorded.worktree_path.to_string_lossy(),
        "/Users/matt/projects/web__worktrees/ajax-fix-login"
    );
    assert_eq!(recorded.lifecycle_status, LifecycleStatus::Active);
    assert_eq!(recorded.agent_attempts.len(), 1);
    assert_eq!(
        recorded.agent_attempts[0].launch_target,
        "/Users/matt/projects/web__worktrees/ajax-fix-login"
    );
}
#[test]
fn new_execute_runs_repo_bootstrap_in_worktree_before_agent_launch() {
    let mut repo = ManagedRepo::new("web", "/Users/matt/projects/web", "main");
    repo.bootstrap = Some("npm ci".to_string());
    let mut context = CommandContext::new(
        Config {
            repos: vec![repo],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    );
    let mut runner = RecordingCommandRunner::default();
    run_start_with_attach_mode(
        [
            "ajax",
            "start",
            "--repo",
            "web",
            "--title",
            "Fix login",
            "--agent",
            "codex",
            "--execute",
        ],
        &mut context,
        &mut runner,
    )
    .unwrap();
    let mut expected_commands =
        expected_sync_default_branch_commands("/Users/matt/projects/web", "main");
    expected_commands.extend([
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "worktree",
                "add",
                "-b",
                "ajax/fix-login",
                "/Users/matt/projects/web__worktrees/ajax-fix-login",
                "origin/main",
            ],
        ),
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
                "/Users/matt/projects/web__worktrees/ajax-fix-login",
            ],
        ),
        expected_task_setup_command(
            "/Users/matt/projects/web",
            "/Users/matt/projects/web__worktrees/ajax-fix-login",
            Some("npm ci"),
        ),
        expected_task_launch_command(
            "ajax-web-fix-login",
            "web/fix-login",
            "/Users/matt/projects/web__worktrees/ajax-fix-login",
        ),
        CommandSpec::new("tmux", ["select-window", "-t", "ajax-web-fix-login:task"]),
        expected_new_task_open_command("ajax-web-fix-login"),
    ]);
    assert_eq!(runner.commands(), expected_commands.as_slice());
}
#[test]
fn new_execute_rejects_existing_task_before_native_provisioning() {
    let mut context = sample_context();
    let mut runner = RecordingCommandRunner::default();
    let error = run_with_context_and_runner(
        [
            "ajax",
            "start",
            "--repo",
            "web",
            "--title",
            "Fix login",
            "--execute",
        ],
        &mut context,
        &mut runner,
    )
    .unwrap_err();
    assert_eq!(
        error,
        CliError::CommandFailed(
            "plan blocked: task already exists: web/fix-login".to_string()
        )
    );
    assert!(runner.commands().is_empty());
}
#[test]
fn new_execute_provisioning_failure_records_visible_partial_state() {
    let mut context = CommandContext::new(
        Config {
            repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    );
    let mut runner = QueuedRunner::new(vec![
        output(0, ""),
        output(0, ""),
        CommandOutput {
            status_code: 42,
            stdout: String::new(),
            stderr: "tmux failed".to_string(),
        },
    ]);
    let error = run_with_context_and_runner(
        [
            "ajax",
            "start",
            "--repo",
            "web",
            "--title",
            "Fix login",
            "--execute",
        ],
        &mut context,
        &mut runner,
    )
    .unwrap_err();
    assert!(
        matches!(error, CliError::CommandFailedAfterStateChange(message)
                if message == "command failed: tmux exited with status 42: tmux failed")
    );
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == "web/fix-login")
        .expect("provisioning task should remain visible");
    assert_eq!(task.lifecycle_status, LifecycleStatus::Error);
    assert!(task
        .git_status
        .as_ref()
        .is_some_and(|status| { status.worktree_exists && status.branch_exists }));
    assert_eq!(task.tmux_status, None);
    let mut expected_commands =
        expected_sync_default_branch_commands("/Users/matt/projects/web", "main");
    expected_commands.extend([
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "worktree",
                "add",
                "-b",
                "ajax/fix-login",
                "/Users/matt/projects/web__worktrees/ajax-fix-login",
                "origin/main",
            ],
        ),
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
                "/Users/matt/projects/web__worktrees/ajax-fix-login",
            ],
        ),
    ]);
    assert_eq!(runner.commands, expected_commands);
}
#[test]
fn new_execute_bootstrap_failure_records_error_without_launching_agent() {
    let mut repo = ManagedRepo::new("web", "/Users/matt/projects/web", "main");
    repo.bootstrap = Some("npm ci".to_string());
    let mut context = CommandContext::new(
        Config {
            repos: vec![repo],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    );
    let mut runner = QueuedRunner::new(vec![
        output(0, ""),
        output(0, ""),
        output(0, ""),
        CommandOutput {
            status_code: 42,
            stdout: String::new(),
            stderr: "npm failed".to_string(),
        },
    ]);
    let error = run_with_context_and_runner(
        [
            "ajax",
            "start",
            "--repo",
            "web",
            "--title",
            "Fix login",
            "--execute",
        ],
        &mut context,
        &mut runner,
    )
    .unwrap_err();
    assert!(
        matches!(error, CliError::CommandFailedAfterStateChange(message)
                if message == "command failed: sh exited with status 42 in /Users/matt/projects/web: npm failed")
    );
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == "web/fix-login")
        .expect("provisioning task should remain visible");
    assert_eq!(task.lifecycle_status, LifecycleStatus::Error);
    assert!(task.has_side_flag(SideFlag::NeedsInput));
    assert!(task.agent_attempts.is_empty());
    let mut expected_commands =
        expected_sync_default_branch_commands("/Users/matt/projects/web", "main");
    expected_commands.extend([
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "worktree",
                "add",
                "-b",
                "ajax/fix-login",
                "/Users/matt/projects/web__worktrees/ajax-fix-login",
                "origin/main",
            ],
        ),
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
                "/Users/matt/projects/web__worktrees/ajax-fix-login",
            ],
        ),
        expected_task_setup_command(
            "/Users/matt/projects/web",
            "/Users/matt/projects/web__worktrees/ajax-fix-login",
            Some("npm ci"),
        ),
    ]);
    assert_eq!(runner.commands, expected_commands);
}
#[test]
fn new_execute_records_provisioning_task_before_first_command_failure() {
    let mut context = CommandContext::new(
        Config {
            repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    );
    let mut runner = QueuedRunner::new(vec![CommandOutput {
        status_code: 42,
        stdout: String::new(),
        stderr: "git failed".to_string(),
    }]);
    let error = run_with_context_and_runner(
        [
            "ajax",
            "start",
            "--repo",
            "web",
            "--title",
            "Fix login",
            "--execute",
        ],
        &mut context,
        &mut runner,
    )
    .unwrap_err();
    assert!(
        matches!(error, CliError::CommandFailedAfterStateChange(message)
                if message == "command failed: git exited with status 42: git failed")
    );
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == "web/fix-login")
        .expect("provisioning task should be visible after first command failure");
    assert_eq!(task.lifecycle_status, LifecycleStatus::Error);
    assert_eq!(ajax_core::commands::inbox(&context).items.len(), 1);
}
#[test]
fn new_execute_allows_reusing_removed_task_handle() {
    let mut context = CommandContext::new(
        Config {
            repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    );
    let mut removed = Task::new(
        TaskId::new("web/fix-login"),
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
    removed.lifecycle_status = LifecycleStatus::Removed;
    context.registry.create_task(removed).unwrap();
    let mut runner = RecordingCommandRunner::default();
    let output = run_start_with_attach_mode(
        [
            "ajax",
            "start",
            "--repo",
            "web",
            "--title",
            "Fix login",
            "--execute",
        ],
        &mut context,
        &mut runner,
    )
    .unwrap();
    assert_eq!(
        output
            .lines()
            .find(|line| line.starts_with("recorded task:")),
        Some("recorded task: web/fix-login")
    );
    let mut expected_commands =
        expected_sync_default_branch_commands("/Users/matt/projects/web", "main");
    expected_commands.extend([
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "worktree",
                "add",
                "-b",
                "ajax/fix-login",
                "/Users/matt/projects/web__worktrees/ajax-fix-login",
                "origin/main",
            ],
        ),
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
                "/Users/matt/projects/web__worktrees/ajax-fix-login",
            ],
        ),
        expected_task_setup_command(
            "/Users/matt/projects/web",
            "/Users/matt/projects/web__worktrees/ajax-fix-login",
            None,
        ),
        expected_task_launch_command(
            "ajax-web-fix-login",
            "web/fix-login",
            "/Users/matt/projects/web__worktrees/ajax-fix-login",
        ),
        CommandSpec::new("tmux", ["select-window", "-t", "ajax-web-fix-login:task"]),
        expected_new_task_open_command("ajax-web-fix-login"),
    ]);
    assert_eq!(runner.commands(), expected_commands.as_slice());
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::Active
    );
}
#[test]
fn new_execute_requires_task_title_before_native_provisioning() {
    let mut context = sample_context();
    let mut runner = RecordingCommandRunner::default();
    let error = run_with_context_and_runner(
        ["ajax", "start", "--repo", "web", "--execute"],
        &mut context,
        &mut runner,
    )
    .unwrap_err();
    assert_eq!(
        error,
        CliError::CommandFailed("task title is required; pass --title".to_string())
    );
    assert!(runner.commands().is_empty());
}
#[test]
fn new_execute_saves_registry_to_sqlite_state_file() {
    let directory = std::env::temp_dir().join(format!(
        "ajax-cli-new-execute-{}-{}",
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
    let mut runner = RecordingCommandRunner::default();
    let output = run_with_context_paths_and_runner(
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
    .unwrap();
    let restored = SqliteRegistryStore::new(&state_file).load().unwrap();
    std::fs::remove_dir_all(Path::new(&directory)).unwrap();
    assert_eq!(
        output
            .lines()
            .find(|line| line.starts_with("recorded task:")),
        Some("recorded task: web/fix-login")
    );
    let recorded = restored
        .list_tasks()
        .iter()
        .find(|task| task.qualified_handle() == "web/fix-login")
        .cloned()
        .expect("start task should be persisted");
    assert_eq!(
        recorded.worktree_path.to_string_lossy(),
        "/Users/matt/projects/web__worktrees/ajax-fix-login"
    );
}

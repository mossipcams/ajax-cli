#[test]
fn command_flow_fixture_records_partial_success_before_failure() {
    let mut plan = ajax_core::commands::CommandPlan::new("partial failure");
    plan.commands.push(CommandSpec::new("git", ["status"]));
    plan.commands.push(CommandSpec::new(
        "tmux",
        ["attach-session", "-t", "missing"],
    ));
    let mut runner = command_flow_runner(vec![
        output(0, "clean"),
        CommandOutput {
            status_code: 7,
            stdout: String::new(),
            stderr: "missing session".to_string(),
        },
    ]);
    let error = ajax_core::commands::execute_plan(&plan, true, &mut runner).unwrap_err();
    assert_eq!(
        error,
        ajax_core::commands::CommandError::CommandRun(CommandRunError::NonZeroExit {
            program: "tmux".to_string(),
            status_code: 7,
            stderr: "missing session".to_string(),
            cwd: None,
        })
    );
    assert_eq!(
        runner.commands,
        vec![
            CommandSpec::new("git", ["status"]),
            CommandSpec::new("tmux", ["attach-session", "-t", "missing"]),
        ]
    );
}
/// Working lifecycle opens the AoE running-reconcile capture gate for Codex.
/// Runtime refresh now ends with a GitHub PR check probe for the fixture
/// task; sequence-asserting tests append this to `tmux_live_commands()`.
// `run_with_context_and_runner` resolves the open mode from the ambient
// `$TMUX` env var, which makes full command-sequence assertions
// non-deterministic across environments. Pin `Attach` through the dispatch
// seam so expectations stay hermetic.
#[test]
fn cli_error_display_omits_internal_enum_wrapping() {
    let error = CliError::CommandFailed("task title is required; pass --title".to_string());
    assert_eq!(error.to_string(), "task title is required; pass --title");
    assert_eq!(error.to_string().find("CommandFailed"), None);
}
#[test]
fn binary_prints_cli_errors_with_display_formatting() {
    let directory =
        std::env::temp_dir().join(format!("ajax-cli-empty-context-{}", std::process::id()));
    let output = std::process::Command::new(ajax_binary_path())
        .args(["start", "--execute"])
        .env("AJAX_CONFIG", directory.join("missing-config.toml"))
        .env("AJAX_STATE", directory.join("missing-state.db"))
        .output()
        .unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(!output.status.success());
    assert_eq!(stderr.trim(), "task title is required; pass --title");
}
#[test]
fn reads_use_only_the_selected_profile_db() {
    let (directory, stable_paths, dev_paths) = seeded_profile_homes("selected-db-read");
    let mut read_runner = RecordingCommandRunner::default();
    let dev_output = run_with_context_paths_and_runner(
        ["ajax-cli", "tasks", "--json"],
        &dev_paths,
        &mut read_runner,
    )
    .unwrap();
    let dev_parsed: serde_json::Value = serde_json::from_str(&dev_output).unwrap();
    let dev_handles = dev_parsed["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["qualified_handle"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(dev_handles, vec!["web/dev-task"]);
    let mut stable_read_runner = RecordingCommandRunner::default();
    let stable_output = run_with_context_paths_and_runner(
        ["ajax-cli", "tasks", "--json"],
        &stable_paths,
        &mut stable_read_runner,
    )
    .unwrap();
    let stable_parsed: serde_json::Value = serde_json::from_str(&stable_output).unwrap();
    let stable_handles = stable_parsed["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["qualified_handle"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(stable_handles, vec!["web/stable-task"]);
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn writes_persist_only_to_the_selected_profile_db() {
    let (directory, stable_paths, dev_paths) = seeded_profile_homes("selected-db-write");
    let mut write_runner = RecordingCommandRunner::default();
    run_with_context_paths_and_runner(
        [
            "ajax-cli",
            "start",
            "--repo",
            "web",
            "--title",
            "new dev task",
            "--agent",
            "codex",
            "--execute",
        ],
        &dev_paths,
        &mut write_runner,
    )
    .unwrap();
    let stable_after = SqliteRegistryStore::new(&stable_paths.state_file)
        .load()
        .unwrap();
    let dev_after = SqliteRegistryStore::new(&dev_paths.state_file)
        .load()
        .unwrap();
    assert!(stable_after
        .list_tasks()
        .iter()
        .any(|task| task.qualified_handle() == "web/stable-task"));
    assert!(!stable_after
        .list_tasks()
        .iter()
        .any(|task| task.qualified_handle() == "web/new-dev-task"));
    assert!(dev_after
        .list_tasks()
        .iter()
        .any(|task| task.qualified_handle() == "web/dev-task"));
    assert!(dev_after
        .list_tasks()
        .iter()
        .any(|task| task.qualified_handle() == "web/new-dev-task"));
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn writer_entrypoint_uses_selected_runtime_paths() {
    let directory = std::env::temp_dir().join(format!(
        "ajax-cli-writer-selected-db-{}",
        std::process::id()
    ));
    let home = directory.join("home");
    let stable_paths = CliContextPaths::from_runtime_paths(
        RuntimePathRequest::new(&home)
            .with_cli_profile("stable")
            .resolve(),
    );
    let dev_paths = CliContextPaths::from_runtime_paths(
        RuntimePathRequest::new(&home)
            .with_cli_profile("dev")
            .resolve(),
    );
    let config = r#"
            [[repos]]
            name = "web"
            path = "/Users/matt/projects/web"
            default_branch = "main"
            "#;
    std::fs::create_dir_all(stable_paths.config_file.parent().unwrap()).unwrap();
    std::fs::create_dir_all(stable_paths.state_file.parent().unwrap()).unwrap();
    std::fs::create_dir_all(dev_paths.config_file.parent().unwrap()).unwrap();
    std::fs::write(&stable_paths.config_file, config).unwrap();
    std::fs::write(&dev_paths.config_file, config).unwrap();
    let mut stable_registry = registry_with_task("stable-task");
    let fresh_runtime = RuntimeProjection::new(
        RuntimeHealth::Healthy,
        SystemTime::now(),
        RuntimeObservationSource::TmuxProbe,
    );
    stable_registry
        .get_task_mut(&TaskId::new("web/stable-task"))
        .unwrap()
        .runtime_projection = fresh_runtime.clone();
    let mut dev_registry = registry_with_task("dev-task");
    dev_registry
        .get_task_mut(&TaskId::new("web/dev-task"))
        .unwrap()
        .runtime_projection = fresh_runtime;
    SqliteRegistryStore::new(&stable_paths.state_file)
        .save(&stable_registry)
        .unwrap();
    SqliteRegistryStore::new(&dev_paths.state_file)
        .save(&dev_registry)
        .unwrap();
    let mut output = Vec::new();
    run_with_args_to_writer(
        [
            "ajax-cli",
            "--config",
            dev_paths.config_file.to_str().unwrap(),
            "--state",
            dev_paths.state_file.to_str().unwrap(),
            "tasks",
            "--json",
        ],
        &mut output,
    )
    .unwrap();
    let output = String::from_utf8(output).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    let handles = parsed["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["qualified_handle"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(handles, vec!["web/dev-task"]);
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn writer_entrypoint_initializes_logs_dir_and_records_command() {
    let directory =
        std::env::temp_dir().join(format!("ajax-cli-writer-logging-{}", std::process::id()));
    let home = directory.join("home");
    let paths = CliContextPaths::from_runtime_paths(
        RuntimePathRequest::new(&home)
            .with_cli_home(&home)
            .with_cli_profile("dev")
            .resolve(),
    );
    let config = r#"
            [[repos]]
            name = "web"
            path = "/Users/matt/projects/web"
            default_branch = "main"
            "#;
    std::fs::create_dir_all(paths.config_file.parent().unwrap()).unwrap();
    std::fs::create_dir_all(paths.state_file.parent().unwrap()).unwrap();
    std::fs::write(&paths.config_file, config).unwrap();
    SqliteRegistryStore::new(&paths.state_file)
        .save(&InMemoryRegistry::default())
        .unwrap();
    let mut output = Vec::new();
    run_with_args_to_writer(
        [
            "ajax-cli",
            "--home",
            home.to_str().unwrap(),
            "--profile",
            "dev",
            "--config",
            paths.config_file.to_str().unwrap(),
            "--state",
            paths.state_file.to_str().unwrap(),
            "tasks",
            "--json",
        ],
        &mut output,
    )
    .unwrap();
    let output = String::from_utf8(output).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert!(parsed["tasks"].as_array().is_some());
    let log_path = paths.runtime_paths.logs_dir.join("ajax.log");
    assert!(
        log_path.is_file(),
        "expected {} to exist",
        log_path.display()
    );
    let log_contents = std::fs::read_to_string(&log_path).unwrap();
    assert!(
        log_contents.contains("command="),
        "log should contain command= field: {log_contents}"
    );
    assert!(
        log_contents.contains("tasks"),
        "log should contain subcommand name: {log_contents}"
    );
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn command_surface_includes_mvp_commands() {
    for args in [
        vec!["ajax", "repos"],
        vec!["ajax", "tasks"],
        vec!["ajax", "inspect", "web/fix-login"],
        vec!["ajax", "start"],
        vec!["ajax", "resume", "web/fix-login"],
        vec!["ajax", "repair", "web/fix-login"],
        vec!["ajax", "repair", "web/fix-login"],
        vec!["ajax", "review", "web/fix-login"],
        vec!["ajax", "ship", "web/fix-login"],
        vec!["ajax", "drop", "web/fix-login"],
        vec!["ajax", "tidy"],
        vec!["ajax", "next"],
        vec!["ajax", "inbox"],
        vec!["ajax", "ready"],
        vec!["ajax", "status"],
        vec!["ajax", "doctor"],
        vec!["ajax", "supervise", "--prompt", "fix tests"],
        vec!["ajax", "stable"],
        vec!["ajax", "dev"],
        vec!["ajax", "cockpit"],
    ] {
        let matches = build_cli().try_get_matches_from(args.clone());
        assert!(matches.is_ok(), "{args:?} should parse");
    }
}
#[test]
fn command_surface_excludes_reconcile() {
    let matches = build_cli().try_get_matches_from(["ajax", "reconcile"]);
    assert!(matches.is_err());
}
#[test]
fn cockpit_no_longer_accepts_textual_frontend_flag() {
    let matches = build_cli().try_get_matches_from(["ajax", "cockpit", "--textual"]);
    assert!(matches.is_err());
}
#[test]
fn read_only_cockpit_rejects_interactive_mode_before_navigation_only_tui() {
    let matches = build_cli()
        .try_get_matches_from(["ajax", "cockpit"])
        .unwrap();
    let Some(("cockpit", subcommand)) = matches.subcommand() else {
        panic!("expected cockpit subcommand");
    };
    let error = render_cockpit_command(&sample_context(), subcommand).unwrap_err();
    assert_eq!(
        error,
        CliError::CommandFailed(
            "interactive cockpit requires command execution support".to_string()
        )
    );
}
#[test]
fn cockpit_watch_renders_dashboard_from_backend_state() {
    let context = sample_context();
    let snapshot = crate::cockpit_backend::build_cockpit_snapshot(&context);
    assert_eq!(snapshot.cards.len(), 1);
    assert_eq!(snapshot.cards[0].qualified_handle, "web/fix-login");
    assert_eq!(
        snapshot.cards[0].status_explanation.as_deref(),
        Some("Ready for review")
    );
    assert_eq!(snapshot.inbox.items.len(), 1);
    assert_eq!(snapshot.inbox.items[0].task_handle, "web/fix-login");
    let output = run_with_context(
        [
            "ajax",
            "cockpit",
            "--watch",
            "--iterations",
            "1",
            "--interval-ms",
            "0",
        ],
        &context,
    )
    .unwrap();
    assert_eq!(output.matches("Ajax Cockpit").count(), 1);
    assert!(output.lines().any(|line| line == "Inbox"));
}
#[test]
fn cockpit_watch_renders_repeated_frames() {
    let context = sample_context();
    let output = run_with_context(
        [
            "ajax",
            "cockpit",
            "--watch",
            "--iterations",
            "2",
            "--interval-ms",
            "0",
        ],
        &context,
    )
    .unwrap();
    assert_eq!(output.matches("Ajax Cockpit").count(), 2);
}
#[test]
fn cockpit_rejects_invalid_interval() {
    let error = run_with_context(
        ["ajax", "cockpit", "--watch", "--interval-ms", "nope"],
        &sample_context(),
    )
    .unwrap_err();
    assert_eq!(
        error,
        CliError::CommandFailed("invalid --interval-ms value: nope".to_string())
    );
}
#[test]
fn cockpit_rejects_invalid_iterations() {
    let error = run_with_context(
        ["ajax", "cockpit", "--watch", "--iterations", "many"],
        &sample_context(),
    )
    .unwrap_err();
    assert_eq!(
        error,
        CliError::CommandFailed("invalid --iterations value: many".to_string())
    );
}
#[test]
fn cockpit_json_returns_single_startup_snapshot() {
    let context = sample_context();
    let output = run_with_context(["ajax", "cockpit", "--json"], &context).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["repos"]["repos"][0]["name"], "web");
    assert_eq!(
        parsed["tasks"]["tasks"][0]["qualified_handle"],
        "web/fix-login"
    );
    assert_eq!(
        parsed["review"]["tasks"][0]["qualified_handle"],
        "web/fix-login"
    );
    assert_eq!(parsed["inbox"]["items"][0]["task_handle"], "web/fix-login");
    assert_eq!(parsed["next"]["item"]["task_handle"], "web/fix-login");
}
#[test]
fn cockpit_json_refreshes_live_status_from_tmux() {
    let mut context = sample_context();
    let cache_dir = prepare_active_task_agent_status(&mut context, "task-1", "ask");
    let mut outputs = tmux_live_outputs();
    outputs.extend(ci_monitor_live_outputs());
    let mut runner = QueuedRunner::new(outputs);
    let output =
        run_with_context_and_runner(["ajax", "cockpit", "--json"], &mut context, &mut runner)
            .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(
        parsed["tasks"]["tasks"][0]["qualified_handle"],
        "web/fix-login"
    );
    assert_eq!(
        parsed["tasks"]["tasks"][0]["live_status"]["summary"],
        "waiting for approval"
    );
    assert_eq!(parsed["inbox"]["items"][0]["task_handle"], "web/fix-login");
    let mut expected = tmux_live_commands();
    extend_expected_ci_monitor_commands(&mut expected);
    assert_eq!(runner.commands, expected);
    let _ = std::fs::remove_dir_all(cache_dir);
}

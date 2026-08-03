#[test]
fn live_refresh_updates_changed_tmux_status_before_window_failure() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.remove_side_flag(SideFlag::NeedsInput);
    task.tmux_status = Some(TmuxStatus {
        exists: true,
        session_name: "stale-session".to_string(),
    });
    let mut runner = QueuedRunner::new(vec![output(0, "ajax-web-fix-login\n"), output(1, "")]);
    crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_eq!(
        task.tmux_status
            .as_ref()
            .map(|status| status.session_name.as_str()),
        Some("ajax-web-fix-login")
    );
    let expected_commands = tmux_live_commands();
    assert_eq!(
        runner.commands,
        vec![
            expected_commands[0].clone(),
            expected_commands[1].clone(),
            expected_ci_probe_command(),
        ]
    );
}
#[test]
fn live_refresh_marks_stale_present_tmux_status_missing_when_session_disappears() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.remove_side_flag(SideFlag::NeedsInput);
    task.tmux_status = Some(TmuxStatus {
        exists: true,
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
    task.task_window_status = Some(TaskWindowStatus {
        exists: true,
        window_name: "task".to_string(),
        current_path: "/tmp/worktrees/web-fix-login".into(),
        points_at_expected_path: true,
    });
    task.runtime_projection = ajax_core::models::RuntimeProjection::new(
        ajax_core::models::RuntimeHealth::Healthy,
        SystemTime::now(),
        ajax_core::models::RuntimeObservationSource::TmuxProbe,
    );
    let mut outputs = git_live_outputs();
    outputs.push(output(0, "other-session\n"));
    let mut runner = QueuedRunner::new(outputs);
    let changed = crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(changed);
    assert!(task
        .tmux_status
        .as_ref()
        .is_some_and(|status| !status.exists));
    assert_eq!(
        task.runtime_projection.health,
        ajax_core::models::RuntimeHealth::MissingSession
    );
    assert_eq!(
        task.live_status
            .as_ref()
            .map(|status| status.summary.as_str()),
        Some("tmux session missing")
    );
}
#[test]
fn live_refresh_clears_stale_tmux_missing_flag_when_status_matches() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.agent_status = AgentRuntimeStatus::Unknown;
    task.remove_side_flag(SideFlag::NeedsInput);
    task.add_side_flag(SideFlag::TmuxMissing);
    task.tmux_status = Some(TmuxStatus {
        exists: true,
        session_name: "ajax-web-fix-login".to_string(),
    });
    task.task_window_status = Some(TaskWindowStatus {
        exists: true,
        window_name: "task".to_string(),
        current_path: "/tmp/worktrees/web-fix-login".into(),
        points_at_expected_path: true,
    });
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::Unknown,
        "pane is empty",
    ));
    let mut runner = QueuedRunner::new(vec![
        output(0, "ajax-web-fix-login\n"),
        output(
            0,
            "ajax-web-fix-login\ttask\t/tmp/worktrees/web-fix-login\n",
        ),
        output(0, ""),
    ]);
    let changed = crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(changed);
    assert!(!task.has_side_flag(SideFlag::TmuxMissing));
    assert!(!task.has_side_flag(SideFlag::TaskWindowMissing));
    let mut expected = tmux_live_commands();
    expected.push(expected_ci_probe_command());
    assert_eq!(runner.commands, expected);
}
#[test]
fn live_refresh_updates_changed_task_window_status_before_pane_failure() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.remove_side_flag(SideFlag::NeedsInput);
    task.tmux_status = Some(TmuxStatus {
        exists: true,
        session_name: "ajax-web-fix-login".to_string(),
    });
    task.task_window_status = Some(TaskWindowStatus {
        exists: true,
        window_name: "task".to_string(),
        current_path: "/tmp/wrong".into(),
        points_at_expected_path: false,
    });
    let mut runner = QueuedRunner::new(vec![
        output(0, "ajax-web-fix-login\n"),
        output(
            0,
            "ajax-web-fix-login\ttask\t/tmp/worktrees/web-fix-login\n",
        ),
        output(0, ""),
    ]);
    crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_eq!(
        task.task_window_status
            .as_ref()
            .map(|status| status.current_path.as_path()),
        Some(Path::new("/tmp/worktrees/web-fix-login"))
    );
    assert!(task
        .task_window_status
        .as_ref()
        .is_some_and(|status| status.points_at_expected_path));
    let mut expected = tmux_live_commands();
    expected.push(expected_ci_probe_command());
    assert_eq!(runner.commands, expected);
}
#[test]
fn live_refresh_clears_stale_task_window_missing_flag_when_status_matches() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.agent_status = AgentRuntimeStatus::Unknown;
    task.remove_side_flag(SideFlag::NeedsInput);
    task.add_side_flag(SideFlag::TaskWindowMissing);
    task.tmux_status = Some(TmuxStatus {
        exists: true,
        session_name: "ajax-web-fix-login".to_string(),
    });
    task.task_window_status = Some(TaskWindowStatus {
        exists: true,
        window_name: "task".to_string(),
        current_path: "/tmp/worktrees/web-fix-login".into(),
        points_at_expected_path: true,
    });
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::Unknown,
        "pane is empty",
    ));
    let mut runner = QueuedRunner::new(vec![
        output(0, "ajax-web-fix-login\n"),
        output(
            0,
            "ajax-web-fix-login\ttask\t/tmp/worktrees/web-fix-login\n",
        ),
        output(0, ""),
    ]);
    let changed = crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(changed);
    assert!(!task.has_side_flag(SideFlag::TaskWindowMissing));
    let mut expected = tmux_live_commands();
    expected.push(expected_ci_probe_command());
    assert_eq!(runner.commands, expected);
}
#[test]
fn live_cockpit_watch_accumulates_state_change_after_unchanged_frame() {
    let mut context = sample_context();
    let cache_dir = prepare_active_task_agent_status(&mut context, "task-1", "ask");
    let mut outputs = watch_refresh_outputs();
    outputs.insert(0, output(0, ""));
    let mut runner = QueuedRunner::new(outputs);
    let matches = build_cli()
        .try_get_matches_from([
            "ajax",
            "cockpit",
            "--watch",
            "--iterations",
            "2",
            "--interval-ms",
            "0",
        ])
        .unwrap();
    let (_, subcommand) = matches.subcommand().unwrap();
    let rendered =
        crate::cockpit_backend::render_live_cockpit_command(&mut context, subcommand, &mut runner)
            .unwrap();
    assert!(rendered.state_changed);
    assert_eq!(rendered.output.matches("Ajax Cockpit").count(), 2);
    assert!(runner.commands.len() >= 4);
    let _ = std::fs::remove_dir_all(cache_dir);
}
#[test]
fn supervise_command_runs_codex_json_adapter_and_renders_events() {
    let fake_codex =
        std::env::temp_dir().join(format!("ajax-cli-fake-codex-{}", std::process::id()));
    std::fs::write(
            &fake_codex,
            "#!/bin/sh\nprintf '{\"type\":\"started\"}\\n'\nprintf '{\"type\":\"approval_request\",\"command\":\"cargo test\"}\\n'\n",
        )
        .unwrap();
    let mut permissions = std::fs::metadata(&fake_codex).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    std::fs::set_permissions(&fake_codex, permissions).unwrap();
    let matches = build_cli()
        .try_get_matches_from([
            "ajax",
            "supervise",
            "--prompt",
            "fix tests",
            "--codex-bin",
            &fake_codex.display().to_string(),
        ])
        .unwrap();
    let (_, subcommand) = matches.subcommand().unwrap();
    let logs_dir = std::env::temp_dir().join(format!("ajax-supervise-logs-{}", std::process::id()));
    let (output, _) =
        crate::supervise::supervise_command_output_and_events(subcommand, None, &logs_dir).unwrap();
    let events: Vec<&str> = output
        .lines()
        .filter(|line| {
            line.starts_with("process started: ")
                || line.starts_with("agent started: ")
                || line.starts_with("waiting for approval")
                || line.starts_with("process exited: ")
        })
        .collect();
    assert_eq!(events.len(), 4);
    assert!(events[0].starts_with("process started: "));
    assert_eq!(
        events[1..],
        [
            "agent started: codex",
            "waiting for approval: cargo test",
            "process exited: 0",
        ]
    );
    let _ = std::fs::remove_file(fake_codex);
}
#[test]
fn supervise_command_runs_cursor_stream_json_adapter_and_renders_events() {
    let fake_cursor =
        std::env::temp_dir().join(format!("ajax-cli-fake-cursor-{}", std::process::id()));
    std::fs::write(
            &fake_cursor,
            "#!/bin/sh\nprintf '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"abc\"}\\n'\nprintf '{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"Approval required to run cargo test\"}]}}\\n'\n",
        )
        .unwrap();
    let mut permissions = std::fs::metadata(&fake_cursor).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    std::fs::set_permissions(&fake_cursor, permissions).unwrap();
    let matches = build_cli()
        .try_get_matches_from([
            "ajax",
            "supervise",
            "--agent",
            "cursor",
            "--prompt",
            "fix tests",
            "--cursor-bin",
            &fake_cursor.display().to_string(),
        ])
        .unwrap();
    let (_, subcommand) = matches.subcommand().unwrap();
    let logs_dir = std::env::temp_dir().join(format!("ajax-supervise-logs-{}", std::process::id()));
    let (output, _) =
        crate::supervise::supervise_command_output_and_events(subcommand, None, &logs_dir).unwrap();
    let events: Vec<&str> = output
        .lines()
        .filter(|line| {
            line.starts_with("process started: ")
                || line.starts_with("agent started: ")
                || line.starts_with("waiting for approval")
                || line.starts_with("process exited: ")
        })
        .collect();
    assert_eq!(events.len(), 4);
    assert!(events[0].starts_with("process started: "));
    assert_eq!(
        events[1..],
        [
            "agent started: cursor",
            "waiting for approval",
            "process exited: 0",
        ]
    );
    let _ = std::fs::remove_file(fake_cursor);
}
#[test]
fn supervise_command_reports_nonzero_agent_exit() {
    let fake_codex = std::env::temp_dir().join(format!(
        "ajax-cli-fake-codex-nonzero-{}",
        std::process::id()
    ));
    std::fs::write(
        &fake_codex,
        "#!/bin/sh\nprintf '{\"type\":\"started\"}\\n'\nexit 42\n",
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&fake_codex).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    std::fs::set_permissions(&fake_codex, permissions).unwrap();
    let matches = build_cli()
        .try_get_matches_from([
            "ajax",
            "supervise",
            "--prompt",
            "fix tests",
            "--codex-bin",
            &fake_codex.display().to_string(),
        ])
        .unwrap();
    let (_, subcommand) = matches.subcommand().unwrap();
    let logs_dir =
        std::env::temp_dir().join(format!("ajax-supervise-logs-err-{}", std::process::id()));
    let error = crate::supervise::supervise_command_output_and_events(subcommand, None, &logs_dir)
        .unwrap_err();
    let _ = std::fs::remove_file(fake_codex);
    assert!(matches!(error, CliError::CommandFailed(message)
                if message == "supervisor failed: process error: codex exited with status 42"));
}
#[test]
fn supervise_command_keeps_stderr_context_on_agent_exit() {
    let fake_codex =
        std::env::temp_dir().join(format!("ajax-cli-fake-codex-stderr-{}", std::process::id()));
    std::fs::write(
        &fake_codex,
        "#!/bin/sh\nprintf '{\"type\":\"started\"}\\n'\nprintf 'auth expired\\n' >&2\nexit 42\n",
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&fake_codex).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    std::fs::set_permissions(&fake_codex, permissions).unwrap();
    let matches = build_cli()
        .try_get_matches_from([
            "ajax",
            "supervise",
            "--prompt",
            "fix tests",
            "--codex-bin",
            &fake_codex.display().to_string(),
        ])
        .unwrap();
    let (_, subcommand) = matches.subcommand().unwrap();
    let logs_dir =
        std::env::temp_dir().join(format!("ajax-supervise-logs-err-{}", std::process::id()));
    let error = crate::supervise::supervise_command_output_and_events(subcommand, None, &logs_dir)
        .unwrap_err();
    let _ = std::fs::remove_file(fake_codex);
    assert_eq!(
        error,
        CliError::CommandFailed(
            "supervisor failed: process error: codex exited with status 42; stderr: auth expired"
                .to_string()
        )
    );
}
#[test]
fn supervise_with_task_runs_for_visible_task() {
    let fake_codex = write_fake_codex("visible-task");
    let mut context = sample_context();
    let mut runner = QueuedRunner::default();
    let output = run_with_context_and_runner(
        [
            "ajax",
            "supervise",
            "--task",
            "web/fix-login",
            "--prompt",
            "fix tests",
            "--codex-bin",
            &fake_codex.display().to_string(),
        ],
        &mut context,
        &mut runner,
    )
    .unwrap();
    let _ = std::fs::remove_file(fake_codex);
    assert!(output.lines().any(|line| line == "agent started: codex"));
}
#[test]
fn supervise_with_task_rejects_unknown_task() {
    let mut context = sample_context();
    let mut runner = QueuedRunner::default();
    let error = run_with_context_and_runner(
        [
            "ajax",
            "supervise",
            "--task",
            "web/missing",
            "--prompt",
            "fix tests",
            "--codex-bin",
            "/path/that/should/not/run",
        ],
        &mut context,
        &mut runner,
    )
    .unwrap_err();
    assert!(matches!(error, CliError::CommandFailed(message)
                if message == "task not found: web/missing"));
}
#[test]
fn supervise_with_task_rejects_removed_task() {
    let mut context = sample_context();
    let mut runner = QueuedRunner::default();
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .lifecycle_status = LifecycleStatus::Removed;
    let error = run_with_context_and_runner(
        [
            "ajax",
            "supervise",
            "--task",
            "web/fix-login",
            "--prompt",
            "fix tests",
            "--codex-bin",
            "/path/that/should/not/run",
        ],
        &mut context,
        &mut runner,
    )
    .unwrap_err();
    assert!(matches!(error, CliError::CommandFailed(message)
                if message == "task not found: web/fix-login"));
}
#[test]
fn supervise_with_task_persists_supervisor_state_to_sqlite() {
    let directory = std::env::temp_dir().join(format!(
        "ajax-cli-supervise-task-{}-{}",
        std::process::id(),
        "state"
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let config_file = directory.join("config.toml");
    let state_file = directory.join("state.db");
    let fake_codex = directory.join("fake-codex");
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
    std::fs::write(
            &fake_codex,
            "#!/bin/sh\nprintf '{\"type\":\"started\"}\\n'\nprintf '{\"type\":\"approval_request\",\"command\":\"cargo test\"}\\n'\n",
        )
        .unwrap();
    let mut permissions = std::fs::metadata(&fake_codex).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    std::fs::set_permissions(&fake_codex, permissions).unwrap();
    let mut runner = QueuedRunner::default();
    let output = run_with_context_paths_and_runner(
        [
            "ajax",
            "supervise",
            "--task",
            "web/fix-login",
            "--prompt",
            "fix tests",
            "--codex-bin",
            &fake_codex.display().to_string(),
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
            .find(|line| line.starts_with("waiting for approval")),
        Some("waiting for approval: cargo test")
    );
    let task = restored
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == "web/fix-login")
        .expect("task should persist");
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::Done)
    );
    assert_eq!(task.lifecycle_status, LifecycleStatus::Reviewable);
    assert!(!task.has_side_flag(SideFlag::NeedsInput));
}
#[test]
fn help_output_is_successful() {
    let context = sample_context();
    let output = run_with_context(["ajax-cli", "--help"], &context).unwrap();
    assert!(output
        .lines()
        .any(|line| line == "Usage: ajax-cli [OPTIONS] [COMMAND]"));
    assert!(output.lines().any(|line| line == "Commands:"));
}
#[test]
fn bare_command_reports_missing_subcommand_as_error() {
    let error = run_with_context(["ajax"], &sample_context()).unwrap_err();
    assert_eq!(
        error,
        CliError::CommandFailed("command is required; pass --help".to_string())
    );
}
#[test]
fn readonly_context_rejects_supervise_instead_of_reporting_placeholder_success() {
    let error = run_with_context(
        ["ajax", "supervise", "--prompt", "fix tests"],
        &sample_context(),
    )
    .unwrap_err();
    assert_eq!(
        error,
        CliError::CommandFailed(
            "supervise requires mutable context and runner support".to_string()
        )
    );
}
#[test]
fn reconcile_command_is_not_supported() {
    let matches = build_cli().try_get_matches_from(["ajax", "reconcile", "--json"]);
    assert!(matches.is_err());
}
#[test]
fn json_flag_is_available_for_ui_consumed_commands() {
    for args in [
        ["ajax", "repos", "--json", ""],
        ["ajax", "tasks", "--json", ""],
        ["ajax", "inspect", "web/fix-login", "--json"],
        ["ajax", "inbox", "--json", ""],
        ["ajax", "next", "--json", ""],
        ["ajax", "ready", "--json", ""],
        ["ajax", "status", "--json", ""],
        ["ajax", "doctor", "--json", ""],
        ["ajax", "cockpit", "--json", ""],
    ] {
        let filtered_args = args.into_iter().filter(|arg| !arg.is_empty());
        let matches = build_cli().try_get_matches_from(filtered_args);
        assert!(matches.is_ok(), "{args:?} should parse");
    }
}
#[test]
fn doctor_reports_context_path_health() {
    let directory = std::env::temp_dir().join(format!("ajax-doctor-paths-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let config_file = directory.join("config.toml");
    let state_file = directory.join("state").join("ajax.db");
    std::fs::write(
        &config_file,
        r#"
            [[repos]]
            name = "web"
            path = "/missing/web"
            default_branch = "main"
            "#,
    )
    .unwrap();
    let output = run_with_context_paths(
        ["ajax", "doctor"],
        &CliContextPaths::new(&config_file, &state_file),
    )
    .unwrap();
    let config_line = format!("config:path\ttrue\tfile exists: {}", config_file.display());
    assert!(output.lines().any(|line| line == config_line));
    assert!(output
        .lines()
        .any(|line| line == "state:path\ttrue\tparent directory can be created"));
    std::fs::remove_dir_all(&directory).unwrap();
}

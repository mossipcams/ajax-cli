#[test]
fn doctor_accepts_relative_state_paths_with_creatable_parents() {
    assert!(parent_directory_available(Path::new("ajax.db")));
    assert!(parent_directory_available(Path::new(
        "state/ajax.db"
    )));
}
#[test]
fn refreshed_read_persists_recovered_ajax_task_without_duplicates() {
    let directory = std::env::temp_dir().join(format!("ajax-recovery-save-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let config_file = directory.join("config.toml");
    let state_file = directory.join("state").join("ajax.db");
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
    let paths = CliContextPaths::new(&config_file, &state_file);
    let mut registry = InMemoryRegistry::default();
    let mut existing = Task::new(
        TaskId::new("task-1"),
        "web",
        "existing",
        "existing",
        "ajax/existing",
        "main",
        "/Users/matt/projects/web__worktrees/ajax-existing",
        "ajax-web-existing",
        "task",
        AgentClient::Codex,
    );
    existing.lifecycle_status = LifecycleStatus::Active;
    registry.create_task(existing).unwrap();
    SqliteRegistryStore::new(&state_file)
        .save(&registry)
        .unwrap();
    let mut first_runner = RecoveryRunner::new();
    let _first_output =
        run_with_context_paths_and_runner(["ajax", "tasks"], &paths, &mut first_runner).unwrap();
    let saved = SqliteRegistryStore::new(&state_file).load().unwrap();
    assert_eq!(
        saved
            .list_tasks()
            .into_iter()
            .filter(|task| task.qualified_handle() == "web/code")
            .count(),
        1
    );
    assert_eq!(
        saved
            .list_tasks()
            .into_iter()
            .filter(|task| task.branch == "topic")
            .count(),
        0
    );
    let mut second_runner = RecoveryRunner::new();
    let _second_output =
        run_with_context_paths_and_runner(["ajax", "tasks"], &paths, &mut second_runner).unwrap();
    let saved_again = SqliteRegistryStore::new(&state_file).load().unwrap();
    assert_eq!(
        saved_again
            .list_tasks()
            .into_iter()
            .filter(|task| task.qualified_handle() == "web/code")
            .count(),
        1
    );
    std::fs::remove_dir_all(&directory).unwrap();
}
#[test]
fn state_export_writes_registry_snapshot_without_overwriting() {
    let directory = std::env::temp_dir().join(format!("ajax-state-export-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let export_path = directory.join("backup.json");
    let context = sample_context();
    let output = run_with_context(
        [
            "ajax",
            "state",
            "export",
            "--output",
            export_path.to_str().unwrap(),
        ],
        &context,
    )
    .unwrap();
    let snapshot = std::fs::read_to_string(&export_path).unwrap();
    let overwrite_error = run_with_context(
        [
            "ajax",
            "state",
            "export",
            "--output",
            export_path.to_str().unwrap(),
        ],
        &context,
    )
    .unwrap_err();
    assert_eq!(
        output,
        format!("exported state snapshot: {}", export_path.display())
    );
    let exported: serde_json::Value = serde_json::from_str(&snapshot).unwrap();
    let task = exported["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["handle"] == "fix-login")
        .expect("exported snapshot should include fix-login task");
    assert_eq!(task["repo"], "web");
    assert_eq!(task["handle"], "fix-login");
    assert_eq!(
        overwrite_error,
        CliError::CommandFailed(format!(
            "state export target already exists: {}",
            export_path.display()
        ))
    );
    std::fs::remove_dir_all(&directory).unwrap();
}
#[test]
fn executable_commands_accept_execute_and_yes_flags() {
    for args in [
        vec!["ajax", "start", "--repo", "web", "--execute"],
        vec!["ajax", "resume", "web/fix-login", "--execute"],
        vec!["ajax", "repair", "web/fix-login", "--execute"],
        vec!["ajax", "review", "web/fix-login", "--execute"],
        vec!["ajax", "ship", "web/fix-login", "--execute", "--yes"],
        vec!["ajax", "drop", "web/fix-login", "--execute", "--yes"],
        vec!["ajax", "tidy", "--execute", "--yes"],
    ] {
        let matches = build_cli().try_get_matches_from(args.clone());
        assert!(matches.is_ok(), "{args:?} should parse");
    }
}
#[test]
fn task_scoped_commands_require_explicit_task_handle() {
    for (args, command) in [
        (vec!["ajax", "resume"], "resume"),
        (vec!["ajax", "repair"], "repair"),
        (vec!["ajax", "repair"], "repair"),
        (vec!["ajax", "review"], "review"),
        (vec!["ajax", "ship"], "ship"),
        (vec!["ajax", "drop"], "drop"),
    ] {
        let error = run_with_context(args.clone(), &sample_context()).unwrap_err();
        let message = match error {
            CliError::CommandFailed(message) => message,
            other => panic!("{args:?} should require task arg, got {other:?}"),
        };
        assert_eq!(
            message.trim_end(),
            format!(
                "error: the following required arguments were not provided:\n  <REPO/HANDLE>\n\nUsage: ajax {command} <REPO/HANDLE>\n\nFor more information, try '--help'."
            )
        );
    }
}
#[test]
fn workspace_manifest_pins_repository_metadata_and_lints() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let workspace_manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    let cli_manifest = std::fs::read_to_string(root.join("crates/ajax-cli/Cargo.toml")).unwrap();
    assert!(!workspace_manifest.contains("https://github.com/example/ajax-cli"));
    assert!(workspace_manifest.contains("repository = \"https://github.com/mossipcams/ajax-cli\""));
    assert!(workspace_manifest.contains("version = \"0.1.0\""));
    assert!(workspace_manifest.contains("[workspace.lints.rust]"));
    assert!(workspace_manifest.contains("unsafe_op_in_unsafe_fn = \"deny\""));
    assert!(cli_manifest.contains("[[bin]]\nname = \"ajax-cli\""));
    assert!(!cli_manifest.contains("name = \"ajax\"\npath = \"src/main.rs\""));
    assert!(cli_manifest.contains("path = \"src/main.rs\""));
}
#[test]
fn workspace_members_inherit_metadata_lints_and_dependencies() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let workspace_manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(workspace_manifest.contains("[workspace.dependencies]"));
    for dependency in ["serde", "serde_json", "tokio", "rstest"] {
        assert!(
            workspace_manifest.contains(&format!("{dependency} = ")),
            "workspace manifest should centralize {dependency}"
        );
    }
    for crate_name in ["ajax-cli", "ajax-core", "ajax-supervisor", "ajax-tui"] {
        let manifest =
            std::fs::read_to_string(root.join(format!("crates/{crate_name}/Cargo.toml"))).unwrap();
        assert!(manifest
            .lines()
            .any(|line| line.trim_start().starts_with("version = \"")));
        assert!(!manifest.contains("\nversion.workspace = true"));
        assert!(manifest.contains("edition.workspace = true"));
        assert!(manifest.contains("[lints]"));
        assert!(manifest.contains("workspace = true"));
        for repeated_dependency in ["serde_json", "rstest"] {
            assert!(
                !manifest.contains(&format!("{repeated_dependency} = \"")),
                "{crate_name} should inherit {repeated_dependency} from the workspace"
            );
        }
    }
}
#[test]
fn workspace_toolchain_and_lint_configs_are_pinned() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let clippy = std::fs::read_to_string(root.join("clippy.toml")).unwrap();
    let rustfmt = std::fs::read_to_string(root.join("rustfmt.toml")).unwrap();
    let toolchain = std::fs::read_to_string(root.join("rust-toolchain.toml")).unwrap();
    assert!(clippy.contains("doc-valid-idents"));
    assert!(rustfmt.contains("edition = \"2021\""));
    assert!(toolchain.contains("channel = \"1.88.0\""));
}
#[test]
fn tui_dependency_uses_audit_clean_ratatui_feature_set() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let tui_manifest = std::fs::read_to_string(root.join("crates/ajax-tui/Cargo.toml")).unwrap();
    let workspace_manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    let toolchain = std::fs::read_to_string(root.join("rust-toolchain.toml")).unwrap();
    for needle in ["ratatui = { version = \"0.30\"", "rust-version = \"1.88\""] {
        assert_ne!(
            workspace_manifest.find(needle),
            None,
            "workspace manifest missing {needle}"
        );
    }
    for needle in [
        "default-features = false",
        "\"crossterm\"",
        "\"underline-color\"",
        "\"layout-cache\"",
    ] {
        assert_ne!(
            tui_manifest.find(needle),
            None,
            "tui manifest missing {needle}"
        );
    }
    assert_eq!(tui_manifest.find("all-widgets"), None);
    assert_eq!(
        toolchain
            .lines()
            .find(|line| line.starts_with("channel = ")),
        Some("channel = \"1.88.0\"")
    );
}
#[test]
fn new_command_renders_plan_without_json_panic() {
    let output = run_with_context(
        [
            "ajax",
            "start",
            "--repo",
            "web",
            "--title",
            "fix logout",
            "--agent",
            "codex",
        ],
        &sample_context(),
    )
    .unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "create task: fix logout");
    assert!(lines.iter().any(|line| {
        *line
            == "$ git -C /Users/matt/projects/web worktree add -b ajax/fix-logout /Users/matt/projects/web__worktrees/ajax-fix-logout origin/main"
    }));
    assert!(lines.iter().any(|line| {
        *line
            == "$ tmux new-session -d -s ajax-web-fix-logout -n task -c /Users/matt/projects/web__worktrees/ajax-fix-logout"
    }));
    assert!(lines
        .iter()
        .any(|line| line.starts_with("$ tmux send-keys -t ajax-web-fix-logout:task")));
}
#[test]
fn new_command_requires_task_title() {
    let error =
        run_with_context(["ajax", "start", "--repo", "web"], &sample_context()).unwrap_err();
    assert_eq!(
        error,
        CliError::CommandFailed("task title is required; pass --title".to_string())
    );
}
#[test]
fn new_command_rejects_empty_task_titles() {
    for title in ["", "   "] {
        let error = run_with_context(
            ["ajax", "start", "--repo", "web", "--title", title],
            &sample_context(),
        )
        .unwrap_err();
        assert_eq!(
            error,
            CliError::CommandFailed("task title is required; pass --title".to_string())
        );
    }
}
#[test]
fn repos_command_renders_human_output() {
    let context = sample_context();
    let output = run_with_context(["ajax", "repos"], &context).unwrap();
    assert_eq!(
        output,
        "web\t/Users/matt/projects/web\tactive:0 reviewable:1 cleanable:0"
    );
}
#[test]
fn tasks_command_renders_json_output() {
    let context = sample_context();
    let output = run_with_context(["ajax", "tasks", "--json"], &context).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["tasks"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["tasks"][0]["qualified_handle"], "web/fix-login");
}
#[test]
fn inspect_reports_missing_task_as_error() {
    let context = sample_context();
    let error = run_with_context(["ajax", "inspect", "web/missing"], &context).unwrap_err();
    assert_eq!(
        error,
        CliError::CommandFailed("task not found: web/missing".to_string())
    );
}
#[test]
fn open_command_renders_command_plan() {
    let context = sample_context();
    let output = run_with_context(["ajax", "resume", "web/fix-login"], &context).unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "open task: web/fix-login");
    assert_eq!(
        lines
            .iter()
            .find(|line| line.starts_with("$ tmux select-window")),
        Some(&"$ tmux select-window -t ajax-web-fix-login:task")
    );
    match current_open_mode() {
        OpenMode::Attach => {
            assert_eq!(
                lines
                    .iter()
                    .find(|line| line.starts_with("$ tmux attach-session")),
                Some(&"$ tmux attach-session -t ajax-web-fix-login")
            );
        }
        OpenMode::SwitchClient => {
            assert_eq!(
                lines
                    .iter()
                    .find(|line| line.starts_with("$ tmux switch-client")),
                Some(&"$ tmux switch-client -t ajax-web-fix-login")
            );
        }
        OpenMode::NoAttach => unreachable!("CLI tests never run in NoAttach mode"),
    }
}
#[test]
fn open_execute_switches_client_when_inside_tmux() {
    let mut context = sample_context();
    let mut runner = RecordingCommandRunner::default();
    let matches = build_cli()
        .try_get_matches_from(["ajax", "resume", "web/fix-login", "--execute"])
        .unwrap();
    let (_, subcommand) = matches.subcommand().unwrap();
    render_task_command(
        TaskCommandKind::Resume,
        subcommand,
        &mut context,
        &mut runner,
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
}
#[test]
fn readonly_context_rejects_execute_before_running_external_commands() {
    let context = sample_context();
    let error =
        run_with_context(["ajax", "resume", "web/fix-login", "--execute"], &context).unwrap_err();
    assert_eq!(
        error,
        CliError::CommandFailed(
            "execution requires mutable context and runner support".to_string()
        )
    );
}
#[test]
fn merge_command_renders_json_plan() {
    let context = sample_context();
    let output = run_with_context(["ajax", "ship", "web/fix-login", "--json"], &context).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["title"], "merge task: web/fix-login");
    assert_eq!(parsed["requires_confirmation"], true);
    assert_eq!(parsed["commands"][0]["program"], "git");
    assert_eq!(
        parsed["commands"][0]["args"],
        serde_json::json!(["-C", "/Users/matt/projects/web", "switch", "main"])
    );
    assert_eq!(
        parsed["commands"][1]["args"],
        serde_json::json!([
            "-C",
            "/Users/matt/projects/web",
            "merge",
            "--ff-only",
            "ajax/fix-login"
        ])
    );
}
#[test]
fn repair_mismatch_cli_plan_renders_typed_adoption_and_requires_confirmation() {
    let context = sample_context_with_named_checkout_mismatch();
    let human = run_with_context(["ajax", "repair", "web/fix-login"], &context).unwrap();
    assert_eq!(
        human,
        "repair task: web/fix-login\nrequires confirmation\nadopt branch: fix/pane-stuck (expected ajax/fix-login)"
    );
    let json_output =
        run_with_context(["ajax", "repair", "web/fix-login", "--json"], &context).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json_output).unwrap();
    assert_eq!(parsed["requires_confirmation"], true);
    assert_eq!(parsed["commands"], serde_json::json!([]));
    assert_eq!(
        parsed["branch_adoption"]["expected_branch"],
        "ajax/fix-login"
    );
    assert_eq!(
        parsed["branch_adoption"]["observed_branch"],
        "fix/pane-stuck"
    );
}
#[test]
fn repair_mismatch_cli_decline_preserves_branch_intent() {
    let mut context = sample_context_with_named_checkout_mismatch();
    let mut runner = QueuedRunner::new(checkout_mismatch_refresh_outputs());
    let error = run_with_context_and_runner(
        ["ajax", "repair", "web/fix-login", "--execute"],
        &mut context,
        &mut runner,
    )
    .unwrap_err();
    assert_eq!(
        error,
        CliError::CommandFailed("confirmation required; pass --yes".to_string())
    );
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_eq!(task.branch, "ajax/fix-login");
    assert_git_observation_only(&runner.commands);
}
#[test]
fn repair_mismatch_cli_yes_persists_adopted_branch_without_switching() {
    let directory = std::env::temp_dir().join(format!(
        "ajax-cli-repair-mismatch-{}-{}",
        std::process::id(),
        "adopt"
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
    let context = sample_context_with_named_checkout_mismatch();
    let task_before = context
        .registry
        .get_task(&TaskId::new("task-1"))
        .unwrap()
        .clone();
    SqliteRegistryStore::new(&state_file)
        .save(&context.registry)
        .unwrap();
    let paths = CliContextPaths::new(&config_file, &state_file);
    let mut runner = QueuedRunner::new(checkout_mismatch_refresh_outputs());
    let output = run_with_context_paths_and_runner(
        ["ajax", "repair", "web/fix-login", "--execute", "--yes"],
        &paths,
        &mut runner,
    )
    .unwrap();
    let restored = SqliteRegistryStore::new(&state_file).load().unwrap();
    let task_after = restored.get_task(&TaskId::new("task-1")).unwrap();
    std::fs::remove_dir_all(Path::new(&directory)).unwrap();
    assert!(output.is_empty());
    assert_eq!(task_after.branch, "fix/pane-stuck");
    assert_eq!(task_after.id, task_before.id);
    assert_eq!(
        task_after.qualified_handle(),
        task_before.qualified_handle()
    );
    assert_eq!(task_after.worktree_path, task_before.worktree_path);
    assert_eq!(task_after.tmux_session, task_before.tmux_session);
    assert!(!task_after.has_checkout_mismatch());
    assert_git_observation_only(&runner.commands);
}
#[test]
fn repair_command_renders_configured_test_plan() {
    let mut context = sample_context();
    context.config.test_commands = vec![ajax_core::config::TestCommand::new("web", "cargo test")];
    let output = run_with_context(["ajax", "repair", "web/fix-login"], &context).unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "repair task: web/fix-login");
    assert!(lines
        .iter()
        .any(|line| { *line == "$ (cd /tmp/worktrees/web-fix-login && sh -lc 'cargo test')" }));
}
#[test]
fn review_command_renders_diff_summary_plan() {
    let context = sample_context();
    let output = run_with_context(["ajax", "review", "web/fix-login"], &context).unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "diff task: web/fix-login");
    assert!(lines.iter().any(|line| {
        *line == "$ (cd /tmp/worktrees/web-fix-login && git diff --stat main...HEAD)"
    }));
}
#[test]
fn next_command_renders_attention_item() {
    let context = sample_context();
    let output = run_with_context(["ajax", "next"], &context).unwrap();
    assert_eq!(output, "web/fix-login: needs_input -> resume");
}
#[test]
fn ready_command_renders_review_queue() {
    let context = sample_context();
    let output = run_with_context(["ajax", "ready", "--json"], &context).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["tasks"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["tasks"][0]["qualified_handle"], "web/fix-login");
    assert_eq!(parsed["tasks"][0]["lifecycle_status"], "Reviewable");
}
#[test]
fn cli_loads_context_from_config_and_state_files() {
    let directory = std::env::temp_dir().join(format!(
        "ajax-cli-context-{}-{}",
        std::process::id(),
        "load"
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
    let output = run_with_context_paths(
        ["ajax", "tasks", "--json"],
        &CliContextPaths::new(&config_file, &state_file),
    )
    .unwrap();
    std::fs::remove_dir_all(Path::new(&directory)).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["tasks"][0]["qualified_handle"], "web/fix-login");
}
#[test]
fn cli_missing_config_and_state_files_use_empty_context() {
    let directory = std::env::temp_dir().join(format!(
        "ajax-cli-context-{}-{}",
        std::process::id(),
        "missing"
    ));
    let config_file = directory.join("missing-config.toml");
    let state_file = directory.join("missing-state.db");
    let output = run_with_context_paths(
        ["ajax", "tasks", "--json"],
        &CliContextPaths::new(&config_file, &state_file),
    )
    .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["tasks"].as_array().unwrap().len(), 0);
}


#[test]
fn web_bridge_rejects_empty_save_over_non_empty_sqlite_state() {
    let dir = scratch_dir("empty-save-guard");
    let paths = CliContextPaths::new(dir.join("config.toml"), dir.join("state.db"));
    let saved_context = reviewable_context();
    let store = SqliteRegistryStore::new(&paths.state_file);
    store.save(&saved_context.registry).unwrap();
    let mut context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let mut bridge = CliRuntimeBridge {
        paths: Some(paths.clone()),
        last_loaded_mtime: crate::context::state_file_mtime(&paths),
        save_state: crate::context::ContextSaveState {
            loaded_registry: InMemoryRegistry::default(),
            loaded_revision: store.current_revision().unwrap(),
            allow_empty_registry_once: false,
        },
    };

    let error = bridge.persist_changed_state(&mut context).unwrap_err();

    assert!(error
        .to_string()
        .contains("refusing to save empty registry"));
    let reloaded = crate::context::load_context(&paths).expect("reload after rejected save");
    assert!(reloaded
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .is_some());

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn web_bridge_persists_confirmed_mismatch_branch_adoption() {
    let dir = scratch_dir("mismatch-adoption");
    let paths = CliContextPaths::new(dir.join("config.toml"), dir.join("state.db"));
    let mut saved_context = reviewable_context();
    {
        let task = saved_context
            .registry
            .get_task_mut(&TaskId::new("web/fix-login"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Active;
        task.git_status = Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("fix/pane-stuck".to_string()),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        });
    }
    SqliteRegistryStore::new(&paths.state_file)
        .save(&saved_context.registry)
        .unwrap();
    let mut context = saved_context;
    let mut bridge = CliRuntimeBridge::for_context(Some(&paths), &context).unwrap();
    let mut runner = MismatchAdoptionRunner::default();

    let outcome = bridge
        .execute_operate(
            OperateRequest {
                task_handle: "web/fix-login".to_string(),
                action: "repair".to_string(),
                confirmed: true,
                branch_adoption: Some(ajax_core::commands::BranchAdoptionPlan {
                    expected_branch: "ajax/fix-login".to_string(),
                    observed_branch: "fix/pane-stuck".to_string(),
                }),
            },
            &mut context,
            &mut runner,
        )
        .expect("confirmed mismatch repair should adopt");

    assert!(outcome.state_changed);
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap()
            .branch,
        "fix/pane-stuck"
    );
    assert_eq!(runner.commands.len(), 2);
    assert!(runner.commands.iter().all(|command| {
        command.program == "git"
            && (command
                .args
                .windows(2)
                .any(|window| window == ["worktree", "list"])
                || command
                    .args
                    .windows(2)
                    .any(|window| window == ["branch", "--format=%(refname:short)"]))
    }));

    let reloaded = crate::context::load_context(&paths).expect("reload after adoption");
    let task = reloaded
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .unwrap();
    assert_eq!(task.branch, "fix/pane-stuck");
    assert_eq!(
        task.worktree_path,
        std::path::Path::new("/repo/web__worktrees/ajax-fix-login")
    );
    assert_eq!(task.tmux_session, "ajax-web-fix-login");
    assert!(!task.has_checkout_mismatch());

    let _ = std::fs::remove_dir_all(dir);
}

#[derive(Default)]
struct MismatchAdoptionRunner {
    commands: Vec<CommandSpec>,
}

impl CommandRunner for MismatchAdoptionRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        self.commands.push(command.clone());
        let stdout = match command.args.as_slice() {
            [_, repo, subcommand, ..] if repo == "/repo/web" && subcommand == "worktree" => {
                "worktree /repo/web__worktrees/ajax-fix-login\nHEAD 2222222\nbranch refs/heads/fix/pane-stuck\n\n"
            }
            [_, repo, subcommand, format]
                if repo == "/repo/web"
                    && subcommand == "branch"
                    && format == "--format=%(refname:short)" =>
            {
                "main\najax/fix-login\n"
            }
            _ => "",
        };
        Ok(CommandOutput {
            status_code: 0,
            stdout: stdout.to_string(),
            stderr: String::new(),
        })
    }
}

#[test]
fn web_bridge_drop_of_last_task_persists_empty_registry() {
    let dir = scratch_dir("drop-empty-registry");
    let paths = CliContextPaths::new(dir.join("config.toml"), dir.join("state.db"));
    let context = reviewable_context();
    SqliteRegistryStore::new(&paths.state_file)
        .save(&context.registry)
        .unwrap();
    let mut context = context;
    let mut bridge = CliRuntimeBridge::for_context(Some(&paths), &context).unwrap();
    let mut runner = AbsentDropRunner;

    let outcome = bridge
        .execute_operate(
            OperateRequest {
                task_handle: "web/fix-login".to_string(),
                action: "drop".to_string(),
                confirmed: false,
                branch_adoption: None,
            },
            &mut context,
            &mut runner,
        )
        .expect("drop of the sole task should succeed");

    assert!(outcome.state_changed);
    assert!(context.registry.list_tasks().is_empty());

    let reloaded = crate::context::load_context(&paths).expect("reload after drop");
    assert!(
        reloaded.registry.list_tasks().is_empty(),
        "empty registry should be persisted after dropping the last task"
    );

    let _ = std::fs::remove_dir_all(dir);
}

fn scratch_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("ajax-web-be-{tag}-{}-{nanos}", std::process::id()))
}

#[derive(Clone)]
struct OkRunner;

impl CommandRunner for OkRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        let stdout = match command.args.as_slice() {
            [_, repo, subcommand, action, flag]
                if repo == "/repo/web"
                    && subcommand == "worktree"
                    && action == "list"
                    && flag == "--porcelain" =>
            {
                "worktree /repo/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /repo/web__worktrees/ajax-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n"
            }
            [_, repo, subcommand, format]
                if repo == "/repo/web"
                    && subcommand == "branch"
                    && format == "--format=%(refname:short)" =>
            {
                "main\najax/fix-login\n"
            }
            _ => "diff stat",
        };

        Ok(CommandOutput {
            status_code: 0,
            stdout: stdout.to_string(),
            stderr: String::new(),
        })
    }
}

#[derive(Clone)]
struct AbsentDropRunner;

impl CommandRunner for AbsentDropRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        let stdout = match command.args.as_slice() {
            [command, ..] if command == "list-sessions" => "",
            [_, repo, subcommand, action, flag]
                if repo == "/repo/web"
                    && subcommand == "worktree"
                    && action == "list"
                    && flag == "--porcelain" =>
            {
                "worktree /repo/web\nHEAD 1111111\nbranch refs/heads/main\n\n"
            }
            [_, repo, subcommand, format]
                if repo == "/repo/web"
                    && subcommand == "branch"
                    && format == "--format=%(refname:short)" =>
            {
                "main\n"
            }
            _ => "",
        };

        Ok(CommandOutput {
            status_code: 0,
            stdout: stdout.to_string(),
            stderr: String::new(),
        })
    }
}

#[derive(Clone)]
struct LiveRefreshRunner;

impl CommandRunner for LiveRefreshRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        let stdout = match command.args.as_slice() {
            [command, ..] if command == "list-sessions" => "ajax-web-fix-login\n",
            [_, repo, subcommand, action, flag]
                if repo == "/repo/web"
                    && subcommand == "worktree"
                    && action == "list"
                    && flag == "--porcelain" =>
            {
                "worktree /repo/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /repo/web__worktrees/ajax-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n"
            }
            [_, repo, subcommand, format]
                if repo == "/repo/web"
                    && subcommand == "branch"
                    && format == "--format=%(refname:short)" =>
            {
                "main\najax/fix-login\n"
            }
            [command, ..] if command == "list-windows" => {
                "ajax-web-fix-login\ttask\t/repo/web__worktrees/ajax-fix-login\n"
            }
            [command, ..] if command == "capture-pane" => {
                // Structured Cursor lifecycle evidence — generic busy chrome
                // alone no longer projects AgentRunning.
                "{\"type\":\"thinking\"}\n"
            }
            _ => "",
        };

        Ok(CommandOutput {
            status_code: 0,
            stdout: stdout.to_string(),
            stderr: String::new(),
        })
    }
}

fn reviewable_context() -> CommandContext<InMemoryRegistry> {
    let mut context = CommandContext::new(
        Config {
            repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    );
    let mut task = Task::new(
        TaskId::new("web/fix-login"),
        "web",
        "fix-login",
        "Fix login",
        "ajax/fix-login",
        "main",
        "/repo/web__worktrees/ajax-fix-login",
        "ajax-web-fix-login",
        "task",
        AgentClient::Codex,
    );
    task.lifecycle_status = LifecycleStatus::Reviewable;
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
    task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
    task.task_window_status = Some(TaskWindowStatus::present(
        "task",
        "/repo/web__worktrees/ajax-fix-login",
    ));
    context.registry.create_task(task).unwrap();
    context
}

#[test]
fn acknowledge_operator_input_marks_attention_and_persists_across_reload() {
    let dir = scratch_dir("ack-operator-input");
    let paths = CliContextPaths::new(dir.join("config.toml"), dir.join("state.db"));
    let mut context = reviewable_context();
    // Make the task waiting & un-acknowledged: live evidence observed after
    // the last acknowledgment (which is None), so the bridge acknowledges.
    {
        let task = context
            .registry
            .get_task_mut(&TaskId::new("web/fix-login"))
            .expect("task present");
        task.live_status_observed_at = Some(SystemTime::now());
        task.attention_acknowledged_at = None;
    }
    SqliteRegistryStore::new(&paths.state_file)
        .save(&context.registry)
        .unwrap();
    let mut bridge = CliRuntimeBridge::for_context(Some(&paths), &context).unwrap();

    let acked =
        <CliRuntimeBridge as RuntimeBridge<NoopRunner>>::acknowledge_operator_input(
            &mut bridge,
            &mut context,
            "web/fix-login",
        )
        .expect("ack ok");
    assert!(acked, "first operator input acknowledges and persists");
    assert!(
        context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap()
            .attention_acknowledged_at
            .is_some(),
        "in-context task stamped with attention_acknowledged_at"
    );

    // Persisted across reload: the saved state carries the acknowledgment.
    let reloaded = crate::context::load_context(&paths).expect("reload saved state");
    assert!(
        reloaded
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap()
            .attention_acknowledged_at
            .is_some(),
        "acknowledgment survived reload"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn acknowledge_operator_input_skips_persist_without_newer_evidence() {
    let dir = scratch_dir("ack-no-newer-evidence");
    let paths = CliContextPaths::new(dir.join("config.toml"), dir.join("state.db"));
    let mut context = reviewable_context();
    // Stamp live evidence strictly before the last ack, so needs_ack is
    // false: the operator has already acknowledged everything newer.
    let earlier = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000);
    let later = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(2_000);
    {
        let task = context
            .registry
            .get_task_mut(&TaskId::new("web/fix-login"))
            .expect("task present");
        task.live_status_observed_at = Some(earlier);
        task.attention_acknowledged_at = Some(later);
    }
    SqliteRegistryStore::new(&paths.state_file)
        .save(&context.registry)
        .unwrap();
    let mut bridge = CliRuntimeBridge::for_context(Some(&paths), &context).unwrap();

    let store = SqliteRegistryStore::new(&paths.state_file);
    let revision_before = store.current_revision().unwrap();

    let acked =
        <CliRuntimeBridge as RuntimeBridge<NoopRunner>>::acknowledge_operator_input(
            &mut bridge,
            &mut context,
            "web/fix-login",
        )
        .expect("ack ok");
    assert!(!acked, "no newer evidence => no ack");

    let revision_after = store.current_revision().unwrap();
    assert_eq!(
        revision_before, revision_after,
        "idempotent call did not persist a new revision"
    );
    // The in-context acknowledgment is unchanged too.
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap()
            .attention_acknowledged_at,
        Some(later),
        "in-context acknowledgment unchanged"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn persist_registry_snapshot_writes_diff_review_metadata_across_reload() {
    let dir = scratch_dir("diff-review-persist");
    let paths = CliContextPaths::new(dir.join("config.toml"), dir.join("state.db"));
    let mut context = reviewable_context();
    SqliteRegistryStore::new(&paths.state_file)
        .save(&context.registry)
        .unwrap();
    let mut bridge = CliRuntimeBridge::for_context(Some(&paths), &context).unwrap();

    {
        let task = context
            .registry
            .get_task_mut(&TaskId::new("web/fix-login"))
            .expect("task present");
        ajax_core::diff_review::remember_pull_requests(
            task,
            &[ajax_core::diff_review::PullRequestRef {
                number: 12,
                title: "Retry".into(),
                url: "https://example.com/12".into(),
                state: ajax_core::diff_review::PullRequestState::Open,
                head_ref: "ajax/fix-login".into(),
                head_sha: Some("abc".into()),
            }],
        );
    }

    <CliRuntimeBridge as RuntimeBridge<NoopRunner>>::persist_registry_snapshot(
        &mut bridge,
        &mut context,
    )
    .expect("persist ok");

    let reloaded = crate::context::load_context(&paths).expect("reload saved state");
    let stored = ajax_core::diff_review::stored_pull_requests(
        reloaded
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .expect("task"),
    );
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].number, 12);

    let _ = std::fs::remove_dir_all(dir);
}

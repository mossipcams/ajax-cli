#[test]
fn tasks_json_projects_core_task_identity_and_lifecycle() {
    let context = sample_context();
    let matches = build_cli()
        .try_get_matches_from(["ajax", "tasks", "--json"])
        .unwrap();
    let output = crate::snapshot_dispatch::render_snapshot_matches(&matches, &context).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["tasks"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["tasks"][0]["qualified_handle"], "web/fix-login");
    assert_eq!(parsed["tasks"][0]["lifecycle_status"], "Reviewable");
}
#[test]
fn cli_manifest_compiles_tui_and_supervisor_unconditionally() {
    let manifest = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"),
    )
    .unwrap();
    for dependency in ["ajax-supervisor", "ajax-tui", "nix", "tokio"] {
        let line = manifest
            .lines()
            .find(|line| line.trim_start().starts_with(&format!("{dependency} =")))
            .unwrap_or_else(|| panic!("{dependency} dependency should be declared"));
        assert!(
            !line.contains("optional = true"),
            "{dependency} must be unconditional: {line}"
        );
    }
    assert!(
        !manifest.contains("[features]"),
        "ajax-cli must not declare feature flags"
    );
    assert!(
        !manifest
            .lines()
            .any(|line| line.trim_start().starts_with("interactive =")),
        "ajax-cli must not declare interactive feature"
    );
    assert!(
        !manifest
            .lines()
            .any(|line| line.trim_start().starts_with("supervisor =")),
        "ajax-cli must not declare supervisor feature"
    );
    assert!(
        manifest.contains("ajax-web = { path = \"../ajax-web\", version = \""),
        "ajax-web is the always-compiled browser boundary used by the web companion"
    );
}
#[test]
fn ci_web_job_runs_mobile_webkit_smoke() {
    let workflow = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows/ci.yml"),
    )
    .unwrap();
    assert!(
        workflow.contains("npm run web:smoke -- --project=mobile-webkit"),
        "CI web job should run the mobile-WebKit smoke suite"
    );
}
#[test]
fn adapter_contract_start_dispatch_executes_core_command_plan() {
    let mut context = sample_context();
    let mut runner = RecordingCommandRunner::default();
    let matches = build_cli()
        .try_get_matches_from([
            "ajax",
            "start",
            "--repo",
            "web",
            "--title",
            "Fix logout",
            "--execute",
        ])
        .unwrap();
    let rendered = crate::execution_dispatch::render_matches_mut(
        &matches,
        &mut context,
        &mut runner,
        OpenMode::Attach,
    )
    .unwrap();
    assert!(rendered.state_changed);
    assert!(rendered
        .output
        .lines()
        .any(|line| line == "recorded task: web/fix-logout"));
    assert!(context
        .registry
        .list_tasks()
        .iter()
        .any(|task| task.qualified_handle() == "web/fix-logout"));
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
                "ajax/fix-logout",
                "/Users/matt/projects/web__worktrees/ajax-fix-logout",
                "origin/main",
            ],
        ),
        CommandSpec::new(
            "tmux",
            [
                "new-session",
                "-d",
                "-s",
                "ajax-web-fix-logout",
                "-n",
                "task",
                "-c",
                "/Users/matt/projects/web__worktrees/ajax-fix-logout",
            ],
        ),
        expected_task_launch_command(
            "ajax-web-fix-logout",
            "web/fix-logout",
            "/Users/matt/projects/web__worktrees/ajax-fix-logout",
            None,
        ),
        CommandSpec::new("tmux", ["select-window", "-t", "ajax-web-fix-logout:task"]),
        expected_new_task_open_command("ajax-web-fix-logout"),
    ]);
    assert_eq!(runner.commands(), expected_commands.as_slice());
}
#[test]
fn native_cockpit_projects_core_task_and_attention() {
    let context = sample_context();
    let snapshot = crate::cockpit_backend::build_cockpit_snapshot(&context);
    assert_eq!(snapshot.cards.len(), 1);
    assert_eq!(snapshot.cards[0].qualified_handle, "web/fix-login");
    assert_eq!(snapshot.inbox.items.len(), 1);
    assert_eq!(snapshot.inbox.items[0].task_handle, "web/fix-login");
    let frame = crate::cockpit_backend::render_cockpit_frame(&context);
    assert_eq!(frame.matches("Ajax Cockpit").count(), 1);
}
#[test]
fn native_cockpit_hides_stale_tasks_and_projects_missing_substrate_as_error() {
    let mut stale_context = sample_context();
    let stale_task = stale_context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    stale_task.remove_side_flag(SideFlag::NeedsInput);
    stale_task.add_side_flag(SideFlag::Stale);
    let stale_snapshot = crate::cockpit_backend::build_cockpit_snapshot(&stale_context);
    assert!(stale_snapshot.cards.is_empty());
    assert!(stale_snapshot.inbox.items.is_empty());
    let mut broken_context = sample_context();
    let broken_task = broken_context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    broken_task.remove_side_flag(SideFlag::NeedsInput);
    broken_task.add_side_flag(SideFlag::WorktreeMissing);
    let broken_snapshot = crate::cockpit_backend::build_cockpit_snapshot(&broken_context);
    assert_eq!(broken_snapshot.cards.len(), 1);
    assert_eq!(broken_snapshot.cards[0].qualified_handle, "web/fix-login");
    assert_eq!(
        broken_snapshot.cards[0].status,
        ajax_core::ui_state::TaskStatus::Error
    );
    assert_eq!(
        broken_snapshot.cards[0].attention,
        ajax_core::ui_state::AttentionBand::NeedsYou
    );
    assert_eq!(broken_snapshot.inbox.items.len(), 1);
    assert_eq!(broken_snapshot.inbox.items[0].task_handle, "web/fix-login");
}
#[test]
fn open_mode_uses_switch_client_only_inside_tmux() {
    assert_eq!(open_mode_from_tmux_env(None), OpenMode::Attach);
    assert_eq!(
        open_mode_from_tmux_env(Some(std::ffi::OsStr::new(""))),
        OpenMode::Attach
    );
    assert_eq!(
        open_mode_from_tmux_env(Some(std::ffi::OsStr::new("/tmp/tmux-501/default,1,0"))),
        OpenMode::SwitchClient
    );
}
fn safe_merge_context() -> CommandContext<InMemoryRegistry> {
    let mut context = sample_context();
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .remove_side_flag(SideFlag::NeedsInput);
    context
}
fn cleanable_context() -> CommandContext<InMemoryRegistry> {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task(&TaskId::new("task-1"))
        .cloned()
        .unwrap();
    let mut cleanable = task;
    cleanable.lifecycle_status = LifecycleStatus::Cleanable;
    cleanable.git_status = Some(GitStatus {
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
    context.registry = InMemoryRegistry::default();
    context.registry.create_task(cleanable).unwrap();
    context
}
fn two_cleanable_tasks_context() -> CommandContext<InMemoryRegistry> {
    let mut context = cleanable_context();
    let mut task = Task::new(
        TaskId::new("task-2"),
        "web",
        "fix-sidebar",
        "Fix sidebar",
        "ajax/fix-sidebar",
        "main",
        "/tmp/worktrees/web-fix-sidebar",
        "ajax-web-fix-sidebar",
        "task",
        AgentClient::Codex,
    );
    task.lifecycle_status = LifecycleStatus::Cleanable;
    task.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/fix-sidebar".to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: true,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: None,
    });
    context.registry.create_task(task).unwrap();
    context
}
fn two_active_tasks_context() -> CommandContext<InMemoryRegistry> {
    let mut context = sample_context();
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .lifecycle_status = LifecycleStatus::Active;
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .remove_side_flag(SideFlag::NeedsInput);
    let mut task = Task::new(
        TaskId::new("task-2"),
        "web",
        "fix-sidebar",
        "Fix sidebar",
        "ajax/fix-sidebar",
        "main",
        "/tmp/worktrees/web-fix-sidebar",
        "ajax-web-fix-sidebar",
        "task",
        AgentClient::Codex,
    );
    task.lifecycle_status = LifecycleStatus::Active;
    associate_task_with_pr(&mut task, 43, "3333333");
    context.registry.create_task(task).unwrap();
    context
}
#[derive(Default)]
struct QueuedRunner {
    outputs: std::collections::VecDeque<CommandOutput>,
    commands: Vec<CommandSpec>,
}
impl QueuedRunner {
    fn new(outputs: Vec<CommandOutput>) -> Self {
        Self {
            outputs: outputs.into(),
            commands: Vec::new(),
        }
    }
}
fn command_flow_runner(outputs: Vec<CommandOutput>) -> QueuedRunner {
    QueuedRunner::new(outputs)
}
impl CommandRunner for QueuedRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        self.commands.push(command.clone());
        self.outputs
            .pop_front()
            .ok_or_else(|| CommandRunError::SpawnFailed("missing queued output".to_string()))
    }
}
#[derive(Default)]
struct FlushingWriter {
    output: String,
    flushes: u32,
}
impl std::io::Write for FlushingWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.output.push_str(&String::from_utf8_lossy(buffer));
        Ok(buffer.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.flushes += 1;
        Ok(())
    }
}
struct OpenNewTaskTaskSessionRunner;
impl crate::tmux_task_session::TaskSessionRunner for OpenNewTaskTaskSessionRunner {
    fn run_task_session(
        &mut self,
        _command: &CommandSpec,
        _context: &crate::tmux_task_session::TaskSessionContext,
    ) -> Result<crate::tmux_task_session::TaskSessionEnd, CliError> {
        Ok(crate::tmux_task_session::TaskSessionEnd::OpenNewTask)
    }
}
#[derive(Default)]
struct RecordingTaskSessionRunner {
    commands: Vec<CommandSpec>,
}
impl crate::tmux_task_session::TaskSessionRunner for RecordingTaskSessionRunner {
    fn run_task_session(
        &mut self,
        command: &CommandSpec,
        _context: &crate::tmux_task_session::TaskSessionContext,
    ) -> Result<crate::tmux_task_session::TaskSessionEnd, CliError> {
        self.commands.push(command.clone());
        Ok(crate::tmux_task_session::TaskSessionEnd::Normal)
    }
}
struct FailingTaskSessionRunner {
    message: &'static str,
}
impl crate::tmux_task_session::TaskSessionRunner for FailingTaskSessionRunner {
    fn run_task_session(
        &mut self,
        _command: &CommandSpec,
        _context: &crate::tmux_task_session::TaskSessionContext,
    ) -> Result<crate::tmux_task_session::TaskSessionEnd, CliError> {
        Err(CliError::CommandFailed(self.message.to_string()))
    }
}
struct PanicRunner;
impl CommandRunner for PanicRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        panic!("cockpit navigation attempted to run {command:?}");
    }
}
fn output(status_code: i32, stdout: &str) -> CommandOutput {
    CommandOutput {
        status_code,
        stdout: stdout.to_string(),
        stderr: String::new(),
    }
}
fn git_live_outputs() -> Vec<CommandOutput> {
    vec![
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n",
        ),
        output(0, "main\najax/fix-login\n"),
        output(0, "origin/main\norigin/ajax/fix-login\n"),
        ]
}
fn checkout_mismatch_refresh_outputs() -> Vec<CommandOutput> {
    vec![
        output(
            0,
            "worktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/fix/pane-stuck\n\n",
        ),
        output(0, "main\najax/fix-login\nfix/pane-stuck\n"),
        output(0, "origin/main\norigin/ajax/fix-login\n"),
        ]
}
fn expected_git_observation_commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "worktree",
                "list",
                "--porcelain",
            ],
        ),
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "branch",
                "--format=%(refname:short)",
            ],
        ),
    ]
}
fn assert_git_observation_only(commands: &[CommandSpec]) {
    assert_eq!(commands, &expected_git_observation_commands());
    for command in commands {
        assert_ne!(command.program, "tmux");
        assert_ne!(command.program, "sh");
        let joined = std::iter::once(command.program.as_str())
            .chain(command.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ");
        assert!(!joined.contains("switch"));
        assert!(!joined.contains("checkout"));
    }
}
fn sample_context_with_named_checkout_mismatch() -> CommandContext<InMemoryRegistry> {
    let mut context = sample_context();
    context
        .registry
        .update_git_status(
            &TaskId::new("task-1"),
            GitStatus {
                worktree_exists: true,
                branch_exists: false,
                current_branch: Some("fix/pane-stuck".to_string()),
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
}
fn tmux_live_outputs() -> Vec<CommandOutput> {
    vec![
        output(0, "ajax-web-fix-login\n"),
        output(
            0,
            "ajax-web-fix-login\ttask\t/tmp/worktrees/web-fix-login\n",
        ),
    ]
}
fn test_agent_cache_directory(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "ajax-cli-agent-cache-{}-{}-{label}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
fn write_agent_status_event(cache_dir: &Path, task_id: &str, value: &str) {
    use crate::agent_runtime::{task_file_stem, AgentRuntimeSnapshot, AgentRuntimeState};
    let events_dir = cache_dir.join("agent-events");
    let runtime_dir = cache_dir.join("agent-runtime");
    std::fs::create_dir_all(&events_dir).unwrap();
    std::fs::create_dir_all(&runtime_dir).unwrap();
    let now_millis = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let stem = task_file_stem(task_id);
    // Wrapper runtime snapshot: alive for non-terminal, exited for terminal.
    let state = match value {
        "done" => AgentRuntimeState::ExitedSuccess,
        "failed" => AgentRuntimeState::ExitedFailure,
        _ => AgentRuntimeState::Running,
    };
    let snapshot = AgentRuntimeSnapshot {
        task_id: task_id.to_string(),
        state,
        observed_at_unix_millis: now_millis,
        pid: Some(1),
        exit_code: None,
        message: None,
    };
    std::fs::write(
        runtime_dir.join(format!("{stem}.json")),
        serde_json::to_vec(&snapshot).unwrap(),
    )
    .unwrap();
    // Canonical JSONL envelope for the native lifecycle event.
    let (kind, detail) = match value {
        "ask" => (
            "attention_requested",
            serde_json::json!({"attention": {"attention": "permission"}}),
        ),
        "wait" => (
            "attention_requested",
            serde_json::json!({"attention": {"attention": "question"}}),
        ),
        "done" => (
            "turn_settled",
            serde_json::json!({"outcome": {"outcome": "completed"}}),
        ),
        "failed" => (
            "turn_settled",
            serde_json::json!({"outcome": {"outcome": "failed"}}),
        ),
        _ => ("turn_started", serde_json::Value::Null),
    };
    let mut envelope = serde_json::json!({
        "schema_version": 1,
        "kind": kind,
        "received_at_unix_millis": now_millis,
        "occurred_at_unix_millis": now_millis,
    });
    if !detail.is_null() {
        envelope["detail"] = detail;
    }
    std::fs::write(
        events_dir.join(format!("{stem}.jsonl")),
        format!("{}\n", serde_json::to_string(&envelope).unwrap()),
    )
    .unwrap();
}
fn prepare_active_task_agent_status(
    context: &mut CommandContext<InMemoryRegistry>,
    task_id: &str,
    value: &str,
) -> PathBuf {
    let cache_dir = test_agent_cache_directory(value);
    context.runtime_paths.cache_dir = cache_dir.clone();
    write_agent_status_event(&cache_dir, task_id, value);
    let task = context
        .registry
        .get_task_mut(&TaskId::new(task_id))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.remove_side_flag(SideFlag::NeedsInput);
    cache_dir
}
fn watch_refresh_outputs() -> Vec<CommandOutput> {
    let git_worktree = output(
        0,
        "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n",
    );
    let ci_discovery = ci_pr_list_output();
    let ci_checks = ci_pr_checks_output();
    let tmux = tmux_live_outputs();
    vec![
        tmux[0].clone(),
        tmux[1].clone(),
        git_worktree.clone(),
        ci_discovery.clone(),
        ci_checks.clone(),
        tmux[0].clone(),
        tmux[1].clone(),
        git_worktree,
        ci_discovery,
        ci_checks,
    ]
}
struct AgentStatusMutatingRunner {
    inner: QueuedRunner,
    cache_dir: PathBuf,
    task_id: String,
    values: Vec<&'static str>,
    value_index: usize,
}
impl AgentStatusMutatingRunner {
    fn new(
        outputs: Vec<CommandOutput>,
        cache_dir: PathBuf,
        task_id: &str,
        values: &[&'static str],
    ) -> Self {
        Self {
            inner: QueuedRunner::new(outputs),
            cache_dir,
            task_id: task_id.to_string(),
            values: values.to_vec(),
            value_index: 0,
        }
    }
    fn update_agent_status(&self, value: &str) {
        write_agent_status_event(&self.cache_dir, &self.task_id, value);
    }
}
impl CommandRunner for AgentStatusMutatingRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        let result = self.inner.run(command)?;
        if command.program == "gh"
            && self.value_index + 1 < self.values.len()
            && command.args.first().is_some_and(|arg| arg == "pr")
            && command.args.get(1).is_some_and(|arg| arg == "checks")
        {
            self.value_index += 1;
            self.update_agent_status(self.values[self.value_index]);
        }
        Ok(result)
    }
}

use super::{
    build_cli, run_with_context, run_with_context_and_runner,
    run_with_context_and_runner_to_writer, run_with_context_paths,
    run_with_context_paths_and_runner, CliContextPaths, CliError,
};
use ajax_core::{
    adapters::{
        CommandMode, CommandOutput, CommandRunError, CommandRunner, CommandSpec,
        RecordingCommandRunner,
    },
    commands::{CommandContext, OpenMode},
    config::{Config, ManagedRepo, RuntimePathRequest},
    models::{
        AgentClient, AgentRuntimeStatus, GitStatus, LifecycleStatus, LiveObservation,
        LiveStatusKind, OperatorAction, RuntimeHealth, RuntimeObservationSource, RuntimeProjection,
        SideFlag, Task, TaskId, TmuxStatus, WorktrunkStatus,
    },
    registry::{InMemoryRegistry, Registry, RegistryStore, SqliteRegistryStore},
};
use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

fn sample_context() -> CommandContext<InMemoryRegistry> {
    let config = Config {
        repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
        ..Config::default()
    };
    let mut registry = InMemoryRegistry::default();
    let mut task = Task::new(
        TaskId::new("task-1"),
        "web",
        "fix-login",
        "Fix login",
        "ajax/fix-login",
        "main",
        "/tmp/worktrees/web-fix-login",
        "ajax-web-fix-login",
        "worktrunk",
        AgentClient::Codex,
    );
    task.lifecycle_status = LifecycleStatus::Reviewable;
    task.add_side_flag(SideFlag::NeedsInput);
    registry.create_task(task).unwrap();

    CommandContext::new(config, registry)
}

#[test]
fn snapshot_dispatch_module_routes_read_commands() {
    let context = sample_context();
    let matches = build_cli()
        .try_get_matches_from(["ajax", "tasks", "--json"])
        .unwrap();

    let output = crate::snapshot_dispatch::render_snapshot_matches(&matches, &context).unwrap();

    assert!(output.contains("\"tasks\""));
    assert!(output.contains("web/fix-login"));
}

#[test]
fn cli_manifest_exposes_lightweight_build_without_interactive_dependencies() {
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
            line.contains("optional = true"),
            "{dependency} must be optional so lightweight builds can exclude it: {line}"
        );
    }

    assert!(manifest.contains("interactive = [\"dep:ajax-tui\", \"dep:nix\"]"));
    assert!(manifest.contains("supervisor = [\"dep:ajax-supervisor\", \"dep:tokio\"]"));
    assert!(
        manifest.contains("ajax-web = { path = \"../ajax-web\", version = \""),
        "ajax-web is the always-compiled browser boundary used by the web companion"
    );
}

#[test]
fn execution_dispatch_module_routes_mutating_commands() {
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
    assert!(rendered.output.contains("recorded task: web/fix-logout"));
}

#[test]
fn cockpit_backend_module_renders_snapshot_frame() {
    let frame = crate::cockpit_backend::render_cockpit_frame(&sample_context());

    assert!(frame.contains("Ajax"));
    assert!(frame.contains("web/fix-login"));
}

#[test]
fn cockpit_snapshot_excludes_stale_tasks_but_keeps_missing_substrate_tasks_visible() {
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
    assert_eq!(broken_snapshot.inbox.items.len(), 1);
    assert_eq!(broken_snapshot.inbox.items[0].task_handle, "web/fix-login");
}

#[test]
fn open_mode_uses_switch_client_only_inside_tmux() {
    assert_eq!(super::open_mode_from_tmux_env(None), OpenMode::Attach);
    assert_eq!(
        super::open_mode_from_tmux_env(Some(std::ffi::OsStr::new(""))),
        OpenMode::Attach
    );
    assert_eq!(
        super::open_mode_from_tmux_env(Some(std::ffi::OsStr::new("/tmp/tmux-501/default,1,0"))),
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
        "worktrunk",
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
        "worktrunk",
        AgentClient::Codex,
    );
    task.lifecycle_status = LifecycleStatus::Active;
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

impl crate::task_session::TaskSessionRunner for OpenNewTaskTaskSessionRunner {
    fn run_task_session(
        &mut self,
        _command: &CommandSpec,
        _context: &crate::task_session::TaskSessionContext,
    ) -> Result<crate::task_session::TaskSessionEnd, CliError> {
        Ok(crate::task_session::TaskSessionEnd::OpenNewTask)
    }
}

#[derive(Default)]
struct RecordingTaskSessionRunner {
    commands: Vec<CommandSpec>,
}

impl crate::task_session::TaskSessionRunner for RecordingTaskSessionRunner {
    fn run_task_session(
        &mut self,
        command: &CommandSpec,
        _context: &crate::task_session::TaskSessionContext,
    ) -> Result<crate::task_session::TaskSessionEnd, CliError> {
        self.commands.push(command.clone());
        Ok(crate::task_session::TaskSessionEnd::Normal)
    }
}

struct FailingTaskSessionRunner {
    message: &'static str,
}

impl crate::task_session::TaskSessionRunner for FailingTaskSessionRunner {
    fn run_task_session(
        &mut self,
        _command: &CommandSpec,
        _context: &crate::task_session::TaskSessionContext,
    ) -> Result<crate::task_session::TaskSessionEnd, CliError> {
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
    ]
}

fn tmux_live_outputs(pane: &str) -> Vec<CommandOutput> {
    vec![
        output(0, "ajax-web-fix-login\n"),
        output(
            0,
            "ajax-web-fix-login\tworktrunk\t/tmp/worktrees/web-fix-login\n",
        ),
        output(0, pane),
    ]
}

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

fn tmux_probe_commands() -> Vec<CommandSpec> {
    tmux_live_commands().into_iter().take(3).collect()
}

fn tmux_probe_and_orphan_scan_commands() -> Vec<CommandSpec> {
    let mut commands = tmux_probe_commands();
    commands.push(CommandSpec::new(
        "git",
        [
            "-C",
            "/Users/matt/projects/web",
            "worktree",
            "list",
            "--porcelain",
        ],
    ));
    commands
}

fn tmux_live_commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("tmux", ["list-sessions", "-F", "#{session_name}"])
            .with_timeout(std::time::Duration::from_secs(8)),
        CommandSpec::new(
            "tmux",
            [
                "list-windows",
                "-a",
                "-F",
                "#{session_name}\t#{window_name}\t#{pane_current_path}",
            ],
        )
        .with_timeout(std::time::Duration::from_secs(8)),
        CommandSpec::new(
            "tmux",
            [
                "capture-pane",
                "-p",
                "-t",
                "ajax-web-fix-login:worktrunk",
                "-S",
                "-80",
            ],
        )
        .with_timeout(std::time::Duration::from_secs(8)),
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
    ]
}

fn expected_new_task_open_command(session: &str) -> CommandSpec {
    CommandSpec::new("tmux", ["attach-session", "-t", session]).with_mode(CommandMode::InheritStdio)
}

// `run_with_context_and_runner` resolves the open mode from the ambient
// `$TMUX` env var, which makes full command-sequence assertions
// non-deterministic across environments. Pin `Attach` through the dispatch
// seam so expectations stay hermetic.
fn run_start_with_attach_mode(
    args: impl IntoIterator<Item = &'static str>,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut impl ajax_core::adapters::CommandRunner,
) -> Result<String, CliError> {
    let matches = build_cli().try_get_matches_from(args).unwrap();
    crate::execution_dispatch::render_matches_mut(&matches, context, runner, OpenMode::Attach)
        .map(|rendered| rendered.output)
}

fn expected_task_launch_command(session: &str, task_id: &str, worktree_path: &str) -> CommandSpec {
    CommandSpec {
        program: "tmux".to_string(),
        args: vec![
            "send-keys".to_string(),
            "-t".to_string(),
            format!("{session}:worktrunk"),
            format!(
                "ajax-cli __agent-runtime --task-id {task_id} --state-root .cache/ajax/agent-runtime -- codex --cd {worktree_path}"
            ),
            "Enter".to_string(),
        ],
        cwd: None,
        mode: CommandMode::Capture,
        timeout: None,
    }
}

fn expected_task_setup_command(
    repo_path: &str,
    worktree_path: &str,
    bootstrap: Option<&str>,
) -> CommandSpec {
    let mut command_line = String::from("if [ -d \"$1\" ]; then cd \"$1\" && ");
    command_line.push_str(
        "if [ -f package.json ] && [ -f .husky/pre-commit ]; then npm exec --yes husky; fi",
    );
    if let Some(bootstrap) = bootstrap {
        command_line.push_str("; ");
        command_line.push_str(bootstrap);
    }
    command_line.push_str("; fi");

    CommandSpec::new(
        "sh",
        [
            "-lc",
            command_line.as_str(),
            "ajax-setup-task",
            worktree_path,
        ],
    )
    .with_cwd(repo_path)
}

fn expected_sync_default_branch_commands(repo_path: &str, branch: &str) -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("git", ["-C", repo_path, "fetch", "origin", branch])
            .with_timeout(std::time::Duration::from_secs(60)),
    ]
}

fn ajax_binary_path() -> PathBuf {
    if let Some(binary) = std::env::var_os("CARGO_BIN_EXE_ajax-cli") {
        return binary.into();
    }

    let current_exe = std::env::current_exe().unwrap();
    let deps_dir = current_exe
        .parent()
        .expect("test binary should live under target debug deps");
    let debug_dir = deps_dir
        .parent()
        .expect("test binary should live under target debug deps");
    debug_dir.join(if cfg!(windows) {
        "ajax-cli.exe"
    } else {
        "ajax-cli"
    })
}

#[test]
fn cli_error_display_omits_internal_enum_wrapping() {
    let error = CliError::CommandFailed("task title is required; pass --title".to_string());

    assert_eq!(error.to_string(), "task title is required; pass --title");
    assert!(!error.to_string().contains("CommandFailed"));
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
    assert!(stderr.contains("task title is required; pass --title"));
    assert!(!stderr.contains("CommandFailed"));
}

fn seeded_profile_homes(tag: &str) -> (PathBuf, CliContextPaths, CliContextPaths) {
    let directory = std::env::temp_dir().join(format!("ajax-cli-{tag}-{}", std::process::id()));
    let stable_paths = CliContextPaths::from_runtime_paths(
        RuntimePathRequest::new(directory.join("stable-home"))
            .with_cli_profile("stable")
            .resolve(),
    );
    let dev_paths = CliContextPaths::from_runtime_paths(
        RuntimePathRequest::new(directory.join("dev-home"))
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
    std::fs::create_dir_all(dev_paths.state_file.parent().unwrap()).unwrap();
    std::fs::write(&stable_paths.config_file, config).unwrap();
    std::fs::write(&dev_paths.config_file, config).unwrap();
    SqliteRegistryStore::new(&stable_paths.state_file)
        .save(&registry_with_task("stable-task"))
        .unwrap();
    SqliteRegistryStore::new(&dev_paths.state_file)
        .save(&registry_with_task("dev-task"))
        .unwrap();
    (directory, stable_paths, dev_paths)
}

#[test]
fn reads_use_only_the_selected_profile_db() {
    let (directory, stable_paths, dev_paths) = seeded_profile_homes("selected-db-read");
    let mut read_runner = RecordingCommandRunner::default();

    let dev_output =
        run_with_context_paths_and_runner(["ajax-cli", "tasks"], &dev_paths, &mut read_runner)
            .unwrap();

    assert!(dev_output.contains("web/dev-task"));
    assert!(!dev_output.contains("web/stable-task"));

    let mut stable_read_runner = RecordingCommandRunner::default();
    let stable_output = run_with_context_paths_and_runner(
        ["ajax-cli", "tasks"],
        &stable_paths,
        &mut stable_read_runner,
    )
    .unwrap();

    assert!(stable_output.contains("web/stable-task"));
    assert!(!stable_output.contains("web/dev-task"));

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
    super::run_with_args_to_writer(
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

    assert!(output.contains("web/dev-task"), "{output}");
    assert!(!output.contains("web/stable-task"), "{output}");

    std::fs::remove_dir_all(directory).unwrap();
}

fn registry_with_task(handle: &str) -> InMemoryRegistry {
    let mut registry = InMemoryRegistry::default();
    let mut task = Task::new(
        TaskId::new(format!("web/{handle}")),
        "web",
        handle,
        handle.replace('-', " "),
        format!("ajax/{handle}"),
        "main",
        format!("/tmp/worktrees/web-{handle}"),
        format!("ajax-web-{handle}"),
        "worktrunk",
        AgentClient::Codex,
    );
    task.lifecycle_status = LifecycleStatus::Cleanable;
    registry.create_task(task).unwrap();
    registry
}

fn cockpit_item(handle: &str, action: &str) -> ajax_core::models::CockpitActionItem {
    ajax_core::models::CockpitActionItem {
        task_id: TaskId::new(format!("__cockpit_action__{action}")),
        task_handle: handle.to_string(),
        reason: action.to_string(),
        priority: 0,
        action: action.to_string(),
    }
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

    let error = super::render_cockpit_command(&sample_context(), subcommand).unwrap_err();

    assert!(matches!(
        error,
        super::CliError::CommandFailed(message)
            if message.contains("interactive cockpit requires command execution support")
    ));
}

#[test]
fn cockpit_watch_renders_dashboard_from_backend_state() {
    let context = sample_context();
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

    assert!(output.contains("Ajax Cockpit"));
    assert!(output.contains("Inbox"));
    assert!(output.contains("web/fix-login"));
    assert!(output.contains("Ready for review"));
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
        super::CliError::CommandFailed("invalid --interval-ms value: nope".to_string())
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
        super::CliError::CommandFailed("invalid --iterations value: many".to_string())
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
    let task = context
        .registry
        .get_task(&TaskId::new("task-1"))
        .cloned()
        .unwrap();
    let mut active = task;
    active.lifecycle_status = LifecycleStatus::Active;
    active.remove_side_flag(SideFlag::NeedsInput);
    context.registry = InMemoryRegistry::default();
    context.registry.create_task(active).unwrap();
    let mut runner = QueuedRunner::new(tmux_live_outputs("Do you want to proceed? y/n\n"));

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
    assert_eq!(runner.commands, tmux_live_commands());
}

#[test]
fn cockpit_json_refreshes_live_status_even_when_projection_is_fresh() {
    let mut context = sample_context();
    {
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Active;
        task.remove_side_flag(SideFlag::NeedsInput);
        task.runtime_projection = RuntimeProjection::new(
            RuntimeHealth::Healthy,
            SystemTime::now(),
            RuntimeObservationSource::TmuxProbe,
        );
    }
    let mut runner = QueuedRunner::new(tmux_live_outputs("codex is working\n"));

    let output =
        run_with_context_and_runner(["ajax", "cockpit", "--json"], &mut context, &mut runner)
            .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert_eq!(
        parsed["tasks"]["tasks"][0]["live_status"]["summary"],
        "agent running"
    );
}

#[test]
fn cockpit_json_watch_renders_refreshed_live_status_over_iterations() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.remove_side_flag(SideFlag::NeedsInput);
    let first_refresh = tmux_live_outputs("Do you want to proceed? y/n\n");
    let second_refresh = tmux_live_outputs("codex is working\n");
    let mut runner = QueuedRunner::new(vec![
        first_refresh[0].clone(),
        first_refresh[1].clone(),
        first_refresh[2].clone(),
        output(0, ""),
        second_refresh[0].clone(),
        second_refresh[1].clone(),
        second_refresh[2].clone(),
    ]);

    let output = run_with_context_and_runner(
        [
            "ajax",
            "cockpit",
            "--json",
            "--watch",
            "--iterations",
            "2",
            "--interval-ms",
            "0",
        ],
        &mut context,
        &mut runner,
    )
    .unwrap();
    let frames: Vec<_> = output.split("\n\n").collect();

    assert_eq!(frames.len(), 2);
    let first: serde_json::Value = serde_json::from_str(frames[0]).unwrap();
    let second: serde_json::Value = serde_json::from_str(frames[1]).unwrap();
    assert_eq!(
        first["tasks"]["tasks"][0]["live_status"]["summary"],
        "waiting for approval"
    );
    assert_eq!(
        second["tasks"]["tasks"][0]["live_status"]["summary"],
        "agent running"
    );
    assert!(runner.commands.len() >= 5);
}

#[test]
fn cockpit_json_watch_streams_each_refreshed_frame_to_writer() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.remove_side_flag(SideFlag::NeedsInput);
    let first_refresh = tmux_live_outputs("Do you want to proceed? y/n\n");
    let second_refresh = tmux_live_outputs("codex is working\n");
    let mut runner = QueuedRunner::new(vec![
        first_refresh[0].clone(),
        first_refresh[1].clone(),
        first_refresh[2].clone(),
        output(0, ""),
        second_refresh[0].clone(),
        second_refresh[1].clone(),
        second_refresh[2].clone(),
    ]);
    let mut writer = FlushingWriter::default();

    let state_changed = run_with_context_and_runner_to_writer(
        [
            "ajax",
            "cockpit",
            "--json",
            "--watch",
            "--iterations",
            "2",
            "--interval-ms",
            "0",
        ],
        &mut context,
        &mut runner,
        &mut writer,
    )
    .unwrap();

    assert!(state_changed);
    assert_eq!(writer.flushes, 2);
    let frames: Vec<_> = writer.output.trim_end().split("\n\n").collect();
    assert_eq!(frames.len(), 2);
    let first: serde_json::Value = serde_json::from_str(frames[0]).unwrap();
    let second: serde_json::Value = serde_json::from_str(frames[1]).unwrap();
    assert_eq!(
        first["tasks"]["tasks"][0]["live_status"]["summary"],
        "waiting for approval"
    );
    assert_eq!(
        second["tasks"]["tasks"][0]["live_status"]["summary"],
        "agent running"
    );
}

#[test]
fn cockpit_watch_renders_refreshed_live_status_in_frame() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.remove_side_flag(SideFlag::NeedsInput);
    let mut runner = QueuedRunner::new(tmux_live_outputs("codex is working\n"));

    let output = run_with_context_and_runner(
        ["ajax", "cockpit", "--watch", "--iterations", "1"],
        &mut context,
        &mut runner,
    )
    .unwrap();

    assert!(output.contains("web/fix-login\tRunning - Agent working\tFix login"));
    assert_eq!(runner.commands, tmux_live_commands());
}

#[test]
fn status_command_refreshes_live_state_from_tmux() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.remove_side_flag(SideFlag::NeedsInput);
    let mut runner = QueuedRunner::new(tmux_live_outputs("Do you want to proceed? y/n\n"));

    let output =
        run_with_context_and_runner(["ajax", "status"], &mut context, &mut runner).unwrap();

    assert!(output.contains("web/fix-login\tWaiting - Waiting for approval\tFix login"));
    assert!(context
        .registry
        .get_task(&TaskId::new("task-1"))
        .unwrap()
        .has_side_flag(SideFlag::NeedsInput));
    assert_eq!(runner.commands, tmux_live_commands());
}

#[test]
fn status_command_renders_json_from_refreshed_live_state() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.remove_side_flag(SideFlag::NeedsInput);
    let mut runner = QueuedRunner::new(tmux_live_outputs("codex is working\n"));

    let output =
        run_with_context_and_runner(["ajax", "status", "--json"], &mut context, &mut runner)
            .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert_eq!(parsed["tasks"][0]["qualified_handle"], "web/fix-login");
    assert_eq!(
        parsed["tasks"][0]["live_status"]["summary"],
        "agent running"
    );
    assert_eq!(runner.commands, tmux_live_commands());
}

#[test]
fn read_json_commands_refresh_live_state_even_when_projection_is_fresh() {
    for command in [
        vec!["ajax", "tasks", "--json"],
        vec!["ajax", "status", "--json"],
        vec!["ajax", "cockpit", "--json"],
    ] {
        let mut context = sample_context();
        {
            let task = context
                .registry
                .get_task_mut(&TaskId::new("task-1"))
                .unwrap();
            task.lifecycle_status = LifecycleStatus::Active;
            task.remove_side_flag(SideFlag::NeedsInput);
            task.runtime_projection = RuntimeProjection::new(
                RuntimeHealth::Healthy,
                SystemTime::now(),
                RuntimeObservationSource::TmuxProbe,
            );
        }

        let mut outputs = tmux_live_outputs("codex is working\n");
        outputs.push(output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n",
        ));
        let mut runner = QueuedRunner::new(outputs);
        let output = run_with_context_and_runner(command.clone(), &mut context, &mut runner)
            .unwrap_or_else(|error| panic!("{command:?} failed: {error}"));
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        let task_json = if command[1] == "cockpit" {
            &parsed["tasks"]["tasks"]
        } else {
            &parsed["tasks"]
        };

        assert_eq!(task_json[0]["qualified_handle"], "web/fix-login");
        assert_eq!(task_json[0]["live_status"]["summary"], "agent running");
        assert_eq!(
            runner.commands,
            tmux_probe_and_orphan_scan_commands(),
            "{command:?}"
        );
    }
}

#[test]
fn read_commands_share_live_refresh_contract() {
    for args in [
        vec!["ajax", "repos", "--json"],
        vec!["ajax", "tasks", "--json"],
        vec!["ajax", "inbox", "--json"],
        vec!["ajax", "next", "--json"],
        vec!["ajax", "ready", "--json"],
        vec!["ajax", "status", "--json"],
        vec!["ajax", "cockpit", "--json"],
    ] {
        let mut context = sample_context();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Active;
        task.remove_side_flag(SideFlag::NeedsInput);
        let mut runner = QueuedRunner::new(tmux_live_outputs("codex is working\n"));

        let output = run_with_context_and_runner(args.clone(), &mut context, &mut runner)
            .unwrap_or_else(|error| panic!("{args:?} failed: {error}"));

        assert!(!output.is_empty(), "{args:?} should render a response");
        assert_eq!(runner.commands, tmux_live_commands(), "{args:?}");
    }
}

#[test]
fn read_command_skips_live_pane_probe_when_cached_runtime_is_fresh() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.remove_side_flag(SideFlag::NeedsInput);
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
    task.worktrunk_status = Some(WorktrunkStatus::present(
        "worktrunk",
        "/tmp/worktrees/web-fix-login",
    ));
    task.runtime_projection = RuntimeProjection::new(
        RuntimeHealth::Healthy,
        SystemTime::now(),
        RuntimeObservationSource::TmuxProbe,
    );
    let mut runner = QueuedRunner::default();

    let output = run_with_context_and_runner(["ajax", "tasks"], &mut context, &mut runner).unwrap();

    assert!(output.contains("web/fix-login"));
    assert!(runner.commands.is_empty());
}

#[test]
fn read_refresh_failure_keeps_task_visible_with_missing_tmux_attention() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.remove_side_flag(SideFlag::NeedsInput);
    let mut outputs = git_live_outputs();
    outputs.push(output(0, "other-session\n"));
    let mut runner = QueuedRunner::new(outputs);

    let output =
        run_with_context_and_runner(["ajax", "tasks", "--json"], &mut context, &mut runner)
            .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();

    assert_eq!(parsed["tasks"][0]["qualified_handle"], "web/fix-login");
    assert_eq!(
        parsed["tasks"][0]["live_status"]["summary"],
        "tmux session missing"
    );
    assert!(task.has_side_flag(SideFlag::TmuxMissing));
    assert_eq!(
        runner.commands,
        vec![
            tmux_live_commands()[0].clone(),
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
        ]
    );
}

#[test]
fn read_refresh_updates_stale_git_substrate_evidence() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.remove_side_flag(SideFlag::NeedsInput);
    task.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/fix-login".to_string()),
        dirty: true,
        ahead: 1,
        behind: 0,
        merged: false,
        untracked_files: 1,
        unpushed_commits: 1,
        conflicted: true,
        last_commit: Some("abc123".to_string()),
    });
    let mut runner = QueuedRunner::new(vec![
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
        ),
        output(0, "main\n"),
        output(0, "other-session\n"),
    ]);

    let output =
        run_with_context_and_runner(["ajax", "tasks", "--json"], &mut context, &mut runner)
            .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    let git_status = task.git_status.as_ref().unwrap();

    assert_eq!(parsed["tasks"][0]["qualified_handle"], "web/fix-login");
    assert!(!git_status.worktree_exists);
    assert!(!git_status.branch_exists);
    assert_eq!(git_status.current_branch, None);
    assert!(task.has_side_flag(SideFlag::WorktreeMissing));
    assert!(task.has_side_flag(SideFlag::BranchMissing));
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
            ),
            tmux_live_commands()[0].clone(),
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
        ]
    );
}

#[test]
fn cockpit_refresh_snapshot_reports_refreshed_tmux_state() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.remove_side_flag(SideFlag::NeedsInput);
    let mut runner = QueuedRunner::new(tmux_live_outputs("Do you want to proceed? y/n\n"));
    let mut state_changed = false;

    let snapshot =
        super::refresh_cockpit_snapshot(&mut context, &mut runner, &mut state_changed, &mut None)
            .unwrap();

    assert!(state_changed);
    assert_eq!(
        snapshot.cards[0].status_explanation.as_deref(),
        Some("Waiting for approval")
    );
    assert_eq!(snapshot.inbox.items[0].task_handle, "web/fix-login");
    assert_eq!(runner.commands, tmux_live_commands());
}

#[test]
fn live_refresh_clears_stale_tmux_missing_when_session_exists_without_worktrunk() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.add_side_flag(SideFlag::TmuxMissing);
    let mut runner = QueuedRunner::new(vec![
        output(0, "ajax-web-fix-login\n"),
        output(
            0,
            "ajax-web-fix-login\tagent\t/tmp/worktrees/web-fix-login\n",
        ),
    ]);
    let mut state_changed = false;

    let snapshot =
        super::refresh_cockpit_snapshot(&mut context, &mut runner, &mut state_changed, &mut None)
            .unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();

    assert!(state_changed);
    assert!(!task.has_side_flag(SideFlag::TmuxMissing));
    assert!(task.has_side_flag(SideFlag::WorktrunkMissing));
    assert_eq!(snapshot.cards.len(), 1);
    assert_eq!(snapshot.cards[0].qualified_handle, "web/fix-login");
    assert_eq!(snapshot.inbox.items.len(), 1);
    assert_eq!(snapshot.inbox.items[0].task_handle, "web/fix-login");
}

#[test]
fn live_refresh_reports_changed_when_same_status_updates_activity() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.remove_side_flag(SideFlag::NeedsInput);
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::AgentRunning,
        "agent running",
    ));
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
    task.worktrunk_status = Some(WorktrunkStatus {
        exists: true,
        window_name: "worktrunk".to_string(),
        current_path: "/tmp/worktrees/web-fix-login".into(),
        points_at_expected_path: true,
    });
    task.runtime_projection = RuntimeProjection::new(
        RuntimeHealth::Healthy,
        SystemTime::now(),
        RuntimeObservationSource::TmuxProbe,
    );
    task.last_activity_at = SystemTime::UNIX_EPOCH;
    let previous_activity = task.last_activity_at;
    let mut runner = QueuedRunner::new(tmux_live_outputs("codex is working\n"));
    let mut state_changed = false;

    let _snapshot =
        super::refresh_cockpit_snapshot(&mut context, &mut runner, &mut state_changed, &mut None)
            .unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();

    assert!(state_changed);
    assert!(task.last_activity_at > previous_activity);
    assert_eq!(
        task.live_status
            .as_ref()
            .map(|status| status.summary.as_str()),
        Some("agent running")
    );
}

#[test]
fn live_refresh_records_nonzero_session_listing_as_probe_failure() {
    let mut context = sample_context();
    let mut runner = QueuedRunner::new(vec![output(1, "ajax-web-fix-login\n")]);

    let changed = crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();

    assert!(changed);
    assert!(task.tmux_status.is_none());
    assert!(task.worktrunk_status.is_none());
    assert_eq!(
        task.runtime_projection.observation_error.as_deref(),
        Some("tmux list-sessions probe failed: exited with status 1")
    );
    assert_eq!(runner.commands, vec![tmux_live_commands()[0].clone()]);
}

#[test]
fn live_refresh_skips_cleanable_tasks_without_tmux_probe() {
    let mut context = cleanable_context();
    let mut runner = QueuedRunner::new(Vec::new());

    let changed = crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();

    assert!(!changed);
    assert!(runner.commands.is_empty());
}

#[test]
fn live_refresh_lists_tmux_windows_once_for_multiple_active_tasks() {
    let mut context = two_active_tasks_context();
    let mut runner = QueuedRunner::new(vec![
            output(0, "ajax-web-fix-login\najax-web-fix-sidebar\n"),
            output(
                0,
                "ajax-web-fix-login\tworktrunk\t/tmp/worktrees/web-fix-login\najax-web-fix-sidebar\tworktrunk\t/tmp/worktrees/web-fix-sidebar\n",
            ),
            output(0, "codex is working\n"),
            output(0, "Do you want to proceed? y/n\n"),
        ]);

    let changed = crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();

    assert!(changed);
    assert_eq!(
        runner.commands,
        vec![
            CommandSpec::new("tmux", ["list-sessions", "-F", "#{session_name}"])
                .with_timeout(std::time::Duration::from_secs(8)),
            CommandSpec::new(
                "tmux",
                [
                    "list-windows",
                    "-a",
                    "-F",
                    "#{session_name}\t#{window_name}\t#{pane_current_path}",
                ],
            )
            .with_timeout(std::time::Duration::from_secs(8)),
            CommandSpec::new(
                "tmux",
                [
                    "capture-pane",
                    "-p",
                    "-t",
                    "ajax-web-fix-login:worktrunk",
                    "-S",
                    "-80",
                ],
            )
            .with_timeout(std::time::Duration::from_secs(8)),
            CommandSpec::new(
                "tmux",
                [
                    "capture-pane",
                    "-p",
                    "-t",
                    "ajax-web-fix-sidebar:worktrunk",
                    "-S",
                    "-80",
                ],
            )
            .with_timeout(std::time::Duration::from_secs(8)),
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
        ]
    );
}

#[test]
fn live_refresh_nonzero_window_listing_preserves_evidence_and_stops_before_pane_capture() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.remove_side_flag(SideFlag::NeedsInput);
    let mut runner = QueuedRunner::new(vec![
        output(0, "ajax-web-fix-login\n"),
        output(
            1,
            "ajax-web-fix-login\tworktrunk\t/tmp/worktrees/web-fix-login\n",
        ),
    ]);

    let changed = crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();

    assert!(changed);
    let expected_commands = tmux_live_commands();
    assert_eq!(runner.commands, expected_commands[..2]);
    assert!(!task.has_side_flag(SideFlag::WorktrunkMissing));
    assert!(task.worktrunk_status.is_none());
    assert_eq!(
        task.runtime_projection.observation_error.as_deref(),
        Some("tmux list-windows probe failed: exited with status 1")
    );
}

#[test]
fn live_refresh_nonzero_pane_capture_reports_probe_failure() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.remove_side_flag(SideFlag::NeedsInput);
    let mut runner = QueuedRunner::new(vec![
        output(0, "ajax-web-fix-login\n"),
        output(
            0,
            "ajax-web-fix-login\tworktrunk\t/tmp/worktrees/web-fix-login\n",
        ),
        output(1, "codex is working\n"),
    ]);

    let changed = crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();

    assert!(changed);
    assert_eq!(runner.commands, tmux_live_commands());
    assert!(task.live_status.is_none());
    assert_eq!(
        task.runtime_projection.observation_error.as_deref(),
        Some("tmux capture-pane probe failed: exited with status 1")
    );
}

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
    assert_eq!(runner.commands, tmux_live_commands()[..2]);
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
    task.worktrunk_status = Some(WorktrunkStatus {
        exists: true,
        window_name: "worktrunk".to_string(),
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
    task.worktrunk_status = Some(WorktrunkStatus {
        exists: true,
        window_name: "worktrunk".to_string(),
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
            "ajax-web-fix-login\tworktrunk\t/tmp/worktrees/web-fix-login\n",
        ),
        output(0, ""),
    ]);

    let changed = crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();

    assert!(changed);
    assert!(!task.has_side_flag(SideFlag::TmuxMissing));
    assert!(!task.has_side_flag(SideFlag::WorktrunkMissing));
    assert_eq!(runner.commands, tmux_live_commands());
}

#[test]
fn live_refresh_updates_changed_worktrunk_status_before_pane_failure() {
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
    task.worktrunk_status = Some(WorktrunkStatus {
        exists: true,
        window_name: "worktrunk".to_string(),
        current_path: "/tmp/wrong".into(),
        points_at_expected_path: false,
    });
    let mut runner = QueuedRunner::new(vec![
        output(0, "ajax-web-fix-login\n"),
        output(
            0,
            "ajax-web-fix-login\tworktrunk\t/tmp/worktrees/web-fix-login\n",
        ),
        output(1, ""),
    ]);

    crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();

    assert_eq!(
        task.worktrunk_status
            .as_ref()
            .map(|status| status.current_path.as_path()),
        Some(Path::new("/tmp/worktrees/web-fix-login"))
    );
    assert!(task
        .worktrunk_status
        .as_ref()
        .is_some_and(|status| status.points_at_expected_path));
    assert_eq!(runner.commands, tmux_live_commands());
}

#[test]
fn live_refresh_clears_stale_worktrunk_missing_flag_when_status_matches() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.agent_status = AgentRuntimeStatus::Unknown;
    task.remove_side_flag(SideFlag::NeedsInput);
    task.add_side_flag(SideFlag::WorktrunkMissing);
    task.tmux_status = Some(TmuxStatus {
        exists: true,
        session_name: "ajax-web-fix-login".to_string(),
    });
    task.worktrunk_status = Some(WorktrunkStatus {
        exists: true,
        window_name: "worktrunk".to_string(),
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
            "ajax-web-fix-login\tworktrunk\t/tmp/worktrees/web-fix-login\n",
        ),
        output(0, ""),
    ]);

    let changed = crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();

    assert!(changed);
    assert!(!task.has_side_flag(SideFlag::WorktrunkMissing));
    assert_eq!(runner.commands, tmux_live_commands());
}

#[test]
fn live_cockpit_watch_accumulates_state_change_after_unchanged_frame() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.remove_side_flag(SideFlag::NeedsInput);
    let mut outputs = vec![output(0, "")];
    outputs.extend(tmux_live_outputs("Do you want to proceed? y/n\n"));
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
    assert_eq!(runner.commands.len(), 5);
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

    let (output, _) =
        crate::supervise::supervise_command_output_and_events(subcommand, None).unwrap();

    assert!(output.contains("process started"));
    assert!(output.contains("agent started: codex"));
    assert!(output.contains("waiting for approval: cargo test"));
    assert!(output.contains("process exited: 0"));

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

    let (output, _) =
        crate::supervise::supervise_command_output_and_events(subcommand, None).unwrap();

    assert!(output.contains("process started"));
    assert!(output.contains("agent started: cursor"));
    assert!(output.contains("waiting for approval"));
    assert!(output.contains("process exited: 0"));

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

    let error =
        crate::supervise::supervise_command_output_and_events(subcommand, None).unwrap_err();

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

    let error =
        crate::supervise::supervise_command_output_and_events(subcommand, None).unwrap_err();

    let _ = std::fs::remove_file(fake_codex);
    assert!(error.to_string().contains("codex exited with status 42"));
    assert!(error.to_string().contains("stderr: auth expired"));
}

fn write_fake_codex(tag: &str) -> PathBuf {
    let fake_codex =
        std::env::temp_dir().join(format!("ajax-cli-fake-codex-{tag}-{}", std::process::id()));
    std::fs::write(
        &fake_codex,
        "#!/bin/sh\nprintf '{\"type\":\"started\"}\\n'\n",
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&fake_codex).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    std::fs::set_permissions(&fake_codex, permissions).unwrap();
    fake_codex
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
    assert!(output.contains("agent started: codex"));
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
    assert!(output.contains("waiting for approval: cargo test"));
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

    assert!(output.contains("Usage: ajax-cli [OPTIONS] [COMMAND]"));
    assert!(output.contains("Commands:"));
}

#[test]
fn bare_command_reports_missing_subcommand_as_error() {
    let error = run_with_context(["ajax"], &sample_context()).unwrap_err();

    assert!(matches!(error, super::CliError::CommandFailed(message)
                if message.contains("command is required; pass --help")));
}

#[test]
fn readonly_context_rejects_supervise_instead_of_reporting_placeholder_success() {
    let error = run_with_context(
        ["ajax", "supervise", "--prompt", "fix tests"],
        &sample_context(),
    )
    .unwrap_err();

    assert!(matches!(error, super::CliError::CommandFailed(message)
                if message.contains("supervise requires mutable context and runner support")));
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

    assert!(output.contains("config:path\ttrue\t"));
    assert!(output.contains("state:path\ttrue\tparent directory can be created"));
    std::fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn doctor_accepts_relative_state_paths_with_creatable_parents() {
    assert!(super::parent_directory_available(Path::new("ajax.db")));
    assert!(super::parent_directory_available(Path::new(
        "state/ajax.db"
    )));
}

struct RecoveryRunner {
    commands: Vec<CommandSpec>,
}

impl RecoveryRunner {
    fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }
}

impl CommandRunner for RecoveryRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        self.commands.push(command.clone());
        let stdout = match command.args.as_slice() {
                [_, repo, subcommand, action, flag]
                    if repo == "/Users/matt/projects/web"
                        && subcommand == "worktree"
                        && action == "list"
                        && flag == "--porcelain" =>
                {
                    "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /Users/matt/projects/web__worktrees/ajax-code\nHEAD 2222222\nbranch refs/heads/ajax/code\n\nworktree /Users/matt/projects/web__worktrees/other-topic\nHEAD 3333333\nbranch refs/heads/topic\n\n"
                }
                [command, ..] if command == "list-sessions" => {
                    "ajax-web-existing\najax-web-code\n"
                }
                [command, ..] if command == "list-windows" => {
                    "worktrunk\t/Users/matt/projects/web__worktrees/ajax-code\n"
                }
                [command, ..] if command == "capture-pane" => "codex is working\n",
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
        "worktrunk",
        AgentClient::Codex,
    );
    existing.lifecycle_status = LifecycleStatus::Active;
    registry.create_task(existing).unwrap();
    SqliteRegistryStore::new(&state_file)
        .save(&registry)
        .unwrap();
    let mut first_runner = RecoveryRunner::new();

    let first_output =
        run_with_context_paths_and_runner(["ajax", "tasks"], &paths, &mut first_runner).unwrap();

    assert!(first_output.contains("web/code"));
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
    let second_output =
        run_with_context_paths_and_runner(["ajax", "tasks"], &paths, &mut second_runner).unwrap();
    let saved_again = SqliteRegistryStore::new(&state_file).load().unwrap();

    assert!(second_output.contains("web/code"));
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

    assert!(output.contains("exported state snapshot"));
    assert!(snapshot.contains("\"repo\": \"web\""));
    assert!(snapshot.contains("\"handle\": \"fix-login\""));
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
    for args in [
        vec!["ajax", "resume"],
        vec!["ajax", "repair"],
        vec!["ajax", "repair"],
        vec!["ajax", "review"],
        vec!["ajax", "ship"],
        vec!["ajax", "drop"],
    ] {
        let error = run_with_context(args.clone(), &sample_context()).unwrap_err();
        assert!(
            matches!(error, super::CliError::CommandFailed(ref message) if message.contains("required")),
            "{args:?} should require task arg, got {error:?}"
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

    assert!(workspace_manifest.contains("ratatui = { version = \"0.30\""));
    assert!(tui_manifest.contains("default-features = false"));
    assert!(tui_manifest.contains("\"crossterm\""));
    assert!(tui_manifest.contains("\"underline-color\""));
    assert!(tui_manifest.contains("\"layout-cache\""));
    assert!(!tui_manifest.contains("all-widgets"));
    assert!(workspace_manifest.contains("rust-version = \"1.88\""));
    assert!(toolchain.contains("channel = \"1.88.0\""));
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

    assert!(output.contains("create task: fix logout"));
    assert!(output.contains("git -C /Users/matt/projects/web worktree add -b ajax/fix-logout /Users/matt/projects/web__worktrees/ajax-fix-logout origin/main"));
    assert!(output.contains("tmux new-session -d -s ajax-web-fix-logout -n worktrunk -c /Users/matt/projects/web__worktrees/ajax-fix-logout"));
    assert!(output.contains("tmux send-keys -t ajax-web-fix-logout:worktrunk"));
}

#[test]
fn new_command_requires_task_title() {
    let error =
        run_with_context(["ajax", "start", "--repo", "web"], &sample_context()).unwrap_err();

    assert!(matches!(error, super::CliError::CommandFailed(message)
            if message.contains("task title is required")));
}

#[test]
fn repos_command_renders_human_output() {
    let context = sample_context();
    let output = run_with_context(["ajax", "repos"], &context).unwrap();

    assert!(output.contains("web"));
    assert!(output.contains("/Users/matt/projects/web"));
}

#[test]
fn tasks_command_renders_json_output() {
    let context = sample_context();
    let output = run_with_context(["ajax", "tasks", "--json"], &context).unwrap();

    assert!(output.contains("\"tasks\""));
    assert!(output.contains("web/fix-login"));
}

#[test]
fn inspect_reports_missing_task_as_error() {
    let context = sample_context();
    let error = run_with_context(["ajax", "inspect", "web/missing"], &context).unwrap_err();

    assert_eq!(
        error,
        super::CliError::CommandFailed("task not found: web/missing".to_string())
    );
}

#[test]
fn open_command_renders_command_plan() {
    let context = sample_context();
    let output = run_with_context(["ajax", "resume", "web/fix-login"], &context).unwrap();

    assert!(output.contains("tmux select-window -t ajax-web-fix-login:worktrunk"));
    match super::current_open_mode() {
        OpenMode::Attach => {
            assert!(output.contains("tmux attach-session -t ajax-web-fix-login"));
        }
        OpenMode::SwitchClient => {
            assert!(output.contains("tmux switch-client -t ajax-web-fix-login"));
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

    super::render_task_command(
        super::TaskCommandKind::Resume,
        subcommand,
        &mut context,
        &mut runner,
        OpenMode::SwitchClient,
    )
    .unwrap();

    assert_eq!(
        runner.commands(),
        &[
            CommandSpec::new(
                "tmux",
                ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
            ),
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

    assert!(matches!(error, super::CliError::CommandFailed(message)
                if message.contains("execution requires mutable context and runner support")));
}

#[test]
fn merge_command_renders_json_plan() {
    let context = sample_context();
    let output = run_with_context(["ajax", "ship", "web/fix-login", "--json"], &context).unwrap();

    assert!(output.contains("\"requires_confirmation\": true"));
    assert!(output.contains("\"program\": \"git\""));
    assert!(output.contains("\"merge\""));
}

#[test]
fn repair_command_renders_configured_test_plan() {
    let mut context = sample_context();
    context.config.test_commands = vec![ajax_core::config::TestCommand::new("web", "cargo test")];

    let output = run_with_context(["ajax", "repair", "web/fix-login"], &context).unwrap();

    assert!(output.contains("repair task: web/fix-login"));
    assert!(output.contains("(cd /tmp/worktrees/web-fix-login && sh -lc 'cargo test')"));
}

#[test]
fn review_command_renders_diff_summary_plan() {
    let context = sample_context();
    let output = run_with_context(["ajax", "review", "web/fix-login"], &context).unwrap();

    assert!(output.contains("diff task: web/fix-login"));
    assert!(output
        .contains("(cd /tmp/worktrees/web-fix-login && git diff --stat main...ajax/fix-login)"));
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

    assert!(output.contains("\"tasks\""));
    assert!(output.contains("web/fix-login"));
    assert!(output.contains("Reviewable"));
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
    assert!(output.contains("web/fix-login"));
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

    assert!(output.contains("\"tasks\": []"));
    assert!(!output.contains("web/fix-login"));
}

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
    assert!(
        matches!(error, super::CliError::ContextLoad(message) if message.contains("legacy JSON state is unsupported") && !message.contains("file is not a database"))
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
    assert!(matches!(error, super::CliError::ContextLoad(message)
                if message.contains("state load failed: database error:")
                    && !message.contains("Database(")));
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

    assert!(output.contains("recorded task: web/fix-login"));
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
                "worktrunk",
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
        CommandSpec::new(
            "tmux",
            ["select-window", "-t", "ajax-web-fix-login:worktrunk"],
        ),
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
                "worktrunk",
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
        CommandSpec::new(
            "tmux",
            ["select-window", "-t", "ajax-web-fix-login:worktrunk"],
        ),
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

    assert!(matches!(error, super::CliError::CommandFailed(message)
                if message.contains("task already exists: web/fix-login")));
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
        matches!(error, super::CliError::CommandFailedAfterStateChange(message)
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
                "worktrunk",
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
        matches!(error, super::CliError::CommandFailedAfterStateChange(message)
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
                "worktrunk",
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
        matches!(error, super::CliError::CommandFailedAfterStateChange(message)
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
        "worktrunk",
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

    assert!(output.contains("recorded task: web/fix-login"));
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
                "worktrunk",
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
        CommandSpec::new(
            "tmux",
            ["select-window", "-t", "ajax-web-fix-login:worktrunk"],
        ),
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

    assert!(matches!(error, super::CliError::CommandFailed(message)
            if message.contains("task title is required")));
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
    assert!(output.contains("recorded task: web/fix-login"));
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
        matches!(error, super::CliError::CommandFailedAfterStateChange(message)
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
        super::CliError::CommandFailed("confirmation required; pass --yes".to_string())
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
        matches!(error, super::CliError::CommandFailedAfterStateChange(message)
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

    assert!(
        matches!(&error, super::CliError::CommandFailedAfterStateChange(message)
                if message == "command failed: git exited with status 42: merge failed")
    );
    assert!(!error.to_string().contains("NonZeroExit"));
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
            output(0, ""),
            output(0, ""),
            output(0, ""),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
            ),
            output(0, "main\n"),
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
    assert_eq!(runner.commands[3].program, "sh");
    assert_eq!(runner.commands[3].args[0], "-c");
    assert_eq!(
        runner.commands[3].args[1],
        "mkdir -p \"$(dirname \"$3\")\" && { [ ! -e \"$2\" ] || mv \"$2\" \"$3\"; } && { git -C \"$1\" worktree prune || git -C \"$1\" worktree remove --force \"$2\"; } && { rm -rf \"$3\" >/dev/null 2>&1 & }"
    );
    assert_eq!(runner.commands[3].args[2], "ajax-fast-worktree-remove");
    assert_eq!(runner.commands[3].args[3], "/Users/matt/projects/web");
    assert_eq!(runner.commands[3].args[4], "/tmp/worktrees/web-fix-login");
    assert!(runner.commands[3].args[5].starts_with("/tmp/worktrees/.ajax-trash/fix-login-"));
    assert_eq!(
        runner.commands[4],
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "branch",
                "-D",
                "ajax/fix-login"
            ]
        )
    );
    assert_eq!(
        runner.commands[5],
        CommandSpec::new("tmux", ["list-sessions", "-F", "#{session_name}"])
            .with_timeout(std::time::Duration::from_secs(8))
    );
    assert_eq!(
        runner.commands[6],
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
        runner.commands[7],
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
        output(0, ""),
        output(0, ""),
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
        ),
        output(0, "main\n"),
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
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "branch",
                    "-d",
                    "ajax/fix-login"
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
    assert!(context.registry.get_task(&TaskId::new("task-1")).is_none());
}

#[test]
fn cleanup_execute_uses_safe_cleanup_path() {
    let mut context = cleanable_context();
    let mut runner = QueuedRunner::new(vec![
            output(0, ""),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n",
            ),
            output(0, "main\najax/fix-login\n"),
            output(0, ""),
            output(0, ""),
            output(0, ""),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
            ),
            output(0, "main\n"),
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
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "worktree",
                    "remove",
                    "/tmp/worktrees/web-fix-login"
                ]
            ),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "branch",
                    "-d",
                    "ajax/fix-login"
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
        super::CliError::CommandFailed("confirmation required; pass --yes".to_string())
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
            output(0, ""),
            output(0, ""),
            output(0, ""),
            output(0, ""),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
            ),
            output(0, "main\n"),
        ]);

    run_with_context_and_runner(
        ["ajax", "drop", "web/fix-login", "--execute", "--yes"],
        &mut context,
        &mut runner,
    )
    .unwrap();

    assert_eq!(runner.commands.len(), 9);
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
    assert_eq!(runner.commands[3].program, "sh");
    assert_eq!(runner.commands[3].args[0], "-c");
    assert_eq!(
        runner.commands[3].args[1],
        "mkdir -p \"$(dirname \"$3\")\" && { [ ! -e \"$2\" ] || mv \"$2\" \"$3\"; } && { git -C \"$1\" worktree prune || git -C \"$1\" worktree remove --force \"$2\"; } && { rm -rf \"$3\" >/dev/null 2>&1 & }"
    );
    assert_eq!(runner.commands[3].args[2], "ajax-fast-worktree-remove");
    assert_eq!(runner.commands[3].args[3], "/Users/matt/projects/web");
    assert_eq!(runner.commands[3].args[4], "/tmp/worktrees/web-fix-login");
    assert!(runner.commands[3].args[5].starts_with("/tmp/worktrees/.ajax-trash/fix-login-"));
    assert_eq!(
        runner.commands[4],
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "branch",
                "-D",
                "ajax/fix-login"
            ]
        )
    );
    assert_eq!(
        runner.commands[5],
        CommandSpec::new("tmux", ["kill-session", "-t", "ajax-web-fix-login"])
    );
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
        super::CliError::CommandFailed("confirmation required; pass --yes".to_string())
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
            output(0, ""),
            output(0, ""),
            output(0, ""),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
            ),
            output(0, "main\n"),
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
    assert_eq!(runner.commands[3].program, "sh");
    assert_eq!(runner.commands[3].args[0], "-c");
    assert_eq!(
        runner.commands[3].args[1],
        "mkdir -p \"$(dirname \"$3\")\" && { [ ! -e \"$2\" ] || mv \"$2\" \"$3\"; } && { git -C \"$1\" worktree prune || git -C \"$1\" worktree remove --force \"$2\"; } && { rm -rf \"$3\" >/dev/null 2>&1 & }"
    );
    assert_eq!(runner.commands[3].args[2], "ajax-fast-worktree-remove");
    assert_eq!(runner.commands[3].args[3], "/Users/matt/projects/web");
    assert_eq!(runner.commands[3].args[4], "/tmp/worktrees/web-fix-login");
    assert!(runner.commands[3].args[5].starts_with("/tmp/worktrees/.ajax-trash/fix-login-"));
    assert_eq!(
        runner.commands[4],
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "branch",
                "-D",
                "ajax/fix-login"
            ]
        )
    );
    assert_eq!(
        runner.commands[5],
        CommandSpec::new("tmux", ["list-sessions", "-F", "#{session_name}"])
            .with_timeout(std::time::Duration::from_secs(8))
    );
    assert_eq!(
        runner.commands[6],
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
        runner.commands[7],
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
    task.worktrunk_status = Some(WorktrunkStatus::present(
        "worktrunk",
        "/tmp/worktrees/web-fix-login",
    ));
    let mut runner = QueuedRunner::new(vec![
            output(0, ""),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n",
            ),
            output(0, "main\najax/fix-login\n"),
            output(0, ""),
            output(0, ""),
            output(0, ""),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
            ),
            output(0, "main\n"),
        ]);

    run_with_context_and_runner(
        ["ajax", "drop", "web/fix-login", "--execute"],
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
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "worktree",
                    "remove",
                    "/tmp/worktrees/web-fix-login"
                ]
            ),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "branch",
                    "-d",
                    "ajax/fix-login"
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
        output(0, ""),
        output(0, ""),
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
        ),
        output(0, "main\n"),
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
        output(0, ""),
        output(0, ""),
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
        ),
        output(0, "main\n"),
    ]);

    run_with_context_and_runner(
        ["ajax", "drop", "web/fix-login", "--execute"],
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
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "branch",
                    "-d",
                    "ajax/fix-login"
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
            output(0, ""),
            output(0, ""),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
            ),
            output(0, "main\n"),
        ]);

    run_with_context_and_runner(
        ["ajax", "drop", "web/fix-login", "--execute"],
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
        ]);

    run_with_context_and_runner(
        ["ajax", "drop", "web/fix-login", "--execute", "--yes"],
        &mut context,
        &mut runner,
    )
    .unwrap();

    assert!(context.registry.get_task(&TaskId::new("task-1")).is_none());
}

#[test]
fn drop_execute_keeps_task_when_worktree_remove_fails_before_tmux_session_kill() {
    let mut context = cleanable_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
    task.worktrunk_status = Some(WorktrunkStatus::present(
        "worktrunk",
        "/tmp/worktrees/web-fix-login",
    ));
    let mut runner = QueuedRunner::new(vec![
        output(0, "ajax-web-fix-login\n"),
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n",
        ),
        output(0, "main\najax/fix-login\n"),
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
    assert!(task
        .metadata
        .get("drop_failed_detail")
        .is_some_and(|detail| detail.contains("permission denied")));
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
    task.worktrunk_status = Some(WorktrunkStatus::present(
        "worktrunk",
        "/tmp/worktrees/web-fix-login",
    ));
    let mut runner = QueuedRunner::new(vec![
            output(0, "ajax-web-fix-login\n"),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n",
            ),
            output(0, "main\najax/fix-login\n"),
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
        ]);

    let error = run_with_context_and_runner(
        ["ajax", "drop", "web/fix-login", "--execute"],
        &mut context,
        &mut runner,
    )
    .unwrap_err();

    assert!(
        matches!(error, super::CliError::CommandFailedAfterStateChange(message)
                if message.contains("drop incomplete for web/fix-login at delete branch")
                    && message.contains("branch delete failed")
                    && message.contains("retry with `ajax drop web/fix-login --execute`"))
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
        output(0, ""),
        output(0, ""),
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
        ),
        output(0, "main\n"),
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
fn repair_execute_repairs_trunk_with_injected_runner() {
    let mut context = sample_context();
    let mut runner = RecordingCommandRunner::default();
    let matches = build_cli()
        .try_get_matches_from(["ajax", "repair", "web/fix-login", "--execute"])
        .unwrap();
    let (_, subcommand) = matches.subcommand().unwrap();

    super::render_task_command(
        super::TaskCommandKind::Repair,
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
                    "worktrunk",
                    "-c",
                    "/tmp/worktrees/web-fix-login"
                ]
            ),
            CommandSpec::new(
                "tmux",
                ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
            ),
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

    super::render_task_command(
        super::TaskCommandKind::Repair,
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
fn repair_execute_clears_missing_tmux_and_worktrunk_flags() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.add_side_flag(SideFlag::TmuxMissing);
    task.add_side_flag(SideFlag::WorktrunkMissing);
    task.tmux_status = Some(TmuxStatus {
        exists: false,
        session_name: "ajax-web-fix-login".to_string(),
    });
    task.worktrunk_status = Some(WorktrunkStatus {
        exists: false,
        window_name: "worktrunk".to_string(),
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
    assert!(!task.has_side_flag(SideFlag::WorktrunkMissing));
    assert_eq!(
        task.tmux_status,
        Some(TmuxStatus::present("ajax-web-fix-login"))
    );
    assert_eq!(
        task.worktrunk_status,
        Some(WorktrunkStatus::present(
            "worktrunk",
            "/tmp/worktrees/web-fix-login"
        ))
    );
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
    super::render_task_command(
        super::TaskCommandKind::Repair,
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
                    "worktrunk",
                    "-c",
                    "/tmp/worktrees/web-fix-login",
                ],
            ),
            CommandSpec::new(
                "tmux",
                ["select-window", "-t", "ajax-web-fix-login:worktrunk"],
            ),
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
        matches!(error, super::CliError::CommandFailedAfterStateChange(message)
                if message == "command failed: sh exited with status 42 in /tmp/worktrees/web-fix-login: tests failed")
    );
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
    assert!(task.has_side_flag(SideFlag::TestsFailed));
    assert_eq!(
        task.live_status
            .as_ref()
            .map(|status| (status.kind, status.summary.as_str())),
        Some((LiveStatusKind::CommandFailed, "check failed"))
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
        &[
            CommandSpec::new("git", ["diff", "--stat", "main...ajax/fix-login"])
                .with_cwd("/tmp/worktrees/web-fix-login")
        ]
    );
}

#[test]
fn sweep_execute_uses_injected_runner_and_marks_safe_tasks_removed() {
    let mut context = cleanable_context();
    let mut runner_outputs = vec![output(0, "")];
    runner_outputs.extend(vec![output(0, ""), output(0, "")]);
    runner_outputs.extend(vec![output(0, ""), output(0, ""), output(0, "")]);
    let mut runner = QueuedRunner::new(runner_outputs);

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
    assert!(error.to_string().contains("exited with status 2"));
    assert!(error.to_string().contains("worktree remove failed"));
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
    let outcome = super::tui_cockpit_action(&item, &mut context).unwrap();

    match outcome {
        ajax_tui::ActionOutcome::Message(message) => {
            assert!(message.contains("select a project"));
            assert!(message.contains("start"));
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
        let outcome = super::tui_cockpit_action(&item, &mut context).unwrap();

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

#[test]
fn cockpit_known_actions_never_return_command_hints() {
    for (handle, action) in [
        ("web/fix-login", "resume"),
        ("web/fix-login", "ship"),
        ("web", "start"),
        ("web", "status"),
    ] {
        let mut context = sample_context();
        let item = ajax_core::models::CockpitActionItem {
            task_id: TaskId::new(format!("__cockpit_action__{action}")),
            task_handle: handle.to_string(),
            reason: action.to_string(),
            priority: 0,
            action: action.to_string(),
        };
        let outcome = super::tui_cockpit_action(&item, &mut context).unwrap();

        if let ajax_tui::ActionOutcome::Message(message) = outcome {
            assert!(!message.contains("try: ajax"), "{action}: {message}");
            assert!(!message.contains("run `ajax"), "{action}: {message}");
        }
    }

    let mut context = cleanable_context();
    let item = cockpit_item("web/fix-login", "drop");
    let outcome = super::tui_cockpit_action(&item, &mut context).unwrap();

    if let ajax_tui::ActionOutcome::Message(message) = outcome {
        assert!(!message.contains("try: ajax"), "drop task: {message}");
        assert!(!message.contains("run `ajax"), "drop task: {message}");
    }
}

#[test]
fn removed_cockpit_task_actions_are_unknown() {
    let mut context = sample_context();
    for action in [
        "inspect task",
        "inspect agent",
        "inspect test output",
        "monitor task",
        "review branch",
        "review diff",
    ] {
        let item = cockpit_item("web/fix-login", action);
        let outcome = super::tui_cockpit_action(&item, &mut context).unwrap();

        match outcome {
            ajax_tui::ActionOutcome::Message(message) => {
                assert!(message.contains(action), "{action}: {message}");
                assert!(!message.contains("try: ajax"), "{action}: {message}");
            }
            _ => panic!("{action} should be an unknown cockpit action"),
        }
    }
}

#[test]
fn cockpit_unknown_action_does_not_suggest_shell_command() {
    let mut context = sample_context();
    let item = cockpit_item("web/fix-login", "mystery action");
    let outcome = super::tui_cockpit_action(&item, &mut context).unwrap();

    match outcome {
        ajax_tui::ActionOutcome::Message(message) => {
            assert!(message.contains("mystery action"));
            assert!(!message.contains("try: ajax"));
            assert!(!message.contains("run `ajax"));
        }
        _ => panic!("unknown cockpit action should stay in cockpit"),
    }
}

#[test]
fn cockpit_action_contract_covers_all_current_actions() {
    enum Expected<'a> {
        Defer,
        Message(&'a [&'a str]),
        RefreshAndDefer,
    }

    let cases = [
        (
            "start",
            "web",
            Expected::Message(&["select a project", "start"]),
        ),
        ("resume", "web/fix-login", Expected::Defer),
        ("review", "web/fix-login", Expected::Defer),
        ("ship", "web/fix-login", Expected::Defer),
        ("drop", "web/fix-login", Expected::RefreshAndDefer),
        ("repair", "web/fix-login", Expected::Defer),
        ("status", "web", Expected::Message(&["web: 1 task(s)"])),
    ];
    let covered_actions = cases
        .iter()
        .map(|(action, _, _)| *action)
        .collect::<std::collections::BTreeSet<_>>();
    let product_actions = OperatorAction::all()
        .iter()
        .map(|action| action.as_str())
        .chain(std::iter::once("status"))
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(covered_actions, product_actions);

    for (action, handle, expected) in cases {
        let mut context = if action == "drop" {
            cleanable_context()
        } else {
            sample_context()
        };
        let item = cockpit_item(handle, action);
        let outcome = super::tui_cockpit_action(&item, &mut context).unwrap();

        match expected {
            Expected::Defer => match outcome {
                ajax_tui::ActionOutcome::Defer(pending) => {
                    assert_eq!(pending.task_handle, handle, "{action}");
                    assert_eq!(pending.action, action);
                    assert!(pending.task_title.is_none(), "{action}");
                }
                ajax_tui::ActionOutcome::Message(message) => {
                    panic!("{action} should defer, got message: {message}");
                }
                ajax_tui::ActionOutcome::Confirm(message) => {
                    panic!("{action} should defer, got confirm: {message}");
                }
                ajax_tui::ActionOutcome::Refresh { .. } => {
                    panic!("{action} should defer, got refresh");
                }
                ajax_tui::ActionOutcome::RefreshAndDefer(_, _) => {
                    panic!("{action} should defer without refreshing first");
                }
            },
            Expected::Message(parts) => match outcome {
                ajax_tui::ActionOutcome::Message(message) => {
                    for part in parts {
                        assert!(message.contains(part), "{action}: missing {part:?}");
                    }
                }
                ajax_tui::ActionOutcome::Defer(_) => {
                    panic!("{action} should render in cockpit, got defer");
                }
                ajax_tui::ActionOutcome::Confirm(message) => {
                    panic!("{action} should render in cockpit, got confirm: {message}");
                }
                ajax_tui::ActionOutcome::Refresh(_) => {
                    panic!("{action} should render in cockpit, got refresh");
                }
                ajax_tui::ActionOutcome::RefreshAndDefer(_, _) => {
                    panic!("{action} should render in cockpit, got refresh and defer");
                }
            },
            Expected::RefreshAndDefer => match outcome {
                ajax_tui::ActionOutcome::RefreshAndDefer(snapshot, pending) => {
                    assert_eq!(snapshot.repos.repos.len(), 1, "{action}");
                    assert!(snapshot.cards.is_empty(), "{action}");
                    assert!(snapshot.inbox.items.is_empty(), "{action}");
                    assert_eq!(pending.task_handle, handle, "{action}");
                    assert_eq!(pending.action, action, "{action}");
                }
                ajax_tui::ActionOutcome::Defer(_) => {
                    panic!("{action} should refresh before deferring, got defer");
                }
                ajax_tui::ActionOutcome::Message(message) => {
                    panic!("{action} should refresh before deferring, got message: {message}");
                }
                ajax_tui::ActionOutcome::Confirm(message) => {
                    panic!("{action} should refresh before deferring, got confirm: {message}");
                }
                ajax_tui::ActionOutcome::Refresh(_) => {
                    panic!("{action} should defer backend cleanup after refresh");
                }
            },
        }
    }
}

#[test]
fn cockpit_merge_task_action_stays_inside_ajax() {
    let mut context = sample_context();
    let item = ajax_core::models::CockpitActionItem {
        task_id: TaskId::new("__task_action__web_fix_login__merge"),
        task_handle: "web/fix-login".to_string(),
        reason: "Merge task".to_string(),
        priority: 0,
        action: "ship".to_string(),
    };
    let outcome = super::tui_cockpit_action(&item, &mut context).unwrap();

    match outcome {
        ajax_tui::ActionOutcome::Defer(pending) => {
            assert_eq!(pending.task_handle, "web/fix-login");
            assert_eq!(pending.action, "ship");
            assert!(pending.task_title.is_none());
        }
        _ => panic!("completed task action should defer for execution"),
    }
}

#[test]
fn cockpit_task_action_return_stays_inside_ajax() {
    let mut context = sample_context();
    let item = ajax_core::models::CockpitActionItem {
        task_id: TaskId::new("__task_action__web_fix_login__open"),
        task_handle: "web/fix-login".to_string(),
        reason: "Open task".to_string(),
        priority: 0,
        action: "resume".to_string(),
    };
    let outcome = super::tui_cockpit_action(&item, &mut context).unwrap();

    match outcome {
        ajax_tui::ActionOutcome::Defer(pending) => {
            assert_eq!(pending.task_handle, "web/fix-login");
            assert_eq!(pending.action, "resume");
            assert!(pending.task_title.is_none());
        }
        _ => panic!("task action should defer for execution"),
    }
}

#[test]
fn pending_new_task_action_requires_completed_title() {
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
    let pending = ajax_tui::PendingAction {
        task_handle: "api".to_string(),
        action: "start".to_string(),
        task_title: None,
    };
    let mut runner = PanicRunner;
    let mut state_changed = false;

    let error = super::execute_pending_cockpit_action(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
    )
    .unwrap_err();

    assert!(matches!(error, super::CliError::CommandFailed(message)
                if message.contains("start task title is required")));
    assert!(context.registry.list_tasks().is_empty());
    assert!(!state_changed);
}

#[test]
fn pending_new_task_action_does_not_run_without_title() {
    let mut context = CommandContext::new(
        Config {
            repos: vec![ManagedRepo::new("api", "/Users/matt/projects/api", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    );
    let pending = ajax_tui::PendingAction {
        task_handle: "api".to_string(),
        action: "start".to_string(),
        task_title: None,
    };
    let mut runner = QueuedRunner::new(vec![output(1, "")]);
    let mut state_changed = false;

    let error = super::execute_pending_cockpit_action(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
    )
    .unwrap_err();

    assert!(matches!(error, super::CliError::CommandFailed(message)
                if message.contains("start task title is required")));
    assert!(runner.commands.is_empty());
    assert!(context.registry.list_tasks().is_empty());
    assert!(!state_changed);
}

#[test]
fn failed_pending_new_task_action_marks_state_changed_for_cockpit_recovery() {
    let mut context = CommandContext::new(
        Config {
            repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    );
    let pending = ajax_tui::PendingAction {
        task_handle: "web".to_string(),
        action: "start".to_string(),
        task_title: Some("Fix login".to_string()),
    };
    let mut runner = QueuedRunner::new(vec![CommandOutput {
        status_code: 42,
        stdout: String::new(),
        stderr: "git failed".to_string(),
    }]);
    let mut state_changed = false;

    let error = crate::cockpit_actions::execute_pending_cockpit_action_with_open_mode(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
        OpenMode::Attach,
    )
    .unwrap_err();

    assert!(
        matches!(error, super::CliError::CommandFailedAfterStateChange(message)
                if message == "command failed: git exited with status 42: git failed")
    );
    assert!(state_changed);
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == "web/fix-login")
        .expect("failed cockpit create should leave a visible task");
    assert_eq!(task.lifecycle_status, LifecycleStatus::Error);
    let tasks = ajax_core::commands::list_tasks(&context, None);
    assert_eq!(
        tasks.tasks[0].actions,
        vec![
            OperatorAction::Resume.as_str().to_string(),
            OperatorAction::Drop.as_str().to_string(),
        ]
    );
    let inbox = ajax_core::commands::inbox(&context);
    assert_eq!(inbox.items.len(), 1);
    assert_eq!(inbox.items[0].action, OperatorAction::Resume);
}

#[test]
fn pending_new_task_action_runs_after_title_is_collected() {
    let mut context = CommandContext::new(
        Config {
            repos: vec![ManagedRepo::new("api", "/Users/matt/projects/api", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    );
    let pending = ajax_tui::PendingAction {
        task_handle: "api".to_string(),
        action: "start".to_string(),
        task_title: Some("Fix login".to_string()),
    };
    let mut runner = RecordingCommandRunner::default();
    let mut state_changed = false;

    let outcome = crate::cockpit_actions::execute_pending_cockpit_action_with_open_mode(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
        OpenMode::Attach,
    )
    .unwrap();

    assert!(outcome
        .as_deref()
        .is_some_and(|output| output.contains("recorded task: api/fix-login")));
    let mut expected_commands =
        expected_sync_default_branch_commands("/Users/matt/projects/api", "main");
    expected_commands.extend([
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/api",
                "worktree",
                "add",
                "-b",
                "ajax/fix-login",
                "/Users/matt/projects/api__worktrees/ajax-fix-login",
                "origin/main",
            ],
        ),
        CommandSpec::new(
            "tmux",
            [
                "new-session",
                "-d",
                "-s",
                "ajax-api-fix-login",
                "-n",
                "worktrunk",
                "-c",
                "/Users/matt/projects/api__worktrees/ajax-fix-login",
            ],
        ),
        expected_task_setup_command(
            "/Users/matt/projects/api",
            "/Users/matt/projects/api__worktrees/ajax-fix-login",
            None,
        ),
        expected_task_launch_command(
            "ajax-api-fix-login",
            "api/fix-login",
            "/Users/matt/projects/api__worktrees/ajax-fix-login",
        ),
        CommandSpec::new(
            "tmux",
            ["select-window", "-t", "ajax-api-fix-login:worktrunk"],
        ),
        expected_new_task_open_command("ajax-api-fix-login"),
    ]);
    assert_eq!(runner.commands(), expected_commands.as_slice());
    let task = context
        .registry
        .list_tasks()
        .iter()
        .find(|task| task.qualified_handle() == "api/fix-login")
        .cloned()
        .expect("start task should be recorded");
    assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
    assert!(state_changed);
}

#[test]
fn cockpit_start_persists_task_before_first_external_command() {
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
                        .any(|task| task.qualified_handle() == "api/fix-login"),
                    "cockpit start task should be durable before the first external command"
                );
            }
            self.outputs
                .pop_front()
                .ok_or_else(|| CommandRunError::SpawnFailed("missing queued output".to_string()))
        }
    }

    let directory = std::env::temp_dir().join(format!(
        "ajax-cli-cockpit-start-{}-{}",
        std::process::id(),
        "pre-external"
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let config_file = directory.join("config.toml");
    let state_file = directory.join("state.db");
    let paths = CliContextPaths::new(&config_file, &state_file);
    let mut context = CommandContext::with_runtime_paths(
        Config {
            repos: vec![ManagedRepo::new("api", "/Users/matt/projects/api", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
        paths.runtime_paths.clone(),
    );
    let mut save_state = crate::context::context_save_state_from_registry(&context.registry);
    let pending = ajax_tui::PendingAction {
        task_handle: "api".to_string(),
        action: "start".to_string(),
        task_title: Some("Fix login".to_string()),
    };
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
    let mut task_session = RecordingTaskSessionRunner::default();
    let mut state_changed = false;

    super::cockpit_actions::execute_pending_cockpit_action_with_task_session_and_checkpoint(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
        &mut task_session,
        |checkpoint_context| {
            crate::context::save_context_with_state(&paths, checkpoint_context, &mut save_state)
                .map_err(|error| {
                    ajax_core::commands::CommandError::CommandRun(CommandRunError::SpawnFailed(
                        format!("persist test checkpoint: {error}"),
                    ))
                })
        },
    )
    .unwrap();

    std::fs::remove_dir_all(Path::new(&directory)).unwrap();
    assert!(runner.checked);
}

#[test]
fn task_verbs_render_core_operation_titles() {
    let context = sample_context();

    let resume = run_with_context(["ajax", "resume", "web/fix-login"], &context).unwrap();
    let repair = run_with_context(["ajax", "repair", "web/fix-login"], &context).unwrap();
    let review = run_with_context(["ajax", "review", "web/fix-login"], &context).unwrap();
    let ship = run_with_context(["ajax", "ship", "web/fix-login"], &context).unwrap();

    assert!(resume.contains("open task: web/fix-login"));
    assert!(repair.contains("repair task: web/fix-login"));
    assert!(review.contains("diff task: web/fix-login"));
    assert!(ship.contains("merge task: web/fix-login"));
}

#[test]
fn reconcile_is_not_an_operator_action() {
    assert_eq!(OperatorAction::from_label("reconcile"), None);
}

#[test]
fn drop_plan_refreshes_stale_git_evidence_before_rendering_commands() {
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
        .try_get_matches_from(["ajax", "drop", "web/fix-login", "--json"])
        .unwrap();
    let Some((_, subcommand)) = matches.subcommand() else {
        panic!("drop should parse as a subcommand");
    };
    let mut runner = QueuedRunner::new(vec![
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
        ),
        output(0, "main\n"),
    ]);

    let rendered = super::render_drop_command(subcommand, &mut context, &mut runner).unwrap();

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
    assert!(!rendered.output.contains("worktree"));
    assert!(!rendered.output.contains("branch"));
    let task = context.registry.get_task(&task_id).unwrap();
    let git_status = task.git_status.as_ref().unwrap();
    assert!(!git_status.worktree_exists);
    assert!(!git_status.branch_exists);
}

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

    let rendered = super::render_task_command(
        super::TaskCommandKind::Resume,
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
    assert!(rendered.output.contains("\"blocked_reasons\""));
    assert!(rendered.output.contains("task has missing substrate"));
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

    let error = super::render_drop_command(subcommand, &mut context, &mut runner).unwrap_err();

    assert!(
        matches!(error, super::CliError::CommandFailedAfterStateChange(message)
                if message.contains("drop incomplete for web/fix-login")
                    && message.contains("retry with `ajax drop web/fix-login --execute`"))
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

    let rendered = super::render_drop_command(subcommand, &mut context, &mut runner).unwrap();

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
        ["ajax", "tasks"],
        &paths,
        &mut QueuedRunner::new(vec![]),
    )
    .unwrap();
    let restored = SqliteRegistryStore::new(&state_file).load().unwrap();

    std::fs::remove_dir_all(Path::new(&directory)).unwrap();
    assert!(!tasks_output.contains("web/fix-login"));
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

    let outcome = super::execute_pending_cockpit_action(
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
    let outcome = super::tui_cockpit_action(&item, &mut context).unwrap();

    assert!(matches!(outcome, ajax_tui::ActionOutcome::Confirm(message)
            if message.contains("press enter again") && message.contains("drop")));
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
    let outcome = super::tui_cockpit_confirmed_action(&item, &mut context).unwrap();

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

fn missing_drop_observation_outputs() -> Vec<CommandOutput> {
    vec![
        output(0, "ajax-other\n"),
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
        ),
        output(0, "main\n"),
        output(0, "ajax-other\n"),
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
        ),
        output(0, "main\n"),
    ]
}

fn missing_drop_observation_commands() -> Vec<CommandSpec> {
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
        CommandSpec::new("tmux", ["list-sessions", "-F", "#{session_name}"])
            .with_timeout(std::time::Duration::from_secs(8)),
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

fn present_cleanable_drop_outputs() -> Vec<CommandOutput> {
    vec![
            output(0, ""),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n",
            ),
            output(0, "main\najax/fix-login\n"),
            output(0, ""),
            output(0, ""),
            output(0, ""),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
            ),
            output(0, "main\n"),
        ]
}

fn present_cleanable_drop_commands() -> Vec<CommandSpec> {
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
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "worktree",
                "remove",
                "/tmp/worktrees/web-fix-login",
            ],
        ),
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "branch",
                "-d",
                "ajax/fix-login",
            ],
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

#[test]
fn pending_cockpit_drop_reconciles_missing_substrate_before_registry_removal() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.tmux_status = Some(TmuxStatus {
        exists: false,
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
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: "drop".to_string(),
        task_title: None,
    };
    let mut runner = QueuedRunner::new(missing_drop_observation_outputs());
    let mut state_changed = false;

    let outcome = super::execute_pending_cockpit_action(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
    )
    .unwrap();

    assert_eq!(outcome, None);
    assert_eq!(runner.commands, missing_drop_observation_commands());
    assert!(context.registry.get_task(&TaskId::new("task-1")).is_none());
    assert!(state_changed);
}

#[test]
fn task_session_pending_drop_uses_observed_drop_semantics() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.tmux_status = Some(TmuxStatus {
        exists: false,
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
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: "drop".to_string(),
        task_title: None,
    };
    let mut runner = QueuedRunner::new(missing_drop_observation_outputs());
    let mut task_session = RecordingTaskSessionRunner::default();
    let mut state_changed = false;

    super::execute_pending_cockpit_action_with_task_session(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
        &mut task_session,
    )
    .unwrap();

    assert_eq!(runner.commands, missing_drop_observation_commands());
    assert!(task_session.commands.is_empty());
    assert!(context.registry.get_task(&TaskId::new("task-1")).is_none());
    assert!(state_changed);
}

#[test]
fn pending_cockpit_reconcile_is_unknown() {
    let mut context = CommandContext::new(
        Config {
            repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    );
    let mut runner = PanicRunner;
    let mut state_changed = true;
    let pending = ajax_tui::PendingAction {
        task_handle: "web".to_string(),
        action: "reconcile".to_string(),
        task_title: None,
    };

    let error = super::execute_pending_cockpit_action(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
    )
    .unwrap_err();

    assert!(matches!(error, super::CliError::CommandFailed(message)
            if message == "unknown cockpit action: reconcile"));
    assert!(state_changed);
}

#[test]
fn pending_cockpit_open_and_create_actions_return_to_ajax_after_task_session() {
    let action = "resume";
    let mut context = sample_context();
    let mut runner = RecordingCommandRunner::default();
    let mut task_session = RecordingTaskSessionRunner::default();
    let mut state_changed = false;
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: action.to_string(),
        task_title: None,
    };

    super::cockpit_actions::execute_pending_cockpit_action_with_task_session(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
        &mut task_session,
    )
    .unwrap();

    assert_eq!(
        runner.commands(),
        &[CommandSpec::new(
            "tmux",
            ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
        )]
    );
    assert_eq!(
        task_session.commands,
        vec![
            CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        ]
    );
    assert!(matches!(
        super::cockpit_actions::execute_pending_cockpit_action_with_task_session(
            &pending,
            &mut context,
            &mut runner,
            &mut state_changed,
            &mut OpenNewTaskTaskSessionRunner,
        )
        .unwrap(),
        super::cockpit_actions::PendingCockpitExecution::OpenNewTask { repo } if repo == "web"
    ));

    let mut context = CommandContext::new(
        Config {
            repos: vec![ManagedRepo::new("api", "/Users/matt/projects/api", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    );
    let pending = ajax_tui::PendingAction {
        task_handle: "api".to_string(),
        action: "start".to_string(),
        task_title: Some("Fix login".to_string()),
    };
    let mut runner = RecordingCommandRunner::default();
    let mut task_session = RecordingTaskSessionRunner::default();
    let mut state_changed = false;

    super::cockpit_actions::execute_pending_cockpit_action_with_task_session(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
        &mut task_session,
    )
    .unwrap();

    assert_eq!(
        task_session.commands,
        vec![
            CommandSpec::new("tmux", ["attach-session", "-t", "ajax-api-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        ]
    );
    assert!(!runner.commands().contains(&CommandSpec::new(
        "tmux",
        ["bind-key", "-n", "C-q", "detach-client"]
    )));
    assert!(runner.commands().iter().any(|command| {
        command.program == "tmux"
            && command.args.starts_with(&[
                "new-session".to_string(),
                "-d".to_string(),
                "-s".to_string(),
                "ajax-api-fix-login".to_string(),
            ])
    }));
    assert!(state_changed);
}

#[test]
fn pending_cockpit_resume_task_session_failure_stays_in_cockpit_without_lifecycle_change() {
    let mut context = sample_context();
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: "resume".to_string(),
        task_title: None,
    };
    let mut runner = RecordingCommandRunner::default();
    let mut task_session = FailingTaskSessionRunner {
        message: "tmux attach failed: session gone",
    };
    let mut state_changed = false;

    let error = super::cockpit_actions::execute_pending_cockpit_action_with_task_session(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
        &mut task_session,
    )
    .unwrap_err();

    assert!(matches!(
        error,
        CliError::CommandFailed(message) if message == "tmux attach failed: session gone"
    ));
    assert_eq!(
        runner.commands(),
        &[CommandSpec::new(
            "tmux",
            ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
        )]
    );
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::Reviewable
    );
    assert!(!state_changed);
}

#[test]
fn pending_cockpit_create_task_session_failure_requests_cockpit_reload() {
    let mut context = CommandContext::new(
        Config {
            repos: vec![ManagedRepo::new("api", "/Users/matt/projects/api", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    );
    let pending = ajax_tui::PendingAction {
        task_handle: "api".to_string(),
        action: "start".to_string(),
        task_title: Some("Fix login".to_string()),
    };
    let mut runner = RecordingCommandRunner::default();
    let mut task_session = FailingTaskSessionRunner {
        message: "tmux missing",
    };
    let mut state_changed = false;

    let error = super::cockpit_actions::execute_pending_cockpit_action_with_task_session(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
        &mut task_session,
    )
    .unwrap_err();

    assert!(matches!(
        error,
        CliError::CommandFailedAfterStateChange(message) if message == "tmux missing"
    ));
    assert!(state_changed);
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == "api/fix-login")
        .expect("failed start should still record the task");
    assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
}

#[test]
fn pending_cockpit_removed_actions_are_rejected() {
    for action in [
        "inspect agent",
        "inspect test output",
        "monitor task",
        "review branch",
        "review diff",
    ] {
        let mut context = sample_context();
        let pending = ajax_tui::PendingAction {
            task_handle: "web/fix-login".to_string(),
            action: action.to_string(),
            task_title: None,
        };
        let mut runner = PanicRunner;
        let mut state_changed = false;

        let error = super::execute_pending_cockpit_action(
            &pending,
            &mut context,
            &mut runner,
            &mut state_changed,
        )
        .unwrap_err();

        assert!(matches!(error, super::CliError::CommandFailed(message)
                if message.contains(action)));
        assert!(!state_changed, "{action}");
    }
}

#[test]
fn pending_cockpit_repair_runs_trunk_plan() {
    let mut context = sample_context();
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: "repair".to_string(),
        task_title: None,
    };
    let mut runner = RecordingCommandRunner::default();
    let mut state_changed = false;

    let outcome = super::cockpit_actions::execute_pending_cockpit_action_with_open_mode(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
        OpenMode::Attach,
    )
    .unwrap();

    assert_eq!(outcome, None);
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
                    "worktrunk",
                    "-c",
                    "/tmp/worktrees/web-fix-login"
                ]
            ),
            CommandSpec::new(
                "tmux",
                ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
            ),
            CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        ]
    );
    assert!(state_changed);
}

#[test]
fn pending_cockpit_repair_switches_client_when_inside_tmux() {
    let mut context = sample_context();
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: "repair".to_string(),
        task_title: None,
    };
    let mut runner = RecordingCommandRunner::default();
    let mut state_changed = false;

    super::cockpit_actions::execute_pending_cockpit_action_with_open_mode(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
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
    assert!(state_changed);
}

#[test]
fn pending_cockpit_open_alias_actions_run_open_plan_without_lifecycle_change() {
    let action = "resume";
    let mut context = sample_context();
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: action.to_string(),
        task_title: None,
    };
    let mut runner = RecordingCommandRunner::default();
    let mut state_changed = false;

    super::cockpit_actions::execute_pending_cockpit_action_with_open_mode(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
        OpenMode::Attach,
    )
    .unwrap();

    assert_eq!(
        runner.commands(),
        &[
            CommandSpec::new(
                "tmux",
                ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
            ),
            CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        ],
        "{action}"
    );
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::Reviewable,
        "{action} should preserve the task lifecycle"
    );
    assert!(state_changed, "{action}");
}

#[test]
fn pending_cockpit_unknown_action_does_not_open_or_mutate_task() {
    let mut context = sample_context();
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: "mystery action".to_string(),
        task_title: None,
    };
    let mut runner = RecordingCommandRunner::default();
    let mut state_changed = false;

    let error = super::execute_pending_cockpit_action(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
    )
    .unwrap_err();

    assert!(matches!(error, super::CliError::CommandFailed(message)
                if message.contains("unknown cockpit action: mystery action")));
    assert!(runner.commands().is_empty());
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::Reviewable
    );
    assert!(!state_changed);
}

#[test]
fn pending_cockpit_risky_merge_requires_confirmation_without_running() {
    let mut context = sample_context();
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: "ship".to_string(),
        task_title: None,
    };
    let mut runner = RecordingCommandRunner::default();
    let mut state_changed = false;

    let error = super::execute_pending_cockpit_action(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
    )
    .unwrap_err();

    assert!(matches!(
        error,
        super::CliError::CommandFailed(message)
            if message == "confirmation required; pass --yes"
    ));
    assert!(runner.commands().is_empty());
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::Reviewable
    );
    assert!(!state_changed);
}

#[test]
fn pending_cockpit_failed_external_command_does_not_mutate_state() {
    let mut context = safe_merge_context();
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: "ship".to_string(),
        task_title: None,
    };
    let mut runner = QueuedRunner::new(vec![CommandOutput {
        status_code: 42,
        stdout: String::new(),
        stderr: "merge failed".to_string(),
    }]);
    let mut state_changed = false;

    let error = super::execute_pending_cockpit_action(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
    )
    .unwrap_err();

    assert!(matches!(
        error,
        super::CliError::CommandFailed(message)
            if message == "command failed: git exited with status 42: merge failed"
    ));
    assert_eq!(
        runner.commands,
        &[CommandSpec::new(
            "git",
            ["-C", "/Users/matt/projects/web", "switch", "main"]
        )]
    );
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::Reviewable
    );
    assert!(!state_changed);
}

#[test]
fn pending_cockpit_errors_return_to_ajax_with_flash_message() {
    let mut cockpit_flash = None;

    let outcome = super::handle_pending_cockpit_result(
        Err(CliError::CommandFailed(
            "git exited with status 42".to_string(),
        )),
        &mut cockpit_flash,
    );

    assert!(!outcome);
    assert_eq!(cockpit_flash.as_deref(), Some("git exited with status 42"));
}

#[test]
fn pending_cockpit_open_action_runs_task_without_lifecycle_change() {
    let mut context = sample_context();
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: "resume".to_string(),
        task_title: None,
    };
    let mut runner = RecordingCommandRunner::default();
    let mut state_changed = false;

    super::cockpit_actions::execute_pending_cockpit_action_with_open_mode(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
        OpenMode::Attach,
    )
    .unwrap();

    assert_eq!(
        runner.commands(),
        &[
            CommandSpec::new(
                "tmux",
                ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
            ),
            CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        ]
    );
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::Reviewable
    );
    assert!(state_changed);
}

#[test]
fn pending_cockpit_open_action_switches_client_when_inside_tmux() {
    let mut context = sample_context();
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: "resume".to_string(),
        task_title: None,
    };
    let mut runner = RecordingCommandRunner::default();
    let mut state_changed = false;

    super::cockpit_actions::execute_pending_cockpit_action_with_open_mode(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
        OpenMode::SwitchClient,
    )
    .unwrap();

    assert_eq!(
        runner.commands(),
        &[
            CommandSpec::new(
                "tmux",
                ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
            ),
            CommandSpec::new("tmux", ["switch-client", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        ]
    );
    assert!(state_changed);
}

#[test]
fn pending_cockpit_merge_action_runs_task_and_marks_merged() {
    let mut context = safe_merge_context();
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: "ship".to_string(),
        task_title: None,
    };
    let mut runner = RecordingCommandRunner::default();
    let mut state_changed = false;

    super::execute_pending_cockpit_action(&pending, &mut context, &mut runner, &mut state_changed)
        .unwrap();

    assert_eq!(
        runner.commands(),
        &[
            CommandSpec::new("git", ["-C", "/Users/matt/projects/web", "switch", "main"]),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "merge",
                    "--ff-only",
                    "ajax/fix-login"
                ]
            )
        ]
    );
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::Merged
    );
    assert!(state_changed);
}

#[test]
fn pending_cockpit_clean_action_runs_task_and_marks_removed() {
    let mut context = cleanable_context();
    let pending = ajax_tui::PendingAction {
        task_handle: "web/fix-login".to_string(),
        action: "drop".to_string(),
        task_title: None,
    };
    let mut runner = QueuedRunner::new(present_cleanable_drop_outputs());
    let mut state_changed = false;

    let output = super::execute_pending_cockpit_action(
        &pending,
        &mut context,
        &mut runner,
        &mut state_changed,
    )
    .unwrap();

    assert_eq!(output, None);
    assert_eq!(runner.commands, present_cleanable_drop_commands());
    assert!(context.registry.get_task(&TaskId::new("task-1")).is_none());
    assert!(state_changed);
}

#[test]
fn failed_deferred_drop_restores_task_in_next_cockpit_snapshot() {
    let mut context = cleanable_context();
    let item = ajax_core::models::CockpitActionItem {
        task_id: TaskId::new("__task_action__web_fix_login__clean"),
        task_handle: "web/fix-login".to_string(),
        reason: "Clean task".to_string(),
        priority: 0,
        action: "drop".to_string(),
    };
    let mut state_changed = false;
    let outcome = super::tui_cockpit_confirmed_action(&item, &mut context).unwrap();
    let ajax_tui::ActionOutcome::RefreshAndDefer(optimistic, pending) = outcome else {
        panic!("confirmed drop should optimistically refresh and defer cleanup");
    };
    assert!(optimistic.cards.is_empty());

    let mut failing_runner = QueuedRunner::new(vec![
            output(0, ""),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n",
            ),
            output(0, "main\najax/fix-login\n"),
            output(0, ""),
            CommandOutput {
                status_code: 2,
                stdout: String::new(),
                stderr: "branch delete failed".to_string(),
            },
            output(0, ""),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
            ),
            output(0, "main\najax/fix-login\n"),
        ]);
    let error = super::execute_pending_cockpit_action(
        &pending,
        &mut context,
        &mut failing_runner,
        &mut state_changed,
    )
    .unwrap_err();
    let mut flash = None;
    let handled = super::handle_pending_cockpit_result(Err(error), &mut flash);
    let restored = crate::cockpit_backend::build_cockpit_snapshot(&context);

    assert!(!handled);
    assert!(flash
        .as_deref()
        .is_some_and(|message| message.contains("branch delete failed")));
    assert!(restored
        .cards
        .iter()
        .any(|card| card.qualified_handle == "web/fix-login"));
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::TeardownIncomplete
    );
}

#[test]
fn cockpit_reconcile_action_is_unknown() {
    let mut context = sample_context();
    let item = ajax_core::models::CockpitActionItem {
        task_id: TaskId::new("__project_action__web__reconcile"),
        task_handle: "web".to_string(),
        reason: "Reconcile".to_string(),
        priority: 0,
        action: "reconcile".to_string(),
    };
    let outcome = super::tui_cockpit_action(&item, &mut context).unwrap();

    assert!(matches!(outcome, ajax_tui::ActionOutcome::Message(message)
            if message == "cockpit action is not configured: reconcile"));
}

#[test]
fn cockpit_clean_action_requires_confirmation_before_running() {
    let mut context = cleanable_context();
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .add_side_flag(SideFlag::Dirty);
    let item = ajax_core::models::CockpitActionItem {
        task_id: TaskId::new("__task_action__web_fix_login__clean"),
        task_handle: "web/fix-login".to_string(),
        reason: "Clean task".to_string(),
        priority: 0,
        action: "drop".to_string(),
    };
    let outcome = super::tui_cockpit_action(&item, &mut context).unwrap();

    assert!(matches!(outcome, ajax_tui::ActionOutcome::Confirm(message)
            if message.contains("press enter again") && message.contains("drop")));
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
fn confirmed_cockpit_clean_action_removes_from_snapshot_and_defers_cleanup() {
    let mut context = cleanable_context();
    let item = ajax_core::models::CockpitActionItem {
        task_id: TaskId::new("__task_action__web_fix_login__clean"),
        task_handle: "web/fix-login".to_string(),
        reason: "Clean task".to_string(),
        priority: 0,
        action: "drop".to_string(),
    };
    let outcome = super::tui_cockpit_confirmed_action(&item, &mut context).unwrap();

    match outcome {
        ajax_tui::ActionOutcome::RefreshAndDefer(snapshot, pending) => {
            assert_eq!(snapshot.repos.repos.len(), 1);
            assert!(snapshot.cards.is_empty());
            assert!(snapshot.inbox.items.is_empty());
            assert_eq!(pending.task_handle, "web/fix-login");
            assert_eq!(pending.action, "drop");
        }
        ajax_tui::ActionOutcome::Defer(_) => {
            panic!("drop task should optimistically refresh before deferring")
        }
        ajax_tui::ActionOutcome::Refresh(_) => {
            panic!("drop task should defer backend cleanup after refresh")
        }
        ajax_tui::ActionOutcome::Message(message) => {
            panic!("drop task should run instead of showing message: {message}")
        }
        ajax_tui::ActionOutcome::Confirm(message) => {
            panic!("confirmed drop task should run instead of confirming: {message}")
        }
    }
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
fn removed_reconcile_command_does_not_touch_registry_snapshot() {
    let directory = std::env::temp_dir().join(format!(
        "ajax-cli-removed-reconcile-{}-{}",
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
        .save(&sample_context().registry)
        .unwrap();
    let mut runner = QueuedRunner::new(vec![
        output(0, "other-session\n"),
        output(128, "fatal: not a git repository\n"),
    ]);

    let error = run_with_context_paths_and_runner(
        ["ajax", "reconcile", "--json"],
        &CliContextPaths::new(&config_file, &state_file),
        &mut runner,
    )
    .unwrap_err();
    let restored = SqliteRegistryStore::new(&state_file).load().unwrap();

    std::fs::remove_dir_all(Path::new(&directory)).unwrap();
    assert!(matches!(error, CliError::CommandFailed(message)
            if message.contains("unrecognized subcommand 'reconcile'")));
    assert!(runner.commands.is_empty());
    let restored_task = restored.get_task(&TaskId::new("task-1")).unwrap();
    assert!(!restored_task.has_side_flag(SideFlag::WorktreeMissing));
    assert_eq!(
        restored.list_tasks().len(),
        sample_context().registry.list_tasks().len()
    );
}

#[test]
fn agent_runtime_command_runs_without_loading_ajax_context() {
    let directory = std::env::temp_dir().join(format!(
        "ajax-cli-agent-runtime-command-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let output = super::run_with_args([
        "ajax",
        "--config",
        "/definitely/missing/ajax-config.toml",
        "__agent-runtime",
        "--task-id",
        "web/fix-login",
        "--state-root",
        directory.to_str().unwrap(),
        "--",
        "/bin/sh",
        "-c",
        "exit 0",
    ])
    .unwrap();

    assert!(output.is_empty());
    let latest = std::fs::read_to_string(directory.join("web__fix-login.json")).unwrap();
    assert!(latest.contains("\"state\":\"exited_success\""));

    std::fs::remove_dir_all(directory).unwrap();
}

fn runtime_snapshot_directory(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "ajax-cli-runtime-snapshot-{}-{}-{label}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn write_runtime_snapshot(cache_dir: &Path, state: &str, observed_at_unix_millis: u128) {
    let runtime_dir = cache_dir.join("agent-runtime");
    std::fs::create_dir_all(&runtime_dir).unwrap();
    std::fs::write(
        runtime_dir.join("task-1.json"),
        serde_json::json!({
            "task_id": "task-1",
            "state": state,
            "observed_at_unix_millis": observed_at_unix_millis,
            "pid": 42,
            "exit_code": if state == "exited_failure" { Some(9) } else { None::<i32> },
            "message": null
        })
        .to_string(),
    )
    .unwrap();
}

fn active_runtime_context(cache_dir: &Path) -> CommandContext<InMemoryRegistry> {
    let mut context = sample_context();
    context.runtime_paths.cache_dir = cache_dir.to_path_buf();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.live_status = None;
    task.agent_status = AgentRuntimeStatus::NotStarted;
    task.remove_side_flag(SideFlag::NeedsInput);
    context
}

#[test]
fn cockpit_refresh_marks_new_agent_running_from_wrapper_snapshot() {
    let directory = runtime_snapshot_directory("running");
    let now_millis = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    write_runtime_snapshot(&directory, "running", now_millis);
    let mut context = active_runtime_context(&directory);
    let mut runner = QueuedRunner::new(tmux_live_outputs("shell idle\n"));

    crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();

    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_eq!(task.agent_status, AgentRuntimeStatus::Running);
    assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::AgentRunning)
    );
    assert_eq!(
        ajax_core::commands::cockpit_view(&context).cards[0]
            .status_explanation
            .as_deref(),
        Some("Agent working")
    );

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cockpit_refresh_marks_killed_agent_failed_instead_of_unknown() {
    let directory = runtime_snapshot_directory("failed");
    write_runtime_snapshot(&directory, "exited_failure", 1);
    let mut context = active_runtime_context(&directory);
    let mut runner = QueuedRunner::new(tmux_live_outputs("shell idle\n"));

    crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();

    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_ne!(task.agent_status, AgentRuntimeStatus::Unknown);
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::CommandFailed)
    );
    assert_eq!(task.lifecycle_status, LifecycleStatus::Active);

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cockpit_refresh_promotes_wrapper_completion_to_reviewable() {
    let directory = runtime_snapshot_directory("completed");
    write_runtime_snapshot(&directory, "exited_success", 1);
    let mut context = active_runtime_context(&directory);
    let mut runner = QueuedRunner::new(tmux_live_outputs("shell idle\n"));

    crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();

    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_eq!(task.agent_status, AgentRuntimeStatus::Done);
    assert_eq!(task.lifecycle_status, LifecycleStatus::Reviewable);

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn stale_wrapper_running_snapshot_cannot_keep_task_running() {
    let directory = runtime_snapshot_directory("stale");
    write_runtime_snapshot(&directory, "running", 1);
    let mut context = active_runtime_context(&directory);
    let mut runner = QueuedRunner::new(tmux_live_outputs("shell idle\n"));

    crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();

    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_ne!(task.agent_status, AgentRuntimeStatus::Running);
    assert_ne!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::AgentRunning)
    );
    // A stale wrapper-running snapshot is no longer a probe failure; core falls
    // through to a successful agent-aware pane observation instead.
    assert_ne!(
        task.runtime_projection.observation_error.as_deref(),
        Some("agent status stale")
    );

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn tmux_probe_failure_renders_unavailable_without_marking_session_missing() {
    struct FailingTmuxRunner;

    impl CommandRunner for FailingTmuxRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            assert_eq!(
                command.args.first().map(String::as_str),
                Some("list-sessions")
            );
            Err(CommandRunError::SpawnFailed("tmux unavailable".to_string()))
        }
    }

    let directory = runtime_snapshot_directory("tmux-failed");
    let mut context = active_runtime_context(&directory);
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
    let mut runner = FailingTmuxRunner;

    crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();

    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(!task.has_side_flag(SideFlag::TmuxMissing));
    assert!(task
        .tmux_status
        .as_ref()
        .is_some_and(|status| status.exists));
    assert_eq!(
        ajax_core::commands::cockpit_view(&context).cards[0]
            .status_explanation
            .as_deref(),
        Some("Status unavailable")
    );
}

#[test]
fn confirmed_agent_stop_records_dead_instead_of_unknown() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.agent_status = AgentRuntimeStatus::Running;
    task.add_side_flag(SideFlag::AgentRunning);
    task.agent_attempts.push(ajax_core::models::AgentAttempt {
        agent: AgentClient::Codex,
        launch_target: "tmux:%1".to_string(),
        started_at: SystemTime::UNIX_EPOCH,
        finished_at: None,
        status: AgentRuntimeStatus::Running,
    });

    ajax_core::commands::mark_drop_agent_stopped(&mut context, "web/fix-login").unwrap();

    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_eq!(task.agent_status, AgentRuntimeStatus::Dead);
    assert_eq!(task.agent_attempts[0].status, AgentRuntimeStatus::Dead);
    assert!(!task.has_side_flag(SideFlag::AgentRunning));
}

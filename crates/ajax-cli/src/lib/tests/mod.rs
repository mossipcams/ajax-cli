use super::*;
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
        SideFlag, Task, TaskId, TaskWindowStatus, TmuxStatus,
    },
    registry::{InMemoryRegistry, Registry, SqliteRegistryStore},
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
        "task",
        AgentClient::Codex,
    );
    task.lifecycle_status = LifecycleStatus::Reviewable;
    task.add_side_flag(SideFlag::NeedsInput);
    associate_task_with_pr(&mut task, 42, "2222222");
    registry.create_task(task).unwrap();

    CommandContext::new(config, registry)
}

fn associate_task_with_pr(task: &mut Task, number: u64, head_sha: &str) {
    ajax_core::diff_review::remember_pull_requests(
        task,
        &[ajax_core::diff_review::PullRequestRef {
            number,
            title: "Test PR".into(),
            url: format!("https://example.test/pull/{number}"),
            state: ajax_core::diff_review::PullRequestState::Open,
            head_ref: task.branch.clone(),
            head_sha: Some(head_sha.into()),
        }],
    );
    task.metadata.insert(
        "ajax_ci_monitor".into(),
        serde_json::json!({ "pr_number": number, "head_sha": head_sha, "last_pr_discovery_at": 1 })
            .to_string(),
    );
}

fn git_list_remote_branches_command() -> CommandSpec {
    CommandSpec::new(
        "git",
        [
            "-C",
            "/Users/matt/projects/web",
            "branch",
            "-r",
            "--format=%(refname:short)",
        ],
    )
}

// from suite_12.rs
fn missing_drop_observation_outputs() -> Vec<CommandOutput> {
    vec![
        output(0, "ajax-other\n"),
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
        ),
        output(0, "main\n"),
        output(0, "origin/main\n"),
        output(0, "ajax-other\n"),
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
        ),
        output(0, "main\n"),
        output(0, "origin/main\n"),
    ]
}

// from suite_12.rs
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
        git_list_remote_branches_command(),
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
        git_list_remote_branches_command(),
    ]
}

// from suite_12.rs
fn present_cleanable_drop_outputs() -> Vec<CommandOutput> {
    vec![
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
    ]
}

// from suite_12.rs
fn assert_present_cleanable_force_drop_commands(commands: &[CommandSpec]) {
    assert_eq!(commands.len(), 10);
    assert_eq!(
        commands[0],
        CommandSpec::new("tmux", ["list-sessions", "-F", "#{session_name}"])
            .with_timeout(std::time::Duration::from_secs(8))
    );
    assert_eq!(
        commands[1],
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "worktree",
                "list",
                "--porcelain",
            ],
        )
    );
    assert_eq!(
        commands[2],
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "branch",
                "--format=%(refname:short)",
            ],
        )
    );
    assert_eq!(commands[3], git_list_remote_branches_command());
    assert_eq!(commands[4].program, "sh");
    assert_eq!(commands[4].args[0], "-c");
    assert_eq!(
        commands[4].args[1],
        "mkdir -p \"$(dirname \"$3\")\" && { [ ! -e \"$2\" ] || mv \"$2\" \"$3\"; } && { git -C \"$1\" worktree prune || git -C \"$1\" worktree remove --force \"$2\"; } && { rm -rf \"$3\" >/dev/null 2>&1 & }"
    );
    assert_eq!(commands[4].args[2], "ajax-fast-worktree-remove");
    assert_eq!(commands[4].args[3], "/Users/matt/projects/web");
    assert_eq!(commands[4].args[4], "/tmp/worktrees/web-fix-login");
    assert!(commands[4].args[5].starts_with("/tmp/worktrees/.ajax-trash/fix-login-"));
    assert_eq!(commands[5].program, "sh");
    assert_eq!(commands[5].args[2], "ajax-delete-branch");
    assert_eq!(commands[5].args[3], "/Users/matt/projects/web");
    assert_eq!(commands[5].args[4], "ajax/fix-login");
    assert_eq!(
        commands[6],
        CommandSpec::new("tmux", ["list-sessions", "-F", "#{session_name}"])
            .with_timeout(std::time::Duration::from_secs(8))
    );
    assert_eq!(
        commands[7],
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "worktree",
                "list",
                "--porcelain",
            ],
        )
    );
    assert_eq!(
        commands[8],
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "branch",
                "--format=%(refname:short)",
            ],
        )
    );
    assert_eq!(commands[9], git_list_remote_branches_command());
}

// from suite_13.rs
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

// from suite_13.rs
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

// from suite_13.rs
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

// from suite_2.rs
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

// from suite_2.rs
fn tmux_live_commands_with_running_reconcile() -> Vec<CommandSpec> {
    let mut commands = tmux_live_commands();
    commands.insert(
        2,
        CommandSpec::new(
            "tmux",
            ["capture-pane", "-p", "-t", "ajax-web-fix-login:task"],
        )
        .with_timeout(std::time::Duration::from_secs(8)),
    );
    commands
}

// from suite_2.rs
fn expected_ci_probe_command() -> CommandSpec {
    CommandSpec::new("gh", ["pr", "checks", "42", "--json", "name,state,link"])
        .with_cwd("/tmp/worktrees/web-fix-login")
        .with_timeout(std::time::Duration::from_secs(30))
}

// from suite_2.rs
fn expected_new_task_open_command(session: &str) -> CommandSpec {
    CommandSpec::new("tmux", ["attach-session", "-t", session]).with_mode(CommandMode::InheritStdio)
}

// from suite_2.rs
fn run_start_with_attach_mode(
    args: impl IntoIterator<Item = &'static str>,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut impl ajax_core::adapters::CommandRunner,
) -> Result<String, CliError> {
    let matches = build_cli().try_get_matches_from(args).unwrap();
    crate::execution_dispatch::render_matches_mut(&matches, context, runner, OpenMode::Attach)
        .map(|rendered| rendered.output)
}

// from suite_2.rs
const EXPECTED_HUSKY_GUARD: &str =
    "if [ -f package.json ] && [ -f .husky/pre-commit ]; then npm exec --yes husky; fi";

fn folded_agent_launch_line(task_id: &str, worktree_path: &str, bootstrap: Option<&str>) -> String {
    let agent = format!(
        "ajax-cli __agent-runtime --task-id {task_id} --state-root .cache/ajax/agent-runtime -- codex --cd {worktree_path}"
    );
    match bootstrap {
        Some(bootstrap) => format!("{EXPECTED_HUSKY_GUARD}; {bootstrap} && {agent}"),
        None => format!("{EXPECTED_HUSKY_GUARD}; {agent}"),
    }
}

fn expected_task_launch_command(
    session: &str,
    task_id: &str,
    worktree_path: &str,
    bootstrap: Option<&str>,
) -> CommandSpec {
    CommandSpec {
        program: "tmux".to_string(),
        args: vec![
            "send-keys".to_string(),
            "-t".to_string(),
            format!("{session}:task"),
            folded_agent_launch_line(task_id, worktree_path, bootstrap),
            "Enter".to_string(),
        ],
        cwd: None,
        mode: CommandMode::Capture,
        timeout: None,
    }
}

// from suite_2.rs
fn expected_sync_default_branch_commands(repo_path: &str, branch: &str) -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("git", ["-C", repo_path, "fetch", "origin", branch])
            .with_timeout(std::time::Duration::from_secs(60)),
    ]
}

// from suite_2.rs
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

// from suite_2.rs
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

// from suite_2.rs
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
        "task",
        AgentClient::Codex,
    );
    task.lifecycle_status = LifecycleStatus::Cleanable;
    registry.create_task(task).unwrap();
    registry
}

// from suite_2.rs
fn cockpit_item(handle: &str, action: &str) -> ajax_core::models::CockpitActionItem {
    ajax_core::models::CockpitActionItem {
        task_id: TaskId::new(format!("__cockpit_action__{action}")),
        task_handle: handle.to_string(),
        reason: action.to_string(),
        priority: 0,
        action: action.to_string(),
    }
}

// from suite_4.rs
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

// from suite_5.rs
struct RecoveryRunner {
    commands: Vec<CommandSpec>,
}

// from suite_5.rs
impl RecoveryRunner {
    fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }
}

// from suite_5.rs
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
                    "task\t/Users/matt/projects/web__worktrees/ajax-code\n"
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

include!("suite_1.rs");
include!("suite_2.rs");
include!("suite_3.rs");
include!("suite_4.rs");
include!("suite_5.rs");
include!("suite_6.rs");
include!("suite_7.rs");
include!("suite_8.rs");
include!("suite_9.rs");
include!("suite_10.rs");
include!("suite_11.rs");
include!("suite_12.rs");
include!("suite_13.rs");

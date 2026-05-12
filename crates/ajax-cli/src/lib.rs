mod classifiers;
mod cli;
mod cockpit_actions;
mod cockpit_backend;
mod context;
mod dispatch;
mod execution_dispatch;
mod render;
mod snapshot_dispatch;
mod supervise;

use ajax_core::{
    adapters::{CommandRunner, ProcessCommandRunner},
    commands::{self, CommandContext, CommandError},
    registry::InMemoryRegistry,
};
use clap::ArgMatches;
pub use cli::build_cli;
use cli::{parse_args, ParsedArgs};
#[cfg(test)]
use cockpit_actions::{
    execute_pending_cockpit_action, handle_pending_cockpit_result, tui_cockpit_action,
    tui_cockpit_confirmed_action, PendingCockpitOutcome,
};
#[cfg(test)]
use cockpit_backend::{refresh_cockpit_snapshot, render_cockpit_command};
pub use context::CliContextPaths;
use context::{default_context_paths, load_context, save_context};
#[cfg(test)]
use dispatch::{render_task_command, TaskCommandOperation};
use execution_dispatch::{render_matches_mut, render_matches_mut_with_paths};
#[cfg(test)]
use snapshot_dispatch::parent_directory_available;
use snapshot_dispatch::render_matches_with_paths;
use std::ffi::OsStr;
#[cfg(test)]
use supervise::render_supervise_command;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CliError {
    CommandFailed(String),
    CommandFailedAfterStateChange(String),
    JsonSerialization(String),
    ContextLoad(String),
    ContextSave(String),
}

pub(crate) fn current_open_mode() -> commands::OpenMode {
    open_mode_from_tmux_env(std::env::var_os("TMUX").as_deref())
}

fn open_mode_from_tmux_env(tmux: Option<&OsStr>) -> commands::OpenMode {
    if tmux.is_some_and(|value| !value.to_string_lossy().is_empty()) {
        commands::OpenMode::SwitchClient
    } else {
        commands::OpenMode::Attach
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::CommandFailed(message) | CliError::CommandFailedAfterStateChange(message) => {
                write!(formatter, "{message}")
            }
            CliError::JsonSerialization(message) => {
                write!(formatter, "json serialization failed: {message}")
            }
            CliError::ContextLoad(message) => write!(formatter, "context load failed: {message}"),
            CliError::ContextSave(message) => write!(formatter, "context save failed: {message}"),
        }
    }
}

impl std::error::Error for CliError {}

impl CliError {
    fn state_changed(&self) -> bool {
        matches!(self, CliError::CommandFailedAfterStateChange(_))
    }

    fn after_state_change(self) -> Self {
        match self {
            CliError::CommandFailed(message) => CliError::CommandFailedAfterStateChange(message),
            error => error,
        }
    }
}

pub fn run_with_args(
    args: impl IntoIterator<Item = impl Into<std::ffi::OsString> + Clone>,
) -> Result<String, CliError> {
    let matches = match parse_args(args)? {
        ParsedArgs::Matches(matches) => matches,
        ParsedArgs::Message(message) => return Ok(message),
    };

    let paths = default_context_paths()?;
    let mut context = load_context(&paths)?;
    let mut runner = ProcessCommandRunner;
    let rendered =
        match render_matches_mut_with_paths(&matches, &mut context, &mut runner, Some(&paths)) {
            Ok(rendered) => rendered,
            Err(error) => {
                if error.state_changed() {
                    save_context(&paths, &context)?;
                }
                return Err(error);
            }
        };
    if rendered.state_changed {
        save_context(&paths, &context)?;
    }

    Ok(rendered.output)
}

pub fn run_with_context(
    args: impl IntoIterator<Item = impl Into<std::ffi::OsString> + Clone>,
    context: &CommandContext<InMemoryRegistry>,
) -> Result<String, CliError> {
    let matches = match parse_args(args)? {
        ParsedArgs::Matches(matches) => matches,
        ParsedArgs::Message(message) => return Ok(message),
    };

    render_snapshot_matches(&matches, context)
}

pub fn run_with_context_and_runner(
    args: impl IntoIterator<Item = impl Into<std::ffi::OsString> + Clone>,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut impl CommandRunner,
) -> Result<String, CliError> {
    let matches = match parse_args(args)? {
        ParsedArgs::Matches(matches) => matches,
        ParsedArgs::Message(message) => return Ok(message),
    };

    render_matches_mut(&matches, context, runner).map(|rendered| rendered.output)
}

pub fn run_with_context_paths(
    args: impl IntoIterator<Item = impl Into<std::ffi::OsString> + Clone>,
    paths: &CliContextPaths,
) -> Result<String, CliError> {
    let matches = match parse_args(args)? {
        ParsedArgs::Matches(matches) => matches,
        ParsedArgs::Message(message) => return Ok(message),
    };
    let context = load_context(paths)?;

    render_matches_with_paths(&matches, &context, Some(paths))
}

pub fn run_with_context_paths_and_runner(
    args: impl IntoIterator<Item = impl Into<std::ffi::OsString> + Clone>,
    paths: &CliContextPaths,
    runner: &mut impl CommandRunner,
) -> Result<String, CliError> {
    let matches = match parse_args(args)? {
        ParsedArgs::Matches(matches) => matches,
        ParsedArgs::Message(message) => return Ok(message),
    };
    let mut context = load_context(paths)?;
    let rendered = match render_matches_mut_with_paths(&matches, &mut context, runner, Some(paths))
    {
        Ok(rendered) => rendered,
        Err(error) => {
            if error.state_changed() {
                save_context(paths, &context)?;
            }
            return Err(error);
        }
    };
    if rendered.state_changed {
        save_context(paths, &context)?;
    }

    Ok(rendered.output)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RenderedCommand {
    pub(crate) output: String,
    pub(crate) state_changed: bool,
}

fn render_snapshot_matches(
    matches: &ArgMatches,
    context: &CommandContext<InMemoryRegistry>,
) -> Result<String, CliError> {
    snapshot_dispatch::render_snapshot_matches(matches, context)
}

// The refreshed-read path lives in `execution_dispatch::render_refreshed_read_command`.

pub(crate) fn new_task_request(matches: &ArgMatches) -> Result<commands::NewTaskRequest, CliError> {
    let repo = matches
        .get_one::<String>("repo")
        .cloned()
        .unwrap_or_else(|| "web".to_string());
    let title = matches.get_one::<String>("title").cloned().ok_or_else(|| {
        CliError::CommandFailed("task title is required; pass --title".to_string())
    })?;
    let agent = matches
        .get_one::<String>("agent")
        .cloned()
        .unwrap_or_else(|| "codex".to_string());

    Ok(commands::NewTaskRequest { repo, title, agent })
}

pub(crate) fn task_arg(matches: &ArgMatches) -> Result<&str, CliError> {
    matches
        .get_one::<String>("task")
        .map(String::as_str)
        .ok_or_else(|| CliError::CommandFailed("task argument is required".to_string()))
}

pub(crate) fn command_error(error: CommandError) -> CliError {
    match error {
        CommandError::TaskNotFound(task) => {
            CliError::CommandFailed(format!("task not found: {task}"))
        }
        CommandError::RepoNotFound(repo) => {
            CliError::CommandFailed(format!("repo not found: {repo}"))
        }
        CommandError::ConfirmationRequired => {
            CliError::CommandFailed("confirmation required; pass --yes".to_string())
        }
        CommandError::PlanBlocked(reasons) => {
            CliError::CommandFailed(format!("plan blocked: {}", reasons.join(", ")))
        }
        CommandError::CommandRun(error) => {
            CliError::CommandFailed(format!("command failed: {error}"))
        }
        CommandError::Registry(error) => {
            CliError::CommandFailed(format!("registry update failed: {error}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_cli, run_with_context, run_with_context_and_runner, run_with_context_paths,
        run_with_context_paths_and_runner, CliContextPaths, CliError,
    };
    use ajax_core::{
        adapters::{
            CommandMode, CommandOutput, CommandRunError, CommandRunner, CommandSpec,
            RecordingCommandRunner,
        },
        commands::{CommandContext, OpenMode},
        config::{Config, ManagedRepo},
        models::{
            AgentClient, AgentRuntimeStatus, GitStatus, LifecycleStatus, LiveObservation,
            LiveStatusKind, RecommendedAction, SideFlag, Task, TaskId, TmuxStatus, WorktrunkStatus,
        },
        registry::{InMemoryRegistry, Registry, RegistryStore, SqliteRegistryStore},
    };
    use std::path::{Path, PathBuf};

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
    fn execution_dispatch_module_routes_mutating_commands() {
        let mut context = sample_context();
        let mut runner = RecordingCommandRunner::default();
        let matches = build_cli()
            .try_get_matches_from([
                "ajax",
                "new",
                "--repo",
                "web",
                "--title",
                "Fix logout",
                "--execute",
            ])
            .unwrap();

        let rendered =
            crate::execution_dispatch::render_matches_mut(&matches, &mut context, &mut runner)
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
    fn classifiers_module_detects_merge_conflict_errors() {
        let error = CommandRunError::NonZeroExit {
            program: "git".to_string(),
            status_code: 1,
            stderr: "Automatic merge failed; fix conflicts and then commit.".to_string(),
            cwd: None,
        };

        assert!(crate::classifiers::command_error_looks_conflicted(&error));
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

    fn tmux_live_outputs(pane: &str) -> Vec<CommandOutput> {
        vec![
            output(0, "ajax-web-fix-login\n"),
            output(0, "worktrunk\t/tmp/worktrees/web-fix-login\n"),
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

    fn tmux_live_commands() -> Vec<CommandSpec> {
        vec![
            CommandSpec::new("tmux", ["list-sessions", "-F", "#{session_name}"]),
            CommandSpec::new(
                "tmux",
                [
                    "list-windows",
                    "-t",
                    "ajax-web-fix-login",
                    "-F",
                    "#{window_name}\t#{pane_current_path}",
                ],
            ),
            CommandSpec::new(
                "tmux",
                [
                    "capture-pane",
                    "-p",
                    "-t",
                    "ajax-web-fix-login:worktrunk",
                    "-S",
                    "-200",
                ],
            ),
        ]
    }

    fn expected_new_task_open_command(session: &str) -> CommandSpec {
        match super::current_open_mode() {
            OpenMode::Attach => CommandSpec::new("tmux", ["attach-session", "-t", session])
                .with_mode(CommandMode::InheritStdio),
            OpenMode::SwitchClient => CommandSpec::new("tmux", ["switch-client", "-t", session])
                .with_mode(CommandMode::InheritStdio),
        }
    }

    fn ajax_binary_path() -> PathBuf {
        if let Some(binary) = std::env::var_os("CARGO_BIN_EXE_ajax") {
            return binary.into();
        }

        let current_exe = std::env::current_exe().unwrap();
        let deps_dir = current_exe
            .parent()
            .expect("test binary should live under target debug deps");
        let debug_dir = deps_dir
            .parent()
            .expect("test binary should live under target debug deps");
        debug_dir.join(if cfg!(windows) { "ajax.exe" } else { "ajax" })
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
            .env("AJAX_CONFIG", directory.join("missing-config.toml"))
            .env("AJAX_STATE", directory.join("missing-state.db"))
            .output()
            .unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();

        assert!(!output.status.success());
        assert!(stderr.contains("command is required; pass --help"));
        assert!(!stderr.contains("CommandFailed"));
    }

    fn cockpit_item(handle: &str, action: &str) -> ajax_core::models::AttentionItem {
        ajax_core::models::AttentionItem {
            task_id: TaskId::new(format!("__cockpit_action__{action}")),
            task_handle: handle.to_string(),
            reason: action.to_string(),
            priority: 0,
            recommended_action: action.to_string(),
        }
    }

    #[test]
    fn command_surface_includes_mvp_commands() {
        for args in [
            vec!["ajax", "repos"],
            vec!["ajax", "tasks"],
            vec!["ajax", "inspect", "web/fix-login"],
            vec!["ajax", "new"],
            vec!["ajax", "open", "web/fix-login"],
            vec!["ajax", "trunk", "web/fix-login"],
            vec!["ajax", "check", "web/fix-login"],
            vec!["ajax", "diff", "web/fix-login"],
            vec!["ajax", "merge", "web/fix-login"],
            vec!["ajax", "clean", "web/fix-login"],
            vec!["ajax", "sweep"],
            vec!["ajax", "next"],
            vec!["ajax", "inbox"],
            vec!["ajax", "review"],
            vec!["ajax", "status"],
            vec!["ajax", "doctor"],
            vec!["ajax", "supervise", "--prompt", "fix tests"],
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
    fn readonly_dispatch_does_not_have_adapter_wiring_placeholder() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let lib_source = std::fs::read_to_string(manifest_dir.join("src/lib.rs")).unwrap();
        let adapter_placeholder = ["adapter wiring", " pending"].concat();
        let accepted_placeholder = ["command", " accepted"].concat();

        assert!(!lib_source.contains(&adapter_placeholder));
        assert!(!lib_source.contains(&accepted_placeholder));
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
        assert!(output.contains("agent needs input"));
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

        assert!(output.contains("web/fix-login\tagent running\tFix login"));
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

        assert!(output.contains("web/fix-login\tWaiting\tFix login"));
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
    fn read_commands_share_live_refresh_contract() {
        for args in [
            vec!["ajax", "repos", "--json"],
            vec!["ajax", "tasks", "--json"],
            vec!["ajax", "inbox", "--json"],
            vec!["ajax", "next", "--json"],
            vec!["ajax", "review", "--json"],
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
    fn read_refresh_failure_keeps_task_visible_with_missing_tmux_attention() {
        let mut context = sample_context();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Active;
        task.remove_side_flag(SideFlag::NeedsInput);
        let mut runner = QueuedRunner::new(vec![output(0, "other-session\n")]);

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
        assert_eq!(runner.commands, vec![tmux_live_commands()[0].clone()]);
    }

    #[test]
    fn snapshot_only_read_dispatch_is_explicitly_named() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let lib_source = std::fs::read_to_string(manifest_dir.join("src/lib.rs")).unwrap();
        let snapshot_dispatch = ["fn render_", "snapshot_matches("].concat();

        assert!(lib_source.contains(&snapshot_dispatch));
        assert!(lib_source.contains("render_refreshed_read_command"));
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
            super::refresh_cockpit_snapshot(&mut context, &mut runner, &mut state_changed).unwrap();

        assert!(state_changed);
        assert_eq!(
            snapshot.tasks.tasks[0]
                .live_status
                .as_ref()
                .map(|status| status.summary.as_str()),
            Some("waiting for approval")
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
            output(0, "agent\t/tmp/worktrees/web-fix-login\n"),
        ]);
        let mut state_changed = false;

        let snapshot =
            super::refresh_cockpit_snapshot(&mut context, &mut runner, &mut state_changed).unwrap();
        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();

        assert!(state_changed);
        assert!(!task.has_side_flag(SideFlag::TmuxMissing));
        assert!(task.has_side_flag(SideFlag::WorktrunkMissing));
        assert_eq!(snapshot.tasks.tasks.len(), 1);
        assert_eq!(
            snapshot.tasks.tasks[0]
                .live_status
                .as_ref()
                .map(|status| status.summary.as_str()),
            Some("worktrunk missing")
        );
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
        task.worktrunk_status = Some(WorktrunkStatus {
            exists: true,
            window_name: "worktrunk".to_string(),
            current_path: "/tmp/worktrees/web-fix-login".into(),
            points_at_expected_path: true,
        });
        let previous_activity = task.last_activity_at;
        let mut runner = QueuedRunner::new(tmux_live_outputs("codex is working\n"));
        let mut state_changed = false;

        let _snapshot =
            super::refresh_cockpit_snapshot(&mut context, &mut runner, &mut state_changed).unwrap();
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
    fn live_refresh_ignores_nonzero_session_listing_without_mutation() {
        let mut context = sample_context();
        let mut runner = QueuedRunner::new(vec![output(1, "ajax-web-fix-login\n")]);

        let changed =
            crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();
        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();

        assert!(!changed);
        assert!(task.tmux_status.is_none());
        assert!(task.worktrunk_status.is_none());
        assert_eq!(runner.commands, vec![tmux_live_commands()[0].clone()]);
    }

    #[test]
    fn live_refresh_nonzero_window_listing_stops_before_pane_capture() {
        let mut context = sample_context();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Active;
        task.remove_side_flag(SideFlag::NeedsInput);
        let mut runner = QueuedRunner::new(vec![
            output(0, "ajax-web-fix-login\n"),
            output(1, "worktrunk\t/tmp/worktrees/web-fix-login\n"),
        ]);

        let changed =
            crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();
        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();

        assert!(changed);
        let expected_commands = tmux_live_commands();
        assert_eq!(runner.commands, expected_commands[..2]);
        assert!(task.has_side_flag(SideFlag::WorktrunkMissing));
        assert_eq!(
            task.live_status
                .as_ref()
                .map(|status| status.summary.as_str()),
            Some("worktrunk missing")
        );
    }

    #[test]
    fn live_refresh_nonzero_pane_capture_reports_command_failure() {
        let mut context = sample_context();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Active;
        task.remove_side_flag(SideFlag::NeedsInput);
        let mut runner = QueuedRunner::new(vec![
            output(0, "ajax-web-fix-login\n"),
            output(0, "worktrunk\t/tmp/worktrees/web-fix-login\n"),
            output(1, "codex is working\n"),
        ]);

        let changed =
            crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();
        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();

        assert!(changed);
        assert_eq!(runner.commands, tmux_live_commands());
        assert_eq!(
            task.live_status
                .as_ref()
                .map(|status| status.summary.as_str()),
            Some("live refresh failed")
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
            output(0, "worktrunk\t/tmp/worktrees/web-fix-login\n"),
            output(0, ""),
        ]);

        let changed =
            crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();
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
            output(0, "worktrunk\t/tmp/worktrees/web-fix-login\n"),
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
            output(0, "worktrunk\t/tmp/worktrees/web-fix-login\n"),
            output(0, ""),
        ]);

        let changed =
            crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();
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

        let rendered = crate::cockpit_backend::render_live_cockpit_command(
            &mut context,
            subcommand,
            &mut runner,
        )
        .unwrap();

        assert!(rendered.state_changed);
        assert_eq!(rendered.output.matches("Ajax Cockpit").count(), 2);
        assert_eq!(runner.commands.len(), 4);
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

        let output = super::render_supervise_command(subcommand).unwrap();

        assert!(output.contains("process started"));
        assert!(output.contains("agent started: codex"));
        assert!(output.contains("waiting for approval: cargo test"));
        assert!(output.contains("process exited: 0"));

        let _ = std::fs::remove_file(fake_codex);
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

        let error = super::render_supervise_command(subcommand).unwrap_err();

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

        let error = super::render_supervise_command(subcommand).unwrap_err();

        let _ = std::fs::remove_file(fake_codex);
        assert!(error.to_string().contains("codex exited with status 42"));
        assert!(error.to_string().contains("stderr: auth expired"));
    }

    #[test]
    fn supervise_with_task_requires_existing_visible_task() {
        let fake_codex =
            std::env::temp_dir().join(format!("ajax-cli-fake-codex-task-{}", std::process::id()));
        std::fs::write(
            &fake_codex,
            "#!/bin/sh\nprintf '{\"type\":\"started\"}\\n'\n",
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&fake_codex).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
        std::fs::set_permissions(&fake_codex, permissions).unwrap();
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

        assert!(output.contains("agent started: codex"));

        let error = run_with_context_and_runner(
            [
                "ajax",
                "supervise",
                "--task",
                "web/missing",
                "--prompt",
                "fix tests",
                "--codex-bin",
                &fake_codex.display().to_string(),
            ],
            &mut context,
            &mut runner,
        )
        .unwrap_err();

        let _ = std::fs::remove_file(fake_codex);
        assert!(matches!(error, CliError::CommandFailed(message)
                if message == "task not found: web/missing"));

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
        let output = run_with_context(["ajax", "--help"], &context).unwrap();

        assert!(output.contains("Usage: ajax [COMMAND]"));
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
    fn cli_context_and_render_logic_live_in_modules() {
        let crate_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let lib = std::fs::read_to_string(crate_root.join("src/lib.rs")).unwrap();

        assert!(lib.contains("mod cli;"));
        assert!(lib.contains("mod context;"));
        assert!(lib.contains("mod cockpit_actions;"));
        assert!(lib.contains("mod dispatch;"));
        assert!(lib.contains("mod render;"));
        assert!(lib.contains("mod supervise;"));
        assert!(crate_root.join("src/cli.rs").exists());
        assert!(crate_root.join("src/cockpit_actions.rs").exists());
        assert!(crate_root.join("src/context.rs").exists());
        assert!(crate_root.join("src/dispatch.rs").exists());
        assert!(crate_root.join("src/render.rs").exists());
        assert!(crate_root.join("src/supervise.rs").exists());
    }

    #[test]
    fn architecture_documents_no_legacy_json_state_migration() {
        let architecture = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../architecture.md"),
        )
        .unwrap();

        assert!(architecture.contains("Legacy JSON state is not migrated"));
        assert!(architecture.contains("full rewrite"));
    }

    #[test]
    fn agents_documents_no_legacy_code_rule() {
        let agents = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../AGENTS.md"),
        )
        .unwrap();

        assert!(agents.contains("Do NOT keep legacy code"));
        assert!(agents.contains("When adding new code always fully replace legacy code"));
        assert!(agents.contains("It is not a migration"));
    }

    #[test]
    fn architecture_documents_current_workspace_boundaries() {
        let architecture = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../architecture.md"),
        )
        .unwrap();

        for crate_name in ["ajax-core", "ajax-cli", "ajax-tui", "ajax-supervisor"] {
            assert!(
                architecture.contains(crate_name),
                "architecture.md should document the {crate_name} crate boundary"
            );
        }
        assert!(architecture.contains("supervised agent execution"));
    }

    #[test]
    fn architecture_documents_current_persistence_and_cockpit_stack() {
        let architecture = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../architecture.md"),
        )
        .unwrap();

        assert!(architecture.contains("current durable registry store"));
        assert!(architecture.contains("SqliteRegistryStore"));
        assert!(architecture.contains("Ratatui"));
        assert!(architecture.contains("current interactive TUI foundation"));
    }

    #[test]
    fn architecture_documents_current_execution_and_cli_shape() {
        let architecture = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../architecture.md"),
        )
        .unwrap();

        for command_mode in ["CommandMode::Capture", "CommandMode::InheritStdio"] {
            assert!(
                architecture.contains(command_mode),
                "architecture.md should name the current {command_mode} execution path"
            );
        }
        assert!(
            !architecture.contains("CommandMode::Spawn"),
            "architecture.md should not document unused detached spawn semantics"
        );

        assert!(architecture.contains("current `ajax-cli` split"));
        assert!(!architecture.contains("Consider Ratatui"));
        assert!(!architecture.contains("long-term implementation should"));
        assert!(!architecture.contains("The intended persistence boundary"));
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
            ["ajax", "review", "--json", ""],
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
        let directory =
            std::env::temp_dir().join(format!("ajax-doctor-paths-{}", std::process::id()));
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

    #[test]
    fn state_export_writes_registry_snapshot_without_overwriting() {
        let directory =
            std::env::temp_dir().join(format!("ajax-state-export-{}", std::process::id()));
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
            vec!["ajax", "new", "--repo", "web", "--execute"],
            vec!["ajax", "open", "web/fix-login", "--execute"],
            vec!["ajax", "check", "web/fix-login", "--execute"],
            vec!["ajax", "diff", "web/fix-login", "--execute"],
            vec!["ajax", "merge", "web/fix-login", "--execute", "--yes"],
            vec!["ajax", "clean", "web/fix-login", "--execute", "--yes"],
            vec!["ajax", "sweep", "--execute", "--yes"],
        ] {
            let matches = build_cli().try_get_matches_from(args.clone());
            assert!(matches.is_ok(), "{args:?} should parse");
        }
    }

    #[test]
    fn task_scoped_commands_require_explicit_task_handle() {
        for args in [
            vec!["ajax", "open"],
            vec!["ajax", "trunk"],
            vec!["ajax", "check"],
            vec!["ajax", "diff"],
            vec!["ajax", "merge"],
            vec!["ajax", "clean"],
        ] {
            let error = run_with_context(args.clone(), &sample_context()).unwrap_err();
            assert!(
                matches!(error, super::CliError::CommandFailed(ref message) if message.contains("required")),
                "{args:?} should require task arg, got {error:?}"
            );
        }
    }

    #[test]
    fn textual_frontend_files_are_removed() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");

        assert!(!root.join("frontends/textual").exists());
    }

    #[test]
    fn textual_startup_scripts_are_removed() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");

        assert!(!root.join("scripts/start-ajax-textual.sh").exists());
        assert!(!root.join("scripts/start-ajax-textual-lib.sh").exists());
        assert!(!root.join("scripts/test-ajax-textual.sh").exists());
    }

    #[test]
    fn readme_documents_native_rust_cockpit() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let readme = std::fs::read_to_string(root.join("README.md")).unwrap();

        assert!(readme.contains("native Rust cockpit"));
        assert!(readme.contains("ajax cockpit"));
        assert!(readme.contains("project-first workflow"));
        assert!(readme.contains("choose a project"));
        assert!(!readme.contains("Textual"));
        assert!(!readme.contains("textual"));
        assert!(!readme.contains("## Startup Script"));
        assert!(!readme.contains("./scripts/start-ajax-textual.sh"));
    }

    #[test]
    fn release_hygiene_documents_install_config_and_release_process() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let workspace_manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
        let readme = std::fs::read_to_string(root.join("README.md")).unwrap();
        let changelog = std::fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
        let release = std::fs::read_to_string(root.join("RELEASE.md")).unwrap();
        let license = std::fs::read_to_string(root.join("LICENSE")).unwrap();

        assert!(!workspace_manifest.contains("https://github.com/example/ajax-cli"));
        assert!(
            workspace_manifest.contains("repository = \"https://github.com/mossipcams/ajax-cli\"")
        );
        assert!(workspace_manifest.contains("version = \"0.1.0\""));
        assert!(workspace_manifest.contains("[workspace.lints.rust]"));
        assert!(workspace_manifest.contains("unsafe_op_in_unsafe_fn = \"deny\""));
        assert!(license.contains("MIT License"));
        assert!(readme.contains("## Install"));
        assert!(readme.contains("## Configuration"));
        assert!(readme.contains("## First Run"));
        assert!(changelog.contains("# Changelog"));
        assert!(release.contains("# Release Process"));
        assert!(release.contains("cargo fmt --check"));
        assert!(release.contains("cargo test --all-features"));
    }

    #[test]
    fn workspace_members_inherit_metadata_lints_and_dependencies() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let workspace_manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();

        assert!(workspace_manifest.contains("[workspace.dependencies]"));
        for dependency in [
            "ajax-core",
            "ajax-supervisor",
            "ajax-tui",
            "serde",
            "serde_json",
            "tokio",
            "rstest",
        ] {
            assert!(
                workspace_manifest.contains(&format!("{dependency} = ")),
                "workspace manifest should centralize {dependency}"
            );
        }

        for crate_name in ["ajax-cli", "ajax-core", "ajax-supervisor", "ajax-tui"] {
            let manifest =
                std::fs::read_to_string(root.join(format!("crates/{crate_name}/Cargo.toml")))
                    .unwrap();

            assert!(manifest.contains("version.workspace = true"));
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
    fn workspace_style_files_document_repo_hygiene() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let clippy = std::fs::read_to_string(root.join("clippy.toml")).unwrap();
        let rustfmt = std::fs::read_to_string(root.join("rustfmt.toml")).unwrap();
        let toolchain = std::fs::read_to_string(root.join("rust-toolchain.toml")).unwrap();
        let style = std::fs::read_to_string(root.join("STYLE.md")).unwrap();
        let agents = std::fs::read_to_string(root.join("AGENTS.md")).unwrap();

        assert!(clippy.contains("doc-valid-idents"));
        assert!(rustfmt.contains("edition = \"2021\""));
        assert!(toolchain.contains("channel = \"1.88.0\""));
        assert!(style.contains("Workspace Hygiene"));
        assert!(style.contains("runtime behavior"));
        assert!(agents.contains("Workspace Hygiene"));

        for boundary in [
            "ajax-cli = CLI parsing, dispatch, rendering, context loading",
            "ajax-core = models, policy, live status, registry",
            "ajax-supervisor = process supervision",
            "ajax-tui = Cockpit screen state, input, layout, rendering",
        ] {
            assert!(
                agents.contains(boundary),
                "AGENTS.md should document boundary: {boundary}"
            );
        }
    }

    #[test]
    fn tui_dependency_uses_audit_clean_ratatui_feature_set() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let tui_manifest =
            std::fs::read_to_string(root.join("crates/ajax-tui/Cargo.toml")).unwrap();
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
    fn audit_policy_has_no_accepted_warnings() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let audit_policy = std::fs::read_to_string(root.join("AUDIT.md")).unwrap();

        assert!(audit_policy.contains("No Accepted Warnings"));
        assert!(audit_policy.contains("cargo audit -D warnings"));
        assert!(!audit_policy.contains("RUSTSEC-2024-0436"));
        assert!(!audit_policy.contains("RUSTSEC-2026-0002"));
    }

    #[test]
    fn smoke_workflow_script_is_documented_for_release_validation() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let smoke = std::fs::read_to_string(root.join("scripts/smoke.sh")).unwrap();
        let readme = std::fs::read_to_string(root.join("README.md")).unwrap();
        let release = std::fs::read_to_string(root.join("RELEASE.md")).unwrap();

        assert!(smoke.contains("ajax doctor"));
        assert!(smoke.contains("ajax new"));
        assert!(smoke.contains("ajax supervise --task"));
        assert!(smoke.contains("ajax merge"));
        assert!(smoke.contains("ajax state export"));
        assert!(!smoke.contains("ajax check"));
        assert!(!smoke.contains("ajax diff"));
        assert!(smoke.contains("assert_json_contains"));
        assert!(smoke.contains("\"lifecycle_status\": \"Active\""));
        assert!(smoke.contains("\"lifecycle_status\": \"Reviewable\""));
        assert!(smoke.contains("\"lifecycle_status\": \"Merged\""));
        assert!(smoke.contains("\"tasks\": []"));
        assert!(smoke.contains("assert_log_contains"));
        assert!(smoke.contains("run_happy_path_journey"));
        assert!(smoke.contains("run_recovery_journey"));
        assert!(smoke.contains("AJAX_SMOKE_FAIL_AFTER_WORKTREE"));
        assert!(smoke.contains("\"lifecycle_status\": \"Error\""));
        assert!(smoke.contains("state export target already exists"));
        assert!(smoke.contains("target/release/ajax"));
        assert!(smoke.contains("cargo build --release -p ajax-cli"));
        assert!(!smoke.contains("target/debug/ajax"));
        assert!(smoke.contains("if [[ -z \"${AJAX_BIN:-}\" ]]"));
        assert!(smoke.contains("ajax binary is not executable"));
        assert!(readme.contains("scripts/smoke.sh"));
        assert!(!readme.contains("running checks"));
        assert!(!readme.contains("viewing diffs"));
        assert!(release.contains("scripts/smoke.sh"));
    }

    #[test]
    fn new_command_renders_plan_without_json_panic() {
        let output = run_with_context(
            [
                "ajax",
                "new",
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
        assert!(output.contains("git -C /Users/matt/projects/web worktree add -b ajax/fix-logout /Users/matt/projects/web__worktrees/ajax-fix-logout main"));
        assert!(output.contains("tmux new-session -d -s ajax-web-fix-logout -n worktrunk -c /Users/matt/projects/web__worktrees/ajax-fix-logout"));
        assert!(output.contains("tmux send-keys -t ajax-web-fix-logout:worktrunk"));
    }

    #[test]
    fn new_command_requires_task_title() {
        let error =
            run_with_context(["ajax", "new", "--repo", "web"], &sample_context()).unwrap_err();

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
        let output = run_with_context(["ajax", "open", "web/fix-login"], &context).unwrap();

        assert!(output.contains("tmux select-window -t ajax-web-fix-login:worktrunk"));
        match super::current_open_mode() {
            OpenMode::Attach => {
                assert!(output.contains("tmux attach-session -t ajax-web-fix-login"));
            }
            OpenMode::SwitchClient => {
                assert!(output.contains("tmux switch-client -t ajax-web-fix-login"));
            }
        }
    }

    #[test]
    fn open_execute_switches_client_when_inside_tmux() {
        let mut context = sample_context();
        let mut runner = RecordingCommandRunner::default();
        let matches = build_cli()
            .try_get_matches_from(["ajax", "open", "web/fix-login", "--execute"])
            .unwrap();
        let (_, subcommand) = matches.subcommand().unwrap();

        super::render_task_command(
            super::TaskCommandOperation::Open,
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
            run_with_context(["ajax", "open", "web/fix-login", "--execute"], &context).unwrap_err();

        assert!(matches!(error, super::CliError::CommandFailed(message)
                if message.contains("execution requires mutable context and runner support")));
    }

    #[test]
    fn merge_command_renders_json_plan() {
        let context = sample_context();
        let output =
            run_with_context(["ajax", "merge", "web/fix-login", "--json"], &context).unwrap();

        assert!(output.contains("\"requires_confirmation\": true"));
        assert!(output.contains("\"program\": \"git\""));
        assert!(output.contains("\"merge\""));
    }

    #[test]
    fn check_command_renders_configured_test_plan() {
        let mut context = sample_context();
        context.config.test_commands =
            vec![ajax_core::config::TestCommand::new("web", "cargo test")];

        let output = run_with_context(["ajax", "check", "web/fix-login"], &context).unwrap();

        assert!(output.contains("check task: web/fix-login"));
        assert!(output.contains("(cd /tmp/worktrees/web-fix-login && sh -lc 'cargo test')"));
    }

    #[test]
    fn diff_command_renders_diff_summary_plan() {
        let context = sample_context();
        let output = run_with_context(["ajax", "diff", "web/fix-login"], &context).unwrap();

        assert!(output.contains("diff task: web/fix-login"));
        assert!(output.contains(
            "(cd /tmp/worktrees/web-fix-login && git diff --stat main...ajax/fix-login)"
        ));
    }

    #[test]
    fn next_command_renders_attention_item() {
        let context = sample_context();
        let output = run_with_context(["ajax", "next"], &context).unwrap();

        assert_eq!(output, "web/fix-login: agent needs input -> open task");
    }

    #[test]
    fn review_command_renders_review_queue() {
        let context = sample_context();
        let output = run_with_context(["ajax", "review", "--json"], &context).unwrap();

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

        let output = run_with_context_and_runner(
            [
                "ajax",
                "new",
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
        assert_eq!(
            runner.commands(),
            &[
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
                        "main"
                    ]
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
                        "/Users/matt/projects/web__worktrees/ajax-fix-login"
                    ]
                ),
                CommandSpec::new(
                    "tmux",
                    [
                        "send-keys",
                        "-t",
                        "ajax-web-fix-login:worktrunk",
                        "codex --cd /Users/matt/projects/web__worktrees/ajax-fix-login 'Fix login'",
                        "Enter"
                    ]
                ),
                CommandSpec::new(
                    "tmux",
                    ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
                ),
                expected_new_task_open_command("ajax-web-fix-login")
            ]
        );
        let recorded = context
            .registry
            .list_tasks()
            .iter()
            .find(|task| task.qualified_handle() == "web/fix-login")
            .cloned()
            .expect("new task should be recorded");
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
    fn new_execute_rejects_existing_task_before_native_provisioning() {
        let mut context = sample_context();
        let mut runner = RecordingCommandRunner::default();

        let error = run_with_context_and_runner(
            [
                "ajax",
                "new",
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
            CommandOutput {
                status_code: 42,
                stdout: String::new(),
                stderr: "tmux failed".to_string(),
            },
        ]);

        let error = run_with_context_and_runner(
            [
                "ajax",
                "new",
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
        assert_eq!(
            runner.commands,
            vec![
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
                        "main"
                    ]
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
                        "/Users/matt/projects/web__worktrees/ajax-fix-login"
                    ]
                )
            ]
        );
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
                "new",
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

        let output = run_with_context_and_runner(
            [
                "ajax",
                "new",
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
        assert_eq!(
            runner.commands(),
            &[
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
                        "main"
                    ]
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
                        "/Users/matt/projects/web__worktrees/ajax-fix-login"
                    ]
                ),
                CommandSpec::new(
                    "tmux",
                    [
                        "send-keys",
                        "-t",
                        "ajax-web-fix-login:worktrunk",
                        "codex --cd /Users/matt/projects/web__worktrees/ajax-fix-login 'Fix login'",
                        "Enter"
                    ]
                ),
                CommandSpec::new(
                    "tmux",
                    ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
                ),
                expected_new_task_open_command("ajax-web-fix-login")
            ]
        );
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
            ["ajax", "new", "--repo", "web", "--execute"],
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
                "new",
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
            .expect("new task should be persisted");
        assert_eq!(
            recorded.worktree_path.to_string_lossy(),
            "/Users/matt/projects/web__worktrees/ajax-fix-login"
        );
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
            CommandOutput {
                status_code: 42,
                stdout: String::new(),
                stderr: "attach failed".to_string(),
            },
        ]);

        let error = run_with_context_paths_and_runner(
            [
                "ajax",
                "new",
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
            ["ajax", "open", "web/fix-login", "--execute"],
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
            ["ajax", "merge", "web/fix-login", "--execute"],
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
            ["ajax", "merge", "web/fix-login", "--execute", "--yes"],
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
            ["ajax", "merge", "web/fix-login", "--execute", "--yes"],
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
            ["ajax", "merge", "web/fix-login", "--execute", "--yes"],
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
            ["ajax", "merge", "web/fix-login", "--execute", "--yes"],
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
    fn clean_execute_marks_task_removed() {
        let mut context = cleanable_context();
        let mut runner = RecordingCommandRunner::default();

        run_with_context_and_runner(
            ["ajax", "clean", "web/fix-login", "--execute"],
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
            LifecycleStatus::Removed
        );
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
            output(0, "## ajax/fix-login...origin/ajax/fix-login\n"),
            output(0, ""),
            output(0, ""),
        ]);

        run_with_context_and_runner(
            ["ajax", "clean", "web/fix-login", "--execute", "--yes"],
            &mut context,
            &mut runner,
        )
        .unwrap();

        assert_eq!(
            runner.commands,
            vec![
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/tmp/worktrees/web-fix-login",
                        "status",
                        "--porcelain=v1",
                        "--branch"
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
                )
            ]
        );
        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert_eq!(task.lifecycle_status, LifecycleStatus::Removed);
        assert!(task.git_status.as_ref().is_some_and(|status| status.merged));
    }

    #[test]
    fn cleanup_execute_uses_safe_cleanup_path() {
        let mut context = cleanable_context();
        let mut runner = RecordingCommandRunner::default();

        run_with_context_and_runner(
            ["ajax", "cleanup", "web/fix-login", "--execute"],
            &mut context,
            &mut runner,
        )
        .unwrap();

        assert_eq!(
            runner.commands(),
            &[
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/tmp/worktrees/web-fix-login",
                        "status",
                        "--porcelain=v1",
                        "--branch"
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
                )
            ]
        );
        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .lifecycle_status,
            LifecycleStatus::Removed
        );
    }

    #[test]
    fn remove_execute_requires_yes_before_running() {
        let mut context = sample_context();
        let mut runner = RecordingCommandRunner::default();

        let error = run_with_context_and_runner(
            ["ajax", "remove", "web/fix-login", "--execute"],
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
        let mut runner = RecordingCommandRunner::default();

        run_with_context_and_runner(
            ["ajax", "remove", "web/fix-login", "--execute", "--yes"],
            &mut context,
            &mut runner,
        )
        .unwrap();

        assert_eq!(
            runner.commands(),
            &[
                CommandSpec::new("tmux", ["kill-session", "-t", "ajax-web-fix-login"]),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "worktree",
                        "remove",
                        "--force",
                        "/tmp/worktrees/web-fix-login"
                    ]
                ),
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
            ]
        );
        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .lifecycle_status,
            LifecycleStatus::Removed
        );
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
            ["ajax", "clean", "web/fix-login", "--execute"],
            &mut context,
            &mut runner,
        )
        .unwrap_err();

        assert_eq!(
            error,
            super::CliError::CommandFailed("confirmation required; pass --yes".to_string())
        );
        assert_eq!(
            runner.commands(),
            &[CommandSpec::new(
                "git",
                [
                    "-C",
                    "/tmp/worktrees/web-fix-login",
                    "status",
                    "--porcelain=v1",
                    "--branch"
                ]
            )]
        );
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
        let mut runner = RecordingCommandRunner::default();

        run_with_context_and_runner(
            ["ajax", "clean", "web/fix-login", "--execute", "--yes"],
            &mut context,
            &mut runner,
        )
        .unwrap();

        assert_eq!(
            runner.commands(),
            &[
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/tmp/worktrees/web-fix-login",
                        "status",
                        "--porcelain=v1",
                        "--branch"
                    ]
                ),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "worktree",
                        "remove",
                        "--force",
                        "/tmp/worktrees/web-fix-login"
                    ]
                ),
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
            ]
        );
        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .lifecycle_status,
            LifecycleStatus::Removed
        );
    }

    #[test]
    fn clean_execute_partial_failure_after_tmux_kill_updates_tmux_evidence() {
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
            output(0, "## ajax/fix-login\n"),
            output(0, ""),
            CommandOutput {
                status_code: 2,
                stdout: String::new(),
                stderr: "remove failed".to_string(),
            },
        ]);

        let error = run_with_context_and_runner(
            ["ajax", "clean", "web/fix-login", "--execute"],
            &mut context,
            &mut runner,
        )
        .unwrap_err();

        assert!(
            matches!(error, super::CliError::CommandFailedAfterStateChange(message)
                if message == "command failed: git exited with status 2: remove failed")
        );
        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert_eq!(task.lifecycle_status, LifecycleStatus::Cleanable);
        assert_eq!(
            task.tmux_status,
            Some(TmuxStatus {
                exists: false,
                session_name: "ajax-web-fix-login".to_string(),
            })
        );
        assert!(task
            .git_status
            .as_ref()
            .is_some_and(|status| { status.worktree_exists && status.branch_exists }));
    }

    #[test]
    fn clean_execute_partial_failure_after_worktree_remove_updates_git_evidence() {
        let mut context = cleanable_context();
        let mut runner = QueuedRunner::new(vec![
            output(0, "## ajax/fix-login\n"),
            output(0, ""),
            CommandOutput {
                status_code: 2,
                stdout: String::new(),
                stderr: "branch delete failed".to_string(),
            },
        ]);

        let error = run_with_context_and_runner(
            ["ajax", "clean", "web/fix-login", "--execute"],
            &mut context,
            &mut runner,
        )
        .unwrap_err();

        assert!(
            matches!(error, super::CliError::CommandFailedAfterStateChange(message)
                if message == "command failed: git exited with status 2: branch delete failed")
        );
        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert_eq!(task.lifecycle_status, LifecycleStatus::Cleanable);
        assert!(task.has_side_flag(SideFlag::WorktreeMissing));
        assert!(task
            .git_status
            .as_ref()
            .is_some_and(|status| { !status.worktree_exists && status.branch_exists }));
        assert!(!ajax_core::commands::list_tasks(&context, None)
            .tasks
            .is_empty());
    }

    #[test]
    fn trunk_execute_uses_injected_runner() {
        let mut context = sample_context();
        let mut runner = RecordingCommandRunner::default();
        let matches = build_cli()
            .try_get_matches_from(["ajax", "trunk", "web/fix-login", "--execute"])
            .unwrap();
        let (_, subcommand) = matches.subcommand().unwrap();

        super::render_task_command(
            super::TaskCommandOperation::Trunk,
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
    fn trunk_execute_switches_client_when_inside_tmux() {
        let mut context = sample_context();
        let mut runner = RecordingCommandRunner::default();
        let matches = build_cli()
            .try_get_matches_from(["ajax", "trunk", "web/fix-login", "--execute"])
            .unwrap();
        let (_, subcommand) = matches.subcommand().unwrap();

        super::render_task_command(
            super::TaskCommandOperation::Trunk,
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
    fn trunk_execute_clears_missing_tmux_and_worktrunk_flags() {
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
            ["ajax", "trunk", "web/fix-login", "--execute"],
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
    fn check_execute_uses_injected_runner() {
        let mut context = sample_context();
        context.config.test_commands =
            vec![ajax_core::config::TestCommand::new("web", "cargo test")];
        let mut runner = RecordingCommandRunner::default();

        run_with_context_and_runner(
            ["ajax", "check", "web/fix-login", "--execute"],
            &mut context,
            &mut runner,
        )
        .unwrap();

        assert_eq!(
            runner.commands(),
            &[CommandSpec::new("sh", ["-lc", "cargo test"])
                .with_cwd("/tmp/worktrees/web-fix-login")]
        );
    }

    #[test]
    fn check_execute_failure_records_tests_failed_attention_without_lifecycle_corruption() {
        let mut context = sample_context();
        context.config.test_commands =
            vec![ajax_core::config::TestCommand::new("web", "cargo test")];
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status = LifecycleStatus::Active;
        let mut runner = QueuedRunner::new(vec![CommandOutput {
            status_code: 42,
            stdout: String::new(),
            stderr: "tests failed".to_string(),
        }]);

        let error = run_with_context_and_runner(
            ["ajax", "check", "web/fix-login", "--execute"],
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
        context.config.test_commands =
            vec![ajax_core::config::TestCommand::new("web", "cargo test")];
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Active;
        task.add_side_flag(SideFlag::TestsFailed);
        let mut runner = RecordingCommandRunner::default();

        run_with_context_and_runner(
            ["ajax", "check", "web/fix-login", "--execute"],
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
            ["ajax", "diff", "web/fix-login", "--execute"],
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
        let mut runner = RecordingCommandRunner::default();

        run_with_context_and_runner(["ajax", "sweep", "--execute"], &mut context, &mut runner)
            .unwrap();

        assert_eq!(
            runner.commands(),
            &[
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
                )
            ]
        );
        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .lifecycle_status,
            LifecycleStatus::Removed
        );
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
        let mut runner = QueuedRunner::new(vec![
            output(0, ""),
            output(0, ""),
            output(0, ""),
            CommandOutput {
                status_code: 2,
                stdout: String::new(),
                stderr: "branch delete failed".to_string(),
            },
        ]);

        let error = run_with_context_paths_and_runner(
            ["ajax", "sweep", "--execute"],
            &CliContextPaths::new(&config_file, &state_file),
            &mut runner,
        )
        .unwrap_err();
        let restored = SqliteRegistryStore::new(&state_file).load().unwrap();

        std::fs::remove_dir_all(Path::new(&directory)).unwrap();
        assert!(error.to_string().contains("git exited with status 2"));
        assert_eq!(
            restored
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .lifecycle_status,
            LifecycleStatus::Removed
        );
        assert_eq!(
            restored
                .get_task(&TaskId::new("task-2"))
                .unwrap()
                .lifecycle_status,
            LifecycleStatus::Cleanable
        );
        let failed_task = restored.get_task(&TaskId::new("task-2")).unwrap();
        assert!(failed_task.has_side_flag(SideFlag::WorktreeMissing));
        assert!(failed_task
            .git_status
            .as_ref()
            .is_some_and(|status| { !status.worktree_exists && status.branch_exists }));
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
        let item = ajax_core::models::AttentionItem {
            task_id: TaskId::new("__project_action__api__new_task"),
            task_handle: "api".to_string(),
            reason: "+ New task".to_string(),
            priority: 0,
            recommended_action: "new task".to_string(),
        };
        let mut runner = RecordingCommandRunner::default();
        let mut state_changed = false;

        let outcome =
            super::tui_cockpit_action(&item, &mut context, &mut runner, &mut state_changed)
                .unwrap();

        match outcome {
            ajax_tui::ActionOutcome::Message(message) => {
                assert!(message.contains("select a project"));
                assert!(message.contains("new task"));
            }
            _ => panic!("new task should remain inside Ajax cockpit"),
        }

        assert!(runner.commands().is_empty());
        assert!(context.registry.list_tasks().is_empty());
        assert!(!state_changed);
    }

    #[test]
    fn cockpit_actions_defer_to_executable_ajax_commands() {
        for (handle, action) in [
            ("web/fix-login", "open task"),
            ("web/fix-login", "merge task"),
        ] {
            let mut context = sample_context();
            let item = ajax_core::models::AttentionItem {
                task_id: TaskId::new(format!("__cockpit_action__{action}")),
                task_handle: handle.to_string(),
                reason: action.to_string(),
                priority: 0,
                recommended_action: action.to_string(),
            };
            let mut runner = PanicRunner;
            let mut state_changed = false;

            let outcome =
                super::tui_cockpit_action(&item, &mut context, &mut runner, &mut state_changed)
                    .unwrap();

            match outcome {
                ajax_tui::ActionOutcome::Defer(pending) => {
                    assert_eq!(pending.task_handle, handle);
                    assert_eq!(pending.recommended_action, action);
                    assert!(pending.task_title.is_none());
                }
                ajax_tui::ActionOutcome::Message(message) => panic!(
                    "{action} should defer for execution instead of showing message: {message}"
                ),
                ajax_tui::ActionOutcome::Refresh { .. } => {
                    panic!("{action} should defer for execution instead of refreshing")
                }
                ajax_tui::ActionOutcome::Confirm(message) => {
                    panic!("{action} should defer for execution instead of confirming: {message}")
                }
            }
            assert!(!state_changed, "{action} should not mutate Ajax state");
        }
    }

    #[test]
    fn cockpit_known_actions_never_return_command_hints() {
        for (handle, action) in [
            ("web/fix-login", "open task"),
            ("web/fix-login", "merge task"),
            ("web", "new task"),
            ("web", "status"),
        ] {
            let mut context = sample_context();
            let item = ajax_core::models::AttentionItem {
                task_id: TaskId::new(format!("__cockpit_action__{action}")),
                task_handle: handle.to_string(),
                reason: action.to_string(),
                priority: 0,
                recommended_action: action.to_string(),
            };
            let mut runner = PanicRunner;
            let mut state_changed = false;

            let outcome =
                super::tui_cockpit_action(&item, &mut context, &mut runner, &mut state_changed)
                    .unwrap();

            if let ajax_tui::ActionOutcome::Message(message) = outcome {
                assert!(!message.contains("try: ajax"), "{action}: {message}");
                assert!(!message.contains("run `ajax"), "{action}: {message}");
            }
        }

        let mut context = cleanable_context();
        let item = cockpit_item("web/fix-login", "clean task");
        let mut runner = RecordingCommandRunner::default();
        let mut state_changed = false;

        let outcome =
            super::tui_cockpit_action(&item, &mut context, &mut runner, &mut state_changed)
                .unwrap();

        if let ajax_tui::ActionOutcome::Message(message) = outcome {
            assert!(!message.contains("try: ajax"), "clean task: {message}");
            assert!(!message.contains("run `ajax"), "clean task: {message}");
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
            "check task",
            "diff task",
        ] {
            let item = cockpit_item("web/fix-login", action);
            let mut runner = PanicRunner;
            let mut state_changed = false;

            let outcome =
                super::tui_cockpit_action(&item, &mut context, &mut runner, &mut state_changed)
                    .unwrap();

            match outcome {
                ajax_tui::ActionOutcome::Message(message) => {
                    assert!(message.contains(action), "{action}: {message}");
                    assert!(!message.contains("try: ajax"), "{action}: {message}");
                }
                _ => panic!("{action} should be an unknown cockpit action"),
            }
            assert!(!state_changed, "{action}");
        }
    }

    #[test]
    fn cockpit_unknown_action_does_not_suggest_shell_command() {
        let mut context = sample_context();
        let item = cockpit_item("web/fix-login", "mystery action");
        let mut runner = PanicRunner;
        let mut state_changed = false;

        let outcome =
            super::tui_cockpit_action(&item, &mut context, &mut runner, &mut state_changed)
                .unwrap();

        match outcome {
            ajax_tui::ActionOutcome::Message(message) => {
                assert!(message.contains("mystery action"));
                assert!(!message.contains("try: ajax"));
                assert!(!message.contains("run `ajax"));
            }
            _ => panic!("unknown cockpit action should stay in cockpit"),
        }
        assert!(!state_changed);
    }

    #[test]
    fn cockpit_action_contract_covers_all_current_actions() {
        enum Expected<'a> {
            Confirm(&'a str),
            Defer,
            Message(&'a [&'a str]),
            Refresh,
        }

        let cases = [
            (
                RecommendedAction::NewTask,
                "web",
                Expected::Message(&["select a project", "new task"]),
            ),
            (
                RecommendedAction::OpenTask,
                "web/fix-login",
                Expected::Defer,
            ),
            (
                RecommendedAction::MergeTask,
                "web/fix-login",
                Expected::Defer,
            ),
            (
                RecommendedAction::CleanTask,
                "web/fix-login",
                Expected::Refresh,
            ),
            (
                RecommendedAction::RemoveTask,
                "web/fix-login",
                Expected::Confirm("remove task"),
            ),
            (
                RecommendedAction::Status,
                "web",
                Expected::Message(&["web: 1 task(s)"]),
            ),
        ];
        let covered_actions = cases
            .iter()
            .map(|(action, _, _)| *action)
            .collect::<std::collections::BTreeSet<_>>();
        let product_actions = RecommendedAction::cockpit_product_actions()
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();

        assert_eq!(covered_actions, product_actions);

        for (action, handle, expected) in cases {
            let action = action.as_str();
            let mut context = if action == "clean task" {
                cleanable_context()
            } else {
                sample_context()
            };
            let item = cockpit_item(handle, action);
            let mut runner = RecordingCommandRunner::default();
            let mut state_changed = false;

            let outcome =
                super::tui_cockpit_action(&item, &mut context, &mut runner, &mut state_changed)
                    .unwrap();

            match expected {
                Expected::Confirm(part) => match outcome {
                    ajax_tui::ActionOutcome::Confirm(message) => {
                        assert!(message.contains(part), "{action}: {message}");
                        assert!(
                            runner.commands().is_empty(),
                            "{action} should not execute before confirmation"
                        );
                        assert!(!state_changed, "{action}");
                    }
                    ajax_tui::ActionOutcome::Defer(_) => {
                        panic!("{action} should request confirmation, got defer");
                    }
                    ajax_tui::ActionOutcome::Message(message) => {
                        panic!("{action} should request confirmation, got message: {message}");
                    }
                    ajax_tui::ActionOutcome::Refresh { .. } => {
                        panic!("{action} should request confirmation, got refresh");
                    }
                },
                Expected::Defer => match outcome {
                    ajax_tui::ActionOutcome::Defer(pending) => {
                        assert_eq!(pending.task_handle, handle, "{action}");
                        assert_eq!(pending.recommended_action, action);
                        assert!(pending.task_title.is_none(), "{action}");
                        assert!(
                            runner.commands().is_empty(),
                            "{action} should not execute before pending handling"
                        );
                        assert!(!state_changed, "{action}");
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
                },
                Expected::Message(parts) => match outcome {
                    ajax_tui::ActionOutcome::Message(message) => {
                        for part in parts {
                            assert!(message.contains(part), "{action}: missing {part:?}");
                        }
                        assert!(
                            runner.commands().is_empty(),
                            "{action} should not execute commands"
                        );
                        assert!(!state_changed, "{action}");
                    }
                    ajax_tui::ActionOutcome::Defer(_) => {
                        panic!("{action} should render in cockpit, got defer");
                    }
                    ajax_tui::ActionOutcome::Confirm(message) => {
                        panic!("{action} should render in cockpit, got confirm: {message}");
                    }
                    ajax_tui::ActionOutcome::Refresh { .. } => {
                        panic!("{action} should render in cockpit, got refresh");
                    }
                },
                Expected::Refresh => match outcome {
                    ajax_tui::ActionOutcome::Refresh {
                        repos,
                        tasks,
                        inbox,
                    } => {
                        assert_eq!(repos.repos.len(), 1, "{action}");
                        if action == "clean task" {
                            assert!(tasks.tasks.is_empty(), "{action}");
                            assert!(inbox.items.is_empty(), "{action}");
                        } else {
                            assert_eq!(tasks.tasks.len(), 1, "{action}");
                            assert!(!inbox.items.is_empty(), "{action}");
                        }
                        assert!(!runner.commands().is_empty(), "{action}");
                        assert!(state_changed, "{action}");
                    }
                    ajax_tui::ActionOutcome::Defer(_) => {
                        panic!("{action} should refresh, got defer");
                    }
                    ajax_tui::ActionOutcome::Message(message) => {
                        panic!("{action} should refresh, got message: {message}");
                    }
                    ajax_tui::ActionOutcome::Confirm(message) => {
                        panic!("{action} should refresh, got confirm: {message}");
                    }
                },
            }
        }
    }

    #[test]
    fn cockpit_merge_task_action_stays_inside_ajax() {
        let mut context = sample_context();
        let item = ajax_core::models::AttentionItem {
            task_id: TaskId::new("__task_action__web_fix_login__merge"),
            task_handle: "web/fix-login".to_string(),
            reason: "Merge task".to_string(),
            priority: 0,
            recommended_action: "merge task".to_string(),
        };
        let mut runner = PanicRunner;
        let mut state_changed = false;

        let outcome =
            super::tui_cockpit_action(&item, &mut context, &mut runner, &mut state_changed)
                .unwrap();

        match outcome {
            ajax_tui::ActionOutcome::Defer(pending) => {
                assert_eq!(pending.task_handle, "web/fix-login");
                assert_eq!(pending.recommended_action, "merge task");
                assert!(pending.task_title.is_none());
            }
            _ => panic!("completed task action should defer for execution"),
        }
        assert!(!state_changed);
    }

    #[test]
    fn cockpit_task_action_return_stays_inside_ajax() {
        let mut context = sample_context();
        let item = ajax_core::models::AttentionItem {
            task_id: TaskId::new("__task_action__web_fix_login__open"),
            task_handle: "web/fix-login".to_string(),
            reason: "Open task".to_string(),
            priority: 0,
            recommended_action: "open task".to_string(),
        };
        let mut runner = PanicRunner;
        let mut state_changed = false;

        let outcome =
            super::tui_cockpit_action(&item, &mut context, &mut runner, &mut state_changed)
                .unwrap();

        match outcome {
            ajax_tui::ActionOutcome::Defer(pending) => {
                assert_eq!(pending.task_handle, "web/fix-login");
                assert_eq!(pending.recommended_action, "open task");
                assert!(pending.task_title.is_none());
            }
            _ => panic!("task action should defer for execution"),
        }
        assert!(!state_changed);
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
            recommended_action: "new task".to_string(),
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
                if message.contains("new task title is required")));
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
            recommended_action: "new task".to_string(),
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
                if message.contains("new task title is required")));
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
            recommended_action: "new task".to_string(),
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
                RecommendedAction::OpenTask.as_str().to_string(),
                RecommendedAction::RemoveTask.as_str().to_string(),
            ]
        );
        let inbox = ajax_core::commands::inbox(&context);
        assert_eq!(inbox.items.len(), 1);
        assert_eq!(
            inbox.items[0].recommended_action,
            RecommendedAction::OpenTask.as_str()
        );
        assert!(RecommendedAction::cockpit_product_actions().contains(&RecommendedAction::OpenTask));
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
            recommended_action: "new task".to_string(),
            task_title: Some("Fix login".to_string()),
        };
        let mut runner = RecordingCommandRunner::default();
        let mut state_changed = false;

        let output = super::execute_pending_cockpit_action(
            &pending,
            &mut context,
            &mut runner,
            &mut state_changed,
        )
        .unwrap();

        assert!(output.contains("recorded task: api/fix-login"));
        assert_eq!(
            runner.commands(),
            &[
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
                        "main"
                    ]
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
                        "/Users/matt/projects/api__worktrees/ajax-fix-login"
                    ]
                ),
                CommandSpec::new(
                    "tmux",
                    [
                        "send-keys",
                        "-t",
                        "ajax-api-fix-login:worktrunk",
                        "codex --cd /Users/matt/projects/api__worktrees/ajax-fix-login 'Fix login'",
                        "Enter"
                    ]
                ),
                CommandSpec::new(
                    "tmux",
                    ["select-window", "-t", "ajax-api-fix-login:worktrunk"]
                ),
                expected_new_task_open_command("ajax-api-fix-login")
            ]
        );
        let task = context
            .registry
            .list_tasks()
            .iter()
            .find(|task| task.qualified_handle() == "api/fix-login")
            .cloned()
            .expect("new task should be recorded");
        assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
        assert!(state_changed);
    }

    #[test]
    fn task_command_operation_maps_cli_commands_and_cockpit_aliases() {
        assert_eq!(
            super::TaskCommandOperation::from_cli_subcommand("open"),
            Some(super::TaskCommandOperation::Open)
        );
        assert_eq!(
            super::TaskCommandOperation::from_cli_subcommand("trunk"),
            Some(super::TaskCommandOperation::Trunk)
        );
        assert_eq!(
            super::TaskCommandOperation::from_cli_subcommand("check"),
            Some(super::TaskCommandOperation::Check)
        );
        assert_eq!(
            super::TaskCommandOperation::from_cli_subcommand("diff"),
            Some(super::TaskCommandOperation::Diff)
        );
        assert_eq!(
            super::TaskCommandOperation::from_cli_subcommand("merge"),
            Some(super::TaskCommandOperation::Merge)
        );
        assert_eq!(
            super::TaskCommandOperation::from_cli_subcommand("cleanup"),
            Some(super::TaskCommandOperation::Cleanup)
        );
        assert_eq!(
            super::TaskCommandOperation::from_cli_subcommand("clean"),
            Some(super::TaskCommandOperation::Clean)
        );
        assert_eq!(
            super::TaskCommandOperation::from_cli_subcommand("remove"),
            Some(super::TaskCommandOperation::Remove)
        );
        assert_eq!(
            super::TaskCommandOperation::from_cli_subcommand("status"),
            None
        );

        assert_eq!(
            super::TaskCommandOperation::from_recommended_action(RecommendedAction::OpenTask),
            Some(super::TaskCommandOperation::Open)
        );
        assert_eq!(
            super::TaskCommandOperation::from_recommended_action(RecommendedAction::MergeTask),
            Some(super::TaskCommandOperation::Merge)
        );
        assert_eq!(
            super::TaskCommandOperation::from_recommended_action(RecommendedAction::CleanTask),
            Some(super::TaskCommandOperation::Cleanup)
        );
        assert_eq!(RecommendedAction::from_label("reconcile"), None);
    }

    #[test]
    fn task_command_operation_defines_cockpit_return_policy() {
        for operation in [
            super::TaskCommandOperation::Open,
            super::TaskCommandOperation::Trunk,
        ] {
            assert!(
                !operation.returns_to_cockpit_after_execute(),
                "{operation:?} should exit to the external command output"
            );
        }

        for operation in [
            super::TaskCommandOperation::Check,
            super::TaskCommandOperation::Diff,
            super::TaskCommandOperation::Merge,
            super::TaskCommandOperation::Cleanup,
            super::TaskCommandOperation::Remove,
        ] {
            assert!(
                operation.returns_to_cockpit_after_execute(),
                "{operation:?} should return to the task picker"
            );
        }
    }

    #[test]
    fn cleanup_and_remove_parse_as_distinct_executable_task_commands() {
        for command in ["cleanup", "remove"] {
            let matches = build_cli()
                .try_get_matches_from(["ajax", command, "web/fix-login", "--execute", "--yes"])
                .unwrap_or_else(|error| panic!("{command} should parse: {error}"));
            let Some((name, subcommand)) = matches.subcommand() else {
                panic!("{command} should parse as a subcommand");
            };

            assert_eq!(name, command);
            assert_eq!(
                subcommand.get_one::<String>("task").map(String::as_str),
                Some("web/fix-login")
            );
            assert!(subcommand.get_flag("execute"));
            assert!(subcommand.get_flag("yes"));
        }
    }

    #[test]
    fn pending_cockpit_merge_returns_to_ajax() {
        let mut merge_context = safe_merge_context();
        let mut merge_runner = QueuedRunner::new(vec![output(0, ""), output(0, "merged\n")]);
        let mut state_changed = false;
        let pending = ajax_tui::PendingAction {
            task_handle: "web/fix-login".to_string(),
            recommended_action: "merge task".to_string(),
            task_title: None,
        };

        let outcome = super::execute_pending_cockpit_action(
            &pending,
            &mut merge_context,
            &mut merge_runner,
            &mut state_changed,
        )
        .unwrap();

        assert_eq!(outcome, super::PendingCockpitOutcome::ReturnToCockpit);
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
        let item = ajax_core::models::AttentionItem {
            task_id: TaskId::new("__task_action__web_fix_login__remove"),
            task_handle: "web/fix-login".to_string(),
            reason: "Remove task".to_string(),
            priority: 0,
            recommended_action: "remove task".to_string(),
        };
        let mut runner = RecordingCommandRunner::default();
        let mut state_changed = false;

        let outcome =
            super::tui_cockpit_action(&item, &mut context, &mut runner, &mut state_changed)
                .unwrap();

        assert!(matches!(outcome, ajax_tui::ActionOutcome::Confirm(message)
            if message.contains("press enter again") && message.contains("remove task")));
        assert!(runner.commands().is_empty());
        assert!(!state_changed);
    }

    #[test]
    fn confirmed_cockpit_remove_action_force_removes_and_refreshes_inside_ajax() {
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
        let item = ajax_core::models::AttentionItem {
            task_id: TaskId::new("__task_action__web_fix_login__remove"),
            task_handle: "web/fix-login".to_string(),
            reason: "Remove task".to_string(),
            priority: 0,
            recommended_action: "remove task".to_string(),
        };
        let mut runner = RecordingCommandRunner::default();
        let mut state_changed = false;

        let outcome = super::tui_cockpit_confirmed_action(
            &item,
            &mut context,
            &mut runner,
            &mut state_changed,
        )
        .unwrap();

        assert!(matches!(outcome, ajax_tui::ActionOutcome::Refresh { .. }));
        assert_eq!(
            runner.commands(),
            &[
                CommandSpec::new("tmux", ["kill-session", "-t", "ajax-web-fix-login"]),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "worktree",
                        "remove",
                        "--force",
                        "/tmp/worktrees/web-fix-login"
                    ]
                ),
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
            ]
        );
        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .lifecycle_status,
            LifecycleStatus::Removed
        );
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
            recommended_action: "reconcile".to_string(),
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
    fn pending_cockpit_open_and_create_actions_exit_ajax() {
        let action = "open task";
        let mut context = sample_context();
        let mut runner = RecordingCommandRunner::default();
        let mut state_changed = false;
        let pending = ajax_tui::PendingAction {
            task_handle: "web/fix-login".to_string(),
            recommended_action: action.to_string(),
            task_title: None,
        };

        let outcome = super::cockpit_actions::execute_pending_cockpit_action_with_open_mode(
            &pending,
            &mut context,
            &mut runner,
            &mut state_changed,
            OpenMode::Attach,
        )
        .unwrap();

        assert!(matches!(outcome, super::PendingCockpitOutcome::Exit(_)));

        let mut context = CommandContext::new(
            Config {
                repos: vec![ManagedRepo::new("api", "/Users/matt/projects/api", "main")],
                ..Config::default()
            },
            InMemoryRegistry::default(),
        );
        let pending = ajax_tui::PendingAction {
            task_handle: "api".to_string(),
            recommended_action: "new task".to_string(),
            task_title: Some("Fix login".to_string()),
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

        assert!(matches!(outcome, super::PendingCockpitOutcome::Exit(_)));
        assert!(state_changed);
    }

    #[test]
    fn pending_cockpit_removed_actions_are_rejected() {
        for action in [
            "inspect agent",
            "inspect test output",
            "monitor task",
            "review branch",
            "review diff",
            "check task",
            "diff task",
        ] {
            let mut context = sample_context();
            let pending = ajax_tui::PendingAction {
                task_handle: "web/fix-login".to_string(),
                recommended_action: action.to_string(),
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
    fn pending_cockpit_open_worktrunk_runs_trunk_plan() {
        let mut context = sample_context();
        let pending = ajax_tui::PendingAction {
            task_handle: "web/fix-login".to_string(),
            recommended_action: "open worktrunk".to_string(),
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

        assert!(matches!(outcome, super::PendingCockpitOutcome::Exit(_)));
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
    fn pending_cockpit_open_worktrunk_switches_client_when_inside_tmux() {
        let mut context = sample_context();
        let pending = ajax_tui::PendingAction {
            task_handle: "web/fix-login".to_string(),
            recommended_action: "open worktrunk".to_string(),
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
        let action = "open task";
        let mut context = sample_context();
        let pending = ajax_tui::PendingAction {
            task_handle: "web/fix-login".to_string(),
            recommended_action: action.to_string(),
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
            recommended_action: "mystery action".to_string(),
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
            recommended_action: "merge task".to_string(),
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
            recommended_action: "merge task".to_string(),
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

        assert!(outcome.is_none());
        assert_eq!(cockpit_flash.as_deref(), Some("git exited with status 42"));
    }

    #[test]
    fn pending_cockpit_open_action_runs_task_without_lifecycle_change() {
        let mut context = sample_context();
        let pending = ajax_tui::PendingAction {
            task_handle: "web/fix-login".to_string(),
            recommended_action: "open task".to_string(),
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
            recommended_action: "open task".to_string(),
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
            recommended_action: "merge task".to_string(),
            task_title: None,
        };
        let mut runner = RecordingCommandRunner::default();
        let mut state_changed = false;

        super::execute_pending_cockpit_action(
            &pending,
            &mut context,
            &mut runner,
            &mut state_changed,
        )
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
            recommended_action: "clean task".to_string(),
            task_title: None,
        };
        let mut runner = RecordingCommandRunner::default();
        let mut state_changed = false;

        let output = super::execute_pending_cockpit_action(
            &pending,
            &mut context,
            &mut runner,
            &mut state_changed,
        )
        .unwrap();

        assert_eq!(output, super::PendingCockpitOutcome::ReturnToCockpit);
        assert_eq!(
            runner.commands(),
            &[
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
                )
            ]
        );
        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .lifecycle_status,
            LifecycleStatus::Removed
        );
        assert!(state_changed);
    }

    #[test]
    fn cockpit_reconcile_action_is_unknown() {
        let mut context = sample_context();
        let item = ajax_core::models::AttentionItem {
            task_id: TaskId::new("__project_action__web__reconcile"),
            task_handle: "web".to_string(),
            reason: "Reconcile".to_string(),
            priority: 0,
            recommended_action: "reconcile".to_string(),
        };
        let mut runner = RecordingCommandRunner::default();
        let mut state_changed = false;

        let outcome =
            super::tui_cockpit_action(&item, &mut context, &mut runner, &mut state_changed)
                .unwrap();

        assert!(matches!(outcome, ajax_tui::ActionOutcome::Message(message)
            if message == "cockpit action is not configured: reconcile"));
        assert!(runner.commands().is_empty());
        assert!(!state_changed);
    }

    #[test]
    fn cockpit_clean_action_requires_confirmation_before_running() {
        let mut context = cleanable_context();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .add_side_flag(SideFlag::Dirty);
        let item = ajax_core::models::AttentionItem {
            task_id: TaskId::new("__task_action__web_fix_login__clean"),
            task_handle: "web/fix-login".to_string(),
            reason: "Clean task".to_string(),
            priority: 0,
            recommended_action: "clean task".to_string(),
        };
        let mut runner = RecordingCommandRunner::default();
        let mut state_changed = false;

        let outcome =
            super::tui_cockpit_action(&item, &mut context, &mut runner, &mut state_changed)
                .unwrap();

        assert!(matches!(outcome, ajax_tui::ActionOutcome::Confirm(message)
            if message.contains("press enter again") && message.contains("clean task")));
        assert!(runner.commands().is_empty());
        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .lifecycle_status,
            LifecycleStatus::Cleanable
        );
        assert!(!state_changed);
    }

    #[test]
    fn confirmed_cockpit_clean_action_runs_and_refreshes_inside_ajax() {
        let mut context = cleanable_context();
        let item = ajax_core::models::AttentionItem {
            task_id: TaskId::new("__task_action__web_fix_login__clean"),
            task_handle: "web/fix-login".to_string(),
            reason: "Clean task".to_string(),
            priority: 0,
            recommended_action: "clean task".to_string(),
        };
        let mut runner = RecordingCommandRunner::default();
        let mut state_changed = false;

        let outcome = super::tui_cockpit_confirmed_action(
            &item,
            &mut context,
            &mut runner,
            &mut state_changed,
        )
        .unwrap();

        match outcome {
            ajax_tui::ActionOutcome::Refresh {
                repos,
                tasks,
                inbox,
            } => {
                assert_eq!(repos.repos.len(), 1);
                assert!(tasks.tasks.is_empty());
                assert!(inbox.items.is_empty());
            }
            ajax_tui::ActionOutcome::Defer(_) => {
                panic!("clean task should refresh Ajax instead of deferring out")
            }
            ajax_tui::ActionOutcome::Message(message) => {
                panic!("clean task should run instead of showing message: {message}")
            }
            ajax_tui::ActionOutcome::Confirm(message) => {
                panic!("confirmed clean task should run instead of confirming: {message}")
            }
        }
        assert_eq!(
            runner.commands(),
            &[
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
                )
            ]
        );
        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .lifecycle_status,
            LifecycleStatus::Removed
        );
        assert!(state_changed);
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
}

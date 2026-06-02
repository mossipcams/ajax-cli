use ajax_core::{
    adapters::CommandRunner,
    commands::{self, CommandContext},
    registry::InMemoryRegistry,
    runtime_refresh::{refresh_runtime_context, refresh_runtime_context_with_agent_status_cache},
};
use ajax_tui::CockpitSnapshot;
use clap::ArgMatches;
use std::{
    io::{ErrorKind, Write},
    net::TcpListener,
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};

use crate::{
    agent_status_cache::TmuxAgentStatusCache,
    cockpit_actions::{
        execute_pending_cockpit_action_with_task_session, handle_pending_cockpit_result,
        tui_cockpit_action, tui_cockpit_confirmed_action,
    },
    render::render_response,
    task_session::PtyTaskSessionRunner,
    CliContextPaths, CliError, RenderedCommand,
};

pub(crate) fn render_cockpit_command(
    context: &CommandContext<InMemoryRegistry>,
    matches: &ArgMatches,
) -> Result<String, CliError> {
    if matches.get_flag("json") {
        return render_response(commands::cockpit(context), true, |_| String::new());
    }

    let iterations = parse_u32_arg(matches, "iterations", 1)?;
    let interval = parse_u64_arg(matches, "interval-ms", 1000)?;

    if matches.get_flag("watch") {
        let interval = Duration::from_millis(interval);
        let frames = (0..iterations.max(1))
            .map(|index| {
                if index > 0 && !interval.is_zero() {
                    std::thread::sleep(interval);
                }
                render_cockpit_frame(context)
            })
            .collect::<Vec<_>>();
        return Ok(frames.join("\n\n"));
    }

    Err(CliError::CommandFailed(
        "interactive cockpit requires command execution support".to_string(),
    ))
}

pub(crate) fn render_cockpit_frame(context: &CommandContext<InMemoryRegistry>) -> String {
    let view = commands::cockpit_view(context);
    ajax_tui::render_cockpit(&view.repos, &view.cards, &view.inbox)
}

pub(crate) fn render_interactive_cockpit_command<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    subcommand: &ArgMatches,
    runner: &mut R,
    mobile_web_port: u16,
    paths: Option<&CliContextPaths>,
) -> Result<RenderedCommand, CliError> {
    let _mobile_web_companion = if subcommand.get_flag("no-web") {
        None
    } else {
        start_mobile_web_companion(mobile_web_port, paths)?
    };
    let mut state_changed = false;
    let mut cockpit_flash = None;
    let mut open_new_task_repo = None;
    state_changed |= refresh_live_context(context, runner)?;
    let refresh_interval = Duration::from_millis(parse_u64_arg(subcommand, "interval-ms", 1000)?);
    loop {
        let mut task_session = PtyTaskSessionRunner;
        let mut cached_snapshot = None;
        let snapshot =
            refresh_cockpit_snapshot(context, runner, &mut state_changed, &mut cached_snapshot)?;
        let pending = ajax_tui::run_interactive_with_flash_and_refresh(
            snapshot.repos,
            snapshot.cards,
            snapshot.inbox,
            cockpit_flash.take(),
            refresh_interval,
            InteractiveCockpitHandler {
                context,
                runner,
                state_changed: &mut state_changed,
                cached_snapshot: &mut cached_snapshot,
            },
            open_new_task_repo.take(),
        )
        .map_err(|e| CliError::CommandFailed(e.to_string()))?;
        let Some(pending) = pending else {
            return Ok(RenderedCommand {
                output: String::new(),
                state_changed,
            });
        };

        match execute_pending_cockpit_action_with_task_session(
            &pending,
            context,
            runner,
            &mut state_changed,
            &mut task_session,
        )? {
            crate::cockpit_actions::PendingCockpitExecution::OpenNewTask { repo } => {
                open_new_task_repo = Some(repo);
            }
            crate::cockpit_actions::PendingCockpitExecution::Continue(message) => {
                if !handle_pending_cockpit_result(Ok(message), &mut cockpit_flash) {
                    continue;
                }
            }
        }
    }
}

const MOBILE_WEB_HOST: &str = "0.0.0.0";
const STABLE_MOBILE_WEB_PORT: u16 = 8787;
const DEV_MOBILE_WEB_PORT: u16 = 8788;

pub(crate) fn mobile_web_port_for_command(command: &str) -> u16 {
    match command {
        "dev" => DEV_MOBILE_WEB_PORT,
        "stable" | "cockpit" => STABLE_MOBILE_WEB_PORT,
        _ => STABLE_MOBILE_WEB_PORT,
    }
}

struct MobileWebCompanion {
    child: Child,
}

impl Drop for MobileWebCompanion {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start_mobile_web_companion(
    port: u16,
    paths: Option<&CliContextPaths>,
) -> Result<Option<MobileWebCompanion>, CliError> {
    match TcpListener::bind((MOBILE_WEB_HOST, port)) {
        Ok(listener) => drop(listener),
        Err(error) if error.kind() == ErrorKind::AddrInUse => return Ok(None),
        Err(error) => {
            return Err(CliError::CommandFailed(format!(
                "Ajax mobile web companion unavailable: {error}"
            )));
        }
    }

    let executable = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            return Err(CliError::CommandFailed(format!(
                "Ajax mobile web companion unavailable: {error}"
            )));
        }
    };
    let mut command = mobile_web_companion_command(&executable, port, paths);

    command
        .spawn()
        .map(|child| Some(MobileWebCompanion { child }))
        .map_err(|error| {
            CliError::CommandFailed(format!("Ajax mobile web companion unavailable: {error}"))
        })
}

fn mobile_web_companion_command(
    executable: &Path,
    port: u16,
    paths: Option<&CliContextPaths>,
) -> Command {
    let mut command = Command::new(executable);
    let port = port.to_string();
    command
        .args(["web", "--host", MOBILE_WEB_HOST, "--port", port.as_str()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    if let Some(paths) = paths {
        command.env_remove("AJAX_HOME");
        command.env_remove("AJAX_WORKTREE_ROOT");
        command.env("AJAX_PROFILE", &paths.runtime_paths.profile);
        command.env("AJAX_CONFIG", &paths.config_file);
        command.env("AJAX_STATE", &paths.state_file);
        if let ajax_core::config::WorktreePlacement::Root(root) =
            &paths.runtime_paths.worktree_placement
        {
            command.env("AJAX_WORKTREE_ROOT", root);
        }
    } else {
        preserve_ajax_context_env(&mut command, "AJAX_CONFIG");
        preserve_ajax_context_env(&mut command, "AJAX_STATE");
    }

    command
}

fn preserve_ajax_context_env(command: &mut Command, name: &str) {
    if let Some(value) = std::env::var_os(name) {
        command.env(name, value);
    }
}

#[cfg(test)]
mod mobile_web_companion_tests {
    use super::{mobile_web_companion_command, mobile_web_port_for_command};
    use crate::CliContextPaths;
    use ajax_core::config::RuntimePathRequest;
    use std::ffi::OsStr;

    #[test]
    fn dev_mobile_web_companion_uses_dev_port() {
        assert_eq!(mobile_web_port_for_command("dev"), 8788);
    }

    #[test]
    fn mobile_web_companion_preserves_full_dev_runtime_context() {
        let paths = CliContextPaths::from_runtime_paths(
            RuntimePathRequest::new("/Users/matt")
                .with_cli_profile("dev")
                .resolve(),
        );
        let command = mobile_web_companion_command(
            std::path::Path::new("/tmp/ajax-cli"),
            mobile_web_port_for_command("dev"),
            Some(&paths),
        );
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let envs = command.get_envs().collect::<Vec<_>>();

        assert_eq!(args, ["web", "--host", "0.0.0.0", "--port", "8788"]);
        assert!(envs.contains(&(
            OsStr::new("AJAX_PROFILE"),
            Some(OsStr::new(paths.runtime_paths.profile.as_str()))
        )));
        assert!(envs.contains(&(
            OsStr::new("AJAX_CONFIG"),
            Some(paths.config_file.as_os_str())
        )));
        assert!(envs.contains(&(OsStr::new("AJAX_STATE"), Some(paths.state_file.as_os_str()))));
        assert!(envs.iter().any(|(name, value)| {
            *name == OsStr::new("AJAX_WORKTREE_ROOT")
                && value.is_some_and(|value| value == "/Users/matt/.ajax-dev/worktrees")
        }));
    }
}

pub(crate) fn render_live_cockpit_command<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    matches: &ArgMatches,
    runner: &mut R,
) -> Result<RenderedCommand, CliError> {
    let iterations = parse_u32_arg(matches, "iterations", 1)?.max(1);
    let interval = parse_u64_arg(matches, "interval-ms", 1000)?;
    let json = matches.get_flag("json");

    let mut state_changed = false;
    let mut frames = Vec::with_capacity(iterations as usize);

    for index in 0..iterations {
        if index > 0 && interval > 0 {
            std::thread::sleep(Duration::from_millis(interval));
        }
        let changed = refresh_live_context(context, runner)?;
        state_changed |= changed;
        if json {
            frames.push(render_response(commands::cockpit(context), true, |_| {
                String::new()
            })?);
        } else {
            frames.push(render_cockpit_frame(context));
        }
    }

    Ok(RenderedCommand {
        output: frames.join("\n\n"),
        state_changed,
    })
}

pub(crate) fn stream_live_cockpit_command<R, W, P>(
    context: &mut CommandContext<InMemoryRegistry>,
    matches: &ArgMatches,
    runner: &mut R,
    writer: &mut W,
    mut persist: P,
) -> Result<bool, CliError>
where
    R: CommandRunner,
    W: Write,
    P: FnMut(&CommandContext<InMemoryRegistry>) -> Result<(), CliError>,
{
    let iterations = parse_optional_u32_arg(matches, "iterations")?.map(|value| value.max(1));
    let interval = parse_u64_arg(matches, "interval-ms", 1000)?;
    let json = matches.get_flag("json");

    let mut state_changed = false;
    let mut index = 0;

    loop {
        if index > 0 && interval > 0 {
            std::thread::sleep(Duration::from_millis(interval));
        }
        let changed = refresh_live_context(context, runner)?;
        state_changed |= changed;
        if changed {
            persist(context)?;
        }

        let frame = if json {
            render_response(commands::cockpit(context), true, |_| String::new())?
        } else {
            render_cockpit_frame(context)
        };
        if !write_stream_frame(writer, &frame)? {
            return Ok(state_changed);
        }

        index += 1;
        if iterations.is_some_and(|limit| index >= limit) {
            return Ok(state_changed);
        }
    }
}

pub(crate) fn refresh_live_context<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
) -> Result<bool, CliError> {
    if let Some(cache) = TmuxAgentStatusCache::from_default_location() {
        refresh_runtime_context_with_agent_status_cache(context, runner, &cache)
            .map_err(crate::command_error)
    } else {
        refresh_runtime_context(context, runner).map_err(crate::command_error)
    }
}

pub(crate) fn refresh_cockpit_snapshot<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    state_changed: &mut bool,
    cached_snapshot: &mut Option<CockpitSnapshot>,
) -> Result<CockpitSnapshot, CliError> {
    let changed = refresh_live_context(context, runner)?;
    *state_changed |= changed;
    if changed || cached_snapshot.is_none() {
        let snapshot = build_cockpit_snapshot(context);
        *cached_snapshot = Some(snapshot.clone());
        Ok(snapshot)
    } else {
        Ok(cached_snapshot
            .as_ref()
            .expect("cached snapshot must exist after first build")
            .clone())
    }
}

pub(crate) fn build_cockpit_snapshot(
    context: &CommandContext<InMemoryRegistry>,
) -> CockpitSnapshot {
    let view = commands::cockpit_view(context);
    CockpitSnapshot {
        repos: view.repos,
        cards: view.cards,
        inbox: view.inbox,
    }
}

struct InteractiveCockpitHandler<'a, R: CommandRunner> {
    context: &'a mut CommandContext<InMemoryRegistry>,
    runner: &'a mut R,
    state_changed: &'a mut bool,
    cached_snapshot: &'a mut Option<CockpitSnapshot>,
}

impl<R: CommandRunner> ajax_tui::CockpitEventHandler for InteractiveCockpitHandler<'_, R> {
    fn on_action(
        &mut self,
        item: &ajax_core::models::CockpitActionItem,
    ) -> std::io::Result<ajax_tui::ActionOutcome> {
        tui_cockpit_action(item, self.context)
    }

    fn on_confirmed_action(
        &mut self,
        item: &ajax_core::models::CockpitActionItem,
    ) -> std::io::Result<ajax_tui::ActionOutcome> {
        tui_cockpit_confirmed_action(item, self.context)
    }

    fn on_refresh(&mut self) -> std::io::Result<Option<CockpitSnapshot>> {
        refresh_cockpit_snapshot(
            self.context,
            self.runner,
            self.state_changed,
            self.cached_snapshot,
        )
        .map(Some)
        .map_err(|error| std::io::Error::other(error.to_string()))
    }
}

fn parse_u32_arg(matches: &ArgMatches, name: &str, default: u32) -> Result<u32, CliError> {
    let Some(value) = matches.get_one::<String>(name) else {
        return Ok(default);
    };

    value
        .parse::<u32>()
        .map_err(|_| CliError::CommandFailed(format!("invalid --{name} value: {value}")))
}

fn parse_optional_u32_arg(matches: &ArgMatches, name: &str) -> Result<Option<u32>, CliError> {
    let Some(value) = matches.get_one::<String>(name) else {
        return Ok(None);
    };

    value
        .parse::<u32>()
        .map(Some)
        .map_err(|_| CliError::CommandFailed(format!("invalid --{name} value: {value}")))
}

fn parse_u64_arg(matches: &ArgMatches, name: &str, default: u64) -> Result<u64, CliError> {
    let Some(value) = matches.get_one::<String>(name) else {
        return Ok(default);
    };

    value
        .parse::<u64>()
        .map_err(|_| CliError::CommandFailed(format!("invalid --{name} value: {value}")))
}

fn write_stream_frame(writer: &mut impl Write, frame: &str) -> Result<bool, CliError> {
    for chunk in [frame.as_bytes(), b"\n\n"] {
        if let Err(error) = writer.write_all(chunk) {
            if error.kind() == std::io::ErrorKind::BrokenPipe {
                return Ok(false);
            }
            return Err(CliError::CommandFailed(error.to_string()));
        }
    }
    if let Err(error) = writer.flush() {
        if error.kind() == std::io::ErrorKind::BrokenPipe {
            return Ok(false);
        }
        return Err(CliError::CommandFailed(error.to_string()));
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::{build_cockpit_snapshot, mobile_web_port_for_command, refresh_cockpit_snapshot};
    use ajax_core::{
        adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec},
        commands::CommandContext,
        config::{Config, ManagedRepo},
        models::{
            AgentClient, AgentRuntimeStatus, GitStatus, LifecycleStatus, LiveObservation,
            LiveStatusKind, OperatorAction, RuntimeHealth, RuntimeObservationSource, SideFlag,
            Task, TaskId, TmuxStatus, WorktrunkStatus,
        },
        registry::{InMemoryRegistry, Registry},
        runtime_refresh::{refresh_runtime_context_with_agent_status_cache, AgentStatusCache},
    };

    #[derive(Default)]
    struct LiveRefreshRunner;

    impl CommandRunner for LiveRefreshRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            let stdout = match command.args.as_slice() {
                [command, ..] if command == "list-sessions" => "ajax-web-fix-login\n",
                [_, repo, subcommand, action, flag]
                    if repo == "/Users/matt/projects/web"
                        && subcommand == "worktree"
                        && action == "list"
                        && flag == "--porcelain" =>
                {
                    "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n"
                }
                [_, repo, subcommand, format]
                    if repo == "/Users/matt/projects/web"
                        && subcommand == "branch"
                        && format == "--format=%(refname:short)" =>
                {
                    "main\najax/fix-login\n"
                }
                [command, ..] if command == "list-windows" => {
                    "ajax-web-fix-login\tworktrunk\t/tmp/worktrees/web-fix-login\n"
                }
                [command, ..] if command == "capture-pane" => "Do you want to proceed? y/n\n",
                _ => "",
            };

            Ok(CommandOutput {
                status_code: 0,
                stdout: stdout.to_string(),
                stderr: String::new(),
            })
        }
    }

    fn context_with_active_task() -> CommandContext<InMemoryRegistry> {
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
        task.lifecycle_status = LifecycleStatus::Active;
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
        registry.create_task(task).unwrap();

        CommandContext::new(config, registry)
    }

    #[test]
    fn cockpit_backend_live_refresh_delegates_runtime_refresh_to_core() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cockpit_backend.rs"),
        )
        .unwrap();
        let refresh_live_context = source
            .split("pub(crate) fn refresh_live_context")
            .nth(1)
            .and_then(|source| {
                source
                    .split("pub(crate) fn refresh_cockpit_snapshot")
                    .next()
            })
            .unwrap();
        let core_refresh = ["refresh_runtime", "_context"].concat();

        assert!(refresh_live_context.contains(&core_refresh));
        assert!(!refresh_live_context.contains("TmuxAdapter::new"));
        assert!(!refresh_live_context.contains("GitAdapter::new"));
    }

    #[test]
    fn cockpit_snapshot_build_explicitly_rebuilds_core_projection() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cockpit_backend.rs"),
        )
        .unwrap();
        let build_cockpit_snapshot = source
            .split("pub(crate) fn build_cockpit_snapshot")
            .nth(1)
            .and_then(|source| source.split("struct InteractiveCockpitHandler").next())
            .unwrap();
        let implicit_view_read = ["commands::", "cockpit_view"].concat();

        assert!(build_cockpit_snapshot.contains(&implicit_view_read));
        assert!(!build_cockpit_snapshot.contains("rebuild_cockpit_view"));
    }

    #[test]
    fn cockpit_backend_does_not_keep_test_only_agent_status_refresh_wrappers() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cockpit_backend.rs"),
        )
        .unwrap();

        for wrapper in [
            "refresh_live_context_with_agent_status_cache",
            "refresh_cockpit_snapshot_with_agent_status_cache",
        ] {
            let function_name = ["fn ", wrapper].concat();
            assert!(!source.contains(&function_name), "{wrapper}");
        }
    }

    #[test]
    fn cockpit_backend_does_not_keep_cockpit_watch_frame_wrapper() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cockpit_backend.rs"),
        )
        .unwrap();
        let wrapper = ["fn ", "render_cockpit_frames"].concat();

        assert!(!source.contains(&wrapper));
    }

    #[test]
    fn interactive_cockpit_auto_starts_mobile_web_companion() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cockpit_backend.rs"),
        )
        .unwrap();
        let interactive = source
            .split("pub(crate) fn render_interactive_cockpit_command")
            .nth(1)
            .and_then(|source| {
                source
                    .split("pub(crate) fn render_live_cockpit_command")
                    .next()
            })
            .unwrap();

        assert!(interactive.contains("start_mobile_web_companion"));
        assert!(interactive.contains("no-web"));
    }

    #[test]
    fn mobile_web_ports_are_separate_for_stable_and_dev() {
        assert_eq!(mobile_web_port_for_command("stable"), 8787);
        assert_eq!(mobile_web_port_for_command("cockpit"), 8787);
        assert_eq!(mobile_web_port_for_command("dev"), 8788);
    }

    #[test]
    fn mobile_web_companion_uses_child_process_and_guard() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cockpit_backend.rs"),
        )
        .unwrap();

        assert!(source.contains("struct MobileWebCompanion"));
        assert!(source.contains("impl Drop for MobileWebCompanion"));
        assert!(source.contains("std::env::current_exe"));
        assert!(source.contains("\"web\", \"--host\", MOBILE_WEB_HOST, \"--port\""));
        assert!(source.contains("port.to_string"));
    }

    #[test]
    fn mobile_web_companion_preserves_parent_ajax_context_environment() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cockpit_backend.rs"),
        )
        .unwrap();
        let production_source = source.split("#[cfg(test)]").next().unwrap();

        assert!(production_source.contains("AJAX_CONFIG"));
        assert!(production_source.contains("AJAX_STATE"));
    }

    #[derive(Default)]
    struct EmptyTmuxRunner;

    impl CommandRunner for EmptyTmuxRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            let stdout = match command.args.as_slice() {
                [command, ..] if command == "list-sessions" => "",
                _ => "",
            };

            Ok(CommandOutput {
                status_code: 0,
                stdout: stdout.to_string(),
                stderr: String::new(),
            })
        }
    }

    fn context_with_cached_running_task() -> CommandContext<InMemoryRegistry> {
        let mut context = context_with_active_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.agent_status = AgentRuntimeStatus::Running;
        task.add_side_flag(SideFlag::AgentRunning);
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::AgentRunning,
            "working on task",
        ));
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
        context
    }

    struct StaticAgentStatusCache {
        values: Vec<String>,
    }

    impl AgentStatusCache for StaticAgentStatusCache {
        fn status_values_for_session(&self, _session: &str) -> Vec<String> {
            self.values.clone()
        }
    }

    #[test]
    fn live_refresh_updates_cached_annotations_for_cockpit_inbox() {
        let mut context = context_with_active_task();
        let mut runner = LiveRefreshRunner;
        let mut state_changed = false;

        let snapshot =
            refresh_cockpit_snapshot(&mut context, &mut runner, &mut state_changed, &mut None)
                .unwrap();

        assert!(state_changed);
        assert_eq!(
            snapshot.cards[0].live_summary.as_deref(),
            Some("waiting for approval")
        );
        assert!(snapshot.inbox.items.iter().any(|item| {
            item.reason == "waiting_for_approval" && item.task_handle == "web/fix-login"
        }));
        assert!(context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .annotations
            .iter()
            .any(|annotation| annotation.evidence.label() == "waiting for approval"));
        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert_eq!(task.runtime_projection.health, RuntimeHealth::Healthy);
        assert_eq!(
            task.runtime_projection.source,
            RuntimeObservationSource::TmuxProbe
        );
    }

    #[test]
    fn cockpit_refresh_uses_hook_backed_agent_status_cache() {
        let mut context = context_with_active_task();
        let mut runner = LiveRefreshRunner;
        let cache = StaticAgentStatusCache {
            values: vec!["working".to_string()],
        };
        let mut state_changed = false;

        state_changed |=
            refresh_runtime_context_with_agent_status_cache(&mut context, &mut runner, &cache)
                .unwrap();
        let snapshot = build_cockpit_snapshot(&context);

        assert!(state_changed);
        let card = snapshot
            .cards
            .iter()
            .find(|card| card.qualified_handle == "web/fix-login")
            .expect("task should stay visible in cockpit");
        assert_eq!(card.status_label, "agent running");
        assert_eq!(card.ui_state, ajax_core::ui_state::UiState::Running);
        assert_eq!(card.live_summary.as_deref(), Some("agent running"));
    }

    #[test]
    fn live_refresh_clears_stale_input_when_codex_prompt_is_working() {
        let mut context = context_with_active_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .expect("fixture task should exist");
        task.lifecycle_status = LifecycleStatus::Waiting;
        task.agent_status = AgentRuntimeStatus::Waiting;
        task.add_side_flag(SideFlag::NeedsInput);
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::WaitingForInput,
            "waiting for input",
        ));
        task.annotations = ajax_core::attention::annotate(task);
        let mut runner = WorkingPromptRunner;
        let mut state_changed = false;

        let snapshot =
            refresh_cockpit_snapshot(&mut context, &mut runner, &mut state_changed, &mut None)
                .unwrap();

        assert!(state_changed);
        let card = snapshot
            .cards
            .iter()
            .find(|card| card.qualified_handle == "web/fix-login")
            .expect("task should stay visible in cockpit");
        assert_eq!(card.status_label, "agent running");
        assert_eq!(card.ui_state, ajax_core::ui_state::UiState::Running);
        assert!(card.annotations.is_empty(), "{:?}", card.annotations);
        assert!(!snapshot
            .inbox
            .items
            .iter()
            .any(|item| item.task_handle == "web/fix-login"));

        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert_eq!(task.agent_status, AgentRuntimeStatus::Running);
        assert!(!task.has_side_flag(SideFlag::NeedsInput));
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::AgentRunning)
        );
    }

    struct WorkingPromptRunner;

    impl CommandRunner for WorkingPromptRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            let stdout = match command.args.as_slice() {
                [command, ..] if command == "list-sessions" => "ajax-web-fix-login\n",
                [_, repo, subcommand, action, flag]
                    if repo == "/Users/matt/projects/web"
                        && subcommand == "worktree"
                        && action == "list"
                        && flag == "--porcelain" =>
                {
                    "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n"
                }
                [_, repo, subcommand, format]
                    if repo == "/Users/matt/projects/web"
                        && subcommand == "branch"
                        && format == "--format=%(refname:short)" =>
                {
                    "main\najax/fix-login\n"
                }
                [command, ..] if command == "list-windows" => {
                    "ajax-web-fix-login\tworktrunk\t/tmp/worktrees/web-fix-login\n"
                }
                [command, ..] if command == "capture-pane" => {
                    "• Working (3m 00s • esc to interrupt) · 1 background terminal running · /ps to…\n\n› Improve documentation in @filename\n\n  gpt-5.5 high · ~/Desktop/Projects/ajax-cli__worktrees/ajax-ci\n"
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
    fn live_refresh_marks_cached_running_task_invalid_when_tmux_sessions_are_empty() {
        let mut context = context_with_cached_running_task();
        let mut runner = EmptyTmuxRunner;
        let mut state_changed = false;

        let snapshot =
            refresh_cockpit_snapshot(&mut context, &mut runner, &mut state_changed, &mut None)
                .unwrap();

        assert!(state_changed);
        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert!(task.has_side_flag(SideFlag::TmuxMissing));
        assert!(!task.has_side_flag(SideFlag::AgentRunning));
        assert_eq!(task.agent_status, AgentRuntimeStatus::Unknown);
        assert_eq!(
            task.tmux_status.as_ref().map(|status| status.exists),
            Some(false)
        );
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::TmuxMissing)
        );
        let card = snapshot
            .cards
            .iter()
            .find(|card| card.qualified_handle == "web/fix-login")
            .expect("invalid task should stay visible in cockpit");
        assert_eq!(card.primary_action, OperatorAction::Drop);
        assert_eq!(card.available_actions, vec![OperatorAction::Drop]);
        assert!(
            snapshot
                .inbox
                .items
                .iter()
                .any(|item| item.task_handle == "web/fix-login"
                    && item.action == OperatorAction::Drop)
        );
        assert!(ajax_core::commands::inbox(&context)
            .items
            .iter()
            .any(|item| {
                item.task_handle == "web/fix-login" && item.action == OperatorAction::Drop
            }));
    }

    #[test]
    fn live_refresh_marks_cached_present_tmux_missing_even_after_fresh_command_result() {
        let mut context = context_with_active_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .expect("fixture task should exist");
        task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
        task.runtime_projection = ajax_core::models::RuntimeProjection::new(
            RuntimeHealth::Healthy,
            std::time::SystemTime::now(),
            RuntimeObservationSource::CommandResult,
        );
        let mut runner = EmptyTmuxRunner;

        let changed = super::refresh_live_context(&mut context, &mut runner).unwrap();
        let task = context
            .registry
            .get_task(&TaskId::new("task-1"))
            .expect("fixture task should remain registered");

        assert!(changed);
        assert!(task.has_side_flag(SideFlag::TmuxMissing));
        assert_eq!(
            task.tmux_status.as_ref().map(|status| status.exists),
            Some(false)
        );
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::TmuxMissing)
        );
    }

    #[test]
    fn live_refresh_reprobes_error_task_with_recoverable_conflict_prompt() {
        let mut context = context_with_active_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .expect("fixture task should exist");
        task.lifecycle_status = LifecycleStatus::Error;
        task.agent_status = AgentRuntimeStatus::Blocked;
        task.add_side_flag(SideFlag::Conflicted);
        task.add_side_flag(SideFlag::NeedsInput);
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::MergeConflict,
            "merge conflict needs attention",
        ));
        task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
        task.worktrunk_status = Some(WorktrunkStatus::present(
            "worktrunk",
            "/tmp/worktrees/web-fix-login",
        ));
        let mut runner = SpaghettiRecoveryRunner::default();

        let changed = super::refresh_live_context(&mut context, &mut runner).unwrap();
        let task = context
            .registry
            .get_task(&TaskId::new("task-1"))
            .expect("fixture task should remain registered");

        assert!(changed);
        assert!(runner.commands.iter().any(
            |command| matches!(command.args.as_slice(), [command, ..] if command == "capture-pane")
        ));
        assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting);
        assert!(!task.has_side_flag(SideFlag::Conflicted));
        assert!(task.has_side_flag(SideFlag::NeedsInput));
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::WaitingForInput)
        );
    }

    #[derive(Default)]
    struct SpaghettiRecoveryRunner {
        commands: Vec<CommandSpec>,
    }

    impl CommandRunner for SpaghettiRecoveryRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.commands.push(command.clone());
            let stdout = match command.args.as_slice() {
                [command, ..] if command == "list-sessions" => "ajax-web-fix-login\n",
                [command, ..] if command == "list-windows" => {
                    "ajax-web-fix-login\tworktrunk\t/tmp/worktrees/web-fix-login\n"
                }
                [command, ..] if command == "capture-pane" => {
                    "› Improve documentation in @filename\n\n  gpt-5.5 high · ~/Desktop/Projects/ajax-cli__worktrees/ajax-spaghetti\n"
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

    #[derive(Default)]
    struct SubstrateRecoveryRunner {
        commands: Vec<CommandSpec>,
    }

    impl CommandRunner for SubstrateRecoveryRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.commands.push(command.clone());
            let stdout = match command.args.as_slice() {
                [_, repo, subcommand, action, flag]
                    if repo == "/Users/matt/projects/web"
                        && subcommand == "worktree"
                        && action == "list"
                        && flag == "--porcelain" =>
                {
                    "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /Users/matt/projects/web__worktrees/ajax-code\nHEAD 2222222\nbranch refs/heads/ajax/code\n\n"
                }
                [command, ..] if command == "list-sessions" => {
                    "ajax-web-existing\najax-web-code\n"
                }
                [command, ..] if command == "list-windows" => {
                    "ajax-web-code\tworktrunk\t/Users/matt/projects/web__worktrees/ajax-code\n"
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
    fn refresh_recovers_missing_registry_task_from_existing_ajax_worktree_and_tmux() {
        let config = Config {
            repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
            ..Config::default()
        };
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
        let mut context = CommandContext::new(config, registry);
        let mut runner = SubstrateRecoveryRunner::default();
        let mut state_changed = false;

        let snapshot =
            refresh_cockpit_snapshot(&mut context, &mut runner, &mut state_changed, &mut None)
                .unwrap();

        assert!(state_changed);
        assert!(snapshot
            .cards
            .iter()
            .any(|card| card.qualified_handle == "web/code"));
        let task = context
            .registry
            .get_task(&TaskId::new("web/code"))
            .expect("missing Ajax worktree should be recovered into the registry");
        assert_eq!(task.branch, "ajax/code");
        assert_eq!(
            task.worktree_path.to_string_lossy(),
            "/Users/matt/projects/web__worktrees/ajax-code"
        );
        assert_eq!(task.tmux_session, "ajax-web-code");
        assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
    }

    #[derive(Default)]
    struct OrphanWorktreeRecoveryRunner {
        commands: Vec<CommandSpec>,
    }

    impl CommandRunner for OrphanWorktreeRecoveryRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.commands.push(command.clone());
            let stdout = match command.args.as_slice() {
                [_, repo, subcommand, action, flag]
                    if repo == "/Users/matt/projects/web"
                        && subcommand == "worktree"
                        && action == "list"
                        && flag == "--porcelain" =>
                {
                    "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /Users/matt/projects/web__worktrees/ajax-orphan\nHEAD 2222222\nbranch refs/heads/ajax/orphan\n\n"
                }
                [command, ..] if command == "list-sessions" => "ajax-web-existing\n",
                [command, ..] if command == "list-windows" => "",
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
    fn refresh_recovers_missing_registry_task_from_orphaned_ajax_worktree_without_tmux() {
        let config = Config {
            repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
            ..Config::default()
        };
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
        let mut context = CommandContext::new(config, registry);
        let mut runner = OrphanWorktreeRecoveryRunner::default();

        let changed = super::refresh_live_context(&mut context, &mut runner).unwrap();

        assert!(changed);
        let task = context
            .registry
            .get_task(&TaskId::new("web/orphan"))
            .expect("orphaned Ajax worktree should be recovered into the registry");
        assert_eq!(task.branch, "ajax/orphan");
        assert_eq!(
            task.worktree_path.to_string_lossy(),
            "/Users/matt/projects/web__worktrees/ajax-orphan"
        );
        assert!(task
            .git_status
            .as_ref()
            .is_some_and(|status| status.worktree_exists && status.branch_exists));
        assert_eq!(
            task.tmux_status.as_ref().map(|status| status.exists),
            Some(false)
        );
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::TmuxMissing)
        );
        assert!(task.has_side_flag(SideFlag::TmuxMissing));
    }

    #[derive(Default)]
    struct CountingLiveRefreshRunner {
        commands: Vec<CommandSpec>,
    }

    impl CommandRunner for CountingLiveRefreshRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.commands.push(command.clone());
            let stdout = match command.args.as_slice() {
                [command, ..] if command == "list-sessions" => "ajax-web-fix-login\n",
                [command, ..] if command == "list-windows" => {
                    "worktrunk\t/tmp/worktrees/web-fix-login\n"
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
    fn live_refresh_skips_window_and_pane_probes_for_non_live_tasks() {
        let mut context = context_with_active_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .expect("fixture task should exist");
        task.lifecycle_status = LifecycleStatus::Cleanable;
        task.tmux_status = Some(ajax_core::models::TmuxStatus::present(
            task.tmux_session.clone(),
        ));
        task.worktrunk_status = Some(ajax_core::models::WorktrunkStatus {
            exists: true,
            window_name: task.worktrunk_window.clone(),
            current_path: task.worktree_path.clone(),
            points_at_expected_path: true,
        });

        let mut runner = CountingLiveRefreshRunner::default();

        let changed = super::refresh_live_context(&mut context, &mut runner).unwrap();

        assert!(!changed);
        assert!(!runner.commands.iter().any(
            |command| matches!(command.args.as_slice(), [command, ..] if command == "list-sessions")
        ));
        assert!(!runner.commands.iter().any(
            |command| matches!(command.args.as_slice(), [command, ..] if command == "list-windows")
        ));
        assert!(!runner.commands.iter().any(
            |command| matches!(command.args.as_slice(), [command, ..] if command == "capture-pane")
        ));
    }

    #[test]
    fn live_refresh_does_not_probe_generic_error_task_without_live_attention() {
        let mut context = context_with_active_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .expect("fixture task should exist");
        task.lifecycle_status = LifecycleStatus::Error;
        task.agent_status = AgentRuntimeStatus::Blocked;
        task.tmux_status = Some(ajax_core::models::TmuxStatus::present(
            task.tmux_session.clone(),
        ));
        task.worktrunk_status = Some(ajax_core::models::WorktrunkStatus {
            exists: true,
            window_name: task.worktrunk_window.clone(),
            current_path: task.worktree_path.clone(),
            points_at_expected_path: true,
        });

        let mut runner = CountingLiveRefreshRunner::default();

        let changed = super::refresh_live_context(&mut context, &mut runner).unwrap();

        assert!(!changed);
        assert!(!runner.commands.iter().any(
            |command| matches!(command.args.as_slice(), [command, ..] if command == "list-sessions")
        ));
        assert!(!runner.commands.iter().any(
            |command| matches!(command.args.as_slice(), [command, ..] if command == "list-windows")
        ));
        assert!(!runner.commands.iter().any(
            |command| matches!(command.args.as_slice(), [command, ..] if command == "capture-pane")
        ));
    }
}

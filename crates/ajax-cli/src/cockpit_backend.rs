use ajax_core::{
    adapters::{CommandRunError, CommandRunner},
    commands::{self, CommandContext, CommandError},
    models::OperatorAction,
    registry::InMemoryRegistry,
    runtime_refresh::{refresh_runtime_context_with_tier, RefreshTier},
};
use ajax_tui::CockpitSnapshot;
use clap::ArgMatches;
use std::{
    io::{ErrorKind, Write},
    net::TcpListener,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime},
};

use crate::{
    agent_status_cache::TmuxAgentStatusSnapshot,
    cockpit_actions::{
        execute_pending_cockpit_action_with_task_session,
        execute_pending_cockpit_action_with_task_session_and_checkpoint,
        handle_pending_cockpit_result, tui_cockpit_action, tui_cockpit_confirmed_action,
    },
    context::{load_context, save_context_with_state, state_file_mtime},
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
    mut save_state: Option<&mut crate::context::ContextSaveState>,
) -> Result<RenderedCommand, CliError> {
    let _mobile_web_companion = if subcommand.get_flag("no-web") {
        None
    } else {
        start_mobile_web_companion(mobile_web_port, paths)?
    };
    let mut state_changed = false;
    let mut cockpit_flash = None;
    let mut open_new_task_repo = None;
    let mut last_loaded_mtime = paths.and_then(state_file_mtime);
    state_changed |= refresh_live_context(context, runner)?;
    let refresh_interval = Duration::from_millis(parse_u64_arg(subcommand, "interval-ms", 1000)?);
    loop {
        let mut task_session = PtyTaskSessionRunner;
        let mut cached_snapshot = None;
        let snapshot = refresh_cockpit_snapshot_with_paths(
            context,
            runner,
            &mut state_changed,
            &mut cached_snapshot,
            paths,
            &mut last_loaded_mtime,
            save_state.as_deref_mut(),
        )?;
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
                paths,
                last_loaded_mtime: &mut last_loaded_mtime,
                save_state: save_state.as_deref_mut(),
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

        let mut checkpoint_saved = false;
        let pending_result =
            if let (Some(paths), Some(save_state)) = (paths, save_state.as_deref_mut()) {
                execute_pending_cockpit_action_with_task_session_and_checkpoint(
                    &pending,
                    context,
                    runner,
                    &mut state_changed,
                    &mut task_session,
                    |checkpoint_context| {
                        save_context_with_state(paths, checkpoint_context, save_state).map_err(
                            |error| {
                                CommandError::CommandRun(CommandRunError::SpawnFailed(format!(
                                    "persist cockpit checkpoint: {error}"
                                )))
                            },
                        )?;
                        checkpoint_saved = true;
                        Ok(())
                    },
                )
            } else {
                execute_pending_cockpit_action_with_task_session(
                    &pending,
                    context,
                    runner,
                    &mut state_changed,
                    &mut task_session,
                )
            };

        match pending_result? {
            crate::cockpit_actions::PendingCockpitExecution::OpenNewTask { repo } => {
                open_new_task_repo = Some(repo);
            }
            crate::cockpit_actions::PendingCockpitExecution::Continue(message) => {
                if !handle_pending_cockpit_result(Ok(message), &mut cockpit_flash) {
                    continue;
                }
            }
        }
        if checkpoint_saved {
            if let Some(paths) = paths {
                last_loaded_mtime = state_file_mtime(paths);
            }
        }

        if state_changed {
            if let (Some(paths), Some(save_state)) = (paths, save_state.as_deref_mut()) {
                if pending.action == OperatorAction::Drop.as_str() {
                    save_state.allow_empty_registry_once();
                }
                save_cockpit_state_to_sqlite(paths, context, save_state, &mut last_loaded_mtime)?;
                state_changed = false;
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
    let cache = TmuxAgentStatusSnapshot::from_runtime_cache(&context.runtime_paths.cache_dir);
    let refreshed = refresh_runtime_context_with_tier(context, runner, &cache, RefreshTier::Full)
        .map_err(crate::command_error)?;
    let notified = crate::notify::notify_attention_transitions(context, runner);
    Ok(refreshed || notified)
}

fn cached_snapshot_needs_rebuild(
    context: &CommandContext<InMemoryRegistry>,
    cached_snapshot: &CockpitSnapshot,
) -> bool {
    use std::collections::BTreeSet;

    let view = commands::cockpit_view(context);
    let visible_handles: BTreeSet<_> = view
        .cards
        .iter()
        .map(|card| card.qualified_handle.as_str())
        .collect();
    let cached_handles: BTreeSet<_> = cached_snapshot
        .cards
        .iter()
        .map(|card| card.qualified_handle.as_str())
        .collect();
    visible_handles != cached_handles
}

pub(crate) fn refresh_cockpit_snapshot<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    state_changed: &mut bool,
    cached_snapshot: &mut Option<CockpitSnapshot>,
) -> Result<CockpitSnapshot, CliError> {
    let changed = refresh_live_context(context, runner)?;
    *state_changed |= changed;
    let cache_stale = cached_snapshot
        .as_ref()
        .is_some_and(|snapshot| cached_snapshot_needs_rebuild(context, snapshot));
    if changed || cached_snapshot.is_none() || cache_stale {
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

pub(crate) fn refresh_cockpit_snapshot_with_paths<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    state_changed: &mut bool,
    cached_snapshot: &mut Option<CockpitSnapshot>,
    paths: Option<&CliContextPaths>,
    last_loaded_mtime: &mut Option<SystemTime>,
    save_state: Option<&mut crate::context::ContextSaveState>,
) -> Result<CockpitSnapshot, CliError> {
    if let Some(paths) = paths {
        reload_cockpit_context_if_stale(context, paths, last_loaded_mtime, save_state)?;
    }
    refresh_cockpit_snapshot(context, runner, state_changed, cached_snapshot)
}

fn reload_cockpit_context_if_stale(
    context: &mut CommandContext<InMemoryRegistry>,
    paths: &CliContextPaths,
    last_loaded_mtime: &mut Option<SystemTime>,
    save_state: Option<&mut crate::context::ContextSaveState>,
) -> Result<bool, CliError> {
    if let Some(save_state) = save_state {
        let revision = ajax_core::registry::SqliteRegistryStore::new(&paths.state_file)
            .current_revision()
            .map_err(|error| CliError::ContextLoad(format!("state revision failed: {error}")))?;
        if revision == save_state.loaded_revision {
            *last_loaded_mtime = state_file_mtime(paths);
            return Ok(false);
        }
        let fresh = load_context(paths)?;
        *save_state = crate::context::tracked_save_state(paths, &fresh.registry)?;
        context.registry = fresh.registry;
        *last_loaded_mtime = state_file_mtime(paths);
        return Ok(true);
    }
    let Some(mtime) = state_file_mtime(paths) else {
        return Ok(false);
    };
    if *last_loaded_mtime == Some(mtime) {
        return Ok(false);
    }
    let fresh = load_context(paths)?;
    context.registry = fresh.registry;
    *last_loaded_mtime = Some(mtime);
    Ok(true)
}

pub(crate) fn save_cockpit_state_to_sqlite(
    paths: &CliContextPaths,
    context: &CommandContext<InMemoryRegistry>,
    save_state: &mut crate::context::ContextSaveState,
    last_loaded_mtime: &mut Option<SystemTime>,
) -> Result<(), CliError> {
    crate::context::save_context_with_state(paths, context, save_state)?;
    *last_loaded_mtime = state_file_mtime(paths);
    Ok(())
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
    paths: Option<&'a CliContextPaths>,
    last_loaded_mtime: &'a mut Option<SystemTime>,
    save_state: Option<&'a mut crate::context::ContextSaveState>,
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
        refresh_cockpit_snapshot_with_paths(
            self.context,
            self.runner,
            self.state_changed,
            self.cached_snapshot,
            self.paths,
            self.last_loaded_mtime,
            self.save_state.as_deref_mut(),
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
            Task, TaskId, TaskWindowStatus, TmuxStatus,
        },
        output::TaskCard,
        registry::{InMemoryRegistry, Registry},
        runtime_refresh::{refresh_runtime_context_with_tier, AgentStatusCache, RefreshTier},
    };
    use ajax_tui::CockpitSnapshot;

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
                    "ajax-web-fix-login\ttask\t/tmp/worktrees/web-fix-login\n"
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
            "task",
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
    fn mobile_web_ports_are_separate_for_stable_and_dev() {
        assert_eq!(mobile_web_port_for_command("stable"), 8787);
        assert_eq!(mobile_web_port_for_command("cockpit"), 8787);
        assert_eq!(mobile_web_port_for_command("dev"), 8788);
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
        task.task_window_status = Some(TaskWindowStatus::present(
            "task",
            "/tmp/worktrees/web-fix-login",
        ));
        context
    }

    struct StaticAgentStatusCache {
        values: Vec<String>,
    }

    impl AgentStatusCache for StaticAgentStatusCache {
        fn status_entries_for_session(
            &self,
            _session: &str,
        ) -> Vec<ajax_core::runtime_refresh::AgentStatusCacheEntry> {
            self.values
                .iter()
                .cloned()
                .map(|value| ajax_core::runtime_refresh::AgentStatusCacheEntry {
                    value,
                    observed_at: std::time::SystemTime::now(),
                    fresh: true,
                    source: ajax_core::runtime_refresh::AgentStatusCacheSource::Hook,
                })
                .collect()
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
            snapshot.cards[0].status_explanation.as_deref(),
            Some("Waiting for approval")
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
            refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Full)
                .unwrap();
        let snapshot = build_cockpit_snapshot(&context);

        assert!(state_changed);
        let card = snapshot
            .cards
            .iter()
            .find(|card| card.qualified_handle == "web/fix-login")
            .expect("task should stay visible in cockpit");
        assert_eq!(card.status, ajax_core::ui_state::TaskStatus::Running);
        assert_eq!(card.status_explanation.as_deref(), Some("Agent working"));
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
        assert_eq!(card.status, ajax_core::ui_state::TaskStatus::Waiting);
        assert_eq!(
            card.status_explanation.as_deref(),
            Some("Waiting for input")
        );
        assert!(
            card.annotations
                .iter()
                .any(|annotation| { annotation.evidence.label() == "waiting for input" }),
            "{:?}",
            card.annotations
        );
        assert!(snapshot
            .inbox
            .items
            .iter()
            .any(|item| item.task_handle == "web/fix-login"));

        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting);
        assert!(task.has_side_flag(SideFlag::NeedsInput));
        assert!(
            task.metadata
                .contains_key(ajax_core::live::RUNNING_CANDIDATE_SINCE_KEY),
            "running_candidate_since should be recorded while dwell is pending"
        );

        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        let backdated = now_secs.saturating_sub(10);
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .metadata
            .insert(
                ajax_core::live::RUNNING_CANDIDATE_SINCE_KEY.to_string(),
                backdated.to_string(),
            );

        let snapshot =
            refresh_cockpit_snapshot(&mut context, &mut runner, &mut state_changed, &mut None)
                .unwrap();

        let card = snapshot
            .cards
            .iter()
            .find(|card| card.qualified_handle == "web/fix-login")
            .expect("task should stay visible in cockpit");
        assert_eq!(card.status, ajax_core::ui_state::TaskStatus::Running);
        assert_eq!(card.status_explanation.as_deref(), Some("Agent working"));
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
        assert!(!task
            .metadata
            .contains_key(ajax_core::live::RUNNING_CANDIDATE_SINCE_KEY));
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
                    "ajax-web-fix-login\ttask\t/tmp/worktrees/web-fix-login\n"
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
        assert_eq!(task.agent_status, AgentRuntimeStatus::Dead);
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
        task.task_window_status = Some(TaskWindowStatus::present(
            "task",
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
                    "ajax-web-fix-login\ttask\t/tmp/worktrees/web-fix-login\n"
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
                    "ajax-web-code\ttask\t/Users/matt/projects/web__worktrees/ajax-code\n"
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
            "task",
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
            "task",
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
                    "task\t/tmp/worktrees/web-fix-login\n"
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
        task.task_window_status = Some(ajax_core::models::TaskWindowStatus {
            exists: true,
            window_name: task.task_window.clone(),
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
    fn cockpit_snapshot_rebuilds_after_cached_task_is_removed() {
        let mut context = context_with_active_task();
        let initial_snapshot = build_cockpit_snapshot(&context);
        assert_eq!(initial_snapshot.cards.len(), 1);
        assert_eq!(initial_snapshot.cards[0].qualified_handle, "web/fix-login");

        let mut cached_snapshot = Some(initial_snapshot);
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .expect("fixture task should exist");
        task.lifecycle_status = LifecycleStatus::Removed;

        let mut runner = EmptyTmuxRunner;
        let mut state_changed = false;
        let snapshot = refresh_cockpit_snapshot(
            &mut context,
            &mut runner,
            &mut state_changed,
            &mut cached_snapshot,
        )
        .unwrap();

        assert!(snapshot.cards.is_empty());
        assert!(cached_snapshot.as_ref().unwrap().cards.is_empty());
        assert!(snapshot
            .repos
            .repos
            .iter()
            .all(|repo| repo.active_tasks == 0));
        assert!(snapshot.inbox.items.is_empty());
    }

    #[test]
    fn cockpit_snapshot_reuses_cache_when_visible_tasks_are_unchanged() {
        let mut context = context_with_active_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .expect("fixture task should exist");
        task.lifecycle_status = LifecycleStatus::Cleanable;
        task.tmux_status = Some(TmuxStatus::present(task.tmux_session.clone()));
        task.task_window_status = Some(TaskWindowStatus {
            exists: true,
            window_name: task.task_window.clone(),
            current_path: task.worktree_path.clone(),
            points_at_expected_path: true,
        });
        let fresh_snapshot = build_cockpit_snapshot(&context);
        let mut cached_snapshot = Some(CockpitSnapshot {
            repos: fresh_snapshot.repos,
            cards: vec![TaskCard {
                status_explanation: Some("cached-only summary".to_string()),
                ..fresh_snapshot.cards[0].clone()
            }],
            inbox: fresh_snapshot.inbox,
        });
        let mut runner = EmptyTmuxRunner;
        let mut state_changed = false;

        let snapshot = refresh_cockpit_snapshot(
            &mut context,
            &mut runner,
            &mut state_changed,
            &mut cached_snapshot,
        )
        .unwrap();

        assert!(!state_changed);
        assert_eq!(
            snapshot.cards[0].status_explanation.as_deref(),
            Some("cached-only summary")
        );
        assert_eq!(
            cached_snapshot.as_ref().unwrap().cards[0]
                .status_explanation
                .as_deref(),
            Some("cached-only summary")
        );
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
        task.task_window_status = Some(ajax_core::models::TaskWindowStatus {
            exists: true,
            window_name: task.task_window.clone(),
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

#[cfg(test)]
mod cockpit_persistence_tests {
    use super::{
        refresh_cockpit_snapshot_with_paths, save_cockpit_state_to_sqlite,
        InteractiveCockpitHandler,
    };
    use crate::context::{load_context, load_tracked_context, state_file_mtime, ContextSaveState};
    use crate::CliContextPaths;
    use ajax_core::{
        adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec},
        commands::CommandContext,
        config::Config,
        models::{AgentClient, LifecycleStatus, Task, TaskId},
        registry::{InMemoryRegistry, Registry as _, SqliteRegistryStore},
    };
    use ajax_tui::CockpitEventHandler;
    use std::{thread, time::Duration};

    struct EmptyTmuxRunner;

    impl CommandRunner for EmptyTmuxRunner {
        fn run(&mut self, _command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            Ok(CommandOutput {
                status_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

    fn sample_active_task(handle: &str) -> Task {
        let mut task = Task::new(
            TaskId::new(format!("web/{handle}")),
            "web",
            handle,
            handle,
            format!("ajax/{handle}"),
            "main",
            format!("/tmp/worktrees/web-{handle}"),
            format!("ajax-web-{handle}"),
            "task",
            AgentClient::Codex,
        );
        task.lifecycle_status = LifecycleStatus::Active;
        task
    }

    fn temp_state_paths(label: &str) -> CliContextPaths {
        let root = std::env::temp_dir().join(format!(
            "ajax-cockpit-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        CliContextPaths::new(root.join("config.toml"), root.join("state.db"))
    }

    #[test]
    fn refresh_cockpit_snapshot_with_paths_reloads_sqlite_when_mtime_advances() {
        let paths = temp_state_paths("reload-on-mtime");
        let mut initial = InMemoryRegistry::default();
        initial.create_task(sample_active_task("a")).unwrap();
        SqliteRegistryStore::new(&paths.state_file)
            .save(&initial)
            .unwrap();

        let mut context = load_context(&paths).unwrap();
        let mut last_loaded_mtime = state_file_mtime(&paths);
        let mut cached_snapshot = None;
        let mut state_changed = false;
        let mut runner = EmptyTmuxRunner;

        let first = refresh_cockpit_snapshot_with_paths(
            &mut context,
            &mut runner,
            &mut state_changed,
            &mut cached_snapshot,
            Some(&paths),
            &mut last_loaded_mtime,
            None,
        )
        .unwrap();
        assert_eq!(first.cards.len(), 1);
        assert!(first
            .cards
            .iter()
            .any(|card| card.qualified_handle == "web/a"));

        thread::sleep(Duration::from_millis(50));
        let mut next = InMemoryRegistry::default();
        next.create_task(sample_active_task("a")).unwrap();
        next.create_task(sample_active_task("b")).unwrap();
        SqliteRegistryStore::new(&paths.state_file)
            .save(&next)
            .unwrap();

        let mtime_before_reload = last_loaded_mtime;
        let mut runner = EmptyTmuxRunner;
        let second = refresh_cockpit_snapshot_with_paths(
            &mut context,
            &mut runner,
            &mut state_changed,
            &mut cached_snapshot,
            Some(&paths),
            &mut last_loaded_mtime,
            None,
        )
        .unwrap();

        let handles: Vec<&str> = second
            .cards
            .iter()
            .map(|card| card.qualified_handle.as_str())
            .collect();
        assert!(
            handles.contains(&"web/a") && handles.contains(&"web/b"),
            "expected both web/a and web/b after sqlite advance, got {handles:?}"
        );
        assert_eq!(second.cards.len(), 2);
        assert_ne!(
            mtime_before_reload, last_loaded_mtime,
            "last_loaded_mtime should advance after SQLite revision changes"
        );

        let _ = std::fs::remove_dir_all(paths.state_file.parent().unwrap());
    }

    #[test]
    fn cockpit_save_uses_reloaded_sqlite_state_as_its_concurrency_baseline() {
        let paths = temp_state_paths("reload-save-baseline");
        let mut initial = InMemoryRegistry::default();
        initial.create_task(sample_active_task("a")).unwrap();
        SqliteRegistryStore::new(&paths.state_file)
            .save(&initial)
            .unwrap();

        let mut tracked = load_tracked_context(&paths).unwrap();
        let mut last_loaded_mtime = state_file_mtime(&paths);

        thread::sleep(Duration::from_millis(50));
        let mut concurrent = initial.clone();
        concurrent
            .get_task_mut(&TaskId::new("web/a"))
            .expect("concurrent task")
            .metadata
            .insert("web".to_string(), "persisted".to_string());
        SqliteRegistryStore::new(&paths.state_file)
            .save(&concurrent)
            .unwrap();

        let mut cached_snapshot = None;
        let mut state_changed = false;
        let mut runner = EmptyTmuxRunner;
        refresh_cockpit_snapshot_with_paths(
            &mut tracked.context,
            &mut runner,
            &mut state_changed,
            &mut cached_snapshot,
            Some(&paths),
            &mut last_loaded_mtime,
            Some(&mut tracked.save_state),
        )
        .expect("reload concurrent SQLite state");

        tracked
            .context
            .registry
            .get_task_mut(&TaskId::new("web/a"))
            .expect("reloaded task")
            .metadata
            .insert("native".to_string(), "persisted".to_string());

        save_cockpit_state_to_sqlite(
            &paths,
            &tracked.context,
            &mut tracked.save_state,
            &mut last_loaded_mtime,
        )
        .expect("save after Cockpit reload");

        let reloaded = load_context(&paths).expect("reload saved state");
        let task = reloaded
            .registry
            .get_task(&TaskId::new("web/a"))
            .expect("saved task");
        assert_eq!(
            task.metadata.get("web").map(String::as_str),
            Some("persisted")
        );
        assert_eq!(
            task.metadata.get("native").map(String::as_str),
            Some("persisted")
        );

        let _ = std::fs::remove_dir_all(paths.state_file.parent().unwrap());
    }

    #[test]
    fn cockpit_save_reloads_sqlite_even_when_mtime_stays_the_same() {
        let paths = temp_state_paths("reload-save-mtime-stall");
        let mut initial = InMemoryRegistry::default();
        initial.create_task(sample_active_task("a")).unwrap();
        SqliteRegistryStore::new(&paths.state_file)
            .save(&initial)
            .unwrap();

        let mut tracked = load_tracked_context(&paths).unwrap();
        let mut last_loaded_mtime;

        let mut concurrent = initial.clone();
        concurrent
            .get_task_mut(&TaskId::new("web/a"))
            .expect("concurrent task")
            .metadata
            .insert("web".to_string(), "persisted".to_string());
        SqliteRegistryStore::new(&paths.state_file)
            .save(&concurrent)
            .unwrap();

        // Simulate a filesystem where the timestamp cache did not advance
        // even though SQLite revision did. The reload path should still notice
        // the revision change and refresh the save baseline.
        last_loaded_mtime = state_file_mtime(&paths);

        let mut cached_snapshot = None;
        let mut state_changed = false;
        let mut runner = EmptyTmuxRunner;
        refresh_cockpit_snapshot_with_paths(
            &mut tracked.context,
            &mut runner,
            &mut state_changed,
            &mut cached_snapshot,
            Some(&paths),
            &mut last_loaded_mtime,
            Some(&mut tracked.save_state),
        )
        .expect("reload concurrent SQLite state even when mtime is unchanged");

        tracked
            .context
            .registry
            .get_task_mut(&TaskId::new("web/a"))
            .expect("reloaded task")
            .metadata
            .insert("native".to_string(), "persisted".to_string());

        save_cockpit_state_to_sqlite(
            &paths,
            &tracked.context,
            &mut tracked.save_state,
            &mut last_loaded_mtime,
        )
        .expect("save after Cockpit reload with stale mtime");

        let reloaded = load_context(&paths).expect("reload saved state");
        let task = reloaded
            .registry
            .get_task(&TaskId::new("web/a"))
            .expect("saved task");
        assert_eq!(
            task.metadata.get("web").map(String::as_str),
            Some("persisted")
        );
        assert_eq!(
            task.metadata.get("native").map(String::as_str),
            Some("persisted")
        );

        let _ = std::fs::remove_dir_all(paths.state_file.parent().unwrap());
    }

    #[test]
    fn interactive_cockpit_handler_on_refresh_reloads_sqlite_via_paths() {
        let paths = temp_state_paths("handler-on-refresh");
        let mut initial = InMemoryRegistry::default();
        initial.create_task(sample_active_task("a")).unwrap();
        SqliteRegistryStore::new(&paths.state_file)
            .save(&initial)
            .unwrap();

        let mut context = load_context(&paths).unwrap();
        let mut last_loaded_mtime = state_file_mtime(&paths);
        let mut cached_snapshot = None;
        let mut state_changed = false;
        let mut runner = EmptyTmuxRunner;

        let first = {
            let mut handler = InteractiveCockpitHandler {
                context: &mut context,
                runner: &mut runner,
                state_changed: &mut state_changed,
                cached_snapshot: &mut cached_snapshot,
                paths: Some(&paths),
                last_loaded_mtime: &mut last_loaded_mtime,
                save_state: None,
            };
            handler.on_refresh().unwrap().expect("first snapshot")
        };
        assert_eq!(first.cards.len(), 1);

        thread::sleep(Duration::from_millis(50));
        let mut next = InMemoryRegistry::default();
        next.create_task(sample_active_task("a")).unwrap();
        next.create_task(sample_active_task("b")).unwrap();
        SqliteRegistryStore::new(&paths.state_file)
            .save(&next)
            .unwrap();

        let mut runner = EmptyTmuxRunner;
        let second = {
            let mut handler = InteractiveCockpitHandler {
                context: &mut context,
                runner: &mut runner,
                state_changed: &mut state_changed,
                cached_snapshot: &mut cached_snapshot,
                paths: Some(&paths),
                last_loaded_mtime: &mut last_loaded_mtime,
                save_state: None,
            };
            handler.on_refresh().unwrap().expect("second snapshot")
        };

        let handles: Vec<&str> = second
            .cards
            .iter()
            .map(|card| card.qualified_handle.as_str())
            .collect();
        assert!(
            handles.contains(&"web/a") && handles.contains(&"web/b"),
            "expected handler.on_refresh to pick up SQLite advance, got {handles:?}"
        );

        let _ = std::fs::remove_dir_all(paths.state_file.parent().unwrap());
    }

    #[test]
    fn save_cockpit_state_to_sqlite_persists_in_memory_mutations() {
        let paths = temp_state_paths("save-during-loop");
        let mut initial = InMemoryRegistry::default();
        initial.create_task(sample_active_task("a")).unwrap();
        SqliteRegistryStore::new(&paths.state_file)
            .save(&initial)
            .unwrap();

        let mut tracked = load_tracked_context(&paths).unwrap();
        let mut last_loaded_mtime = state_file_mtime(&paths);

        tracked
            .context
            .registry
            .get_task_mut(&TaskId::new("web/a"))
            .expect("seeded task")
            .title = "Renamed by native cockpit".to_string();

        save_cockpit_state_to_sqlite(
            &paths,
            &tracked.context,
            &mut tracked.save_state,
            &mut last_loaded_mtime,
        )
        .expect("save during interactive cockpit loop");

        let on_disk = SqliteRegistryStore::new(&paths.state_file)
            .load_tasks_only()
            .expect("reload SQLite after cockpit save");
        let task = on_disk
            .get_task(&TaskId::new("web/a"))
            .expect("persisted task")
            .clone();
        assert_eq!(
            task.title, "Renamed by native cockpit",
            "cockpit save should persist in-memory task mutations during the interactive loop"
        );
        assert!(last_loaded_mtime.is_some(), "mtime should be tracked");

        let _ = std::fs::remove_dir_all(paths.state_file.parent().unwrap());
    }

    #[test]
    fn save_cockpit_state_to_sqlite_rejects_empty_save_over_non_empty_disk() {
        let paths = temp_state_paths("empty-save-guard");
        let mut initial = InMemoryRegistry::default();
        initial.create_task(sample_active_task("a")).unwrap();
        let store = SqliteRegistryStore::new(&paths.state_file);
        store.save(&initial).unwrap();
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let mut save_state = ContextSaveState {
            loaded_registry: InMemoryRegistry::default(),
            loaded_revision: store.current_revision().unwrap(),
            allow_empty_registry_once: false,
        };
        let mut last_loaded_mtime = state_file_mtime(&paths);

        let error =
            save_cockpit_state_to_sqlite(&paths, &context, &mut save_state, &mut last_loaded_mtime)
                .unwrap_err();

        assert!(error
            .to_string()
            .contains("refusing to save empty registry"));
        let on_disk = SqliteRegistryStore::new(&paths.state_file)
            .load_tasks_only()
            .expect("reload SQLite after rejected cockpit save");
        assert!(on_disk.get_task(&TaskId::new("web/a")).is_some());

        let _ = std::fs::remove_dir_all(paths.state_file.parent().unwrap());
    }
}

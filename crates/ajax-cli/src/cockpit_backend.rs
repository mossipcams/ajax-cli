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
    agent_status_cache::AgentStatusFiles,
    cockpit_actions::{
        cockpit_action_outcome, execute_pending_cockpit_action_with_task_session,
        execute_pending_cockpit_action_with_task_session_and_checkpoint,
        handle_pending_cockpit_result,
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
    let mut retained_repair_plan = None;
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
                retained_repair_plan: &mut retained_repair_plan,
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
                    retained_repair_plan.as_ref(),
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
                    retained_repair_plan.as_ref(),
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
                match save_cockpit_state_to_sqlite(
                    paths,
                    context,
                    save_state,
                    &mut last_loaded_mtime,
                ) {
                    Ok(()) => {}
                    Err(error) => match recover_cockpit_save_error(
                        paths,
                        context,
                        save_state,
                        &mut last_loaded_mtime,
                        error,
                    ) {
                        Ok(flash) => {
                            cockpit_flash = flash;
                        }
                        Err(error) => return Err(error),
                    },
                }
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
mod mobile_web_companion_tests;

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
    let source = AgentStatusFiles::shared_from_runtime_cache(&context.runtime_paths.cache_dir);
    let refreshed =
        refresh_runtime_context_with_tier(context, runner, source.as_ref(), RefreshTier::Full)
            .map_err(crate::command_error)?;
    Ok(refreshed)
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

// Recover from a rejected post-session Cockpit save by reloading disk state
// into the in-memory context and resetting the tracked save baseline, instead
// of propagating the error out of the Cockpit loop. Only `CliError::ContextSave`
// is recoverable; every other error is returned unchanged.
fn recover_cockpit_save_error(
    paths: &CliContextPaths,
    context: &mut CommandContext<InMemoryRegistry>,
    save_state: &mut crate::context::ContextSaveState,
    last_loaded_mtime: &mut Option<SystemTime>,
    error: CliError,
) -> Result<Option<String>, CliError> {
    if !matches!(error, CliError::ContextSave(_)) {
        return Err(error);
    }
    let fresh = load_context(paths)?;
    *save_state = crate::context::tracked_save_state(paths, &fresh.registry)?;
    context.registry = fresh.registry;
    *last_loaded_mtime = state_file_mtime(paths);
    Ok(Some(error.to_string()))
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
    retained_repair_plan: &'a mut Option<commands::CommandPlan>,
}

impl<R: CommandRunner> ajax_tui::CockpitEventHandler for InteractiveCockpitHandler<'_, R> {
    fn on_action(
        &mut self,
        item: &ajax_core::models::CockpitActionItem,
    ) -> std::io::Result<ajax_tui::ActionOutcome> {
        cockpit_action_outcome(item, self.context, false, self.retained_repair_plan)
    }

    fn on_confirmed_action(
        &mut self,
        item: &ajax_core::models::CockpitActionItem,
    ) -> std::io::Result<ajax_tui::ActionOutcome> {
        cockpit_action_outcome(item, self.context, true, self.retained_repair_plan)
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
mod tests;

#[cfg(test)]
mod cockpit_persistence_tests;

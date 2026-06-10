mod agent_runtime;
#[cfg(feature = "interactive")]
mod agent_status_cache;
#[cfg(feature = "interactive")]
mod bgtmux;
mod cli;
#[cfg(feature = "interactive")]
mod cockpit_actions;
#[cfg(feature = "interactive")]
mod cockpit_backend;
mod context;
mod dispatch;
mod execution_dispatch;
mod render;
mod snapshot_dispatch;
#[cfg(feature = "supervisor")]
mod supervise;
#[cfg(feature = "interactive")]
mod task_session;
#[path = "web_backend.rs"]
mod web_companion_backend;

#[cfg(test)]
pub(crate) use ajax_core::task_operations::task_command::TaskCommandKind;
#[cfg(test)]
#[cfg(feature = "interactive")]
pub(crate) use cockpit_actions::{
    execute_pending_cockpit_action, execute_pending_cockpit_action_with_task_session,
    handle_pending_cockpit_result, tui_cockpit_action, tui_cockpit_confirmed_action,
};
#[cfg(test)]
#[cfg(feature = "interactive")]
pub(crate) use cockpit_backend::{refresh_cockpit_snapshot, render_cockpit_command};
#[cfg(test)]
pub(crate) use dispatch::{render_drop_command, render_task_command};
#[cfg(test)]
pub(crate) use snapshot_dispatch::parent_directory_available;

use ajax_core::{
    adapters::{CommandRunner, ProcessCommandRunner},
    commands::{self, CommandContext, CommandError},
    registry::InMemoryRegistry,
};
use clap::ArgMatches;
pub use cli::build_cli;
use cli::{parse_args, ParsedArgs};
#[cfg(feature = "interactive")]
use cockpit_backend::stream_live_cockpit_command;
pub use context::CliContextPaths;
use context::{
    context_paths_from_matches, load_context, load_context_with_events, load_tracked_context,
    save_tracked_context, TrackedContext,
};
use execution_dispatch::{render_matches_mut, render_matches_mut_with_paths};
use snapshot_dispatch::render_matches_with_paths;
use std::{ffi::OsStr, io::Write};

#[cfg(feature = "interactive")]
pub fn run_bgtmux_to_writer(
    args: impl IntoIterator<Item = impl Into<std::ffi::OsString> + Clone>,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    let mut runner = task_session::PtyTaskSessionRunner;
    bgtmux::run_bgtmux_with_runner(args, writer, &mut runner)
}

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

    if let Some(("__agent-runtime", subcommand)) = matches.subcommand() {
        return agent_runtime::run_agent_runtime_command(subcommand);
    }

    let paths = context_paths_from_matches(&matches)?;
    if let Some(("runtime", subcommand)) = matches.subcommand() {
        return snapshot_dispatch::render_runtime_paths(
            &paths.runtime_paths,
            subcommand.get_flag("json"),
        );
    }

    let mut tracked = load_tracked_context_for_matches(&paths, &matches)?;
    let mut runner = ProcessCommandRunner;
    let rendered = match render_matches_mut_with_paths(
        &matches,
        &mut tracked.context,
        &mut runner,
        Some(&paths),
        Some(&mut tracked.save_state),
    ) {
        Ok(rendered) => rendered,
        Err(error) => {
            if error.state_changed() {
                save_tracked_context(&paths, &mut tracked)?;
            }
            return Err(error);
        }
    };
    if rendered.state_changed {
        save_tracked_context(&paths, &mut tracked)?;
    }

    Ok(rendered.output)
}

pub fn run_with_args_to_writer(
    args: impl IntoIterator<Item = impl Into<std::ffi::OsString> + Clone>,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    let matches = match parse_args(args)? {
        ParsedArgs::Matches(matches) => matches,
        ParsedArgs::Message(message) => return write_command_output(writer, &message),
    };

    if let Some(("__agent-runtime", subcommand)) = matches.subcommand() {
        let output = agent_runtime::run_agent_runtime_command(subcommand)?;
        return write_command_output(writer, &output);
    }

    let paths = context_paths_from_matches(&matches)?;
    let mut tracked = load_tracked_context_for_matches(&paths, &matches)?;
    let mut runner = ProcessCommandRunner;

    let mut persist_state = tracked.save_state.clone();
    if let Some(result) = stream_command_to_writer(
        &matches,
        &mut tracked.context,
        &mut runner,
        writer,
        |context| context::save_context_with_state(&paths, context, &mut persist_state),
    ) {
        tracked.save_state = persist_state;
        result?;
        return Ok(());
    }

    let rendered = match render_matches_mut_with_paths(
        &matches,
        &mut tracked.context,
        &mut runner,
        Some(&paths),
        Some(&mut tracked.save_state),
    ) {
        Ok(rendered) => rendered,
        Err(error) => {
            if error.state_changed() {
                save_tracked_context(&paths, &mut tracked)?;
            }
            return Err(error);
        }
    };
    if rendered.state_changed {
        save_tracked_context(&paths, &mut tracked)?;
    }
    write_command_output(writer, &rendered.output)
}

pub fn run_with_context(
    args: impl IntoIterator<Item = impl Into<std::ffi::OsString> + Clone>,
    context: &CommandContext<InMemoryRegistry>,
) -> Result<String, CliError> {
    let matches = match parse_args(args)? {
        ParsedArgs::Matches(matches) => matches,
        ParsedArgs::Message(message) => return Ok(message),
    };

    snapshot_dispatch::render_snapshot_matches(&matches, context)
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

pub fn run_with_context_and_runner_to_writer(
    args: impl IntoIterator<Item = impl Into<std::ffi::OsString> + Clone>,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut impl CommandRunner,
    writer: &mut impl Write,
) -> Result<bool, CliError> {
    let matches = match parse_args(args)? {
        ParsedArgs::Matches(matches) => matches,
        ParsedArgs::Message(message) => {
            write_command_output(writer, &message)?;
            return Ok(false);
        }
    };

    if let Some(result) =
        stream_command_to_writer(&matches, context, runner, writer, |_context| Ok(()))
    {
        return result;
    }

    let rendered = render_matches_mut(&matches, context, runner)?;
    write_command_output(writer, &rendered.output)?;
    Ok(rendered.state_changed)
}

pub fn run_with_context_paths(
    args: impl IntoIterator<Item = impl Into<std::ffi::OsString> + Clone>,
    paths: &CliContextPaths,
) -> Result<String, CliError> {
    let matches = match parse_args(args)? {
        ParsedArgs::Matches(matches) => matches,
        ParsedArgs::Message(message) => return Ok(message),
    };
    let context = load_context_for_matches(paths, &matches)?;

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
    let mut tracked = load_tracked_context_for_matches(paths, &matches)?;
    let rendered = match render_matches_mut_with_paths(
        &matches,
        &mut tracked.context,
        runner,
        Some(paths),
        Some(&mut tracked.save_state),
    ) {
        Ok(rendered) => rendered,
        Err(error) => {
            if error.state_changed() {
                save_tracked_context(paths, &mut tracked)?;
            }
            return Err(error);
        }
    };
    if rendered.state_changed {
        save_tracked_context(paths, &mut tracked)?;
    }

    Ok(rendered.output)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RenderedCommand {
    pub(crate) output: String,
    pub(crate) state_changed: bool,
}

// The refreshed-read path lives in `execution_dispatch::render_refreshed_read_command`.

fn load_context_for_matches(
    paths: &CliContextPaths,
    matches: &ArgMatches,
) -> Result<CommandContext<InMemoryRegistry>, CliError> {
    if matches.subcommand().is_some_and(|(name, subcommand)| {
        name == "state" && matches!(subcommand.subcommand(), Some(("export", _)))
    }) {
        load_context_with_events(paths)
    } else {
        load_context(paths)
    }
}

fn load_tracked_context_for_matches(
    paths: &CliContextPaths,
    matches: &ArgMatches,
) -> Result<TrackedContext, CliError> {
    if matches.subcommand().is_some_and(|(name, subcommand)| {
        name == "state" && matches!(subcommand.subcommand(), Some(("export", _)))
    }) {
        let context = load_context_with_events(paths)?;
        Ok(TrackedContext {
            save_state: context::context_save_state_from_registry(&context.registry),
            context,
        })
    } else {
        load_tracked_context(paths)
    }
}

#[cfg(feature = "interactive")]
fn stream_command_to_writer<R, W, P>(
    matches: &ArgMatches,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    writer: &mut W,
    persist: P,
) -> Option<Result<bool, CliError>>
where
    R: CommandRunner,
    W: Write,
    P: FnMut(&CommandContext<InMemoryRegistry>) -> Result<(), CliError>,
{
    if matches
        .subcommand()
        .is_some_and(|(name, subcommand)| name == "cockpit" && subcommand.get_flag("watch"))
    {
        return Some(stream_live_cockpit_command(
            context,
            matches.subcommand().unwrap().1,
            runner,
            writer,
            persist,
        ));
    }

    None
}

#[cfg(not(feature = "interactive"))]
fn stream_command_to_writer<R, W, P>(
    _matches: &ArgMatches,
    _context: &mut CommandContext<InMemoryRegistry>,
    _runner: &mut R,
    _writer: &mut W,
    _persist: P,
) -> Option<Result<bool, CliError>>
where
    R: CommandRunner,
    W: Write,
    P: FnMut(&CommandContext<InMemoryRegistry>) -> Result<(), CliError>,
{
    None
}

fn write_command_output(writer: &mut impl Write, output: &str) -> Result<(), CliError> {
    match writeln!(writer, "{output}") {
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(CliError::CommandFailed(error.to_string())),
        Ok(()) => Ok(()),
    }
}

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
#[path = "lib/tests.rs"]
mod tests;

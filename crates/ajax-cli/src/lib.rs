mod cli;
mod cockpit_actions;
mod context;
mod dispatch;
mod render;
mod supervise;

use ajax_core::{
    adapters::{CommandOutput, CommandRunner, ProcessCommandRunner},
    commands::{self, CommandContext, CommandError},
    output::{CockpitResponse, DoctorCheck},
    registry::{InMemoryRegistry, Registry},
};
use clap::ArgMatches;
pub use cli::build_cli;
use cli::{parse_args, ParsedArgs};
use cockpit_actions::{
    execute_pending_cockpit_action, handle_pending_cockpit_result, tui_cockpit_action,
    PendingCockpitOutcome,
};
pub use context::CliContextPaths;
use context::{default_context_paths, load_context, save_context};
use dispatch::{render_task_command, TaskCommandOperation};
use render::{
    render_doctor_human, render_execution_outputs, render_inbox_human, render_inspect_human,
    render_next_human, render_plan, render_reconcile_human, render_repos_human, render_response,
    render_tasks_human,
};
use std::{ffi::OsStr, time::Duration};
use supervise::render_supervise_command;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CliError {
    CommandFailed(String),
    CommandFailedAfterStateChange(String),
    JsonSerialization(String),
    ContextLoad(String),
    ContextSave(String),
}

fn current_open_mode() -> commands::OpenMode {
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

    render_matches(&matches, context)
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

fn render_matches(
    matches: &ArgMatches,
    context: &CommandContext<InMemoryRegistry>,
) -> Result<String, CliError> {
    render_matches_with_paths(matches, context, None)
}

fn render_matches_with_paths(
    matches: &ArgMatches,
    context: &CommandContext<InMemoryRegistry>,
    paths: Option<&CliContextPaths>,
) -> Result<String, CliError> {
    match matches.subcommand() {
        Some(("repos", subcommand)) => render_response(
            commands::list_repos(context),
            subcommand.get_flag("json"),
            render_repos_human,
        ),
        Some(("tasks", subcommand)) => render_response(
            commands::list_tasks(
                context,
                subcommand.get_one::<String>("repo").map(String::as_str),
            ),
            subcommand.get_flag("json"),
            render_tasks_human,
        ),
        Some(("inspect", subcommand)) => {
            let task = subcommand
                .get_one::<String>("task")
                .map(String::as_str)
                .unwrap_or_default();
            let response = commands::inspect_task(context, task).map_err(command_error)?;
            render_response(response, subcommand.get_flag("json"), render_inspect_human)
        }
        Some(("new", subcommand)) => {
            let request = new_task_request(subcommand)?;
            let plan = commands::new_task_plan(context, request).map_err(command_error)?;
            render_readonly_plan(plan, subcommand)
        }
        Some(("open", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::open_task_plan(context, task, current_open_mode())
                .map_err(command_error)?;
            render_readonly_plan(plan, subcommand)
        }
        Some(("trunk", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::trunk_task_plan(context, task).map_err(command_error)?;
            render_readonly_plan(plan, subcommand)
        }
        Some(("check", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::check_task_plan(context, task).map_err(command_error)?;
            render_readonly_plan(plan, subcommand)
        }
        Some(("diff", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::diff_task_plan(context, task).map_err(command_error)?;
            render_readonly_plan(plan, subcommand)
        }
        Some(("merge", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::merge_task_plan(context, task).map_err(command_error)?;
            render_readonly_plan(plan, subcommand)
        }
        Some(("clean", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::clean_task_plan(context, task).map_err(command_error)?;
            render_readonly_plan(plan, subcommand)
        }
        Some(("sweep", subcommand)) => {
            render_readonly_plan(commands::sweep_cleanup_plan(context), subcommand)
        }
        Some(("next", subcommand)) => render_response(
            commands::next(context),
            subcommand.get_flag("json"),
            render_next_human,
        ),
        Some(("inbox", subcommand)) => render_response(
            commands::inbox(context),
            subcommand.get_flag("json"),
            render_inbox_human,
        ),
        Some(("review", subcommand)) => render_response(
            commands::review_queue(context),
            subcommand.get_flag("json"),
            render_tasks_human,
        ),
        Some(("doctor", subcommand)) => {
            let mut response = commands::doctor(context);
            if let Some(paths) = paths {
                response.checks.extend(context_path_checks(paths));
            }
            render_response(response, subcommand.get_flag("json"), render_doctor_human)
        }
        Some(("status", subcommand)) => render_response(
            commands::status(context),
            subcommand.get_flag("json"),
            render_tasks_human,
        ),
        Some(("state", subcommand)) => render_state_command(context, subcommand),
        Some(("cockpit", subcommand)) => render_cockpit_command(context, subcommand),
        Some(("reconcile", _)) => Err(CliError::CommandFailed(
            "reconcile requires mutable context and runner support".to_string(),
        )),
        Some(("supervise", _)) => Err(CliError::CommandFailed(
            "supervise requires mutable context and runner support".to_string(),
        )),
        Some((name, _)) => Err(CliError::CommandFailed(format!(
            "unsupported command: {name}"
        ))),
        None => Err(CliError::CommandFailed(
            "command is required; pass --help".to_string(),
        )),
    }
}

fn render_state_command(
    context: &CommandContext<InMemoryRegistry>,
    matches: &ArgMatches,
) -> Result<String, CliError> {
    match matches.subcommand() {
        Some(("export", subcommand)) => {
            let output = subcommand.get_one::<String>("output").ok_or_else(|| {
                CliError::CommandFailed("state export --output is required".to_string())
            })?;
            export_state_snapshot(context, std::path::Path::new(output))
        }
        Some((name, _)) => Err(CliError::CommandFailed(format!(
            "unknown state subcommand: {name}"
        ))),
        None => Err(CliError::CommandFailed(
            "state subcommand is required".to_string(),
        )),
    }
}

fn export_state_snapshot(
    context: &CommandContext<InMemoryRegistry>,
    path: &std::path::Path,
) -> Result<String, CliError> {
    if path.exists() {
        return Err(CliError::CommandFailed(format!(
            "state export target already exists: {}",
            path.display()
        )));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| CliError::CommandFailed(error.to_string()))?;
    }
    context
        .registry
        .export_json_snapshot_file(path)
        .map_err(|error| CliError::CommandFailed(format!("state export failed: {error}")))?;

    Ok(format!("exported state snapshot: {}", path.display()))
}

fn context_path_checks(paths: &CliContextPaths) -> Vec<DoctorCheck> {
    let config_exists = paths.config_file.is_file();
    let state_exists = paths.state_file.is_file();
    let state_parent_creatable = state_exists || parent_directory_available(&paths.state_file);

    vec![
        DoctorCheck {
            name: "config:path".to_string(),
            ok: config_exists,
            message: if config_exists {
                format!("file exists: {}", paths.config_file.display())
            } else {
                format!(
                    "file not found; defaults in use: {}",
                    paths.config_file.display()
                )
            },
        },
        DoctorCheck {
            name: "state:path".to_string(),
            ok: state_parent_creatable,
            message: if state_exists {
                format!("file exists: {}", paths.state_file.display())
            } else if state_parent_creatable {
                "parent directory can be created".to_string()
            } else {
                format!(
                    "parent directory unavailable: {}",
                    paths.state_file.display()
                )
            },
        },
    ]
}

fn parent_directory_available(path: &std::path::Path) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    let parent = if parent.as_os_str().is_empty() {
        std::env::current_dir().ok()
    } else if parent.is_absolute() {
        Some(parent.to_path_buf())
    } else {
        std::env::current_dir()
            .ok()
            .map(|current_dir| current_dir.join(parent))
    };

    parent.is_some_and(|parent| {
        parent.is_dir() || parent.ancestors().skip(1).any(|ancestor| ancestor.is_dir())
    })
}

fn render_cockpit_command(
    context: &CommandContext<InMemoryRegistry>,
    matches: &ArgMatches,
) -> Result<String, CliError> {
    if matches.get_flag("json") {
        return render_response(commands::cockpit(context), true, |_| String::new());
    }

    let iterations = parse_u32_arg(matches, "iterations", 1)?;
    let interval = parse_u64_arg(matches, "interval-ms", 1000)?;

    if matches.get_flag("watch") {
        return Ok(render_cockpit_frames(
            context,
            iterations.max(1),
            Duration::from_millis(interval),
        ));
    }

    Err(CliError::CommandFailed(
        "interactive cockpit requires command execution support".to_string(),
    ))
}

fn render_cockpit_frames(
    context: &CommandContext<InMemoryRegistry>,
    iterations: u32,
    interval: Duration,
) -> String {
    let frames = (0..iterations)
        .map(|index| {
            if index > 0 && !interval.is_zero() {
                std::thread::sleep(interval);
            }
            render_cockpit_frame(context)
        })
        .collect::<Vec<_>>();

    frames.join("\n\n")
}

fn render_cockpit_frame(context: &CommandContext<InMemoryRegistry>) -> String {
    ajax_tui::render_cockpit(
        &commands::list_repos(context),
        &commands::list_tasks(context, None),
        &commands::inbox(context),
    )
}

fn render_matches_mut(
    matches: &ArgMatches,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut impl CommandRunner,
) -> Result<RenderedCommand, CliError> {
    match matches.subcommand() {
        Some(("new", subcommand)) => {
            let request = new_task_request(subcommand)?;
            let plan = commands::new_task_plan(context, request.clone()).map_err(command_error)?;

            if !subcommand.get_flag("execute") {
                return Ok(RenderedCommand {
                    output: render_plan(plan, subcommand.get_flag("json"))?,
                    state_changed: false,
                });
            }

            let (outputs, task) = execute_new_task_plan(
                context,
                runner,
                &request,
                &plan,
                subcommand.get_flag("yes"),
            )?;
            Ok(RenderedCommand {
                output: render_execution_outputs(&outputs, Some(&task.qualified_handle())),
                state_changed: true,
            })
        }
        Some(("reconcile", subcommand)) => {
            let response = commands::reconcile_external(context, runner).map_err(command_error)?;
            Ok(RenderedCommand {
                output: render_response(
                    response.clone(),
                    subcommand.get_flag("json"),
                    render_reconcile_human,
                )?,
                state_changed: response.tasks_changed > 0,
            })
        }
        Some(("status", subcommand)) => {
            let changed = refresh_live_context(context, runner)?;
            Ok(RenderedCommand {
                output: render_response(
                    commands::status(context),
                    subcommand.get_flag("json"),
                    render_tasks_human,
                )?,
                state_changed: changed,
            })
        }
        Some((name @ ("open" | "trunk" | "check" | "diff" | "merge" | "clean"), subcommand)) => {
            let operation = TaskCommandOperation::from_cli_subcommand(name).ok_or_else(|| {
                CliError::CommandFailed(format!("unsupported task command: {name}"))
            })?;
            render_task_command(operation, subcommand, context, runner, current_open_mode())
        }
        Some(("sweep", subcommand)) => {
            let plan = commands::sweep_cleanup_plan(context);
            let candidates = commands::sweep_cleanup_candidates(context);
            if !subcommand.get_flag("execute") {
                return Ok(RenderedCommand {
                    output: render_plan(plan, subcommand.get_flag("json"))?,
                    state_changed: false,
                });
            }
            let outputs =
                execute_sweep_cleanup(context, runner, &candidates, subcommand.get_flag("yes"))?;
            Ok(RenderedCommand {
                output: render_execution_outputs(&outputs, None),
                state_changed: !candidates.is_empty(),
            })
        }
        Some(("supervise", subcommand)) => Ok(RenderedCommand {
            output: render_supervise_command(subcommand)?,
            state_changed: false,
        }),
        Some(("cockpit", subcommand)) => {
            if subcommand.get_flag("json") || subcommand.get_flag("watch") {
                return render_live_cockpit_command(context, subcommand, runner);
            }
            // Interactive TUI with full action support.
            let mut state_changed = false;
            let mut cockpit_flash = None;
            state_changed |= refresh_live_context(context, runner)?;
            let refresh_interval =
                Duration::from_millis(parse_u64_arg(subcommand, "interval-ms", 1000)?);
            loop {
                let pending = ajax_tui::run_interactive_with_flash_and_refresh(
                    commands::list_repos(context),
                    commands::list_tasks(context, None),
                    commands::inbox(context),
                    cockpit_flash.take(),
                    refresh_interval,
                    InteractiveCockpitHandler {
                        context,
                        runner,
                        state_changed: &mut state_changed,
                    },
                )
                .map_err(|e| CliError::CommandFailed(e.to_string()))?;
                let Some(pending) = pending else {
                    return Ok(RenderedCommand {
                        output: String::new(),
                        state_changed,
                    });
                };

                let Some(outcome) = handle_pending_cockpit_result(
                    execute_pending_cockpit_action(&pending, context, runner, &mut state_changed),
                    &mut cockpit_flash,
                ) else {
                    continue;
                };

                match outcome {
                    PendingCockpitOutcome::Exit(output) => {
                        return Ok(RenderedCommand {
                            output,
                            state_changed,
                        });
                    }
                    PendingCockpitOutcome::ReturnToCockpit => {}
                }
            }
        }
        _ => Ok(RenderedCommand {
            output: render_matches(matches, context)?,
            state_changed: false,
        }),
    }
}

fn render_matches_mut_with_paths(
    matches: &ArgMatches,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut impl CommandRunner,
    paths: Option<&CliContextPaths>,
) -> Result<RenderedCommand, CliError> {
    if matches
        .subcommand()
        .is_some_and(|(name, _)| name == "doctor")
    {
        return Ok(RenderedCommand {
            output: render_matches_with_paths(matches, context, paths)?,
            state_changed: false,
        });
    }

    render_matches_mut(matches, context, runner)
}

fn render_live_cockpit_command<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    matches: &ArgMatches,
    runner: &mut R,
) -> Result<RenderedCommand, CliError> {
    let iterations = parse_u32_arg(matches, "iterations", 1)?.max(1);
    let interval = parse_u64_arg(matches, "interval-ms", 1000)?;

    if matches.get_flag("json") {
        let changed = refresh_live_context(context, runner)?;
        return Ok(RenderedCommand {
            output: render_response(commands::cockpit(context), true, |_| String::new())?,
            state_changed: changed,
        });
    }

    let mut state_changed = false;
    let frames = (0..iterations)
        .map(|index| {
            if index > 0 && interval > 0 {
                std::thread::sleep(Duration::from_millis(interval));
            }
            let changed = refresh_live_context(context, runner)?;
            state_changed |= changed;
            Ok(render_cockpit_frame(context))
        })
        .collect::<Result<Vec<_>, CliError>>()?;

    Ok(RenderedCommand {
        output: frames.join("\n\n"),
        state_changed,
    })
}

fn refresh_live_context<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
) -> Result<bool, CliError> {
    let response = commands::reconcile_external(context, runner).map_err(command_error)?;
    Ok(response.tasks_changed > 0)
}

fn refresh_cockpit_snapshot<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    state_changed: &mut bool,
) -> Result<CockpitResponse, CliError> {
    *state_changed |= refresh_live_context(context, runner)?;
    Ok(commands::cockpit(context))
}

struct InteractiveCockpitHandler<'a, R: CommandRunner> {
    context: &'a mut CommandContext<InMemoryRegistry>,
    runner: &'a mut R,
    state_changed: &'a mut bool,
}

impl<R: CommandRunner> ajax_tui::CockpitEventHandler for InteractiveCockpitHandler<'_, R> {
    fn on_action(
        &mut self,
        item: &ajax_core::models::AttentionItem,
    ) -> std::io::Result<ajax_tui::ActionOutcome> {
        tui_cockpit_action(item, self.context, self.runner, self.state_changed)
    }

    fn on_refresh(&mut self) -> std::io::Result<Option<CockpitResponse>> {
        refresh_cockpit_snapshot(self.context, self.runner, self.state_changed)
            .map(Some)
            .map_err(|error| std::io::Error::other(error.to_string()))
    }
}

pub(crate) fn execute_new_task_plan<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    request: &commands::NewTaskRequest,
    plan: &commands::CommandPlan,
    confirmed: bool,
) -> Result<(Vec<CommandOutput>, ajax_core::models::Task), CliError> {
    let outputs = commands::execute_plan(plan, confirmed, runner).map_err(command_error)?;
    let task = commands::record_new_task(context, request).map_err(command_error)?;
    commands::mark_task_opened(context, &task.qualified_handle()).map_err(command_error)?;

    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|candidate| candidate.id == task.id)
        .cloned()
        .unwrap_or(task);

    Ok((outputs, task))
}

fn execute_sweep_cleanup<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    candidates: &[String],
    confirmed: bool,
) -> Result<Vec<CommandOutput>, CliError> {
    let mut outputs = Vec::new();
    let mut state_changed = false;

    for candidate in candidates {
        let plan = commands::clean_task_plan(context, candidate).map_err(command_error)?;
        match commands::execute_plan(&plan, confirmed, runner) {
            Ok(mut command_outputs) => {
                outputs.append(&mut command_outputs);
                commands::mark_task_removed(context, candidate).map_err(command_error)?;
                state_changed = true;
            }
            Err(error) => {
                let cli_error = command_error(error);
                return Err(if state_changed {
                    cli_error.after_state_change()
                } else {
                    cli_error
                });
            }
        }
    }

    Ok(outputs)
}

fn parse_u32_arg(matches: &ArgMatches, name: &str, default: u32) -> Result<u32, CliError> {
    let Some(value) = matches.get_one::<String>(name) else {
        return Ok(default);
    };

    value
        .parse::<u32>()
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

fn render_readonly_plan(
    plan: commands::CommandPlan,
    matches: &ArgMatches,
) -> Result<String, CliError> {
    if matches.get_flag("execute") {
        return Err(CliError::CommandFailed(
            "execution requires mutable context and runner support".to_string(),
        ));
    }

    render_plan(plan, matches.get_flag("json"))
}

fn new_task_request(matches: &ArgMatches) -> Result<commands::NewTaskRequest, CliError> {
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
            AgentClient, GitStatus, LifecycleStatus, RecommendedAction, SideFlag, Task, TaskId,
        },
        registry::{InMemoryRegistry, Registry, RegistryStore, SqliteRegistryStore},
    };
    use std::path::Path;

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

    #[test]
    fn cli_error_display_omits_internal_enum_wrapping() {
        let error = CliError::CommandFailed("task title is required; pass --title".to_string());

        assert_eq!(error.to_string(), "task title is required; pass --title");
        assert!(!error.to_string().contains("CommandFailed"));
    }

    #[test]
    fn binary_prints_cli_errors_with_display_formatting() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let main_source = std::fs::read_to_string(manifest_dir.join("src/main.rs")).unwrap();

        assert!(main_source.contains("eprintln!(\"{error}\")"));
        assert!(!main_source.contains("eprintln!(\"{error:?}\")"));
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
            vec!["ajax", "reconcile"],
            vec!["ajax", "supervise", "--prompt", "fix tests"],
            vec!["ajax", "cockpit"],
        ] {
            let matches = build_cli().try_get_matches_from(args.clone());
            assert!(matches.is_ok(), "{args:?} should parse");
        }
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
    fn cockpit_json_refreshes_live_worktree_status_from_tmux_pane() {
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
        let mut runner = QueuedRunner::new(vec![
            output(0, "ajax-web-fix-login\n"),
            output(0, "## ajax/fix-login...origin/ajax/fix-login\n"),
            output(1, ""),
            output(0, "worktrunk\t/tmp/worktrees/web-fix-login\n"),
            output(0, "Do you want to proceed? y/n\n"),
        ]);

        let output =
            run_with_context_and_runner(["ajax", "cockpit", "--json"], &mut context, &mut runner)
                .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(
            parsed["tasks"]["tasks"][0]["qualified_handle"],
            "web/fix-login"
        );
        assert_eq!(
            parsed["tasks"]["tasks"][0]["live_status"]["kind"],
            "WaitingForApproval"
        );
        assert_eq!(
            parsed["inbox"]["items"][0]["reason"],
            "waiting for approval"
        );
        assert_eq!(runner.commands.len(), 5);
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
        let mut runner = QueuedRunner::new(vec![
            output(0, "ajax-web-fix-login\n"),
            output(0, "## ajax/fix-login...origin/ajax/fix-login\n"),
            output(1, ""),
            output(0, "worktrunk\t/tmp/worktrees/web-fix-login\n"),
            output(0, "Do you want to proceed? y/n\n"),
        ]);

        let output = run_with_context_and_runner(
            ["ajax", "cockpit", "--watch", "--iterations", "1"],
            &mut context,
            &mut runner,
        )
        .unwrap();

        assert!(output.contains("web/fix-login\twaiting for approval\tFix login"));
        assert_eq!(runner.commands.len(), 5);
    }

    #[test]
    fn status_command_refreshes_live_state_before_rendering() {
        let mut context = sample_context();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Active;
        task.remove_side_flag(SideFlag::NeedsInput);
        let mut runner = QueuedRunner::new(vec![
            output(0, "ajax-web-fix-login\n"),
            output(0, "## ajax/fix-login...origin/ajax/fix-login\n"),
            output(1, ""),
            output(0, "worktrunk\t/tmp/worktrees/web-fix-login\n"),
            output(0, "Do you want to proceed? y/n\n"),
        ]);

        let output =
            run_with_context_and_runner(["ajax", "status"], &mut context, &mut runner).unwrap();

        assert!(output.contains("web/fix-login\tWaiting\tFix login"));
        assert!(context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .has_side_flag(SideFlag::NeedsInput));
        assert_eq!(runner.commands.len(), 5);
    }

    #[test]
    fn status_command_renders_json_after_refreshing_live_state() {
        let mut context = sample_context();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Active;
        task.remove_side_flag(SideFlag::NeedsInput);
        let mut runner = QueuedRunner::new(vec![
            output(0, "ajax-web-fix-login\n"),
            output(0, "## ajax/fix-login...origin/ajax/fix-login\n"),
            output(1, ""),
            output(0, "worktrunk\t/tmp/worktrees/web-fix-login\n"),
            output(0, "Do you want to proceed? y/n\n"),
        ]);

        let output =
            run_with_context_and_runner(["ajax", "status", "--json"], &mut context, &mut runner)
                .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["tasks"][0]["qualified_handle"], "web/fix-login");
        assert_eq!(
            parsed["tasks"][0]["live_status"]["kind"],
            "WaitingForApproval"
        );
    }

    #[test]
    fn cockpit_refresh_snapshot_reconciles_live_state_and_reports_changes() {
        let mut context = sample_context();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Active;
        task.remove_side_flag(SideFlag::NeedsInput);
        let mut runner = QueuedRunner::new(vec![
            output(0, "ajax-web-fix-login\n"),
            output(0, "## ajax/fix-login...origin/ajax/fix-login\n"),
            output(1, ""),
            output(0, "worktrunk\t/tmp/worktrees/web-fix-login\n"),
            output(0, "Do you want to proceed? y/n\n"),
        ]);
        let mut state_changed = false;

        let snapshot =
            super::refresh_cockpit_snapshot(&mut context, &mut runner, &mut state_changed).unwrap();

        assert!(state_changed);
        assert_eq!(
            snapshot.tasks.tasks[0].live_status.as_ref().unwrap().kind,
            ajax_core::models::LiveStatusKind::WaitingForApproval
        );
        assert_eq!(snapshot.inbox.items[0].reason, "waiting for approval");
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

        for command_mode in [
            "CommandMode::Capture",
            "CommandMode::InheritStdio",
            "CommandMode::Spawn",
        ] {
            assert!(
                architecture.contains(command_mode),
                "architecture.md should name the current {command_mode} execution path"
            );
        }

        assert!(architecture.contains("current `ajax-cli` split"));
        assert!(!architecture.contains("Consider Ratatui"));
        assert!(!architecture.contains("long-term implementation should"));
        assert!(!architecture.contains("The intended persistence boundary"));
    }

    #[test]
    fn reconcile_command_supports_json_output() {
        let matches = build_cli().try_get_matches_from(["ajax", "reconcile", "--json"]);

        assert!(matches.is_ok());
    }

    #[test]
    fn readonly_context_rejects_reconcile_instead_of_reporting_synthetic_success() {
        let context = sample_context();

        let error = run_with_context(["ajax", "reconcile", "--json"], &context).unwrap_err();

        assert!(matches!(error, super::CliError::CommandFailed(message)
                if message.contains("reconcile requires mutable context and runner support")));
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
            "ajax-core = models, policy, reconciliation, registry",
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
        assert!(smoke.contains("ajax merge"));
        assert!(smoke.contains("ajax state export"));
        assert!(smoke.contains("target/release/ajax"));
        assert!(smoke.contains("cargo build --release -p ajax-cli"));
        assert!(!smoke.contains("target/debug/ajax"));
        assert!(smoke.contains("if [[ -z \"${AJAX_BIN:-}\" ]]"));
        assert!(smoke.contains("ajax binary is not executable"));
        assert!(readme.contains("scripts/smoke.sh"));
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
        assert!(output.contains("(cd /tmp/worktrees/web-fix-login && sh -lc cargo test)"));
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
                )
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
    fn new_execute_provisioning_failure_does_not_record_task_state() {
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

        assert!(matches!(error, super::CliError::CommandFailed(message)
                if message == "command failed: tmux exited with status 42: tmux failed"));
        assert!(context.registry.list_tasks().is_empty());
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
                )
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
            LifecycleStatus::Active
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
    fn failed_external_command_does_not_mutate_task_state() {
        let mut context = sample_context();
        let mut runner = QueuedRunner::new(vec![output(42, "")]);

        let error = run_with_context_and_runner(
            ["ajax", "merge", "web/fix-login", "--execute", "--yes"],
            &mut context,
            &mut runner,
        )
        .unwrap_err();

        assert!(matches!(error, super::CliError::CommandFailed(message)
                if message == "command failed: git exited with status 42"));
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
    fn external_command_failure_uses_operator_facing_message() {
        let mut context = sample_context();
        let mut runner = QueuedRunner::new(vec![CommandOutput {
            status_code: 42,
            stdout: String::new(),
            stderr: "merge failed".to_string(),
        }]);

        let error = run_with_context_and_runner(
            ["ajax", "merge", "web/fix-login", "--execute", "--yes"],
            &mut context,
            &mut runner,
        )
        .unwrap_err();

        assert!(matches!(&error, super::CliError::CommandFailed(message)
                if message == "command failed: git exited with status 42: merge failed"));
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
    fn clean_execute_removes_risky_task_without_yes() {
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
            ["ajax", "clean", "web/fix-login", "--execute"],
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
    fn trunk_execute_uses_injected_runner() {
        let mut context = sample_context();
        let mut runner = RecordingCommandRunner::default();

        run_with_context_and_runner(
            ["ajax", "trunk", "web/fix-login", "--execute"],
            &mut context,
            &mut runner,
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
            output(2, "remove failed"),
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
            "open worktrunk",
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
            Defer,
            Message(&'a [&'a str]),
            Refresh,
        }

        for (handle, action, expected) in [
            ("web/fix-login", "open task", Expected::Defer),
            ("web/fix-login", "merge task", Expected::Defer),
            ("web/fix-login", "clean task", Expected::Refresh),
            (
                "web",
                "new task",
                Expected::Message(&["select a project", "new task"]),
            ),
            ("web", "status", Expected::Message(&["web: 1 task(s)"])),
            ("web", "reconcile", Expected::Refresh),
        ] {
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
                        if matches!(action, "clean task" | "reconcile") {
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
                )
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
            super::TaskCommandOperation::from_cli_subcommand("clean"),
            Some(super::TaskCommandOperation::Clean)
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
            Some(super::TaskCommandOperation::Clean)
        );
        assert_eq!(
            super::TaskCommandOperation::from_recommended_action(RecommendedAction::Reconcile),
            None
        );
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
            super::TaskCommandOperation::Clean,
        ] {
            assert!(
                operation.returns_to_cockpit_after_execute(),
                "{operation:?} should return to the task picker"
            );
        }
    }

    #[test]
    fn pending_cockpit_merge_and_reconcile_return_to_ajax() {
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

        let mut reconcile_context = CommandContext::new(
            Config {
                repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
                ..Config::default()
            },
            InMemoryRegistry::default(),
        );
        let mut reconcile_runner = PanicRunner;
        let mut reconcile_state_changed = false;
        let reconcile_pending = ajax_tui::PendingAction {
            task_handle: "web".to_string(),
            recommended_action: "reconcile".to_string(),
            task_title: None,
        };

        let reconcile_outcome = super::execute_pending_cockpit_action(
            &reconcile_pending,
            &mut reconcile_context,
            &mut reconcile_runner,
            &mut reconcile_state_changed,
        )
        .unwrap();

        assert_eq!(
            reconcile_outcome,
            super::PendingCockpitOutcome::ReturnToCockpit
        );
        assert!(!reconcile_state_changed);
    }

    #[test]
    fn pending_cockpit_reconcile_preserves_prior_state_change() {
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

        let outcome = super::execute_pending_cockpit_action(
            &pending,
            &mut context,
            &mut runner,
            &mut state_changed,
        )
        .unwrap();

        assert_eq!(outcome, super::PendingCockpitOutcome::ReturnToCockpit);
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

        let outcome = super::execute_pending_cockpit_action(
            &pending,
            &mut context,
            &mut runner,
            &mut state_changed,
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

        let outcome = super::execute_pending_cockpit_action(
            &pending,
            &mut context,
            &mut runner,
            &mut state_changed,
        )
        .unwrap();

        assert!(matches!(outcome, super::PendingCockpitOutcome::Exit(_)));
        assert!(state_changed);
    }

    #[test]
    fn pending_cockpit_removed_actions_are_rejected() {
        for action in [
            "open worktrunk",
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
    fn pending_cockpit_open_alias_actions_run_open_plan_and_mark_active() {
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
            LifecycleStatus::Active,
            "{action} should mark the task active"
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
    fn pending_cockpit_open_action_runs_task_and_marks_active() {
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
            LifecycleStatus::Active
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
    fn cockpit_reconcile_action_runs_and_refreshes_cockpit() {
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
            ajax_tui::ActionOutcome::Defer(_) => panic!("reconcile should refresh in cockpit"),
            ajax_tui::ActionOutcome::Message(message) => {
                panic!("reconcile should run instead of showing message: {message}")
            }
        }
        assert!(!runner.commands().is_empty());
        assert!(state_changed);
    }

    #[test]
    fn cockpit_reconcile_refreshes_without_runner_when_no_tasks_exist() {
        let mut context = CommandContext::new(
            Config {
                repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
                ..Config::default()
            },
            InMemoryRegistry::default(),
        );
        let item = ajax_core::models::AttentionItem {
            task_id: TaskId::new("__project_action__web__reconcile"),
            task_handle: "web".to_string(),
            reason: "Reconcile".to_string(),
            priority: 0,
            recommended_action: "reconcile".to_string(),
        };
        let mut runner = PanicRunner;
        let mut state_changed = false;

        let outcome =
            super::tui_cockpit_action(&item, &mut context, &mut runner, &mut state_changed)
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
                panic!("reconcile should refresh instead of deferring")
            }
            ajax_tui::ActionOutcome::Message(message) => {
                panic!("reconcile should run instead of showing message: {message}")
            }
        }
        assert!(!state_changed);
    }

    #[test]
    fn cockpit_reconcile_preserves_prior_state_change_when_no_tasks_change() {
        let mut context = CommandContext::new(
            Config {
                repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
                ..Config::default()
            },
            InMemoryRegistry::default(),
        );
        let item = ajax_core::models::AttentionItem {
            task_id: TaskId::new("__project_action__web__reconcile"),
            task_handle: "web".to_string(),
            reason: "Reconcile".to_string(),
            priority: 0,
            recommended_action: "reconcile".to_string(),
        };
        let mut runner = PanicRunner;
        let mut state_changed = true;

        let outcome =
            super::tui_cockpit_action(&item, &mut context, &mut runner, &mut state_changed)
                .unwrap();

        assert!(matches!(outcome, ajax_tui::ActionOutcome::Refresh { .. }));
        assert!(state_changed);
    }

    #[test]
    fn cockpit_clean_action_runs_and_refreshes_inside_ajax() {
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

        let outcome =
            super::tui_cockpit_action(&item, &mut context, &mut runner, &mut state_changed)
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
    fn reconcile_saves_registry_snapshot_when_task_changes() {
        let directory = std::env::temp_dir().join(format!(
            "ajax-cli-reconcile-{}-{}",
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

        let output = run_with_context_paths_and_runner(
            ["ajax", "reconcile", "--json"],
            &CliContextPaths::new(&config_file, &state_file),
            &mut runner,
        )
        .unwrap();
        let restored = SqliteRegistryStore::new(&state_file).load().unwrap();

        std::fs::remove_dir_all(Path::new(&directory)).unwrap();
        assert!(output.contains("\"tasks_changed\": 1"));
        assert_eq!(
            runner.commands,
            vec![
                CommandSpec::new("tmux", ["list-sessions", "-F", "#{session_name}"]),
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
            ]
        );
        assert!(restored
            .list_tasks()
            .iter()
            .any(|task| task.has_side_flag(SideFlag::WorktreeMissing)));
    }
}

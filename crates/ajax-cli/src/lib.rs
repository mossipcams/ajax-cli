use ajax_core::{
    adapters::{CommandRunner, ProcessCommandRunner},
    commands::{self, CommandContext, CommandError},
    config::{Config, ConfigPaths},
    output::{
        DoctorResponse, InboxResponse, InspectResponse, NextResponse, ReconcileResponse,
        ReposResponse, TaskSummary, TasksResponse,
    },
    registry::InMemoryRegistry,
};
use clap::error::ErrorKind;
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::Serialize;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CliError {
    CommandFailed(String),
    JsonSerialization(String),
    ContextLoad(String),
    ContextSave(String),
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
    let rendered = render_matches_mut(&matches, &mut context, &mut runner)?;
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

    render_matches(&matches, &context)
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
    let rendered = render_matches_mut(&matches, &mut context, runner)?;
    if rendered.state_changed {
        save_context(paths, &context)?;
    }

    Ok(rendered.output)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CliContextPaths {
    pub config_file: PathBuf,
    pub state_file: PathBuf,
}

impl CliContextPaths {
    pub fn new(config_file: impl Into<PathBuf>, state_file: impl Into<PathBuf>) -> Self {
        Self {
            config_file: config_file.into(),
            state_file: state_file.into(),
        }
    }
}

pub fn build_cli() -> Command {
    Command::new("ajax")
        .about("Semi-agentic operator console for isolated AI coding tasks")
        .subcommand(repos_command())
        .subcommand(tasks_command())
        .subcommand(task_command("inspect"))
        .subcommand(executable_new_command())
        .subcommand(executable_task_command("open"))
        .subcommand(executable_task_command("trunk"))
        .subcommand(executable_task_command("check"))
        .subcommand(executable_task_command("diff"))
        .subcommand(executable_task_command("merge"))
        .subcommand(executable_task_command("clean"))
        .subcommand(executable_command(
            json_command("sweep").about("Clean safe task environments across repos"),
        ))
        .subcommand(executable_task_command("repair"))
        .subcommand(json_command("next").about("Show the next task needing attention"))
        .subcommand(json_command("inbox").about("Show global attention inbox"))
        .subcommand(json_command("review").about("Show tasks ready for review"))
        .subcommand(Command::new("status").about("Show Ajax status"))
        .subcommand(json_command("doctor").about("Check local Ajax dependencies and health"))
        .subcommand(json_command("reconcile").about("Compare registry state with external reality"))
        .subcommand(cockpit_command())
}

enum ParsedArgs {
    Matches(ArgMatches),
    Message(String),
}

fn parse_args<I, T>(args: I) -> Result<ParsedArgs, CliError>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    match build_cli().try_get_matches_from(args) {
        Ok(matches) => Ok(ParsedArgs::Matches(matches)),
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            Ok(ParsedArgs::Message(error.to_string()))
        }
        Err(error) => Err(CliError::CommandFailed(error.to_string())),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RenderedCommand {
    output: String,
    state_changed: bool,
}

fn repos_command() -> Command {
    json_command("repos").about("List configured repos")
}

fn tasks_command() -> Command {
    json_command("tasks")
        .about("List task environments")
        .arg(Arg::new("repo").long("repo").value_name("REPO"))
}

fn executable_new_command() -> Command {
    executable_command(json_command("new"))
        .about("Create a new task environment")
        .arg(Arg::new("repo").long("repo").value_name("REPO"))
        .arg(Arg::new("title").long("title").value_name("TITLE"))
        .arg(Arg::new("agent").long("agent").value_name("AGENT"))
}

fn task_command(name: &'static str) -> Command {
    json_command(name)
        .about("Operate on a task")
        .arg(Arg::new("task").value_name("REPO/HANDLE").required(true))
}

fn executable_task_command(name: &'static str) -> Command {
    executable_command(task_command(name))
}

fn executable_command(command: Command) -> Command {
    command
        .arg(
            Arg::new("execute")
                .long("execute")
                .help("Execute the planned external commands")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("yes")
                .long("yes")
                .help("Confirm commands that require confirmation")
                .action(ArgAction::SetTrue),
        )
}

fn cockpit_command() -> Command {
    Command::new("cockpit")
        .about("Render the Ajax operator cockpit")
        .arg(
            Arg::new("textual")
                .long("textual")
                .help("Launch the Textual cockpit frontend")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("execute")
                .long("execute")
                .help("Execute the planned Textual frontend launch")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("watch")
                .long("watch")
                .help("Keep rendering cockpit frames")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Emit machine-readable JSON")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("interval-ms")
                .long("interval-ms")
                .value_name("MILLISECONDS")
                .default_value("1000"),
        )
        .arg(
            Arg::new("iterations")
                .long("iterations")
                .value_name("COUNT")
                .hide(true),
        )
}

fn json_command(name: &'static str) -> Command {
    Command::new(name).arg(
        Arg::new("json")
            .long("json")
            .help("Emit machine-readable JSON")
            .action(ArgAction::SetTrue),
    )
}

fn render_matches(
    matches: &ArgMatches,
    context: &CommandContext<InMemoryRegistry>,
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
            let repo = subcommand
                .get_one::<String>("repo")
                .cloned()
                .unwrap_or_else(|| "web".to_string());
            let title = subcommand
                .get_one::<String>("title")
                .cloned()
                .unwrap_or_else(|| "new task".to_string());
            let agent = subcommand
                .get_one::<String>("agent")
                .cloned()
                .unwrap_or_else(|| "codex".to_string());
            let plan =
                commands::new_task_plan(context, commands::NewTaskRequest { repo, title, agent })
                    .map_err(command_error)?;
            render_or_execute_plan(plan, subcommand)
        }
        Some(("open", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::open_task_plan(context, task, commands::OpenMode::Attach)
                .map_err(command_error)?;
            render_or_execute_plan(plan, subcommand)
        }
        Some(("trunk", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::trunk_task_plan(context, task).map_err(command_error)?;
            render_or_execute_plan(plan, subcommand)
        }
        Some(("check", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::check_task_plan(context, task).map_err(command_error)?;
            render_or_execute_plan(plan, subcommand)
        }
        Some(("diff", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::diff_task_plan(context, task).map_err(command_error)?;
            render_or_execute_plan(plan, subcommand)
        }
        Some(("merge", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::merge_task_plan(context, task).map_err(command_error)?;
            render_or_execute_plan(plan, subcommand)
        }
        Some(("clean", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::clean_task_plan(context, task).map_err(command_error)?;
            render_or_execute_plan(plan, subcommand)
        }
        Some(("sweep", subcommand)) => {
            render_or_execute_plan(commands::sweep_cleanup_plan(context), subcommand)
        }
        Some(("repair", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::repair_task_plan(context, task).map_err(command_error)?;
            render_or_execute_plan(plan, subcommand)
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
        Some(("doctor", subcommand)) => render_response(
            commands::doctor(context),
            subcommand.get_flag("json"),
            render_doctor_human,
        ),
        Some(("status", _)) => {
            render_response(commands::status(context), false, render_tasks_human)
        }
        Some(("cockpit", subcommand)) => render_cockpit_command(context, subcommand),
        Some(("reconcile", subcommand)) => render_response(
            ReconcileResponse {
                tasks_checked: 0,
                tasks_changed: 0,
            },
            subcommand.get_flag("json"),
            render_reconcile_human,
        ),
        Some((name, _)) => Ok(format!("{name}: command accepted; adapter wiring pending")),
        None => Ok("ajax: no command provided".to_string()),
    }
}

fn render_cockpit_command(
    context: &CommandContext<InMemoryRegistry>,
    matches: &ArgMatches,
) -> Result<String, CliError> {
    if matches.get_flag("textual") {
        return render_plan(textual_cockpit_plan(), false);
    }

    if matches.get_flag("json") {
        return render_response(commands::cockpit(context), true, |_| String::new());
    }

    let iterations = matches
        .get_one::<String>("iterations")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(1);
    let interval = matches
        .get_one::<String>("interval-ms")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(1000);

    if matches.get_flag("watch") {
        return Ok(render_cockpit_frames(
            context,
            iterations.max(1),
            Duration::from_millis(interval),
        ));
    }

    Ok(render_cockpit_frame(context))
}

fn textual_cockpit_plan() -> commands::CommandPlan {
    let mut plan = commands::CommandPlan::new("launch Textual cockpit");
    plan.commands.push(ajax_core::adapters::CommandSpec {
        program: "python3".to_string(),
        args: vec![
            textual_frontend_path(),
            "--ajax-bin".to_string(),
            "ajax".to_string(),
        ],
        cwd: None,
    });
    plan
}

fn textual_frontend_path() -> String {
    std::env::var("AJAX_TEXTUAL_APP").unwrap_or_else(|_| {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../frontends/textual/ajax_textual.py")
            .canonicalize()
            .unwrap_or_else(|_| {
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../frontends/textual/ajax_textual.py")
            });
        path.display().to_string()
    })
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
        &commands::review_queue(context),
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
            let request = new_task_request(subcommand);
            let plan = commands::new_task_plan(context, request.clone()).map_err(command_error)?;

            if !subcommand.get_flag("execute") {
                return Ok(RenderedCommand {
                    output: render_plan(plan, subcommand.get_flag("json"))?,
                    state_changed: false,
                });
            }

            let outputs = commands::execute_plan(&plan, subcommand.get_flag("yes"), runner)
                .map_err(command_error)?;
            let task = commands::record_new_task(context, &request).map_err(command_error)?;
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
        Some(("open", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::open_task_plan(context, task, commands::OpenMode::Attach)
                .map_err(command_error)?;
            if !subcommand.get_flag("execute") {
                return Ok(RenderedCommand {
                    output: render_plan(plan, subcommand.get_flag("json"))?,
                    state_changed: false,
                });
            }
            let outputs = commands::execute_plan(&plan, subcommand.get_flag("yes"), runner)
                .map_err(command_error)?;
            commands::mark_task_opened(context, task).map_err(command_error)?;
            Ok(RenderedCommand {
                output: render_execution_outputs(&outputs, None),
                state_changed: true,
            })
        }
        Some(("trunk", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::trunk_task_plan(context, task).map_err(command_error)?;
            if !subcommand.get_flag("execute") {
                return Ok(RenderedCommand {
                    output: render_plan(plan, subcommand.get_flag("json"))?,
                    state_changed: false,
                });
            }
            let outputs = commands::execute_plan(&plan, subcommand.get_flag("yes"), runner)
                .map_err(command_error)?;
            Ok(RenderedCommand {
                output: render_execution_outputs(&outputs, None),
                state_changed: false,
            })
        }
        Some(("check", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::check_task_plan(context, task).map_err(command_error)?;
            if !subcommand.get_flag("execute") {
                return Ok(RenderedCommand {
                    output: render_plan(plan, subcommand.get_flag("json"))?,
                    state_changed: false,
                });
            }
            let outputs = commands::execute_plan(&plan, subcommand.get_flag("yes"), runner)
                .map_err(command_error)?;
            Ok(RenderedCommand {
                output: render_execution_outputs(&outputs, None),
                state_changed: false,
            })
        }
        Some(("diff", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::diff_task_plan(context, task).map_err(command_error)?;
            if !subcommand.get_flag("execute") {
                return Ok(RenderedCommand {
                    output: render_plan(plan, subcommand.get_flag("json"))?,
                    state_changed: false,
                });
            }
            let outputs = commands::execute_plan(&plan, subcommand.get_flag("yes"), runner)
                .map_err(command_error)?;
            Ok(RenderedCommand {
                output: render_execution_outputs(&outputs, None),
                state_changed: false,
            })
        }
        Some(("merge", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::merge_task_plan(context, task).map_err(command_error)?;
            if !subcommand.get_flag("execute") {
                return Ok(RenderedCommand {
                    output: render_plan(plan, subcommand.get_flag("json"))?,
                    state_changed: false,
                });
            }
            let outputs = commands::execute_plan(&plan, subcommand.get_flag("yes"), runner)
                .map_err(command_error)?;
            commands::mark_task_merged(context, task).map_err(command_error)?;
            Ok(RenderedCommand {
                output: render_execution_outputs(&outputs, None),
                state_changed: true,
            })
        }
        Some(("clean", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::clean_task_plan(context, task).map_err(command_error)?;
            if !subcommand.get_flag("execute") {
                return Ok(RenderedCommand {
                    output: render_plan(plan, subcommand.get_flag("json"))?,
                    state_changed: false,
                });
            }
            let outputs = commands::execute_plan(&plan, subcommand.get_flag("yes"), runner)
                .map_err(command_error)?;
            commands::mark_task_removed(context, task).map_err(command_error)?;
            Ok(RenderedCommand {
                output: render_execution_outputs(&outputs, None),
                state_changed: true,
            })
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
            let outputs = commands::execute_plan(&plan, subcommand.get_flag("yes"), runner)
                .map_err(command_error)?;
            for candidate in &candidates {
                commands::mark_task_removed(context, candidate).map_err(command_error)?;
            }
            Ok(RenderedCommand {
                output: render_execution_outputs(&outputs, None),
                state_changed: !candidates.is_empty(),
            })
        }
        Some(("repair", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::repair_task_plan(context, task).map_err(command_error)?;
            if !subcommand.get_flag("execute") {
                return Ok(RenderedCommand {
                    output: render_plan(plan, subcommand.get_flag("json"))?,
                    state_changed: false,
                });
            }
            let outputs = commands::execute_plan(&plan, subcommand.get_flag("yes"), runner)
                .map_err(command_error)?;
            Ok(RenderedCommand {
                output: render_execution_outputs(&outputs, None),
                state_changed: false,
            })
        }
        Some(("cockpit", subcommand)) if subcommand.get_flag("textual") => {
            let plan = textual_cockpit_plan();
            if !subcommand.get_flag("execute") {
                return Ok(RenderedCommand {
                    output: render_plan(plan, false)?,
                    state_changed: false,
                });
            }
            let outputs = commands::execute_plan(&plan, false, runner).map_err(command_error)?;
            Ok(RenderedCommand {
                output: render_execution_outputs(&outputs, None),
                state_changed: false,
            })
        }
        _ => Ok(RenderedCommand {
            output: render_matches(matches, context)?,
            state_changed: false,
        }),
    }
}

fn default_context_paths() -> Result<CliContextPaths, CliError> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| CliError::ContextLoad("HOME is not set".to_string()))?;
    let defaults = ConfigPaths::for_home(home);
    let config_file = std::env::var_os("AJAX_CONFIG")
        .map(PathBuf::from)
        .unwrap_or(defaults.config_file);
    let state_file = std::env::var_os("AJAX_STATE")
        .map(PathBuf::from)
        .unwrap_or(defaults.state_db);

    Ok(CliContextPaths {
        config_file,
        state_file,
    })
}

fn load_context(paths: &CliContextPaths) -> Result<CommandContext<InMemoryRegistry>, CliError> {
    let config = if paths.config_file.exists() {
        let contents = std::fs::read_to_string(&paths.config_file)
            .map_err(|error| CliError::ContextLoad(error.to_string()))?;
        Config::from_toml_str(&contents)
            .map_err(|error| CliError::ContextLoad(format!("config parse failed: {error:?}")))?
    } else {
        Config::default()
    };
    let registry = if paths.state_file.exists() {
        InMemoryRegistry::load_json_snapshot(&paths.state_file)
            .map_err(|error| CliError::ContextLoad(format!("state load failed: {error:?}")))?
    } else {
        InMemoryRegistry::default()
    };

    Ok(CommandContext::new(config, registry))
}

fn save_context(
    paths: &CliContextPaths,
    context: &CommandContext<InMemoryRegistry>,
) -> Result<(), CliError> {
    if let Some(parent) = paths.state_file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| CliError::ContextSave(error.to_string()))?;
    }
    context
        .registry
        .save_json_snapshot(&paths.state_file)
        .map_err(|error| CliError::ContextSave(format!("state save failed: {error:?}")))
}

fn render_response<T: Serialize>(
    response: T,
    json: bool,
    human: fn(&T) -> String,
) -> Result<String, CliError> {
    if json {
        serde_json::to_string_pretty(&response)
            .map_err(|error| CliError::JsonSerialization(error.to_string()))
    } else {
        Ok(human(&response))
    }
}

fn render_plan(plan: commands::CommandPlan, json: bool) -> Result<String, CliError> {
    render_response(plan, json, render_plan_human)
}

fn render_or_execute_plan(
    plan: commands::CommandPlan,
    matches: &ArgMatches,
) -> Result<String, CliError> {
    if !matches.get_flag("execute") {
        return render_plan(plan, matches.get_flag("json"));
    }

    let mut runner = ProcessCommandRunner;
    let outputs = commands::execute_plan(&plan, matches.get_flag("yes"), &mut runner)
        .map_err(command_error)?;
    Ok(render_execution_outputs(&outputs, None))
}

fn render_execution_outputs(
    outputs: &[ajax_core::adapters::CommandOutput],
    recorded_task: Option<&str>,
) -> String {
    let mut lines = outputs
        .iter()
        .map(|output| {
            format!(
                "exit:{}\nstdout:{}\nstderr:{}",
                output.status_code, output.stdout, output.stderr
            )
        })
        .collect::<Vec<_>>();

    if let Some(task) = recorded_task {
        lines.push(format!("recorded task: {task}"));
    }

    lines.join("\n")
}

fn new_task_request(matches: &ArgMatches) -> commands::NewTaskRequest {
    let repo = matches
        .get_one::<String>("repo")
        .cloned()
        .unwrap_or_else(|| "web".to_string());
    let title = matches
        .get_one::<String>("title")
        .cloned()
        .unwrap_or_else(|| "new task".to_string());
    let agent = matches
        .get_one::<String>("agent")
        .cloned()
        .unwrap_or_else(|| "codex".to_string());

    commands::NewTaskRequest { repo, title, agent }
}

fn task_arg(matches: &ArgMatches) -> Result<&str, CliError> {
    matches
        .get_one::<String>("task")
        .map(String::as_str)
        .ok_or_else(|| CliError::CommandFailed("task argument is required".to_string()))
}

fn command_error(error: CommandError) -> CliError {
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
            CliError::CommandFailed(format!("command failed: {error:?}"))
        }
        CommandError::Registry(error) => {
            CliError::CommandFailed(format!("registry update failed: {error:?}"))
        }
    }
}

fn render_repos_human(response: &ReposResponse) -> String {
    response
        .repos
        .iter()
        .map(|repo| {
            format!(
                "{}\t{}\tactive:{} reviewable:{} cleanable:{} broken:{}",
                repo.name,
                repo.path,
                repo.active_tasks,
                repo.reviewable_tasks,
                repo.cleanable_tasks,
                repo.broken_tasks
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_tasks_human(response: &TasksResponse) -> String {
    response
        .tasks
        .iter()
        .map(render_task_summary)
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_task_summary(task: &TaskSummary) -> String {
    format!(
        "{}\t{}\t{}",
        task.qualified_handle, task.lifecycle_status, task.title
    )
}

fn render_inspect_human(response: &InspectResponse) -> String {
    format!(
        "{}\nbranch: {}\nworktree: {}\ntmux: {}\nflags: {}",
        render_task_summary(&response.task),
        response.branch,
        response.worktree_path,
        response.tmux_session,
        response.flags.join(", ")
    )
}

fn render_inbox_human(response: &InboxResponse) -> String {
    response
        .items
        .iter()
        .map(render_attention_item_human)
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_next_human(response: &NextResponse) -> String {
    response
        .item
        .as_ref()
        .map(render_attention_item_human)
        .unwrap_or_else(|| "no tasks need attention".to_string())
}

fn render_attention_item_human(item: &ajax_core::models::AttentionItem) -> String {
    format!(
        "{}: {} -> {}",
        item.task_handle, item.reason, item.recommended_action
    )
}

fn render_reconcile_human(response: &ReconcileResponse) -> String {
    format!(
        "checked:{} changed:{}",
        response.tasks_checked, response.tasks_changed
    )
}

fn render_doctor_human(response: &DoctorResponse) -> String {
    response
        .checks
        .iter()
        .map(|check| format!("{}\t{}\t{}", check.name, check.ok, check.message))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_plan_human(plan: &commands::CommandPlan) -> String {
    let mut lines = vec![plan.title.clone()];

    if plan.requires_confirmation {
        lines.push("requires confirmation".to_string());
    }

    lines.extend(
        plan.blocked_reasons
            .iter()
            .map(|reason| format!("blocked: {reason}")),
    );
    lines.extend(plan.commands.iter().map(|command| {
        if let Some(cwd) = &command.cwd {
            format!(
                "$ (cd {} && {} {})",
                cwd,
                command.program,
                command.args.join(" ")
            )
        } else {
            format!("$ {} {}", command.program, command.args.join(" "))
        }
    }));

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::{
        build_cli, run_with_context, run_with_context_and_runner, run_with_context_paths,
        run_with_context_paths_and_runner, CliContextPaths,
    };
    use ajax_core::{
        adapters::{
            CommandOutput, CommandRunError, CommandRunner, CommandSpec, RecordingCommandRunner,
        },
        commands::CommandContext,
        config::{Config, ManagedRepo},
        models::{AgentClient, GitStatus, LifecycleStatus, SideFlag, Task, TaskId},
        registry::{InMemoryRegistry, Registry},
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

    fn output(status_code: i32, stdout: &str) -> CommandOutput {
        CommandOutput {
            status_code,
            stdout: stdout.to_string(),
            stderr: String::new(),
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
            vec!["ajax", "repair", "web/fix-login"],
            vec!["ajax", "next"],
            vec!["ajax", "inbox"],
            vec!["ajax", "review"],
            vec!["ajax", "status"],
            vec!["ajax", "doctor"],
            vec!["ajax", "reconcile"],
            vec!["ajax", "cockpit"],
            vec!["ajax", "cockpit", "--textual"],
        ] {
            let matches = build_cli().try_get_matches_from(args.clone());
            assert!(matches.is_ok(), "{args:?} should parse");
        }
    }

    #[test]
    fn cockpit_command_renders_dashboard_from_backend_state() {
        let context = sample_context();
        let output = run_with_context(["ajax", "cockpit"], &context).unwrap();

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
    }

    #[test]
    fn cockpit_textual_renders_launch_plan() {
        let context = sample_context();
        let output = run_with_context(["ajax", "cockpit", "--textual"], &context).unwrap();

        assert!(output.contains("launch Textual cockpit"));
        assert!(output.contains("python3"));
        assert!(output.contains("frontends/textual/ajax_textual.py"));
        assert!(output.contains("--ajax-bin ajax"));
    }

    #[test]
    fn cockpit_textual_execute_uses_injected_runner() {
        let mut context = sample_context();
        let mut runner = RecordingCommandRunner::default();

        run_with_context_and_runner(
            ["ajax", "cockpit", "--textual", "--execute"],
            &mut context,
            &mut runner,
        )
        .unwrap();

        assert_eq!(runner.commands().len(), 1);
        assert_eq!(runner.commands()[0].program, "python3");
        assert!(runner.commands()[0]
            .args
            .iter()
            .any(|arg| arg.ends_with("frontends/textual/ajax_textual.py")));
        assert!(runner.commands()[0]
            .args
            .contains(&"--ajax-bin".to_string()));
        assert!(runner.commands()[0].args.contains(&"ajax".to_string()));
    }

    #[test]
    fn help_output_is_successful() {
        let context = sample_context();
        let output = run_with_context(["ajax", "--help"], &context).unwrap();

        assert!(output.contains("Usage: ajax [COMMAND]"));
        assert!(output.contains("Commands:"));
    }

    #[test]
    fn reconcile_command_supports_json_output() {
        let matches = build_cli().try_get_matches_from(["ajax", "reconcile", "--json"]);

        assert!(matches.is_ok());
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
            ["ajax", "doctor", "--json", ""],
            ["ajax", "cockpit", "--json", ""],
        ] {
            let filtered_args = args.into_iter().filter(|arg| !arg.is_empty());
            let matches = build_cli().try_get_matches_from(filtered_args);
            assert!(matches.is_ok(), "{args:?} should parse");
        }
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
            vec!["ajax", "repair", "web/fix-login", "--execute"],
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
            vec!["ajax", "repair"],
        ] {
            let error = run_with_context(args.clone(), &sample_context()).unwrap_err();
            assert!(
                matches!(error, super::CliError::CommandFailed(ref message) if message.contains("required")),
                "{args:?} should require task arg, got {error:?}"
            );
        }
    }

    #[test]
    fn textual_frontend_replaces_legacy_shell_example() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let textual_app = root.join("frontends/textual/ajax_textual.py");
        let textual_client = root.join("frontends/textual/ajax_textual_client.py");
        let pyproject = root.join("frontends/textual/pyproject.toml");
        let readme = std::fs::read_to_string(root.join("README.md")).unwrap();
        let app = std::fs::read_to_string(textual_app).unwrap();
        let client = std::fs::read_to_string(textual_client).unwrap();
        let pyproject = std::fs::read_to_string(pyproject).unwrap();

        for command in [
            "from textual.app import App",
            "snapshot = self.client.snapshot()",
            "repos = snapshot.repos",
            "tasks = snapshot.tasks",
            "inbox = snapshot.inbox",
            "review = snapshot.review",
        ] {
            assert!(app.contains(command), "missing {command}");
        }
        assert!(client.contains("json_command(\"cockpit\")"));
        assert!(pyproject.contains("textual"));
        assert!(readme.contains("Textual"));
    }

    #[test]
    fn textual_frontend_uses_mobile_first_stacked_sections() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let app = std::fs::read_to_string(root.join("frontends/textual/ajax_textual.py")).unwrap();
        let layout =
            std::fs::read_to_string(root.join("frontends/textual/ajax_textual_layout.py")).unwrap();

        for expected in [
            "ListView",
            "ListItem",
            "SelectionRow",
            "on_list_view_selected",
            "build_selection_rows",
            "build_dashboard_sections",
            "viewport_layout",
            "Screen.compact #body",
            "ListItem.urgent",
            "ListItem.review",
            "#details",
        ] {
            assert!(app.contains(expected), "missing {expected}");
        }
        for expected in [
            "DashboardSection",
            "\"Attention\"",
            "\"Review\"",
            "\"Active\"",
            "\"Repos\"",
            "label=\"attention\"",
            "tone=\"urgent\"",
        ] {
            assert!(layout.contains(expected), "missing {expected}");
        }

        assert!(!app.contains("DataTable"));
        assert!(!app.contains("Horizontal"));
        assert!(!app.contains("action_new_task_help"));
        assert!(!app.contains("n create task"));
        assert!(!layout.contains("-- Repos --"));
    }

    #[test]
    fn textual_startup_script_launches_built_ajax_binary() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let script = std::fs::read_to_string(root.join("scripts/start-ajax-textual.sh")).unwrap();

        for expected in [
            "AJAX_TEXTUAL_VENV",
            "cargo build",
            "python3 -m venv",
            "target/debug/ajax",
            "frontends/textual/ajax_textual.py",
            "--ajax-bin",
        ] {
            assert!(script.contains(expected), "missing {expected}");
        }

        assert!(!script.contains("gum"));
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
                "fix login",
                "--agent",
                "codex",
            ],
            &sample_context(),
        )
        .unwrap();

        assert!(output.contains("create task: fix login"));
        assert!(output.contains("workmux add --repo web"));
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

        assert!(output.contains("$ workmux open web/fix-login"));
        assert!(output.contains("$ tmux attach-session -t ajax-web-fix-login"));
    }

    #[test]
    fn merge_command_renders_json_plan() {
        let context = sample_context();
        let output =
            run_with_context(["ajax", "merge", "web/fix-login", "--json"], &context).unwrap();

        assert!(output.contains("\"requires_confirmation\": true"));
        assert!(output.contains("workmux"));
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
        let state_file = directory.join("state.json");
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
        sample_context()
            .registry
            .save_json_snapshot(&state_file)
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
        let state_file = directory.join("missing-state.json");

        let output = run_with_context_paths(
            ["ajax", "tasks", "--json"],
            &CliContextPaths::new(&config_file, &state_file),
        )
        .unwrap();

        assert!(output.contains("\"tasks\": []"));
        assert!(!output.contains("web/fix-login"));
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
        assert_eq!(runner.commands().len(), 1);
        assert!(context
            .registry
            .list_tasks()
            .iter()
            .any(|task| task.qualified_handle() == "web/fix-login"));
    }

    #[test]
    fn new_execute_saves_registry_snapshot_to_state_file() {
        let directory = std::env::temp_dir().join(format!(
            "ajax-cli-new-execute-{}-{}",
            std::process::id(),
            "state"
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let config_file = directory.join("config.toml");
        let state_file = directory.join("state.json");
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
        let restored = InMemoryRegistry::load_json_snapshot(&state_file).unwrap();

        std::fs::remove_dir_all(Path::new(&directory)).unwrap();
        assert!(output.contains("recorded task: web/fix-login"));
        assert!(restored
            .list_tasks()
            .iter()
            .any(|task| task.qualified_handle() == "web/fix-login"));
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

        assert!(
            matches!(error, super::CliError::CommandFailed(message) if message.contains("NonZeroExit"))
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
            &[CommandSpec::new(
                "tmux",
                [
                    "new-window",
                    "-t",
                    "ajax-web-fix-login",
                    "-n",
                    "worktrunk",
                    "-c",
                    "/tmp/worktrees/web-fix-login"
                ]
            )]
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
    fn repair_execute_uses_injected_runner() {
        let mut context = sample_context();
        let mut task = context
            .registry
            .get_task(&TaskId::new("task-1"))
            .cloned()
            .unwrap();
        task.add_side_flag(SideFlag::WorktrunkMissing);
        context.registry = InMemoryRegistry::default();
        context.registry.create_task(task).unwrap();
        let mut runner = RecordingCommandRunner::default();

        run_with_context_and_runner(
            ["ajax", "repair", "web/fix-login", "--execute"],
            &mut context,
            &mut runner,
        )
        .unwrap();

        assert_eq!(
            runner.commands(),
            &[CommandSpec::new(
                "tmux",
                [
                    "new-window",
                    "-t",
                    "ajax-web-fix-login",
                    "-n",
                    "worktrunk",
                    "-c",
                    "/tmp/worktrees/web-fix-login"
                ]
            )]
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
            &[CommandSpec::new("workmux", ["remove", "web/fix-login"])]
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
    fn reconcile_saves_registry_snapshot_when_task_changes() {
        let directory = std::env::temp_dir().join(format!(
            "ajax-cli-reconcile-{}-{}",
            std::process::id(),
            "state"
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let config_file = directory.join("config.toml");
        let state_file = directory.join("state.json");
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
        sample_context()
            .registry
            .save_json_snapshot(&state_file)
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
        let restored = InMemoryRegistry::load_json_snapshot(&state_file).unwrap();

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

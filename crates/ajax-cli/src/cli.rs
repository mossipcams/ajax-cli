use clap::error::ErrorKind;
use clap::{Arg, ArgAction, ArgMatches, Command};
use std::ffi::OsString;

use crate::CliError;

pub enum ParsedArgs {
    Matches(ArgMatches),
    Message(String),
}

pub fn build_cli() -> Command {
    Command::new("ajax")
        .about("Semi-agentic operator console for isolated AI coding tasks")
        .subcommand(repos_command())
        .subcommand(tasks_command())
        .subcommand(task_command("inspect"))
        .subcommand(executable_new_command("start"))
        .subcommand(executable_task_command("resume"))
        .subcommand(executable_task_command("repair"))
        .subcommand(executable_task_command("review"))
        .subcommand(executable_task_command("ship"))
        .subcommand(executable_task_command("drop"))
        .subcommand(executable_command(
            json_command("tidy").about("Clean safe task environments across repos"),
        ))
        .subcommand(json_command("next").about("Show the next task needing attention"))
        .subcommand(json_command("inbox").about("Show global attention inbox"))
        .subcommand(json_command("ready").about("Show tasks ready for review"))
        .subcommand(json_command("status").about("Show Ajax status"))
        .subcommand(state_command())
        .subcommand(json_command("doctor").about("Check local Ajax dependencies and health"))
        .subcommand(supervise_command())
        .subcommand(cockpit_command())
}

pub fn parse_args<I, T>(args: I) -> Result<ParsedArgs, CliError>
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

fn repos_command() -> Command {
    json_command("repos").about("List configured repos")
}

fn tasks_command() -> Command {
    json_command("tasks")
        .about("List task environments")
        .arg(Arg::new("repo").long("repo").value_name("REPO"))
}

fn executable_new_command(name: &'static str) -> Command {
    executable_command(json_command(name))
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

fn supervise_command() -> Command {
    json_command("supervise")
        .about("Run Codex under the Ajax live supervisor")
        .arg(Arg::new("task").long("task").value_name("REPO/HANDLE"))
        .arg(
            Arg::new("prompt")
                .long("prompt")
                .value_name("PROMPT")
                .required(true),
        )
        .arg(
            Arg::new("codex-bin")
                .long("codex-bin")
                .value_name("PATH")
                .hide(true),
        )
}

fn state_command() -> Command {
    Command::new("state")
        .about("Manage Ajax durable state")
        .subcommand(
            Command::new("export")
                .about("Export the current registry state as JSON")
                .arg(
                    Arg::new("output")
                        .long("output")
                        .value_name("PATH")
                        .required(true),
                ),
        )
}

fn cockpit_command() -> Command {
    Command::new("cockpit")
        .about("Render the Ajax operator cockpit")
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

use clap::error::ErrorKind;
use clap::{Arg, ArgAction, ArgMatches, Command};
use std::ffi::OsString;

use crate::CliError;

pub enum ParsedArgs {
    Matches(ArgMatches),
    Message(String),
}

pub fn build_cli() -> Command {
    Command::new("ajax-cli")
        .about("Semi-agentic operator console for isolated AI coding tasks")
        .arg(Arg::new("profile").long("profile").value_name("NAME"))
        .arg(Arg::new("home").long("home").value_name("PATH"))
        .arg(Arg::new("config").long("config").value_name("PATH"))
        .arg(Arg::new("state").long("state").value_name("PATH"))
        .arg(
            Arg::new("worktree-root")
                .long("worktree-root")
                .value_name("PATH"),
        )
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
        .subcommand(json_command("runtime").about("Show Ajax runtime paths"))
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

#[cfg(test)]
mod tests {
    use super::build_cli;

    #[test]
    fn global_profile_is_accepted_before_runtime_subcommand() {
        let matches = build_cli()
            .try_get_matches_from(["ajax", "--profile", "dev", "runtime"])
            .unwrap();

        assert_eq!(matches.get_one::<String>("profile").unwrap(), "dev");
        assert_eq!(matches.subcommand_name(), Some("runtime"));
    }

    #[test]
    fn global_home_is_accepted_before_subcommands() {
        let matches = build_cli()
            .try_get_matches_from(["ajax", "--home", "/tmp/ajax-dev", "status"])
            .unwrap();

        assert_eq!(matches.get_one::<String>("home").unwrap(), "/tmp/ajax-dev");
        assert_eq!(matches.subcommand_name(), Some("status"));
    }

    #[test]
    fn global_direct_path_overrides_are_accepted() {
        let matches = build_cli()
            .try_get_matches_from([
                "ajax",
                "--config",
                "/tmp/config.toml",
                "--state",
                "/tmp/ajax.db",
                "--worktree-root",
                "/tmp/worktrees",
                "runtime",
            ])
            .unwrap();

        assert_eq!(
            matches.get_one::<String>("config").unwrap(),
            "/tmp/config.toml"
        );
        assert_eq!(matches.get_one::<String>("state").unwrap(), "/tmp/ajax.db");
        assert_eq!(
            matches.get_one::<String>("worktree-root").unwrap(),
            "/tmp/worktrees"
        );
    }

    #[test]
    fn runtime_command_accepts_json_flag() {
        let matches = build_cli()
            .try_get_matches_from(["ajax", "runtime", "--json"])
            .unwrap();
        let (_, subcommand) = matches.subcommand().unwrap();

        assert!(subcommand.get_flag("json"));
    }
}

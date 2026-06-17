use crate::{
    task_session::{TaskSessionContext, TaskSessionEnd, TaskSessionRunner},
    CliError,
};
use ajax_core::{
    adapters::{CommandOutput, CommandRunner, CommandSpec, ProcessCommandRunner},
    commands::OpenMode,
};
use clap::{Arg, ArgAction, Command};
use std::{ffi::OsString, io::Write, path::PathBuf};

pub(crate) const BGTMUX_TRACE_ENV: &str = "AJAX_TASK_SESSION_TRACE";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BgtmuxArgs {
    session: String,
    fixture: bool,
    trace: Option<PathBuf>,
}

pub(crate) fn run_bgtmux_with_runner(
    args: impl IntoIterator<Item = impl Into<OsString> + Clone>,
    writer: &mut impl Write,
    task_session: &mut impl TaskSessionRunner,
) -> Result<(), CliError> {
    run_bgtmux_with_runner_and_open_mode(args, writer, task_session, crate::current_open_mode())
}

pub(crate) fn run_bgtmux_with_runner_and_open_mode(
    args: impl IntoIterator<Item = impl Into<OsString> + Clone>,
    writer: &mut impl Write,
    task_session: &mut impl TaskSessionRunner,
    open_mode: OpenMode,
) -> Result<(), CliError> {
    let args = parse_bgtmux_args(args)?;
    if args.fixture {
        let mut runner = ProcessCommandRunner;
        execute_bgtmux_fixture(&args.session, &mut runner)?;
    }
    let _trace_guard = ScopedEnvVar::set(BGTMUX_TRACE_ENV, args.trace.as_ref());
    let command = bgtmux_command(&args.session, open_mode);
    match task_session.run_task_session(
        &command,
        &TaskSessionContext {
            new_task_repo: None,
        },
    )? {
        TaskSessionEnd::Normal => {}
        TaskSessionEnd::OpenNewTask => {
            return Err(CliError::CommandFailed(
                "bgtmux does not support opening a new Ajax task".to_string(),
            ));
        }
    }
    writeln!(writer, "bgtmux detached from {}", args.session)
        .map_err(|error| CliError::CommandFailed(error.to_string()))
}

pub(crate) fn parse_bgtmux_args(
    args: impl IntoIterator<Item = impl Into<OsString> + Clone>,
) -> Result<BgtmuxArgs, CliError> {
    let matches = bgtmux_cli()
        .try_get_matches_from(args)
        .map_err(|error| CliError::CommandFailed(error.to_string()))?;
    let session = matches
        .get_one::<String>("session")
        .cloned()
        .ok_or_else(|| CliError::CommandFailed("tmux session is required".to_string()))?;
    Ok(BgtmuxArgs {
        session,
        fixture: matches.get_flag("fixture"),
        trace: matches.get_one::<PathBuf>("trace").cloned(),
    })
}

fn bgtmux_cli() -> Command {
    Command::new("bgtmux")
        .about("Attach to a tmux session through Ajax's task-session bridge")
        .arg(
            Arg::new("fixture")
                .long("fixture")
                .help("Create a deterministic visual stress session before attaching")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("trace")
                .long("trace")
                .value_name("PATH")
                .value_parser(clap::value_parser!(PathBuf)),
        )
        .arg(Arg::new("session").value_name("SESSION").required(true))
}

pub(crate) fn bgtmux_attach_command(session: &str) -> CommandSpec {
    CommandSpec::new("tmux", ["attach-session", "-t", session])
}

pub(crate) fn bgtmux_command(session: &str, open_mode: OpenMode) -> CommandSpec {
    match open_mode {
        OpenMode::Attach => bgtmux_attach_command(session),
        OpenMode::SwitchClient => CommandSpec::new("tmux", ["switch-client", "-t", session]),
        OpenMode::NoAttach => bgtmux_attach_command(session),
    }
}

pub(crate) fn bgtmux_fixture_commands(session: &str) -> Vec<CommandSpec> {
    let script = bgtmux_fixture_script();
    vec![
        CommandSpec::new("tmux", ["kill-session", "-t", session]),
        CommandSpec::new(
            "tmux",
            ["new-session", "-d", "-s", session, "sh", "-lc", &script],
        ),
    ]
}

fn bgtmux_fixture_script() -> String {
    [
        "printf '\\033[2J\\033[H'",
        "printf 'AJAX BGTMUX VISUAL FIXTURE\\n'",
        "printf 'top border: +----------------------------------------------------------+\\n'",
        "i=0",
        "while :; do",
        "  i=$((i + 1))",
        "  printf '\\033[H'",
        "  printf 'AJAX BGTMUX VISUAL FIXTURE frame=%04d size-sensitive redraw\\n' \"$i\"",
        "  printf 'wide line: 0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ wraps here -> %04d\\n' \"$i\"",
        "  printf 'colors: \\033[31mred\\033[0m \\033[32mgreen\\033[0m \\033[34mblue\\033[0m inverse=\\033[7mON\\033[0m\\n'",
        "  printf 'cursor row marker: [%04d]                                                 \\n' \"$i\"",
        "  printf 'bottom border: +-------------------------------------------------------+\\n'",
        "  sleep 1",
        "done",
    ]
    .join("; ")
}

struct ScopedEnvVar {
    name: &'static str,
    previous: Option<OsString>,
}

impl ScopedEnvVar {
    fn set(name: &'static str, value: Option<&PathBuf>) -> Self {
        let previous = std::env::var_os(name);
        match value {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
        Self { name, previous }
    }
}

impl Drop for ScopedEnvVar {
    fn drop(&mut self) {
        match self.previous.as_ref() {
            Some(value) => std::env::set_var(self.name, value),
            None => std::env::remove_var(self.name),
        }
    }
}

#[allow(dead_code)]
fn execute_bgtmux_fixture(
    session: &str,
    runner: &mut impl CommandRunner,
) -> Result<Vec<CommandOutput>, CliError> {
    bgtmux_fixture_commands(session)
        .into_iter()
        .map(|command| {
            runner
                .run(&command)
                .map_err(|error| CliError::CommandFailed(error.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        bgtmux_attach_command, bgtmux_command, parse_bgtmux_args, run_bgtmux_with_runner,
        run_bgtmux_with_runner_and_open_mode, BgtmuxArgs,
    };
    use crate::task_session::{TaskSessionContext, TaskSessionEnd, TaskSessionRunner};
    use ajax_core::adapters::CommandSpec;
    use ajax_core::commands::OpenMode;
    use std::path::PathBuf;

    #[derive(Default)]
    struct RecordingTaskSessionRunner {
        commands: Vec<CommandSpec>,
        contexts: Vec<TaskSessionContext>,
    }

    impl TaskSessionRunner for RecordingTaskSessionRunner {
        fn run_task_session(
            &mut self,
            command: &CommandSpec,
            context: &TaskSessionContext,
        ) -> Result<TaskSessionEnd, crate::CliError> {
            self.commands.push(command.clone());
            self.contexts.push(context.clone());
            Ok(TaskSessionEnd::Normal)
        }
    }

    #[test]
    fn bgtmux_args_parse_required_session_and_trace_path() {
        assert_eq!(
            parse_bgtmux_args([
                "bgtmux",
                "--trace",
                "/tmp/ajax-task-session.trace",
                "ajax-web-fix-login",
            ])
            .unwrap(),
            BgtmuxArgs {
                session: "ajax-web-fix-login".to_string(),
                fixture: false,
                trace: Some(PathBuf::from("/tmp/ajax-task-session.trace")),
            }
        );
    }

    #[test]
    fn bgtmux_attach_command_targets_tmux_session() {
        assert_eq!(
            bgtmux_attach_command("ajax-web-fix-login"),
            CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
        );
    }

    #[test]
    fn bgtmux_command_switches_client_when_launched_inside_tmux() {
        assert_eq!(
            bgtmux_command("ajax-web-fix-login", OpenMode::SwitchClient),
            CommandSpec::new("tmux", ["switch-client", "-t", "ajax-web-fix-login"])
        );
    }

    #[test]
    fn bgtmux_run_attaches_through_task_session_runner() {
        let mut runner = RecordingTaskSessionRunner::default();
        let mut output = Vec::new();

        run_bgtmux_with_runner_and_open_mode(
            ["bgtmux", "ajax-web-fix-login"],
            &mut output,
            &mut runner,
            OpenMode::Attach,
        )
        .unwrap();

        assert_eq!(
            runner.commands,
            vec![CommandSpec::new(
                "tmux",
                ["attach-session", "-t", "ajax-web-fix-login"]
            )]
        );
        assert_eq!(
            runner.contexts,
            vec![TaskSessionContext {
                new_task_repo: None
            }]
        );
    }

    #[test]
    fn bgtmux_run_uses_switch_client_open_mode_inside_tmux() {
        let mut runner = RecordingTaskSessionRunner::default();
        let mut output = Vec::new();

        run_bgtmux_with_runner_and_open_mode(
            ["bgtmux", "ajax-web-fix-login"],
            &mut output,
            &mut runner,
            OpenMode::SwitchClient,
        )
        .unwrap();

        assert_eq!(
            runner.commands,
            vec![CommandSpec::new(
                "tmux",
                ["switch-client", "-t", "ajax-web-fix-login"]
            )]
        );
    }

    #[test]
    fn bgtmux_fixture_plans_visual_stress_session_before_attach() {
        let args = parse_bgtmux_args(["bgtmux", "--fixture", "ajax-visual-check"]).unwrap();

        let setup = super::bgtmux_fixture_commands(&args.session);

        assert_eq!(setup.len(), 2);
        assert_eq!(
            setup[0],
            CommandSpec::new("tmux", ["kill-session", "-t", "ajax-visual-check"])
        );
        assert_eq!(setup[1].program, "tmux");
        assert!(setup[1].args.contains(&"new-session".to_string()));
        assert!(setup[1].args.contains(&"ajax-visual-check".to_string()));
        assert!(
            setup[1]
                .args
                .iter()
                .any(|arg| arg.contains("AJAX BGTMUX VISUAL FIXTURE")),
            "{setup:?}"
        );
    }

    #[test]
    fn bgtmux_trace_path_is_scoped_to_attach_run() {
        let mut runner = RecordingTaskSessionRunner::default();
        let mut output = Vec::new();
        std::env::remove_var(super::BGTMUX_TRACE_ENV);

        run_bgtmux_with_runner(
            [
                "bgtmux",
                "--trace",
                "/tmp/bgtmux.trace",
                "ajax-web-fix-login",
            ],
            &mut output,
            &mut runner,
        )
        .unwrap();

        assert_eq!(std::env::var_os(super::BGTMUX_TRACE_ENV), None);
    }
}

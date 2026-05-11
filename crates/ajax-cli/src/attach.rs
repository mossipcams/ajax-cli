use ajax_core::{
    adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec, TmuxAdapter},
    commands::{self, CommandContext},
    registry::InMemoryRegistry,
};

use crate::{command_error, CliError};

pub(crate) fn attach_task<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    qualified_handle: &str,
) -> Result<Vec<CommandOutput>, CliError> {
    let response = commands::inspect_task(context, qualified_handle).map_err(command_error)?;
    let tmux = TmuxAdapter::new("tmux");
    let mut outputs = Vec::new();
    let current_session = detect_current_session(runner, &tmux);

    if let Some(origin_session) =
        current_session.filter(|session| session != &response.tmux_session)
    {
        outputs.push(run_required(
            runner,
            &tmux.bind_ajax_return_to_session_key(&origin_session),
        )?);
        match run_required(runner, &tmux.switch_client(&response.tmux_session)) {
            Ok(output) => outputs.push(output),
            Err(error) => {
                run_required(runner, &tmux.unbind_ajax_detach_key())?;
                return Err(error);
            }
        }
        commands::mark_task_opened(context, qualified_handle).map_err(command_error)?;
        return Ok(outputs);
    }

    outputs.push(run_required(runner, &tmux.bind_ajax_detach_key())?);
    let attach_result = run_required(runner, &tmux.attach_session(&response.tmux_session));
    let cleanup_result = run_required(runner, &tmux.unbind_ajax_detach_key());

    match (attach_result, cleanup_result) {
        (Ok(output), Ok(cleanup_output)) => {
            outputs.push(output);
            outputs.push(cleanup_output);
        }
        (Err(error), Ok(cleanup_output)) => {
            outputs.push(cleanup_output);
            return Err(error);
        }
        (Ok(output), Err(error)) => {
            outputs.push(output);
            return Err(error);
        }
        (Err(error), Err(_cleanup_error)) => return Err(error),
    }

    commands::mark_task_opened(context, qualified_handle).map_err(command_error)?;
    Ok(outputs)
}

fn run_required<R: CommandRunner>(
    runner: &mut R,
    command: &CommandSpec,
) -> Result<CommandOutput, CliError> {
    let output = runner
        .run(command)
        .map_err(|error| CliError::CommandFailed(format!("command failed: {error}")))?;
    if output.status_code != 0 {
        return Err(CliError::CommandFailed(format!(
            "command failed: {}",
            CommandRunError::NonZeroExit {
                program: command.program.clone(),
                status_code: output.status_code,
                stderr: output.stderr.clone(),
                cwd: command.cwd.clone(),
            }
        )));
    }
    Ok(output)
}

fn detect_current_session<R: CommandRunner>(runner: &mut R, tmux: &TmuxAdapter) -> Option<String> {
    let output = match runner.run(&tmux.current_session()) {
        Ok(output) => output,
        Err(_error) => return None,
    };
    if output.status_code != 0 {
        return None;
    }
    let session = output.stdout.trim();
    if session.is_empty() {
        return None;
    }
    Some(session.to_string())
}

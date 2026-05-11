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

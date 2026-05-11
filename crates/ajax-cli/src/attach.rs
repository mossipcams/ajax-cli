use ajax_core::{
    adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec, TmuxAdapter},
    commands::{self, CommandContext},
    models::Task,
    registry::{InMemoryRegistry, Registry},
};

use crate::{command_error, CliError};

pub(crate) fn attach_task<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    qualified_handle: &str,
) -> Result<Vec<CommandOutput>, CliError> {
    let _response = commands::inspect_task(context, qualified_handle).map_err(command_error)?;
    let task = task_for_handle(context, qualified_handle)?;
    let task_target = task_switch_target(&task);
    let tmux = TmuxAdapter::new("tmux");
    let mut outputs = Vec::new();
    let origin_target = detect_current_client_target(runner, &tmux);

    if let Some(origin_target) = origin_target.filter(|target| target != &task_target) {
        let return_channel = ajax_return_channel(&origin_target, &task_target);
        outputs.push(run_required(
            runner,
            &tmux.bind_ajax_return_to_target_key(&origin_target, &return_channel),
        )?);
        match run_required(runner, &tmux.switch_client(&task_target)) {
            Ok(output) => outputs.push(output),
            Err(error) => {
                run_required(runner, &tmux.unbind_ajax_detach_key())?;
                return Err(error);
            }
        }
        outputs.push(run_required(
            runner,
            &tmux.wait_for_ajax_return(&return_channel),
        )?);
        commands::mark_task_opened(context, qualified_handle).map_err(command_error)?;
        return Ok(outputs);
    }

    outputs.push(run_required(runner, &tmux.bind_ajax_detach_key())?);
    let attach_result = run_required(runner, &tmux.attach_session(&task.tmux_session));
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

fn task_for_handle(
    context: &CommandContext<InMemoryRegistry>,
    qualified_handle: &str,
) -> Result<Task, CliError> {
    context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
        .cloned()
        .ok_or_else(|| CliError::CommandFailed(format!("task not found: {qualified_handle}")))
}

fn task_switch_target(task: &Task) -> String {
    let window = task
        .worktrunk_status
        .as_ref()
        .filter(|status| status.exists && !status.window_name.trim().is_empty())
        .map_or(task.worktrunk_window.trim(), |status| {
            status.window_name.trim()
        });
    if window.is_empty() {
        task.tmux_session.clone()
    } else {
        format!("{}:{window}", task.tmux_session)
    }
}

fn detect_current_client_target<R: CommandRunner>(
    runner: &mut R,
    tmux: &TmuxAdapter,
) -> Option<String> {
    let output = match runner.run(&tmux.current_client_target()) {
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

fn ajax_return_channel(origin_session: &str, task_session: &str) -> String {
    format!(
        "ajax-return-{}-{}",
        sanitize_tmux_channel_part(origin_session),
        sanitize_tmux_channel_part(task_session)
    )
}

fn sanitize_tmux_channel_part(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => character,
            _ => '-',
        })
        .collect()
}

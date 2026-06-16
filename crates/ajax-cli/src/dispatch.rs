use ajax_core::{
    adapters::CommandRunner,
    commands::{self, CommandContext, CommandError},
    models::{LifecycleStatus, OperatorAction, Task},
    registry::{InMemoryRegistry, Registry},
    slices::{drop, repair, resume, review, ship},
};
use clap::ArgMatches;

use crate::{
    command_error,
    render::{render_execution_outputs, render_plan},
    task_arg, CliError, RenderedCommand,
};

pub(crate) fn render_task_command<R: CommandRunner>(
    action: OperatorAction,
    subcommand: &ArgMatches,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    open_mode: commands::OpenMode,
) -> Result<RenderedCommand, CliError> {
    let task = task_arg(subcommand)?;
    let execute = subcommand.get_flag("execute");
    let confirmed = subcommand.get_flag("yes");
    let mut state_changed = false;
    if matches!(
        action,
        OperatorAction::Resume | OperatorAction::Review | OperatorAction::Repair
    ) {
        if let Ok(changed) = commands::refresh_git_substrate_evidence(context, runner) {
            state_changed |= changed;
        }
    }
    let plan = plan_task_action(context, action, task, open_mode).map_err(command_error)?;
    if !execute {
        return Ok(RenderedCommand {
            output: render_plan(plan, subcommand.get_flag("json"))?,
            state_changed,
        });
    }
    let (outputs, operation_state_changed) = execute_task_action(
        context, action, task, &plan, confirmed, runner,
    )
    .map_err(|(error, error_state_changed)| {
        let cli_error = command_error(error);
        if error_state_changed {
            cli_error.after_state_change()
        } else {
            cli_error
        }
    })?;
    Ok(RenderedCommand {
        output: render_execution_outputs(&outputs, None),
        state_changed: state_changed || operation_state_changed,
    })
}

fn plan_task_action(
    context: &CommandContext<InMemoryRegistry>,
    action: OperatorAction,
    task: &str,
    open_mode: commands::OpenMode,
) -> Result<ajax_core::use_cases::CommandPlan, CommandError> {
    match action {
        OperatorAction::Resume => resume::plan(context, task, open_mode),
        OperatorAction::Review => review::plan(context, task),
        OperatorAction::Repair => repair::plan(context, task, open_mode),
        OperatorAction::Ship => ship::plan(context, task),
        OperatorAction::Start | OperatorAction::Drop => unreachable!("not a task command action"),
    }
}

fn execute_task_action<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    action: OperatorAction,
    task: &str,
    plan: &ajax_core::use_cases::CommandPlan,
    confirmed: bool,
    runner: &mut R,
) -> Result<(Vec<ajax_core::adapters::CommandOutput>, bool), (CommandError, bool)> {
    match action {
        OperatorAction::Resume => resume::execute(context, task, plan, confirmed, runner),
        OperatorAction::Review => review::execute(context, task, plan, confirmed, runner),
        OperatorAction::Repair => repair::execute(context, task, plan, confirmed, runner),
        OperatorAction::Ship => ship::execute(context, task, plan, confirmed, runner),
        OperatorAction::Start | OperatorAction::Drop => unreachable!("not a task command action"),
    }
}

pub(crate) fn render_drop_command<R: CommandRunner>(
    subcommand: &ArgMatches,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
) -> Result<RenderedCommand, CliError> {
    let task = task_arg(subcommand)?;
    let execute = subcommand.get_flag("execute");
    let confirmed = subcommand.get_flag("yes");
    if execute {
        return execute_observed_drop(context, task, confirmed, runner);
    }
    let mut state_changed = false;
    if (!execute || confirmed) && !drop_should_refresh_cleanup_evidence(context, task) {
        match commands::refresh_git_substrate_evidence(context, runner) {
            Ok(changed) => state_changed |= changed,
            Err(_) => {
                state_changed |= commands::mark_task_git_substrate_missing(context, task)
                    .map_err(command_error)?;
            }
        }
    }
    let plan = drop::plan_confirmation(context, task).map_err(command_error)?;
    Ok(RenderedCommand {
        output: render_plan(plan, subcommand.get_flag("json"))?,
        state_changed,
    })
}

fn drop_should_refresh_cleanup_evidence<R: Registry>(
    context: &CommandContext<R>,
    task: &str,
) -> bool {
    context
        .registry
        .list_tasks()
        .into_iter()
        .find(|candidate| candidate.qualified_handle() == task)
        .is_some_and(|candidate| {
            matches!(
                candidate.lifecycle_status,
                LifecycleStatus::Merged | LifecycleStatus::Cleanable
            )
        })
}

pub(crate) fn execute_observed_drop<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    task_handle: &str,
    confirmed: bool,
    runner: &mut R,
) -> Result<RenderedCommand, CliError> {
    let confirmation_plan = drop::plan_confirmation(context, task_handle).map_err(command_error)?;
    if !confirmation_plan.blocked_reasons.is_empty() {
        return Err(command_error(CommandError::PlanBlocked(
            confirmation_plan.blocked_reasons,
        )));
    }
    let task_before_drop = cli_task(context, task_handle)?;
    let operation_confirmed =
        drop::resolve_execution_confirmation(task_before_drop, &confirmation_plan, confirmed)
            .map_err(command_error)?;

    let operation = drop::plan_operation(context, task_handle, runner).map_err(command_error)?;
    let (outputs, completion) =
        drop::execute(context, task_handle, operation, operation_confirmed, runner).map_err(
            |error| match error {
                CommandError::ConfirmationRequired | CommandError::PlanBlocked(_) => {
                    command_error(error)
                }
                error => enrich_drop_failure_message(context, task_handle, command_error(error))
                    .after_state_change(),
            },
        )?;
    match completion {
        ajax_core::slices::drop::DropTaskCompletion::Removed => {
            let output = if outputs.is_empty() {
                format!("removed task: {task_handle}")
            } else {
                render_execution_outputs(&outputs, None)
            };
            Ok(RenderedCommand {
                output,
                state_changed: true,
            })
        }
        ajax_core::slices::drop::DropTaskCompletion::TeardownIncomplete {
            failed_step,
            detail,
        } => Err(CliError::CommandFailedAfterStateChange(
            ajax_core::commands::format_drop_teardown_incomplete_message(
                task_handle,
                failed_step,
                &detail,
            ),
        )),
    }
}

fn enrich_drop_failure_message<R: Registry>(
    context: &CommandContext<R>,
    task_handle: &str,
    error: CliError,
) -> CliError {
    let CliError::CommandFailed(message) = error else {
        return error;
    };
    let Some(task) = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == task_handle)
    else {
        return CliError::CommandFailed(message);
    };
    if task.lifecycle_status != LifecycleStatus::TeardownIncomplete {
        return CliError::CommandFailed(message);
    }
    let Some(step_key) = task.metadata.get("drop_failed_step_key") else {
        return CliError::CommandFailed(message);
    };
    let failed_step = match step_key.as_str() {
        "agent_stopped" => ajax_core::commands::DropOp::EnsureAgentStopped,
        "worktree_absent" => ajax_core::commands::DropOp::EnsureWorktreeAbsent,
        "branch_absent" => ajax_core::commands::DropOp::EnsureBranchAbsent,
        "tmux_session_absent" => ajax_core::commands::DropOp::EnsureTmuxSessionAbsent,
        _ => return CliError::CommandFailed(message),
    };
    CliError::CommandFailed(
        ajax_core::commands::format_drop_teardown_incomplete_message(
            task_handle,
            failed_step,
            task.metadata
                .get("drop_failed_detail")
                .map(String::as_str)
                .unwrap_or(message.as_str()),
        ),
    )
}

fn cli_task<'a>(
    context: &'a CommandContext<InMemoryRegistry>,
    task_handle: &str,
) -> Result<&'a Task, CliError> {
    context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == task_handle)
        .ok_or_else(|| command_error(CommandError::TaskNotFound(task_handle.to_string())))
}

use ajax_core::{
    adapters::CommandRunner,
    commands::{self, CommandContext, CommandError},
    models::{LifecycleStatus, Task},
    registry::{InMemoryRegistry, Registry},
    task_operations::drop_task::{
        execute_drop_task_operation, plan_drop_confirmation, plan_drop_task_operation,
        DropTaskCompletion,
    },
    task_operations::task_command::{
        execute_task_command_operation, plan_task_command_operation, TaskCommandKind,
    },
};
use clap::ArgMatches;

use crate::{
    command_error,
    render::{render_execution_outputs, render_plan},
    task_arg, CliError, RenderedCommand,
};

pub(crate) fn render_task_command<R: CommandRunner>(
    kind: TaskCommandKind,
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
        kind,
        TaskCommandKind::Resume | TaskCommandKind::Review | TaskCommandKind::Repair
    ) {
        if let Ok(changed) = commands::refresh_git_substrate_evidence(context, runner) {
            state_changed |= changed;
        }
    }
    let plan =
        plan_task_command_operation(context, kind, task, open_mode).map_err(command_error)?;
    if !execute {
        return Ok(RenderedCommand {
            output: render_plan(plan, subcommand.get_flag("json"))?,
            state_changed,
        });
    }
    let (outputs, operation_state_changed) = execute_task_command_operation(
        context, kind, task, &plan, confirmed, runner,
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
    let plan = plan_drop_confirmation(context, task).map_err(command_error)?;
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
    let confirmation_plan = plan_drop_confirmation(context, task_handle).map_err(command_error)?;
    if !confirmation_plan.blocked_reasons.is_empty() {
        return Err(command_error(CommandError::PlanBlocked(
            confirmation_plan.blocked_reasons,
        )));
    }
    let task_before_drop = cli_task(context, task_handle)?;
    let resuming_incomplete =
        task_before_drop.lifecycle_status == LifecycleStatus::TeardownIncomplete;
    let can_observe_before_confirmation = matches!(
        task_before_drop.lifecycle_status,
        LifecycleStatus::Merged | LifecycleStatus::Cleanable
    ) && !task_before_drop
        .has_side_flag(ajax_core::models::SideFlag::Dirty)
        && !task_before_drop.has_side_flag(ajax_core::models::SideFlag::Conflicted)
        && !task_before_drop.has_side_flag(ajax_core::models::SideFlag::Unpushed)
        && task_before_drop.git_status.as_ref().is_none_or(|status| {
            !status.dirty && !status.conflicted && status.unpushed_commits == 0
        });
    if confirmation_plan.requires_confirmation
        && !confirmed
        && !resuming_incomplete
        && !can_observe_before_confirmation
    {
        return Err(command_error(CommandError::ConfirmationRequired));
    }

    let operation =
        plan_drop_task_operation(context, task_handle, runner).map_err(command_error)?;
    let operation_confirmed = confirmed || resuming_incomplete || can_observe_before_confirmation;
    let (outputs, completion) =
        execute_drop_task_operation(context, task_handle, operation, operation_confirmed, runner)
            .map_err(|error| match error {
            CommandError::ConfirmationRequired | CommandError::PlanBlocked(_) => {
                command_error(error)
            }
            error => enrich_drop_failure_message(context, task_handle, command_error(error))
                .after_state_change(),
        })?;
    match completion {
        DropTaskCompletion::Removed => {
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
        DropTaskCompletion::TeardownIncomplete {
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

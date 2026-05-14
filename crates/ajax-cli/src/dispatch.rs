use ajax_core::{
    adapters::{CommandRunError, CommandRunner},
    commands::{self, CommandContext, CommandError},
    registry::{InMemoryRegistry, Registry},
};
use clap::ArgMatches;

use crate::{
    classifiers::command_error_looks_conflicted,
    command_error,
    render::{render_execution_outputs, render_plan},
    task_arg, CliError, RenderedCommand,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TaskCommandOperation {
    Open,
    Diff,
    Merge,
    Repair,
    Drop,
}

impl TaskCommandOperation {
    pub(crate) fn from_cli_subcommand(name: &str) -> Option<Self> {
        match name {
            "resume" => Some(Self::Open),
            "repair" => Some(Self::Repair),
            "review" => Some(Self::Diff),
            "ship" => Some(Self::Merge),
            "drop" => Some(Self::Drop),
            _ => None,
        }
    }

    pub(crate) fn plan<R: Registry>(
        self,
        context: &CommandContext<R>,
        task: &str,
    ) -> Result<commands::CommandPlan, CommandError> {
        self.plan_with_open_mode(context, task, commands::OpenMode::Attach)
    }

    pub(crate) fn plan_with_open_mode<R: Registry>(
        self,
        context: &CommandContext<R>,
        task: &str,
        open_mode: commands::OpenMode,
    ) -> Result<commands::CommandPlan, CommandError> {
        match self {
            Self::Open => commands::open_task_plan(context, task, open_mode),
            Self::Diff => commands::diff_task_plan(context, task),
            Self::Merge => commands::merge_task_plan(context, task),
            Self::Repair => repair_task_plan(context, task, open_mode),
            Self::Drop => drop_task_plan(context, task),
        }
    }

    pub(crate) fn apply_after_execute<R: Registry>(
        self,
        context: &mut CommandContext<R>,
        task: &str,
    ) -> Result<bool, CommandError> {
        match self {
            Self::Open => {
                commands::mark_task_opened(context, task)?;
                Ok(true)
            }
            Self::Merge => {
                commands::mark_task_merged(context, task)?;
                Ok(true)
            }
            Self::Drop => {
                commands::mark_task_removed(context, task)?;
                Ok(true)
            }
            Self::Repair => {
                commands::mark_task_trunk_repaired(context, task)?;
                Ok(true)
            }
            Self::Diff => Ok(false),
        }
    }

    pub(crate) fn returns_to_cockpit_after_execute(self) -> bool {
        matches!(self, Self::Diff | Self::Merge | Self::Repair | Self::Drop)
    }
}

fn repair_task_plan<R: Registry>(
    context: &CommandContext<R>,
    task: &str,
    open_mode: commands::OpenMode,
) -> Result<commands::CommandPlan, CommandError> {
    let mut plan = commands::trunk_task_plan_with_open_mode(context, task, open_mode)?;
    plan.title = format!("repair task: {task}");
    if let Ok(check_plan) = commands::check_task_plan(context, task) {
        plan.commands.extend(check_plan.commands);
        plan.requires_confirmation |= check_plan.requires_confirmation;
        plan.blocked_reasons.extend(check_plan.blocked_reasons);
    }
    Ok(plan)
}

fn drop_task_plan<R: Registry>(
    context: &CommandContext<R>,
    task: &str,
) -> Result<commands::CommandPlan, CommandError> {
    let clean_plan = commands::clean_task_plan(context, task)?;
    if clean_plan.blocked_reasons.is_empty() {
        Ok(clean_plan)
    } else {
        commands::remove_task_plan(context, task)
    }
}

pub(crate) fn render_task_command<R: CommandRunner>(
    operation: TaskCommandOperation,
    subcommand: &ArgMatches,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    open_mode: commands::OpenMode,
) -> Result<RenderedCommand, CliError> {
    let task = task_arg(subcommand)?;
    let execute = subcommand.get_flag("execute");
    let confirmed = subcommand.get_flag("yes");
    if operation == TaskCommandOperation::Drop
        && drop_should_refresh_cleanup_evidence(context, task)
        && execute
    {
        commands::ensure_cleanup_git_status(context, task, runner).map_err(command_error)?;
    }
    let mut plan = operation
        .plan_with_open_mode(context, task, open_mode)
        .map_err(command_error)?;
    if !execute {
        return Ok(RenderedCommand {
            output: render_plan(plan, subcommand.get_flag("json"))?,
            state_changed: false,
        });
    }
    if operation == TaskCommandOperation::Merge
        && plan.blocked_reasons.is_empty()
        && (!plan.requires_confirmation || confirmed)
        && merge_task_has_cached_git_evidence(context, task)
    {
        refresh_merge_evidence_if_available(context, task, runner);
        plan = operation
            .plan_with_open_mode(context, task, open_mode)
            .map_err(command_error)?;
    }

    if matches!(operation, TaskCommandOperation::Repair) {
        commands::mark_task_check_started(context, task).map_err(command_error)?;
        match commands::execute_plan(&plan, confirmed, runner) {
            Ok(outputs) => {
                if operation == TaskCommandOperation::Repair {
                    commands::mark_task_trunk_repaired(context, task).map_err(command_error)?;
                }
                commands::mark_task_check_succeeded(context, task).map_err(command_error)?;
                return Ok(RenderedCommand {
                    output: render_execution_outputs(&outputs, None),
                    state_changed: true,
                });
            }
            Err(error) => {
                commands::mark_task_check_failed(context, task)
                    .map_err(|mark_error| command_error(mark_error).after_state_change())?;
                return Err(command_error(error).after_state_change());
            }
        }
    }

    if operation == TaskCommandOperation::Merge {
        match commands::execute_plan(&plan, confirmed, runner) {
            Ok(outputs) => {
                commands::mark_task_merged(context, task).map_err(command_error)?;
                return Ok(RenderedCommand {
                    output: render_execution_outputs(&outputs, None),
                    state_changed: true,
                });
            }
            Err(error) => {
                if matches!(error, CommandError::CommandRun(_)) {
                    let conflicted = merge_error_looks_conflicted(&error);
                    commands::mark_task_merge_failed(context, task, conflicted)
                        .map_err(|mark_error| command_error(mark_error).after_state_change())?;
                    return Err(command_error(error).after_state_change());
                }
                return Err(command_error(error));
            }
        }
    }

    if matches!(operation, TaskCommandOperation::Drop) {
        return execute_teardown_plan(context, task, operation, &plan, confirmed, runner);
    }

    let outputs = commands::execute_plan(&plan, confirmed, runner).map_err(command_error)?;
    let state_changed = operation
        .apply_after_execute(context, task)
        .map_err(command_error)?;
    Ok(RenderedCommand {
        output: render_execution_outputs(&outputs, None),
        state_changed,
    })
}

fn merge_task_has_cached_git_evidence<R: Registry>(
    context: &CommandContext<R>,
    task: &str,
) -> bool {
    context
        .registry
        .list_tasks()
        .into_iter()
        .find(|candidate| candidate.qualified_handle() == task)
        .is_some_and(|candidate| candidate.git_status.is_some())
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
                ajax_core::models::LifecycleStatus::Merged
                    | ajax_core::models::LifecycleStatus::Cleanable
            )
        })
}

fn refresh_merge_evidence_if_available<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    task: &str,
    runner: &mut R,
) {
    // Merge still runs the fresh-evidence probe first when available; if the
    // probe itself cannot run, the existing plan remains the operator-facing
    // source of confirmation and execution errors.
    let _refresh_attempted = commands::refresh_git_evidence(context, task, runner, false).is_ok();
}

fn merge_error_looks_conflicted(error: &CommandError) -> bool {
    matches!(
        error,
        CommandError::CommandRun(error) if command_error_looks_conflicted(error)
    )
}

fn execute_teardown_plan<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    task: &str,
    operation: TaskCommandOperation,
    plan: &commands::CommandPlan,
    confirmed: bool,
    runner: &mut R,
) -> Result<RenderedCommand, CliError> {
    if !plan.blocked_reasons.is_empty() {
        return Err(command_error(CommandError::PlanBlocked(
            plan.blocked_reasons.clone(),
        )));
    }
    if plan.requires_confirmation && !confirmed {
        return Err(command_error(CommandError::ConfirmationRequired));
    }

    let mut outputs = Vec::new();
    let mut state_changed = false;
    for command in &plan.commands {
        let output = runner.run(command).map_err(|error| {
            let cli_error = command_error(CommandError::CommandRun(error));
            if state_changed {
                cli_error.after_state_change()
            } else {
                cli_error
            }
        })?;
        if output.status_code != 0 {
            let cli_error = command_error(CommandError::CommandRun(CommandRunError::NonZeroExit {
                program: command.program.clone(),
                status_code: output.status_code,
                stderr: output.stderr.clone(),
                cwd: command.cwd.clone(),
            }));
            return Err(if state_changed {
                cli_error.after_state_change()
            } else {
                cli_error
            });
        }
        outputs.push(output);
        state_changed |= commands::mark_task_cleanup_step_completed(context, task, command)
            .map_err(|error| {
                let cli_error = command_error(error);
                if state_changed {
                    cli_error.after_state_change()
                } else {
                    cli_error
                }
            })?;
    }

    if operation == TaskCommandOperation::Drop && plan.title.starts_with("remove task:") {
        commands::mark_task_force_removed(context, task).map_err(command_error)?;
    } else {
        commands::mark_task_removed(context, task).map_err(command_error)?;
    }
    Ok(RenderedCommand {
        output: render_execution_outputs(&outputs, None),
        state_changed: true,
    })
}

use ajax_core::{
    adapters::CommandRunner,
    commands::{self, CommandContext, CommandError},
    models::{LifecycleStatus, Task},
    registry::{InMemoryRegistry, Registry},
    task_operations::drop_task::{
        execute_drop_task_operation, plan_drop_task_operation, DropTaskCompletion,
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

    #[cfg(test)]
    pub(crate) fn returns_to_cockpit_after_execute(self) -> bool {
        matches!(self, Self::Diff | Self::Merge | Self::Repair | Self::Drop)
    }

    fn core_task_command_kind(self) -> Option<TaskCommandKind> {
        match self {
            Self::Open => Some(TaskCommandKind::Resume),
            Self::Diff => Some(TaskCommandKind::Review),
            Self::Merge => Some(TaskCommandKind::Ship),
            Self::Repair => Some(TaskCommandKind::Repair),
            Self::Drop => None,
        }
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
    if operation == TaskCommandOperation::Drop && execute {
        return execute_observed_drop(context, task, confirmed, runner);
    }
    if let Some(kind) = operation.core_task_command_kind() {
        return render_core_task_command(kind, subcommand, context, runner, open_mode, task);
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
    let plan = drop_task_plan(context, task).map_err(command_error)?;
    Ok(RenderedCommand {
        output: render_plan(plan, subcommand.get_flag("json"))?,
        state_changed,
    })
}

fn render_core_task_command<R: CommandRunner>(
    kind: TaskCommandKind,
    subcommand: &ArgMatches,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    open_mode: commands::OpenMode,
    task: &str,
) -> Result<RenderedCommand, CliError> {
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
    let mut operation =
        plan_task_command_operation(context, kind, task, open_mode).map_err(command_error)?;
    if !execute {
        return Ok(RenderedCommand {
            output: render_plan(operation.plan, subcommand.get_flag("json"))?,
            state_changed,
        });
    }
    if kind == TaskCommandKind::Ship
        && operation.plan.blocked_reasons.is_empty()
        && (!operation.plan.requires_confirmation || confirmed)
        && merge_task_has_cached_git_evidence(context, task)
    {
        refresh_merge_evidence_if_available(context, task, runner);
        operation =
            plan_task_command_operation(context, kind, task, open_mode).map_err(command_error)?;
    }

    let execution = execute_task_command_operation(context, &operation, confirmed, runner)
        .map_err(|error| {
            let cli_error = command_error(error.error);
            if error.state_changed {
                cli_error.after_state_change()
            } else {
                cli_error
            }
        })?;
    Ok(RenderedCommand {
        output: render_execution_outputs(&execution.outputs, None),
        state_changed: state_changed || execution.state_changed,
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

pub(crate) fn execute_observed_drop<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    task_handle: &str,
    confirmed: bool,
    runner: &mut R,
) -> Result<RenderedCommand, CliError> {
    let confirmation_plan = drop_task_plan(context, task_handle).map_err(command_error)?;
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
    let execution =
        execute_drop_task_operation(context, task_handle, operation, operation_confirmed, runner)
            .map_err(|error| match error {
            CommandError::ConfirmationRequired | CommandError::PlanBlocked(_) => {
                command_error(error)
            }
            error => command_error(error).after_state_change(),
        })?;
    match execution.completion {
        DropTaskCompletion::Removed => {
            let output = if execution.outputs.is_empty() {
                format!("removed task: {task_handle}")
            } else {
                render_execution_outputs(&execution.outputs, None)
            };
            Ok(RenderedCommand {
                output,
                state_changed: true,
            })
        }
        DropTaskCompletion::TeardownIncomplete => Err(CliError::CommandFailedAfterStateChange(
            "drop teardown incomplete".to_string(),
        )),
    }
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

#[cfg(test)]
mod tests {
    #[test]
    fn cli_drop_dispatch_delegates_observed_drop_decision_to_core_operation() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/dispatch.rs"),
        )
        .unwrap();
        let local_completion_helper = ["drop_observation", "_all_absent"].concat();
        let local_execution_loop = ["for op in operation.", "ops"].concat();

        assert!(source.contains("plan_drop_task_operation"));
        assert!(source.contains("execute_drop_task_operation"));
        assert!(!source.contains(&local_completion_helper));
        assert!(!source.contains(&local_execution_loop));
    }

    #[test]
    fn cli_resume_review_dispatch_delegates_execution_to_core_task_command_operation() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/dispatch.rs"),
        )
        .unwrap();
        let render_task_command = source
            .split("pub(crate) fn render_task_command")
            .nth(1)
            .and_then(|source| source.split("fn merge_task_has_cached_git_evidence").next())
            .unwrap();

        assert!(render_task_command.contains("plan_task_command_operation"));
        assert!(render_task_command.contains("execute_task_command_operation"));
    }

    #[test]
    fn cli_ship_dispatch_delegates_execution_to_core_task_command_operation() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/dispatch.rs"),
        )
        .unwrap();
        let ship_mapping = ["Self::Merge => Some(TaskCommandKind::", "Ship)"].concat();

        assert!(source.contains(&ship_mapping));
    }

    #[test]
    fn cli_repair_dispatch_delegates_execution_to_core_task_command_operation() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/dispatch.rs"),
        )
        .unwrap();
        let repair_mapping = ["Self::Repair => Some(TaskCommandKind::", "Repair)"].concat();

        assert!(source.contains(&repair_mapping));
    }

    #[test]
    fn cli_task_dispatch_no_longer_owns_legacy_execute_apply_blocks() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/dispatch.rs"),
        )
        .unwrap();
        let render_task_command = source
            .split("pub(crate) fn render_task_command")
            .nth(1)
            .and_then(|source| source.split("fn render_core_task_command").next())
            .unwrap();
        let execute_plan = ["commands::execute", "_plan(&plan"].concat();
        let apply_after_execute = ["apply_after", "_execute"].concat();

        assert!(!render_task_command.contains(&execute_plan));
        assert!(!render_task_command.contains(&apply_after_execute));
        assert!(!render_task_command.contains("mark_task_check_started"));
        assert!(!render_task_command.contains("mark_task_merged"));
    }
}

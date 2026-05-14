use ajax_core::{
    adapters::CommandRunner,
    commands::{self, CommandContext, CommandError},
    models::OperatorAction,
    registry::InMemoryRegistry,
};

use crate::{
    cockpit_backend::build_cockpit_snapshot, command_error, dispatch::TaskCommandOperation,
    execution_dispatch::execute_new_task_plan, render::render_execution_outputs, CliError,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PendingCockpitOutcome {
    Exit(String),
    ReturnToCockpit,
}

impl PendingCockpitOutcome {
    #[cfg(test)]
    pub(crate) fn contains(&self, needle: &str) -> bool {
        match self {
            PendingCockpitOutcome::Exit(output) => output.contains(needle),
            PendingCockpitOutcome::ReturnToCockpit => false,
        }
    }
}

pub(crate) fn handle_pending_cockpit_result(
    result: Result<PendingCockpitOutcome, CliError>,
    cockpit_flash: &mut Option<String>,
) -> Option<PendingCockpitOutcome> {
    match result {
        Ok(outcome) => Some(outcome),
        Err(error) => {
            *cockpit_flash = Some(error.to_string());
            None
        }
    }
}

pub(crate) fn tui_cockpit_action<R: CommandRunner>(
    item: &ajax_core::models::CockpitActionItem,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    state_changed: &mut bool,
) -> std::io::Result<ajax_tui::ActionOutcome> {
    tui_cockpit_action_with_confirmation(item, context, runner, state_changed, false)
}

pub(crate) fn tui_cockpit_confirmed_action<R: CommandRunner>(
    item: &ajax_core::models::CockpitActionItem,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    state_changed: &mut bool,
) -> std::io::Result<ajax_tui::ActionOutcome> {
    tui_cockpit_action_with_confirmation(item, context, runner, state_changed, true)
}

fn tui_cockpit_action_with_confirmation<R: CommandRunner>(
    item: &ajax_core::models::CockpitActionItem,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    state_changed: &mut bool,
    confirmed: bool,
) -> std::io::Result<ajax_tui::ActionOutcome> {
    let handle = &item.task_handle;
    let action = OperatorAction::from_label(item.action.as_str());

    if let Some(operation) = action.and_then(operation_from_operator_action) {
        if operation == TaskCommandOperation::Drop {
            let plan = operation
                .plan(context, handle)
                .map_err(command_error_as_io)?;
            if plan.requires_confirmation && !confirmed {
                return Ok(ajax_tui::ActionOutcome::Confirm(format!(
                    "press enter again to confirm {}",
                    item.action
                )));
            }
            commands::execute_plan(&plan, true, runner).map_err(command_error_as_io)?;
            if plan.title.starts_with("remove task:") {
                commands::mark_task_force_removed(context, handle).map_err(command_error_as_io)?;
            } else {
                commands::mark_task_removed(context, handle).map_err(command_error_as_io)?;
            }
            *state_changed = true;
            return Ok(ajax_tui::ActionOutcome::Refresh(build_cockpit_snapshot(
                context,
            )));
        }

        return Ok(ajax_tui::ActionOutcome::Defer(ajax_tui::PendingAction {
            task_handle: handle.clone(),
            action: item.action.clone(),
            task_title: None,
        }));
    }

    match action {
        Some(OperatorAction::Start) => Ok(ajax_tui::ActionOutcome::Message(
            "select a project, then choose start task to enter a task name".to_string(),
        )),
        None if item.action == "status" => {
            let task_count = commands::list_tasks(context, Some(handle)).tasks.len();
            Ok(ajax_tui::ActionOutcome::Message(format!(
                "{handle}: {task_count} task(s)"
            )))
        }
        Some(action) => Ok(ajax_tui::ActionOutcome::Message(format!(
            "cockpit action is not configured: {}",
            action.as_str()
        ))),
        None => Ok(ajax_tui::ActionOutcome::Message(format!(
            "cockpit action is not configured: {}",
            item.action
        ))),
    }
}

fn command_error_as_io(error: CommandError) -> std::io::Error {
    let message = match command_error(error) {
        CliError::CommandFailed(message)
        | CliError::CommandFailedAfterStateChange(message)
        | CliError::JsonSerialization(message)
        | CliError::ContextLoad(message)
        | CliError::ContextSave(message) => message,
    };
    std::io::Error::other(message)
}

pub(crate) fn execute_pending_cockpit_action<R: CommandRunner>(
    pending: &ajax_tui::PendingAction,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    state_changed: &mut bool,
) -> Result<PendingCockpitOutcome, CliError> {
    execute_pending_cockpit_action_with_open_mode(
        pending,
        context,
        runner,
        state_changed,
        crate::current_open_mode(),
    )
}

pub(crate) fn execute_pending_cockpit_action_with_open_mode<R: CommandRunner>(
    pending: &ajax_tui::PendingAction,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    state_changed: &mut bool,
    open_mode: commands::OpenMode,
) -> Result<PendingCockpitOutcome, CliError> {
    if pending.action == OperatorAction::Start.as_str() {
        let title = pending.task_title.clone().ok_or_else(|| {
            CliError::CommandFailed(
                "start task title is required before cockpit can create the task".to_string(),
            )
        })?;
        let request = commands::NewTaskRequest {
            repo: pending.task_handle.clone(),
            title,
            agent: "codex".to_string(),
        };
        let plan = commands::new_task_plan(context, request.clone()).map_err(command_error)?;
        let (outputs, task) = execute_new_task_plan(
            context, runner, &request, &plan, true, open_mode,
        )
        .inspect_err(|error| {
            if error.state_changed() {
                *state_changed = true;
            }
        })?;
        *state_changed = true;
        return Ok(PendingCockpitOutcome::Exit(render_execution_outputs(
            &outputs,
            Some(&task.qualified_handle()),
        )));
    }

    let action = OperatorAction::from_label(pending.action.as_str());
    let operation = action.and_then(operation_from_operator_action);
    let Some(operation) = operation else {
        return Err(CliError::CommandFailed(format!(
            "unknown cockpit action: {}",
            pending.action
        )));
    };
    let plan = operation
        .plan_with_open_mode(context, &pending.task_handle, open_mode)
        .map_err(command_error)?;
    let outputs = commands::execute_plan(&plan, !plan.requires_confirmation, runner)
        .map_err(command_error)?;
    let changed = operation
        .apply_after_execute(context, &pending.task_handle)
        .map_err(command_error)?;
    *state_changed |= changed;
    if operation.returns_to_cockpit_after_execute() {
        return Ok(PendingCockpitOutcome::ReturnToCockpit);
    }
    Ok(PendingCockpitOutcome::Exit(render_execution_outputs(
        &outputs, None,
    )))
}

fn operation_from_operator_action(action: OperatorAction) -> Option<TaskCommandOperation> {
    match action {
        OperatorAction::Start => None,
        OperatorAction::Resume => Some(TaskCommandOperation::Open),
        OperatorAction::Review => Some(TaskCommandOperation::Diff),
        OperatorAction::Ship => Some(TaskCommandOperation::Merge),
        OperatorAction::Drop => Some(TaskCommandOperation::Drop),
        OperatorAction::Repair => Some(TaskCommandOperation::Repair),
    }
}

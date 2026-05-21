use ajax_core::{
    adapters::CommandRunner,
    commands::{self, CommandContext, CommandError},
    models::{OperatorAction, TaskId},
    registry::{InMemoryRegistry, Registry},
    task_operations::task_command::{
        execute_task_command_operation, plan_task_command_operation, TaskCommandKind,
    },
};

use crate::{
    cockpit_backend::build_cockpit_snapshot,
    command_error,
    dispatch::{execute_observed_drop, TaskCommandOperation},
    execution_dispatch::execute_new_task_plan_with_task_session,
    render::render_execution_outputs,
    task_session::{execute_task_entry_plan, TaskSessionRunner},
    CliError,
};

#[cfg(test)]
use crate::execution_dispatch::execute_new_task_plan;

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
    _runner: &mut R,
    _state_changed: &mut bool,
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
            return Ok(ajax_tui::ActionOutcome::RefreshAndDefer(
                optimistic_drop_snapshot(context, handle, &item.task_id),
                ajax_tui::PendingAction {
                    task_handle: handle.clone(),
                    action: item.action.clone(),
                    task_title: None,
                },
            ));
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

fn optimistic_drop_snapshot(
    context: &CommandContext<InMemoryRegistry>,
    handle: &str,
    fallback_task_id: &TaskId,
) -> ajax_tui::CockpitSnapshot {
    let task_id = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == handle)
        .map(|task| task.id.clone())
        .unwrap_or_else(|| fallback_task_id.clone());
    let mut snapshot = build_cockpit_snapshot(context);
    snapshot
        .cards
        .retain(|card| card.id != task_id && card.qualified_handle != handle);
    snapshot
        .inbox
        .items
        .retain(|item| item.task_id != task_id && item.task_handle != handle);
    snapshot
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

#[cfg(test)]
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

#[cfg(test)]
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

    if operation == TaskCommandOperation::Drop {
        let rendered = execute_observed_drop(context, &pending.task_handle, true, runner)?;
        *state_changed |= rendered.state_changed;
        return Ok(PendingCockpitOutcome::ReturnToCockpit);
    }

    let kind = action
        .and_then(task_command_kind_from_operator_action)
        .ok_or_else(|| {
            CliError::CommandFailed(format!("unknown cockpit action: {}", pending.action))
        })?;
    let operation = plan_task_command_operation(context, kind, &pending.task_handle, open_mode)
        .map_err(command_error)?;
    let confirmed = !operation.plan.requires_confirmation;
    let execution = execute_task_command_operation(context, &operation, confirmed, runner)
        .map_err(|error| task_command_cli_error(error, state_changed))?;
    *state_changed |= execution.state_changed;
    if task_command_returns_to_cockpit(kind) {
        return Ok(PendingCockpitOutcome::ReturnToCockpit);
    }
    Ok(PendingCockpitOutcome::Exit(render_execution_outputs(
        &execution.outputs,
        None,
    )))
}

pub(crate) fn execute_pending_cockpit_action_with_task_session<
    R: CommandRunner,
    S: TaskSessionRunner,
>(
    pending: &ajax_tui::PendingAction,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    state_changed: &mut bool,
    task_session: &mut S,
) -> Result<PendingCockpitOutcome, CliError> {
    let task_entry_open_mode = commands::OpenMode::Attach;
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
        execute_new_task_plan_with_task_session(
            context,
            runner,
            task_session,
            &request,
            &plan,
            true,
            task_entry_open_mode,
        )
        .inspect_err(|error| {
            if error.state_changed() {
                *state_changed = true;
            }
        })?;
        *state_changed = true;
        return Ok(PendingCockpitOutcome::ReturnToCockpit);
    }

    let action = OperatorAction::from_label(pending.action.as_str());
    let operation = action.and_then(operation_from_operator_action);
    let Some(operation) = operation else {
        return Err(CliError::CommandFailed(format!(
            "unknown cockpit action: {}",
            pending.action
        )));
    };

    if operation == TaskCommandOperation::Drop {
        let rendered = execute_observed_drop(context, &pending.task_handle, true, runner)?;
        *state_changed |= rendered.state_changed;
        return Ok(PendingCockpitOutcome::ReturnToCockpit);
    }

    let kind = action
        .and_then(task_command_kind_from_operator_action)
        .ok_or_else(|| {
            CliError::CommandFailed(format!("unknown cockpit action: {}", pending.action))
        })?;
    let operation =
        plan_task_command_operation(context, kind, &pending.task_handle, task_entry_open_mode)
            .map_err(command_error)?;

    if kind != TaskCommandKind::Resume {
        let confirmed = !operation.plan.requires_confirmation;
        let execution = execute_task_command_operation(context, &operation, confirmed, runner)
            .map_err(|error| task_command_cli_error(error, state_changed))?;
        *state_changed |= execution.state_changed;
        if task_command_returns_to_cockpit(kind) {
            return Ok(PendingCockpitOutcome::ReturnToCockpit);
        }
        return Ok(PendingCockpitOutcome::Exit(render_execution_outputs(
            &execution.outputs,
            None,
        )));
    }

    execute_task_entry_plan(&operation.plan, runner, task_session)?;
    commands::mark_task_opened(context, &pending.task_handle).map_err(command_error)?;
    *state_changed = true;
    Ok(PendingCockpitOutcome::ReturnToCockpit)
}

fn task_command_kind_from_operator_action(action: OperatorAction) -> Option<TaskCommandKind> {
    match action {
        OperatorAction::Start | OperatorAction::Drop => None,
        OperatorAction::Resume => Some(TaskCommandKind::Resume),
        OperatorAction::Review => Some(TaskCommandKind::Review),
        OperatorAction::Ship => Some(TaskCommandKind::Ship),
        OperatorAction::Repair => Some(TaskCommandKind::Repair),
    }
}

fn task_command_returns_to_cockpit(kind: TaskCommandKind) -> bool {
    matches!(
        kind,
        TaskCommandKind::Review | TaskCommandKind::Ship | TaskCommandKind::Repair
    )
}

fn task_command_cli_error(
    error: ajax_core::task_operations::task_command::TaskCommandOperationError,
    state_changed: &mut bool,
) -> CliError {
    let changed = error.state_changed || *state_changed;
    let error = command_error(error.error);
    let error = if changed {
        error.after_state_change()
    } else {
        error
    };
    if changed {
        *state_changed = true;
    }
    error
}

fn operation_from_operator_action(action: OperatorAction) -> Option<TaskCommandOperation> {
    match action {
        OperatorAction::Start => None,
        OperatorAction::Resume => Some(TaskCommandOperation::Open),
        OperatorAction::Review => Some(TaskCommandOperation::Review),
        OperatorAction::Ship => Some(TaskCommandOperation::Merge),
        OperatorAction::Drop => Some(TaskCommandOperation::Drop),
        OperatorAction::Repair => Some(TaskCommandOperation::Repair),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn pending_cockpit_task_actions_use_core_task_command_operations() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cockpit_actions.rs"),
        )
        .unwrap();
        let plan_operation = ["plan_task_command", "_operation"].concat();
        let execute_operation = ["execute_task_command", "_operation"].concat();
        let legacy_plan = ["plan_with", "_open_mode"].concat();

        assert!(source.contains(&plan_operation));
        assert!(source.contains(&execute_operation));
        assert!(!source.contains(&legacy_plan));
    }
}

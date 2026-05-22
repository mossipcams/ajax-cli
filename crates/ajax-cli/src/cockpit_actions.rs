#[cfg(test)]
use ajax_core::task_operations::start::{execute_start_task_operation, plan_start_task_operation};
use ajax_core::{
    adapters::CommandRunner,
    commands::{self, CommandContext, CommandError},
    models::{OperatorAction, TaskId},
    registry::{InMemoryRegistry, Registry},
    task_operations::drop_task::plan_drop_task_confirmation,
    task_operations::task_command::{
        execute_task_command_operation, plan_task_command_operation, TaskCommandKind,
    },
};

#[cfg(test)]
use crate::render::render_execution_outputs;
use crate::{
    cockpit_backend::build_cockpit_snapshot,
    command_error,
    dispatch::execute_observed_drop,
    execution_dispatch::execute_new_task_plan_with_task_session,
    task_session::{execute_task_entry_plan, TaskSessionRunner},
    CliError,
};

pub(crate) fn handle_pending_cockpit_result(
    result: Result<(), CliError>,
    cockpit_flash: &mut Option<String>,
) -> bool {
    match result {
        Ok(()) => true,
        Err(error) => {
            *cockpit_flash = Some(error.to_string());
            false
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

    match action {
        Some(OperatorAction::Drop) => {
            let plan = plan_drop_task_confirmation(context, handle).map_err(command_error_as_io)?;
            if plan.requires_confirmation && !confirmed {
                return Ok(ajax_tui::ActionOutcome::Confirm(format!(
                    "press enter again to confirm {}",
                    item.action
                )));
            }
            Ok(ajax_tui::ActionOutcome::RefreshAndDefer(
                optimistic_drop_snapshot(context, handle, &item.task_id),
                ajax_tui::PendingAction {
                    task_handle: handle.clone(),
                    action: item.action.clone(),
                    task_title: None,
                },
            ))
        }
        Some(
            OperatorAction::Resume
            | OperatorAction::Review
            | OperatorAction::Ship
            | OperatorAction::Repair,
        ) => Ok(ajax_tui::ActionOutcome::Defer(ajax_tui::PendingAction {
            task_handle: handle.clone(),
            action: item.action.clone(),
            task_title: None,
        })),
        Some(OperatorAction::Start) => Ok(ajax_tui::ActionOutcome::Message(
            "select a project, then choose start task to enter a task name".to_string(),
        )),
        None if item.action == "status" => {
            let task_count = commands::list_tasks(context, Some(handle)).tasks.len();
            Ok(ajax_tui::ActionOutcome::Message(format!(
                "{handle}: {task_count} task(s)"
            )))
        }
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
) -> Result<Option<String>, CliError> {
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
) -> Result<Option<String>, CliError> {
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
        let operation =
            plan_start_task_operation(context, request.clone()).map_err(command_error)?;
        let (outputs, task) =
            execute_start_task_operation(context, runner, &request, &operation, true, open_mode)
                .map_err(|error| command_error(error).after_state_change())
                .inspect_err(|error| {
                    if error.state_changed() {
                        *state_changed = true;
                    }
                })?;
        *state_changed = true;
        return Ok(Some(render_execution_outputs(
            &outputs,
            Some(&task.qualified_handle()),
        )));
    }

    let Some(action) = OperatorAction::from_label(pending.action.as_str()) else {
        return Err(CliError::CommandFailed(format!(
            "unknown cockpit action: {}",
            pending.action
        )));
    };

    if action == OperatorAction::Drop {
        let rendered = execute_observed_drop(context, &pending.task_handle, true, runner)?;
        *state_changed |= rendered.state_changed;
        return Ok(None);
    }

    let kind = task_command_kind_from_operator_action(action).ok_or_else(|| {
        CliError::CommandFailed(format!("unknown cockpit action: {}", pending.action))
    })?;
    let plan = plan_task_command_operation(context, kind, &pending.task_handle, open_mode)
        .map_err(command_error)?;
    let confirmed = !plan.requires_confirmation;
    let (outputs, operation_state_changed) = execute_task_command_operation(
        context,
        kind,
        &pending.task_handle,
        &plan,
        confirmed,
        runner,
    )
    .map_err(|error| task_command_cli_error(error, state_changed))?;
    *state_changed |= operation_state_changed;
    if kind != TaskCommandKind::Resume {
        return Ok(None);
    }
    Ok(Some(render_execution_outputs(&outputs, None)))
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
) -> Result<(), CliError> {
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
        return Ok(());
    }

    let Some(action) = OperatorAction::from_label(pending.action.as_str()) else {
        return Err(CliError::CommandFailed(format!(
            "unknown cockpit action: {}",
            pending.action
        )));
    };

    if action == OperatorAction::Drop {
        let rendered = execute_observed_drop(context, &pending.task_handle, true, runner)?;
        *state_changed |= rendered.state_changed;
        return Ok(());
    }

    let kind = task_command_kind_from_operator_action(action).ok_or_else(|| {
        CliError::CommandFailed(format!("unknown cockpit action: {}", pending.action))
    })?;
    let plan =
        plan_task_command_operation(context, kind, &pending.task_handle, task_entry_open_mode)
            .map_err(command_error)?;

    if kind != TaskCommandKind::Resume {
        let confirmed = !plan.requires_confirmation;
        let (_outputs, operation_state_changed) = execute_task_command_operation(
            context,
            kind,
            &pending.task_handle,
            &plan,
            confirmed,
            runner,
        )
        .map_err(|error| task_command_cli_error(error, state_changed))?;
        *state_changed |= operation_state_changed;
        return Ok(());
    }

    execute_task_entry_plan(&plan, runner, task_session)?;
    commands::mark_task_opened(context, &pending.task_handle).map_err(command_error)?;
    *state_changed = true;
    Ok(())
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

fn task_command_cli_error(
    (error, error_state_changed): (CommandError, bool),
    state_changed: &mut bool,
) -> CliError {
    let changed = error_state_changed || *state_changed;
    let error = command_error(error);
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
        let local_operation_mapping = ["operation_from", "_operator_action"].concat();
        let outcome_impl = ["impl Pending", "CockpitOutcome"].concat();
        let pending_outcome = ["enum Pending", "CockpitOutcome"].concat();
        let return_helper = ["task_command", "_returns_to_cockpit"].concat();

        assert!(source.contains(&plan_operation));
        assert!(source.contains(&execute_operation));
        assert!(source.contains("task_command_kind_from_operator_action"));
        assert!(!source.contains(&legacy_plan));
        assert!(!source.contains(&local_operation_mapping));
        assert!(!source.contains(&outcome_impl));
        assert!(!source.contains(&pending_outcome));
        assert!(!source.contains(&return_helper));
    }
}

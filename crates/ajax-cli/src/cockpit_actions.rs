#[cfg(test)]
use ajax_core::task_operations::start::execute_start_task_operation;
use ajax_core::{
    adapters::CommandRunner,
    commands::{self, CommandContext, CommandError},
    models::{OperatorAction, TaskId},
    registry::{InMemoryRegistry, Registry},
    remediation::{self, RemediationError},
    task_operations::drop_task::plan_drop_confirmation,
    task_operations::start::plan_start_task_operation,
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
    execution_dispatch::{
        execute_new_task_plan_with_task_session_and_checkpoint, ExecuteNewTaskWithSession,
    },
    task_session::{
        execute_task_entry_plan, TaskEntryPlanOutcome, TaskSessionContext, TaskSessionRunner,
    },
    CliError,
};

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum PendingCockpitExecution {
    Continue(Option<String>),
    OpenNewTask { repo: String },
}

pub(crate) fn handle_pending_cockpit_result(
    result: Result<Option<String>, CliError>,
    cockpit_flash: &mut Option<String>,
) -> bool {
    match result {
        Ok(Some(message)) => {
            *cockpit_flash = Some(message);
            true
        }
        Ok(None) => true,
        Err(error) => {
            *cockpit_flash = Some(error.to_string());
            false
        }
    }
}

pub(crate) fn tui_cockpit_action(
    item: &ajax_core::models::CockpitActionItem,
    context: &mut CommandContext<InMemoryRegistry>,
) -> std::io::Result<ajax_tui::ActionOutcome> {
    tui_cockpit_action_with_confirmation(item, context, false)
}

pub(crate) fn tui_cockpit_confirmed_action(
    item: &ajax_core::models::CockpitActionItem,
    context: &mut CommandContext<InMemoryRegistry>,
) -> std::io::Result<ajax_tui::ActionOutcome> {
    tui_cockpit_action_with_confirmation(item, context, true)
}

fn tui_cockpit_action_with_confirmation(
    item: &ajax_core::models::CockpitActionItem,
    context: &mut CommandContext<InMemoryRegistry>,
    confirmed: bool,
) -> std::io::Result<ajax_tui::ActionOutcome> {
    let handle = &item.task_handle;
    let action = OperatorAction::from_label(item.action.as_str());

    match action {
        Some(OperatorAction::Drop) => {
            let plan = plan_drop_confirmation(context, handle).map_err(command_error_as_io)?;
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
        None if remediation::is_remediation_action(&item.action) => {
            Ok(ajax_tui::ActionOutcome::Defer(ajax_tui::PendingAction {
                task_handle: handle.clone(),
                action: item.action.clone(),
                task_title: None,
            }))
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

fn remediation_cli_error(error: RemediationError) -> CliError {
    match error {
        RemediationError::UnknownRemediation(id) => {
            CliError::CommandFailed(format!("unknown remediation action: {id}"))
        }
        RemediationError::TaskNotFound(handle) => command_error(CommandError::TaskNotFound(handle)),
        RemediationError::UnsupportedCapability(message) => {
            CliError::CommandFailed(message.to_string())
        }
        RemediationError::CommandRun(message) => CliError::CommandFailed(message),
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
        let (_intent, plan) =
            plan_start_task_operation(context, request.clone()).map_err(command_error)?;
        let (outputs, task) =
            execute_start_task_operation(context, runner, &request, &plan, true, open_mode)
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

    if remediation::is_remediation_action(&pending.action) {
        let skill_name = match pending.action.as_str() {
            remediation::FIX_CI => "gh-fix-ci",
            remediation::RESOLVE_MERGE_CONFLICTS => "resolve-merge-conflicts",
            _ => {
                return Err(CliError::CommandFailed(format!(
                    "unknown remediation action: {}",
                    pending.action
                )));
            }
        };
        let skill_path =
            ajax_web::adapters::skills::resolve_skill_path(skill_name).ok_or_else(|| {
                CliError::CommandFailed(
                    "required agent skill is not installed on this host".to_string(),
                )
            })?;
        let outcome = remediation::execute_remediation(
            context,
            runner,
            &pending.task_handle,
            &pending.action,
            &skill_path.display().to_string(),
        )
        .map_err(remediation_cli_error)?;
        return Ok(Some(outcome.output));
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
) -> Result<PendingCockpitExecution, CliError> {
    execute_pending_cockpit_action_with_task_session_and_checkpoint(
        pending,
        context,
        runner,
        state_changed,
        task_session,
        |_| Ok(()),
    )
}

pub(crate) fn execute_pending_cockpit_action_with_task_session_and_checkpoint<
    R: CommandRunner,
    S: TaskSessionRunner,
    C,
>(
    pending: &ajax_tui::PendingAction,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    state_changed: &mut bool,
    task_session: &mut S,
    mut checkpoint: C,
) -> Result<PendingCockpitExecution, CliError>
where
    C: FnMut(&CommandContext<InMemoryRegistry>) -> Result<(), CommandError>,
{
    let session_context = TaskSessionContext::from_task_handle(&pending.task_handle);
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
        let (_intent, plan) =
            plan_start_task_operation(context, request.clone()).map_err(command_error)?;
        match execute_new_task_plan_with_task_session_and_checkpoint(
            context,
            runner,
            task_session,
            &ExecuteNewTaskWithSession {
                request: &request,
                plan: &plan,
                session_context: &session_context,
                confirmed: true,
                open_mode: task_entry_open_mode,
            },
            &mut checkpoint,
        )
        .inspect_err(|error| {
            if error.state_changed() {
                *state_changed = true;
            }
        })? {
            TaskEntryPlanOutcome::Completed(_) => {
                *state_changed = true;
                return Ok(PendingCockpitExecution::Continue(None));
            }
            TaskEntryPlanOutcome::OpenNewTask => {
                *state_changed = true;
                return open_new_task_after_task_session(&session_context);
            }
        }
    }

    if remediation::is_remediation_action(&pending.action) {
        let skill_name = match pending.action.as_str() {
            remediation::FIX_CI => "gh-fix-ci",
            remediation::RESOLVE_MERGE_CONFLICTS => "resolve-merge-conflicts",
            _ => {
                return Err(CliError::CommandFailed(format!(
                    "unknown remediation action: {}",
                    pending.action
                )));
            }
        };
        let skill_path =
            ajax_web::adapters::skills::resolve_skill_path(skill_name).ok_or_else(|| {
                CliError::CommandFailed(
                    "required agent skill is not installed on this host".to_string(),
                )
            })?;
        let outcome = remediation::execute_remediation(
            context,
            runner,
            &pending.task_handle,
            &pending.action,
            &skill_path.display().to_string(),
        )
        .map_err(remediation_cli_error)?;
        return Ok(PendingCockpitExecution::Continue(Some(outcome.output)));
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
        return Ok(PendingCockpitExecution::Continue(None));
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
        return Ok(PendingCockpitExecution::Continue(None));
    }

    match execute_task_entry_plan(&plan, runner, task_session, &session_context)? {
        TaskEntryPlanOutcome::Completed(_) => {
            commands::mark_task_opened(context, &pending.task_handle).map_err(command_error)?;
            *state_changed = true;
            Ok(PendingCockpitExecution::Continue(None))
        }
        TaskEntryPlanOutcome::OpenNewTask => {
            commands::mark_task_opened(context, &pending.task_handle).map_err(command_error)?;
            *state_changed = true;
            open_new_task_after_task_session(&session_context)
        }
    }
}

fn open_new_task_after_task_session(
    session_context: &TaskSessionContext,
) -> Result<PendingCockpitExecution, CliError> {
    let repo = session_context.new_task_repo.clone().ok_or_else(|| {
        CliError::CommandFailed("task handle did not include a repo for create-task".to_string())
    })?;
    Ok(PendingCockpitExecution::OpenNewTask { repo })
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

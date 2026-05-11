use ajax_core::{
    adapters::CommandRunner,
    commands::{self, CommandContext, CommandError},
    models::RecommendedAction,
    registry::InMemoryRegistry,
};

use crate::{
    command_error, dispatch::TaskCommandOperation, execute_new_task_plan,
    render::render_execution_outputs, CliError,
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
    item: &ajax_core::models::AttentionItem,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    state_changed: &mut bool,
) -> std::io::Result<ajax_tui::ActionOutcome> {
    tui_cockpit_action_with_confirmation(item, context, runner, state_changed, false)
}

pub(crate) fn tui_cockpit_confirmed_action<R: CommandRunner>(
    item: &ajax_core::models::AttentionItem,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    state_changed: &mut bool,
) -> std::io::Result<ajax_tui::ActionOutcome> {
    tui_cockpit_action_with_confirmation(item, context, runner, state_changed, true)
}

fn tui_cockpit_action_with_confirmation<R: CommandRunner>(
    item: &ajax_core::models::AttentionItem,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    state_changed: &mut bool,
    confirmed: bool,
) -> std::io::Result<ajax_tui::ActionOutcome> {
    let handle = &item.task_handle;
    let action = RecommendedAction::from_label(item.recommended_action.as_str());

    if let Some(operation) = action.and_then(TaskCommandOperation::from_recommended_action) {
        if matches!(
            operation,
            TaskCommandOperation::Cleanup
                | TaskCommandOperation::Clean
                | TaskCommandOperation::Remove
        ) {
            let plan = operation
                .plan(context, handle)
                .map_err(command_error_as_io)?;
            if plan.requires_confirmation && !confirmed {
                return Ok(ajax_tui::ActionOutcome::Confirm(format!(
                    "press enter again to confirm {}",
                    item.recommended_action
                )));
            }
            commands::execute_plan(&plan, true, runner).map_err(command_error_as_io)?;
            let changed = operation
                .apply_after_execute(context, handle)
                .map_err(command_error_as_io)?;
            *state_changed |= changed;
            return Ok(ajax_tui::ActionOutcome::Refresh {
                repos: commands::list_repos(context),
                tasks: commands::list_tasks(context, None),
                inbox: commands::inbox(context),
            });
        }

        return Ok(ajax_tui::ActionOutcome::Defer(ajax_tui::PendingAction {
            task_handle: handle.clone(),
            recommended_action: item.recommended_action.clone(),
            task_title: None,
        }));
    }

    match action {
        Some(RecommendedAction::NewTask) => Ok(ajax_tui::ActionOutcome::Message(
            "select a project, then choose new task to enter a task name".to_string(),
        )),
        Some(RecommendedAction::Status) => {
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
            item.recommended_action
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
    if pending.recommended_action == RecommendedAction::NewTask.as_str() {
        let title = pending.task_title.clone().ok_or_else(|| {
            CliError::CommandFailed(
                "new task title is required before cockpit can create the task".to_string(),
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

    let action = RecommendedAction::from_label(pending.recommended_action.as_str());
    let operation = match action {
        Some(action) => TaskCommandOperation::from_recommended_action(action),
        None => None,
    };
    let Some(operation) = operation else {
        match action {
            Some(
                RecommendedAction::NewTask
                | RecommendedAction::SelectProject
                | RecommendedAction::Status,
            )
            | None => {
                return Err(CliError::CommandFailed(format!(
                    "unknown cockpit action: {}",
                    pending.recommended_action
                )));
            }
            Some(_) => unreachable!("task actions are mapped by TaskCommandOperation"),
        }
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

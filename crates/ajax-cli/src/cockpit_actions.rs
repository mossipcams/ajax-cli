use ajax_core::{
    adapters::{CommandRunner, TmuxAdapter},
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
    let handle = &item.task_handle;
    let action = RecommendedAction::from_label(item.recommended_action.as_str());

    if let Some(operation) = action.and_then(TaskCommandOperation::from_recommended_action) {
        if operation == TaskCommandOperation::Clean {
            let plan = operation
                .plan(context, handle)
                .map_err(command_error_as_io)?;
            commands::execute_plan(&plan, !plan.requires_confirmation, runner)
                .map_err(command_error_as_io)?;
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
        Some(RecommendedAction::Reconcile) => {
            let response = commands::reconcile_external(context, runner).map_err(|error| {
                let message = match command_error(error) {
                    CliError::CommandFailed(message)
                    | CliError::CommandFailedAfterStateChange(message)
                    | CliError::JsonSerialization(message)
                    | CliError::ContextLoad(message)
                    | CliError::ContextSave(message) => message,
                };
                std::io::Error::other(message)
            })?;
            *state_changed |= response.tasks_changed > 0;
            Ok(ajax_tui::ActionOutcome::Refresh {
                repos: commands::list_repos(context),
                tasks: commands::list_tasks(context, None),
                inbox: commands::inbox(context),
            })
        }
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
    if pending.recommended_action == RecommendedAction::NewTask.as_str() {
        let title = pending.task_title.clone().ok_or_else(|| {
            CliError::CommandFailed(
                "new task title is required before cockpit can run workmux".to_string(),
            )
        })?;
        let request = commands::NewTaskRequest {
            repo: pending.task_handle.clone(),
            title,
            agent: "codex".to_string(),
        };
        let plan = commands::new_task_plan(context, request.clone()).map_err(command_error)?;
        install_ajax_return_hotkey(runner);
        let (outputs, task) = execute_new_task_plan(context, runner, &request, &plan, true)?;
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
            Some(RecommendedAction::Reconcile) => {
                let response =
                    commands::reconcile_external(context, runner).map_err(command_error)?;
                *state_changed |= response.tasks_changed > 0;
                return Ok(PendingCockpitOutcome::ReturnToCockpit);
            }
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
        .plan(context, &pending.task_handle)
        .map_err(command_error)?;
    if operation == TaskCommandOperation::Open {
        install_ajax_return_hotkey(runner);
    }
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

fn install_ajax_return_hotkey(runner: &mut impl CommandRunner) {
    let tmux = TmuxAdapter::new("tmux");
    let Ok(output) = runner.run(&tmux.current_client_target()) else {
        return;
    };
    if output.status_code != 0 {
        return;
    }

    let target = output.stdout.trim();
    if target.is_empty() {
        return;
    }

    for command in [
        tmux.bind_ajax_return_prefix(),
        tmux.bind_ajax_return_key(target),
        tmux.bind_ajax_return_fallback(),
    ] {
        let _ = runner.run(&command);
    }
}

use super::TaskSessionDirectory;
use crate::slices::operate::{OperateError, OperateOutcome};
use ajax_core::{
    adapters::CommandRunError,
    agent_notification::{AgentNotification, AgentNotificationDeliveryStatus},
    commands::{CommandContext, CommandError},
    models::Task,
    registry::InMemoryRegistry,
    remediation::{self, RemediationError},
};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) async fn deliver(
    directory: &Arc<TaskSessionDirectory>,
    task: &Task,
    notification: &AgentNotification,
) -> Result<AgentNotificationDeliveryStatus, String> {
    submit_chat_prompt(
        directory,
        task,
        notification.id().to_string(),
        notification.prompt(),
    )
    .await
}

pub(crate) async fn deliver_remediation(
    directory: &Arc<TaskSessionDirectory>,
    context: &CommandContext<InMemoryRegistry>,
    task_handle: &str,
    remediation_id: &str,
    skill_path: impl FnOnce(&str) -> Option<String>,
) -> Result<OperateOutcome, OperateError> {
    let (task, brief) =
        remediation::chat_remediation_brief(context, task_handle, remediation_id, skill_path)
            .map_err(remediation_error)?;
    let client_message_id = format!(
        "remediation:{remediation_id}:{}:{}",
        task.qualified_handle(),
        unix_nanos()
    );
    let status = submit_chat_prompt(directory, &task, client_message_id, brief)
        .await
        .map_err(submit_error)?;
    let delivery = match status {
        AgentNotificationDeliveryStatus::Queued => "queued behind the active chat turn",
        _ => "sent to chat task",
    };
    Ok(OperateOutcome {
        state_changed: false,
        output: format!("{remediation_id}: skill brief {delivery}"),
    })
}

async fn submit_chat_prompt(
    directory: &Arc<TaskSessionDirectory>,
    task: &Task,
    client_message_id: String,
    text: String,
) -> Result<AgentNotificationDeliveryStatus, String> {
    let handle = task.qualified_handle();
    let model = task.session_model().unwrap_or("auto");
    directory
        .acquire(&handle, &task.worktree_path, model, task.selected_agent)
        .await?;
    let busy = directory
        .attach_snapshot(&handle, model.to_string(), None)
        .await
        .snapshot
        .turn_state
        == "busy";
    let result = directory
        .submit_prompt_with_id(&handle, client_message_id, text, Vec::new())
        .await;
    directory.release(&handle).await;
    result.map(|()| {
        if busy {
            AgentNotificationDeliveryStatus::Queued
        } else {
            AgentNotificationDeliveryStatus::Accepted
        }
    })
}

fn remediation_error(error: RemediationError) -> OperateError {
    match error {
        RemediationError::UnknownRemediation(id) => OperateError::UnknownAction(id),
        RemediationError::TaskNotFound(handle) => {
            OperateError::Command(CommandError::TaskNotFound(handle), false)
        }
        RemediationError::UnsupportedCapability(message) => {
            OperateError::UnsupportedCapability(message)
        }
        RemediationError::CommandRun(message) => submit_error(message),
    }
}

fn submit_error(message: String) -> OperateError {
    OperateError::Command(
        CommandError::CommandRun(CommandRunError::SpawnFailed(message)),
        false,
    )
}

fn unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos())
}

#[cfg(test)]
#[path = "ci_agent_delivery/tests.rs"]
mod tests;

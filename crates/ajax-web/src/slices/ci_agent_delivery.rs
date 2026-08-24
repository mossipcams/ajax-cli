use super::TaskSessionDirectory;
use ajax_core::{
    agent_notification::{AgentNotification, AgentNotificationDeliveryStatus},
    models::Task,
};
use std::sync::Arc;

pub(crate) async fn deliver(
    directory: &Arc<TaskSessionDirectory>,
    task: &Task,
    notification: &AgentNotification,
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
        .submit_prompt_with_id(
            &handle,
            notification.id().to_string(),
            notification.prompt(),
            Vec::new(),
        )
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

#[cfg(test)]
#[path = "ci_agent_delivery/tests.rs"]
mod tests;

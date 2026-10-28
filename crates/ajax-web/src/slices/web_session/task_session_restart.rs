use super::TaskSessionDirectory;
use ajax_core::{
    commands::CommandContext,
    registry::{InMemoryRegistry, Registry as _},
};
use std::sync::Arc;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RestartError {
    TaskNotFound,
    WorktreeMissing,
    Spawn(String),
}

pub(crate) async fn restart_session(
    directory: &Arc<TaskSessionDirectory>,
    context: &CommandContext<InMemoryRegistry>,
    handle: &str,
) -> Result<(), RestartError> {
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == handle)
        .ok_or(RestartError::TaskNotFound)?;
    if !task.worktree_path.exists() {
        return Err(RestartError::WorktreeMissing);
    }
    let holders = directory.attached_holders(handle).await;
    directory.detach_session(handle).await;
    let model = task.session_model().unwrap_or("auto");
    for _ in 0..holders.max(1) {
        directory
            .acquire(handle, &task.worktree_path, model, task.selected_agent)
            .await
            .map_err(RestartError::Spawn)?;
    }
    if holders == 0 {
        directory.release(handle).await;
    }
    Ok(())
}

use super::{CommandContext, CommandError};
use crate::{
    models::{LifecycleStatus, Task},
    registry::Registry,
};

pub(super) fn find_task<'a, R: Registry>(
    context: &'a CommandContext<R>,
    qualified_handle: &str,
) -> Result<&'a Task, CommandError> {
    context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
        .ok_or_else(|| CommandError::TaskNotFound(qualified_handle.to_string()))
}

pub(super) fn task_repo_path<R: Registry>(
    context: &CommandContext<R>,
    task: &Task,
) -> Option<String> {
    context
        .config
        .repos
        .iter()
        .find(|repo| repo.name == task.repo)
        .map(|repo| repo.path.display().to_string())
}

pub(super) fn update_task_lifecycle<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    status: LifecycleStatus,
) -> Result<(), CommandError> {
    let task_id = find_task(context, qualified_handle)?.id.clone();
    context
        .registry
        .update_lifecycle(&task_id, status)
        .map_err(CommandError::Registry)
}

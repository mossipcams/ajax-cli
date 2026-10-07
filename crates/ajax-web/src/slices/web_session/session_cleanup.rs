use ajax_core::{commands::CommandContext, models::LifecycleStatus, registry::Registry};
use std::{collections::HashSet, path::Path};

use crate::adapters::web_session_store;

#[cfg(test)]
pub fn is_session_owned<R: Registry>(context: &CommandContext<R>, handle: &str) -> bool {
    context.registry.list_tasks().into_iter().any(|task| {
        task.qualified_handle() == handle && task.lifecycle_status != LifecycleStatus::Removed
    })
}

pub fn owned_session_handles<R: Registry>(context: &CommandContext<R>) -> HashSet<String> {
    context
        .registry
        .list_tasks()
        .into_iter()
        .filter(|task| task.lifecycle_status != LifecycleStatus::Removed)
        .map(|task| task.qualified_handle())
        .collect()
}

pub fn prune_stale_persisted_sessions(state_dir: &Path, owned: &HashSet<String>) -> Vec<String> {
    web_session_store::list_persisted_handles(state_dir)
        .into_iter()
        .filter(|handle| !owned.contains(handle))
        .filter(|handle| web_session_store::delete_session(state_dir, handle))
        .collect()
}

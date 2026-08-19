//! Registry-backed ownership for persisted orchestration sessions ([#977]).

use ajax_core::{commands::CommandContext, models::LifecycleStatus, registry::Registry};
use std::{collections::HashSet, path::Path};

use crate::adapters::web_session_store;

/// A task owns its session while it exists in the registry and is not `Removed`.
#[cfg(test)]
pub fn is_session_owned<R: Registry>(context: &CommandContext<R>, handle: &str) -> bool {
    context.registry.list_tasks().into_iter().any(|task| {
        task.qualified_handle() == handle && task.lifecycle_status != LifecycleStatus::Removed
    })
}

/// Qualified handles with an active registry owner.
pub fn owned_session_handles<R: Registry>(context: &CommandContext<R>) -> HashSet<String> {
    context
        .registry
        .list_tasks()
        .into_iter()
        .filter(|task| task.lifecycle_status != LifecycleStatus::Removed)
        .map(|task| task.qualified_handle())
        .collect()
}

/// Delete persisted transcripts whose handle is not registry-owned.
pub fn prune_stale_persisted_sessions(state_dir: &Path, owned: &HashSet<String>) -> Vec<String> {
    web_session_store::list_persisted_handles(state_dir)
        .into_iter()
        .filter(|handle| !owned.contains(handle))
        .filter(|handle| web_session_store::delete_session(state_dir, handle))
        .collect()
}

use super::TaskSessionDirectory;
use crate::adapters::web_session_store::prompt_ledger::{self, PromptPhase};
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
) -> Result<bool, RestartError> {
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
    let in_flight = (holders > 0)
        .then(|| {
            prompt_ledger::try_load(
                &directory.storage.registry_store(),
                &directory.storage.state_dir,
                handle,
            )
            .ok()
            .and_then(|ledger| {
                ledger
                    .entries
                    .iter()
                    .find(|entry| entry.phase == PromptPhase::Dispatching)
                    .cloned()
            })
        })
        .flatten()
        .and_then(|entry| {
            let content_blocks = entry
                .content_blocks
                .iter()
                .map(|value| serde_json::from_value(value.clone()))
                .collect::<Result<Vec<super::prompt_content::PromptContentBlockWire>, _>>()
                .ok()?;
            Some((entry, content_blocks))
        });
    directory.detach_session(handle).await;
    if let Some((entry, _)) = &in_flight {
        settle_resent_prompt(directory, handle, &entry.client_message_id);
    }
    let model = task.session_model().unwrap_or("auto");
    for _ in 0..holders.max(1) {
        directory
            .acquire(handle, &task.worktree_path, model, task.selected_agent)
            .await
            .map_err(RestartError::Spawn)?;
    }

    let mut resent = false;
    if let Some((entry, content_blocks)) = in_flight {
        let resumed_phase = prompt_ledger::try_load(
            &directory.storage.registry_store(),
            &directory.storage.state_dir,
            handle,
        )
        .ok()
        .and_then(|ledger| {
            ledger
                .entry(&entry.client_message_id)
                .map(|entry| entry.phase)
        });
        if resumed_phase == Some(PromptPhase::Interrupted) {
            directory
                .submit_prompt_with_id(
                    handle,
                    format!("{}-restart", entry.client_message_id),
                    entry.prompt_text,
                    content_blocks,
                )
                .await
                .map_err(RestartError::Spawn)?;
            resent = true;
        }
    }

    if holders == 0 {
        directory.release(handle).await;
    }
    Ok(resent)
}

fn settle_resent_prompt(directory: &TaskSessionDirectory, handle: &str, client_message_id: &str) {
    let store = directory.storage.registry_store();
    let state_dir = &directory.storage.state_dir;
    if let Ok(mut ledger) = prompt_ledger::try_load(&store, state_dir, handle) {
        if ledger.mark_interrupted(client_message_id) {
            let _ = prompt_ledger::try_persist(&store, state_dir, handle, &ledger);
        }
    }
}

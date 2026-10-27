use std::path::Path;

use ajax_core::adapters::acp_launch_for_agent;
use ajax_core::models::{LiveStatusKind, Task};
use ajax_core::registry::{Registry, SqliteRegistryStore};

use super::SessionServerEvent;
use crate::adapters::web_session_store::prompt_ledger as ledger_module;

pub(crate) struct RecoveryResult {
    pub(crate) recovered_handles: Vec<String>,
    pub(crate) complete: bool,
}

pub(super) fn interrupted_by_restart_event(prompt_id: &str) -> SessionServerEvent {
    SessionServerEvent::Error {
        message: format!("Prompt {prompt_id} was interrupted by host restart and was not retried."),
    }
}

pub(super) fn interrupted_by_restart_events(prompt_id: &str) -> Vec<SessionServerEvent> {
    vec![
        SessionServerEvent::PromptAccepted {
            client_message_id: prompt_id.to_string(),
        },
        interrupted_by_restart_event(prompt_id),
    ]
}

pub(crate) fn recover_interrupted_prompts(
    store: &SqliteRegistryStore,
    state_dir: &Path,
    current_boot_id: &str,
) -> RecoveryResult {
    let mut result = RecoveryResult {
        recovered_handles: Vec::new(),
        complete: true,
    };
    let handles = match store.list_run_task_ids() {
        Ok(handles) => handles,
        Err(error) => {
            tracing::warn!(target: "ajax_web", %error, "startup recovery: cannot list run task ids");
            result.complete = false;
            return result;
        }
    };
    for handle in handles {
        match store.latest_run_for_task(&handle) {
            Ok(Some(run)) if run.owner_boot_id.as_deref() == Some(current_boot_id) => continue,
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(
                    target: "ajax_web",
                    handle = %handle,
                    %error,
                    "startup recovery: cannot read run row"
                );
                result.complete = false;
                continue;
            }
        }

        let mut ledger = match ledger_module::try_load(store, state_dir, &handle) {
            Ok(ledger) => ledger,
            Err(error) => {
                tracing::warn!(
                    target: "ajax_web",
                    handle = %handle,
                    %error,
                    "startup recovery: cannot load prompt ledger"
                );
                result.complete = false;
                continue;
            }
        };

        let (_, interrupted) = ledger.recover_after_restart();
        if interrupted.is_empty() {
            continue;
        }

        let events: Vec<SessionServerEvent> = interrupted
            .iter()
            .flat_map(|id| interrupted_by_restart_events(id))
            .collect();
        match ledger_module::try_persist_with_events(store, state_dir, &handle, &ledger, &events) {
            Ok(()) => result.recovered_handles.push(handle),
            Err(error) => {
                tracing::warn!(
                    target: "ajax_web",
                    handle = %handle,
                    %error,
                    "startup recovery: cannot persist recovered prompt ledger"
                );
                result.complete = false;
            }
        }
    }
    settle_running_chats(store, current_boot_id, &mut result);
    result
}

fn settle_running_chats(
    store: &SqliteRegistryStore,
    current_boot_id: &str,
    result: &mut RecoveryResult,
) {
    let registry = match store.load_tasks_only() {
        Ok(registry) => registry,
        Err(error) => {
            tracing::warn!(target: "ajax_web", %error, "startup recovery: cannot load tasks");
            result.complete = false;
            return;
        }
    };
    for task in registry.list_tasks() {
        if !persisted_chat_running(task) {
            continue;
        }
        let handle = task.qualified_handle();
        if result.recovered_handles.contains(&handle) {
            continue;
        }
        match store.latest_run_for_task(&handle) {
            Ok(Some(run)) if run.owner_boot_id.as_deref() == Some(current_boot_id) => continue,
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(
                    target: "ajax_web",
                    handle = %handle,
                    %error,
                    "startup recovery: cannot read run row for running chat"
                );
                result.complete = false;
                continue;
            }
        }
        result.recovered_handles.push(handle);
    }
}

pub(super) fn persisted_chat_running(task: &Task) -> bool {
    task.live_status.as_ref().is_some_and(|live| {
        matches!(
            live.kind,
            LiveStatusKind::AgentRunning
                | LiveStatusKind::CommandRunning
                | LiveStatusKind::TestsRunning
        )
    }) && task.skip_interactive_agent()
        && acp_launch_for_agent(task.selected_agent).is_some()
}

pub(super) fn persisted_chat_running_for(store: &SqliteRegistryStore, handle: &str) -> bool {
    store.load_tasks_only().is_ok_and(|registry| {
        registry
            .list_tasks()
            .into_iter()
            .any(|task| task.qualified_handle() == handle && persisted_chat_running(task))
    })
}

pub(crate) struct QueuedScan {
    pub(crate) handles: Vec<String>,
    pub(crate) complete: bool,
}

pub(crate) fn queued_prompt_handles(
    store: &SqliteRegistryStore,
    state_dir: &Path,
    current_boot_id: &str,
) -> QueuedScan {
    let mut result = QueuedScan {
        handles: Vec::new(),
        complete: true,
    };
    let handles = match store.list_run_task_ids() {
        Ok(handles) => handles,
        Err(error) => {
            tracing::warn!(target: "ajax_web", %error, "startup dispatch scan: cannot list run task ids");
            result.complete = false;
            return result;
        }
    };
    for handle in handles {
        match store.latest_run_for_task(&handle) {
            Ok(Some(run)) if run.owner_boot_id.as_deref() == Some(current_boot_id) => continue,
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(
                    target: "ajax_web",
                    handle = %handle,
                    %error,
                    "startup dispatch scan: cannot read run row"
                );
                result.complete = false;
                continue;
            }
        }

        let ledger = match ledger_module::try_load(store, state_dir, &handle) {
            Ok(ledger) => ledger,
            Err(error) => {
                tracing::warn!(
                    target: "ajax_web",
                    handle = %handle,
                    %error,
                    "startup dispatch scan: cannot load prompt ledger"
                );
                result.complete = false;
                continue;
            }
        };

        if ledger.queued_count() > 0 {
            result.handles.push(handle);
        }
    }
    result.handles.sort();
    result
}

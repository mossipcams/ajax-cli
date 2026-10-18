//! Startup-time recovery for chat prompts cut off by a host restart.
//!
//! A prompt that reached the `Dispatching` phase when its owning host died is
//! gone: the agent child process no longer exists and nothing will retry the
//! turn. This module applies the same rule as
//! `task_session_exit::recover_prompt_ledger` (which runs per session at
//! attach time) to *every* chat run in the registry: mark those prompts
//! `Interrupted`, append one transcript note per prompt, and leave `Queued`
//! prompts untouched for the next acquire. It never spawns an agent; it only
//! rewrites durable state. Like the orphan sweep, it never fails startup: every
//! error is logged with `tracing::warn` and the scan continues. Runs whose
//! `owner_boot_id` equals this host's boot id are live here and are never
//! touched.

use std::path::Path;

use ajax_core::registry::SqliteRegistryStore;

use super::SessionServerEvent;
use crate::adapters::web_session_store::prompt_ledger as ledger_module;

pub(crate) struct RecoveryResult {
    pub(crate) recovered_handles: Vec<String>,
    pub(crate) complete: bool,
}

/// The transcript note appended when a prompt in flight is cut off by a host restart.
pub(super) fn interrupted_by_restart_event(prompt_id: &str) -> SessionServerEvent {
    SessionServerEvent::Error {
        message: format!("Prompt {prompt_id} was interrupted by host restart and was not retried."),
    }
}

/// For every run whose `owner_boot_id` differs from `current_boot_id`: load the
/// prompt ledger, apply [`crate::adapters::web_session_store::prompt_ledger::PromptLedger::recover_after_restart`],
/// and when any prompt was `Dispatching`, persist the interrupted ledger together
/// with one note event per prompt in one transaction. Returns the handles
/// (task ids) of every run where at least one prompt was interrupted and
/// persisted successfully, in scan order, each handle once. Runs owned by this
/// host are never
/// touched; `Queued` prompts are left as they are for the next acquire. Every
/// error is logged and the scan continues, so this function never panics and
/// never fails startup. Any listing, load, or persist error makes `complete` false.
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
            // Live in this host: leave it alone.
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
            .map(|id| interrupted_by_restart_event(id))
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
    result
}

/// The outcome of the startup dispatch scan: every run whose latest run row
/// is not owned by `current_boot_id` (the same skip rule as
/// [`recover_interrupted_prompts`]) and whose prompt ledger still has at least
/// one `Queued` prompt. Handles are returned sorted for determinism. Every
/// load error is logged with `tracing::warn` and the scan continues, so this
/// function never panics; any listing, run-row, or ledger-load error makes
/// `complete` false while the handles found so far are still returned.
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
            // Live in this host: leave it to its live session.
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

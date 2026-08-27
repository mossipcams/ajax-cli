//! Unexpected ACP child exit reconciliation and durable exit-interruption retry.

use super::prompt_content;
use super::task_session::{dispatch_queued_prompt, TaskSessionState};
use super::{
    acp_drain::PromptTerminalOutcome, prompt_content::PromptContentBlockWire, QueuedPrompt,
    SessionServerEvent,
};
use crate::adapters::web_session_store::prompt_ledger::PromptLedger;

pub(super) const ACP_PROCESS_EXITED: &str = "ACP process exited";

#[derive(Debug, Clone)]
pub(super) struct PendingExitInterruption {
    pub client_message_id: String,
    pub persist_error_reported: bool,
    pub interruption_error_reported: bool,
}

pub(super) fn is_acp_exit_error(event: &SessionServerEvent) -> bool {
    matches!(
        event,
        SessionServerEvent::Error { message } if message == ACP_PROCESS_EXITED
    )
}

pub(super) fn persist_prompt_ledger(state: &TaskSessionState) -> Result<(), String> {
    crate::adapters::web_session_store::prompt_ledger::persist(
        &state.state_dir,
        &state.qualified_handle,
        &state.prompt_ledger,
    )
    .map_err(|error| format!("failed to persist prompt ownership: {error}"))
}

pub(super) fn persist_ledger_update(
    state: &mut TaskSessionState,
    update: impl FnOnce(&mut PromptLedger),
) -> Result<(), String> {
    let mut next = state.prompt_ledger.clone();
    update(&mut next);
    crate::adapters::web_session_store::prompt_ledger::persist(
        &state.state_dir,
        &state.qualified_handle,
        &next,
    )
    .map_err(|error| format!("failed to persist prompt ownership: {error}"))?;
    state.prompt_ledger = next;
    Ok(())
}

pub(super) fn has_healthy_client(state: &mut TaskSessionState) -> bool {
    if !state.acp_alive {
        return false;
    }
    let Some(client) = state.client.as_mut() else {
        return false;
    };
    if client.host_exited() {
        state.acp_alive = false;
        false
    } else {
        true
    }
}

pub(super) fn retry_pending_exit_interruption(state: &mut TaskSessionState) {
    let Some(pending) = state.pending_exit_interruption.clone() else {
        return;
    };
    match persist_ledger_update(state, |ledger| {
        ledger.mark_interrupted(&pending.client_message_id);
    }) {
        Ok(()) => {
            report_exit_interruption(state, &pending.client_message_id, true);
            state.active_prompt = None;
            state.pending_exit_interruption = None;
            try_dispatch_next_if_idle(state);
        }
        Err(message) => {
            let Some(slot) = state.pending_exit_interruption.as_mut() else {
                return;
            };
            if !slot.persist_error_reported {
                slot.persist_error_reported = true;
                state.append_to_log(vec![SessionServerEvent::Error { message }]);
            }
        }
    }
}

pub(super) fn reconcile_unexpected_child_exit(
    state: &mut TaskSessionState,
    host_exit_from_drain: bool,
) {
    if state.child_exit_reconciled {
        return;
    }
    state.child_exit_reconciled = true;
    state.acp_alive = false;

    if !state.suppress_exit_evidence && !host_exit_from_drain {
        state.append_to_log(vec![SessionServerEvent::Error {
            message: ACP_PROCESS_EXITED.to_string(),
        }]);
    }

    if state
        .active_prompt
        .as_ref()
        .is_some_and(|active| active.terminal.is_some())
    {
        try_finalize_active_prompt(state);
        return;
    }

    let Some(client_message_id) = state
        .active_prompt
        .as_ref()
        .and_then(|active| active.client_message_id.clone())
    else {
        state.active_prompt = None;
        return;
    };

    match persist_ledger_update(state, |ledger| {
        ledger.mark_interrupted(&client_message_id);
    }) {
        Ok(()) => {
            report_exit_interruption(state, &client_message_id, false);
            state.active_prompt = None;
            state.pending_exit_interruption = None;
        }
        Err(message) => {
            state.pending_exit_interruption = Some(PendingExitInterruption {
                client_message_id,
                persist_error_reported: false,
                interruption_error_reported: false,
            });
            if let Some(active) = state.active_prompt.as_mut() {
                if !active.persist_error_reported {
                    active.persist_error_reported = true;
                    state.append_to_log(vec![SessionServerEvent::Error { message }]);
                }
            }
        }
    }
}

fn report_exit_interruption(
    state: &mut TaskSessionState,
    client_message_id: &str,
    from_retry: bool,
) {
    if from_retry {
        let Some(pending) = state.pending_exit_interruption.as_mut() else {
            return;
        };
        if pending.interruption_error_reported {
            return;
        }
        pending.interruption_error_reported = true;
    }
    state.append_to_log(vec![SessionServerEvent::Error {
        message: format!(
            "Prompt {client_message_id} was interrupted because the ACP process exited and was not retried."
        ),
    }]);
}

pub(super) fn interrupt_active_prompt(state: &mut TaskSessionState) -> Result<(), String> {
    if state
        .active_prompt
        .as_ref()
        .is_some_and(|active| active.terminal.is_some())
    {
        try_finalize_active_prompt(state);
        if state.active_prompt.is_some() {
            return Err("failed to persist prompt terminal state".to_string());
        }
        return Ok(());
    }
    let Some(client_message_id) = state
        .active_prompt
        .as_ref()
        .and_then(|active| active.client_message_id.clone())
    else {
        state.active_prompt = None;
        return Ok(());
    };
    persist_ledger_update(state, |ledger| {
        ledger.mark_interrupted(&client_message_id);
    })?;
    state.active_prompt = None;
    Ok(())
}

pub(super) fn try_finalize_active_prompt(state: &mut TaskSessionState) {
    let Some(active) = state.active_prompt.as_ref() else {
        return;
    };
    let Some(terminal) = active.terminal.as_ref() else {
        return;
    };
    let client_message_id = active.client_message_id.clone();
    let outcome = terminal.outcome;

    if let Some(client_message_id) = client_message_id {
        let result = persist_ledger_update(state, |ledger| match outcome {
            PromptTerminalOutcome::Success | PromptTerminalOutcome::Cancelled => {
                ledger.mark_completed(&client_message_id);
            }
            PromptTerminalOutcome::Failed => {
                ledger.mark_interrupted(&client_message_id);
            }
        });
        if let Err(message) = result {
            let active = state
                .active_prompt
                .as_mut()
                .expect("active prompt retained");
            if !active.persist_error_reported {
                active.persist_error_reported = true;
                state.append_to_log(vec![SessionServerEvent::Error { message }]);
            }
            return;
        }
    }

    let terminal = state
        .active_prompt
        .take()
        .and_then(|active| active.terminal)
        .expect("terminal prompt retained");
    if !terminal.events.is_empty() {
        state.append_to_log(terminal.events);
    }
}

pub(super) fn recover_prompt_ledger(state: &mut TaskSessionState) -> Result<(), String> {
    retry_pending_exit_interruption(state);
    if state.pending_exit_interruption.is_some() {
        return Err("prompt ownership recovery pending".to_string());
    }
    if state.active_prompt.is_some() {
        return Err("active prompt ownership is not durable".to_string());
    }

    state.prompt_ledger = crate::adapters::web_session_store::prompt_ledger::load(
        &state.state_dir,
        &state.qualified_handle,
    )
    .map_err(|error| error.to_string())?;
    state.ledger_unusable = None;
    let caps = state
        .session_prompt_capabilities
        .clone()
        .unwrap_or_else(prompt_content::default_prompt_capabilities);
    let (queued_entries, interrupted) = state.prompt_ledger.recover_after_restart();
    if !interrupted.is_empty() {
        persist_prompt_ledger(state)?;
    }
    let mut recovery_events = Vec::new();
    for id in interrupted {
        recovery_events.push(SessionServerEvent::Error {
            message: format!("Prompt {id} was interrupted by host restart and was not retried."),
        });
    }
    state.queued.clear();
    for entry in queued_entries {
        let wire_blocks: Vec<PromptContentBlockWire> = entry
            .content_blocks
            .iter()
            .filter_map(|value| serde_json::from_value(value.clone()).ok())
            .collect();
        let payload =
            prompt_content::build_prompt_payload(&entry.prompt_text, &wire_blocks, &caps)?;
        state.queued.push_back(QueuedPrompt {
            client_message_id: entry.client_message_id,
            transcript_text: entry.transcript_text,
            prompt_text: entry.prompt_text,
            wire_blocks,
            blocks: payload.blocks,
        });
    }
    if !recovery_events.is_empty() {
        state.append_to_log(recovery_events);
    }
    Ok(())
}

pub(super) fn try_dispatch_next_if_idle(state: &mut TaskSessionState) {
    if state.active_prompt.is_some() || state.pending_exit_interruption.is_some() {
        return;
    }
    if !has_healthy_client(state) {
        return;
    }
    let Some(client) = state.client.as_ref() else {
        return;
    };
    if client.prompt_in_flight() {
        return;
    }
    let Some(next) = state.queued.front().cloned() else {
        return;
    };
    match dispatch_queued_prompt(state, &next) {
        Ok(()) => {
            state.queued.pop_front();
            state.queue_persist_error_reported = false;
        }
        Err(message) if !state.queue_persist_error_reported => {
            state.queue_persist_error_reported = true;
            state.append_to_log(vec![SessionServerEvent::Error { message }]);
        }
        Err(_) => {}
    }
}

#[cfg(test)]
pub(super) fn ledger_phase(
    state_dir: &std::path::Path,
    handle: &str,
    id: &str,
) -> Option<crate::adapters::web_session_store::prompt_ledger::PromptPhase> {
    crate::adapters::web_session_store::prompt_ledger::load(state_dir, handle)
        .ok()?
        .entry(id)
        .map(|entry| entry.phase)
}

//! Unexpected ACP child exit reconciliation and durable exit-interruption retry.

use super::acp_drain::{PromptTerminal, PromptTerminalOutcome};
use super::prompt_content::{self, PromptContentBlockWire};
use super::prompt_queue::{ActivePrompt, PendingExitInterruption};
use super::task_session::TaskSessionState;
use super::{
    apply_cancel_to_queue, QueuedPrompt, SessionError, SessionServerEvent, MAX_QUEUED_PROMPTS,
};
use crate::adapters::web_session_store::prompt_ledger::{PromptLedger, PromptPhase};

pub(super) const ACP_PROCESS_EXITED: &str = "ACP process exited";

pub(super) fn is_acp_exit_error(event: &SessionServerEvent) -> bool {
    matches!(
        event,
        SessionServerEvent::Error { message } if message == ACP_PROCESS_EXITED
    )
}

pub(super) fn persist_prompt_ledger(state: &TaskSessionState) -> Result<(), SessionError> {
    crate::adapters::web_session_store::prompt_ledger::persist(
        &state.state_dir,
        &state.qualified_handle,
        &state.prompts.prompt_ledger,
    )
    .map_err(|error| SessionError::persist(format!("failed to persist prompt ownership: {error}")))
}

pub(super) fn persist_ledger_update(
    state: &mut TaskSessionState,
    update: impl FnOnce(&mut PromptLedger),
) -> Result<(), SessionError> {
    let mut next = state.prompts.prompt_ledger.clone();
    update(&mut next);
    crate::adapters::web_session_store::prompt_ledger::persist(
        &state.state_dir,
        &state.qualified_handle,
        &next,
    )
    .map_err(|error| {
        SessionError::persist(format!("failed to persist prompt ownership: {error}"))
    })?;
    state.prompts.prompt_ledger = next;
    Ok(())
}

pub(super) fn has_healthy_client(state: &mut TaskSessionState) -> bool {
    if !state.acp.acp_alive {
        return false;
    }
    let Some(client) = state.acp.client.as_mut() else {
        return false;
    };
    if client.host_exited() {
        state.acp.acp_alive = false;
        false
    } else {
        true
    }
}

pub(super) fn retry_pending_exit_interruption(state: &mut TaskSessionState) {
    let Some(pending) = state.prompts.pending_exit_interruption.clone() else {
        return;
    };
    match persist_ledger_update(state, |ledger| {
        ledger.mark_interrupted(&pending.client_message_id);
    }) {
        Ok(()) => {
            report_exit_interruption(state, &pending.client_message_id, true);
            state.prompts.active_prompt = None;
            state.prompts.pending_exit_interruption = None;
            try_dispatch_next_if_idle(state);
        }
        Err(error) => {
            let Some(slot) = state.prompts.pending_exit_interruption.as_mut() else {
                return;
            };
            if !slot.persist_error_reported {
                slot.persist_error_reported = true;
                let _ = state.append_to_log(vec![SessionServerEvent::Error {
                    message: error.to_string(),
                }]);
            }
        }
    }
}

pub(super) fn reconcile_unexpected_child_exit(
    state: &mut TaskSessionState,
    host_exit_from_drain: bool,
) {
    if state.prompts.child_exit_reconciled {
        return;
    }
    state.prompts.child_exit_reconciled = true;
    state.acp.acp_alive = false;

    if !state.prompts.suppress_exit_evidence && !host_exit_from_drain {
        let _ = state.append_to_log(vec![SessionServerEvent::Error {
            message: ACP_PROCESS_EXITED.to_string(),
        }]);
    }

    let client_message_id = state
        .prompts
        .active_prompt
        .as_ref()
        .and_then(|active| active.client_message_id.clone());
    let terminal_outcome = state
        .prompts
        .active_prompt
        .as_ref()
        .and_then(|active| active.terminal.as_ref().map(|terminal| terminal.outcome));

    if matches!(
        terminal_outcome,
        Some(PromptTerminalOutcome::Success | PromptTerminalOutcome::Cancelled)
    ) {
        try_finalize_active_prompt(state);
        return;
    }

    let Some(client_message_id) = client_message_id else {
        state.prompts.active_prompt = None;
        return;
    };

    if state
        .prompts
        .active_prompt
        .as_ref()
        .is_some_and(|active| active.terminal.is_some())
    {
        try_finalize_active_prompt(state);
    }

    match persist_ledger_update(state, |ledger| {
        ledger.mark_interrupted(&client_message_id);
    }) {
        Ok(()) => {
            report_exit_interruption(state, &client_message_id, false);
            state.prompts.active_prompt = None;
            state.prompts.pending_exit_interruption = None;
        }
        Err(error) => {
            state.prompts.pending_exit_interruption = Some(PendingExitInterruption {
                client_message_id,
                persist_error_reported: false,
                interruption_error_reported: false,
            });
            if let Some(active) = state.prompts.active_prompt.as_mut() {
                if !active.persist_error_reported {
                    active.persist_error_reported = true;
                    let _ = state.append_to_log(vec![SessionServerEvent::Error {
                        message: error.to_string(),
                    }]);
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
        let Some(pending) = state.prompts.pending_exit_interruption.as_mut() else {
            return;
        };
        if pending.interruption_error_reported {
            return;
        }
        pending.interruption_error_reported = true;
    }
    let _ = state.append_to_log(vec![SessionServerEvent::Error {
        message: format!(
            "Prompt {client_message_id} was interrupted because the ACP process exited and was not retried."
        ),
    }]);
}

pub(super) fn interrupt_active_prompt(state: &mut TaskSessionState) -> Result<(), SessionError> {
    if state
        .prompts
        .active_prompt
        .as_ref()
        .is_some_and(|active| active.terminal.is_some())
    {
        try_finalize_active_prompt(state);
        if state.prompts.active_prompt.is_some() {
            return Err(SessionError::persist(
                "failed to persist prompt terminal state",
            ));
        }
        return Ok(());
    }
    let Some(client_message_id) = state
        .prompts
        .active_prompt
        .as_ref()
        .and_then(|active| active.client_message_id.clone())
    else {
        state.prompts.active_prompt = None;
        return Ok(());
    };
    persist_ledger_update(state, |ledger| {
        ledger.mark_interrupted(&client_message_id);
    })?;
    state.prompts.active_prompt = None;
    Ok(())
}

pub(super) fn try_finalize_active_prompt(state: &mut TaskSessionState) {
    let Some(active) = state.prompts.active_prompt.as_ref() else {
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
        if let Err(error) = result {
            let active = state
                .prompts
                .active_prompt
                .as_mut()
                .expect("active prompt retained");
            if !active.persist_error_reported {
                active.persist_error_reported = true;
                let _ = state.append_to_log(vec![SessionServerEvent::Error {
                    message: error.to_string(),
                }]);
            }
            return;
        }
    }

    let terminal = state
        .prompts
        .active_prompt
        .take()
        .and_then(|active| active.terminal)
        .expect("terminal prompt retained");
    if !terminal.events.is_empty() {
        let _ = state.append_to_log(terminal.events);
    }
}

pub(super) fn recover_prompt_ledger(state: &mut TaskSessionState) -> Result<(), SessionError> {
    retry_pending_exit_interruption(state);
    if state.prompts.pending_exit_interruption.is_some() {
        return Err(SessionError::persist("prompt ownership recovery pending"));
    }
    if state.prompts.active_prompt.is_some() {
        return Err(SessionError::persist(
            "active prompt ownership is not durable",
        ));
    }

    state.prompts.prompt_ledger = crate::adapters::web_session_store::prompt_ledger::load(
        &state.state_dir,
        &state.qualified_handle,
    )
    .map_err(|error| SessionError::persist(error.to_string()))?;
    state.prompts.ledger_unusable = None;
    let caps = state
        .acp
        .session_prompt_capabilities
        .clone()
        .unwrap_or_else(prompt_content::default_prompt_capabilities);
    let (queued_entries, interrupted) = state.prompts.prompt_ledger.recover_after_restart();
    if !interrupted.is_empty() {
        persist_prompt_ledger(state)?;
    }
    let mut recovery_events = Vec::new();
    for id in interrupted {
        recovery_events.push(SessionServerEvent::Error {
            message: format!("Prompt {id} was interrupted by host restart and was not retried."),
        });
    }
    state.prompts.queued.clear();
    for entry in queued_entries {
        let wire_blocks: Vec<PromptContentBlockWire> = entry
            .content_blocks
            .iter()
            .filter_map(|value| serde_json::from_value(value.clone()).ok())
            .collect();
        let payload = prompt_content::build_prompt_payload(&entry.prompt_text, &wire_blocks, &caps)
            .map_err(SessionError::protocol)?;
        state.prompts.queued.push_back(QueuedPrompt {
            client_message_id: entry.client_message_id,
            transcript_text: entry.transcript_text,
            prompt_text: entry.prompt_text,
            wire_blocks,
            blocks: payload.blocks,
        });
    }
    if !recovery_events.is_empty() {
        let _ = state.append_to_log(recovery_events);
    }
    Ok(())
}

pub(super) fn submit_prompt(
    state: &mut TaskSessionState,
    client_message_id: String,
    text: String,
    content_blocks: Vec<prompt_content::PromptContentBlockWire>,
) -> Result<(), SessionError> {
    if let Some(reason) = &state.prompts.ledger_unusable {
        return Err(SessionError::persist(format!(
            "prompt ownership unavailable: {reason}"
        )));
    }
    if let Some(reason) = &state.evidence.transcript_durability_fault {
        return Err(SessionError::persist(reason.clone()));
    }
    let caps = state
        .acp
        .session_prompt_capabilities
        .clone()
        .unwrap_or_else(prompt_content::default_prompt_capabilities);
    let payload = prompt_content::build_prompt_payload(&text, &content_blocks, &caps)
        .map_err(SessionError::protocol)?;
    let Some(client) = state.acp.client.as_ref() else {
        return Err(SessionError::protocol("session slot missing"));
    };
    if !state.acp.acp_alive {
        return Err(SessionError::protocol(
            "ACP process exited — reconnect to send prompts",
        ));
    }
    if !client_message_id.is_empty() {
        if let Some(entry) = state.prompts.prompt_ledger.entry(&client_message_id) {
            match entry.phase {
                PromptPhase::Interrupted => {
                    return Err(SessionError::operator(format!(
                        "prompt {client_message_id} was interrupted and was not executed"
                    )));
                }
                PromptPhase::Completed | PromptPhase::Queued | PromptPhase::Dispatching => {
                    state.append_to_log(vec![SessionServerEvent::PromptAccepted {
                        client_message_id,
                    }])?;
                    return Ok(());
                }
            }
        }
    }
    let user_event = SessionServerEvent::Message {
        role: "user".to_string(),
        text: payload.transcript_text.clone(),
        content_blocks: Vec::new(),
        item_id: state.stream_normalizer.fresh_item_id(),
        message_id: None,
    };
    let in_flight = state.prompts.active_prompt.is_some() || client.prompt_in_flight();
    if in_flight {
        if state.prompts.queued.len() >= MAX_QUEUED_PROMPTS {
            return Err(SessionError::operator("prompt queue is full"));
        }
        if !client_message_id.is_empty() {
            persist_ledger_update(state, |ledger| {
                ledger.upsert_queued(
                    client_message_id.clone(),
                    payload.transcript_text.clone(),
                    text.trim().to_string(),
                    wire_blocks_to_json(&content_blocks),
                );
            })?;
        }
        state.prompts.queued.push_back(super::QueuedPrompt {
            client_message_id: client_message_id.clone(),
            transcript_text: payload.transcript_text,
            prompt_text: text.trim().to_string(),
            wire_blocks: content_blocks,
            blocks: payload.blocks,
        });
        if !client_message_id.is_empty() {
            state.append_to_log(vec![SessionServerEvent::PromptAccepted {
                client_message_id,
            }])?;
        }
        return Ok(());
    }
    if !client_message_id.is_empty() {
        persist_ledger_update(state, |ledger| {
            ledger.upsert_queued(
                client_message_id.clone(),
                payload.transcript_text.clone(),
                text.trim().to_string(),
                wire_blocks_to_json(&content_blocks),
            );
            ledger.mark_dispatching(&client_message_id);
        })?;
    }
    state
        .append_to_log(vec![user_event])
        .inspect_err(|_error| {
            if !client_message_id.is_empty() {
                let _ = persist_ledger_update(state, |ledger| {
                    ledger.upsert_queued(
                        client_message_id.clone(),
                        payload.transcript_text.clone(),
                        text.trim().to_string(),
                        wire_blocks_to_json(&content_blocks),
                    );
                });
            }
        })?;
    let Some(client) = state.acp.client.as_mut() else {
        return Err(SessionError::protocol("session slot missing"));
    };
    let request_id = match client.begin_prompt(&payload.blocks) {
        Ok(request_id) => request_id,
        Err(error) => {
            retain_begin_failure(
                state,
                (!client_message_id.is_empty()).then_some(client_message_id),
                error.clone(),
            );
            return Err(SessionError::protocol(error));
        }
    };
    state.prompts.active_prompt = Some(ActivePrompt::new(
        request_id,
        (!client_message_id.is_empty()).then_some(client_message_id.clone()),
    ));
    if !client_message_id.is_empty() {
        state.append_to_log(vec![SessionServerEvent::PromptAccepted {
            client_message_id,
        }])?;
    }
    Ok(())
}

pub(super) fn dispatch_queued_prompt(
    state: &mut TaskSessionState,
    next: &super::QueuedPrompt,
) -> Result<(), SessionError> {
    if let Some(reason) = &state.evidence.transcript_durability_fault {
        return Err(SessionError::persist(reason.clone()));
    }
    if !next.client_message_id.is_empty() {
        persist_ledger_update(state, |ledger| {
            ledger.mark_dispatching(&next.client_message_id);
        })?;
    }
    let item_id = state.stream_normalizer.fresh_item_id();
    state.append_to_log(vec![SessionServerEvent::Message {
        role: "user".to_string(),
        text: next.transcript_text.clone(),
        content_blocks: Vec::new(),
        item_id,
        message_id: None,
    }])?;
    let Some(client) = state.acp.client.as_mut() else {
        return Err(SessionError::protocol("session slot missing"));
    };
    match client.begin_prompt(&next.blocks) {
        Ok(request_id) => {
            state.prompts.active_prompt = Some(ActivePrompt::new(
                request_id,
                (!next.client_message_id.is_empty()).then_some(next.client_message_id.clone()),
            ));
            Ok(())
        }
        Err(error) => {
            retain_begin_failure(
                state,
                (!next.client_message_id.is_empty()).then_some(next.client_message_id.clone()),
                format!("queued prompt failed: {error}"),
            );
            Ok(())
        }
    }
}

fn retain_begin_failure(
    state: &mut TaskSessionState,
    client_message_id: Option<String>,
    message: String,
) {
    let mut active = ActivePrompt::new(0, client_message_id);
    active.capture_terminal(PromptTerminal {
        request_id: 0,
        outcome: PromptTerminalOutcome::Failed,
        events: vec![SessionServerEvent::Error { message }],
    });
    state.prompts.active_prompt = Some(active);
    try_finalize_active_prompt(state);
}

fn wire_blocks_to_json(
    blocks: &[prompt_content::PromptContentBlockWire],
) -> Vec<serde_json::Value> {
    blocks
        .iter()
        .filter_map(|block| serde_json::to_value(block).ok())
        .collect()
}

pub(super) fn cancel(state: &mut TaskSessionState, keep_queue: bool) -> Result<(), SessionError> {
    if !keep_queue {
        persist_ledger_update(state, PromptLedger::remove_queued)?;
    }
    apply_cancel_to_queue(&mut state.prompts.queued, keep_queue);
    let Some(client) = state.acp.client.as_mut() else {
        return Err(SessionError::protocol("session slot missing"));
    };
    let cancelled = client.cancel().map_err(SessionError::protocol)?;
    if let Some(active) = state.prompts.active_prompt.as_mut() {
        active.mark_cancel_requested();
    }
    let mut resolved = Vec::new();
    for request_id in cancelled.permissions {
        resolved.push(SessionServerEvent::PermissionResolved {
            request_id,
            approved: false,
        });
    }
    for request_id in cancelled.elicitations {
        resolved.push(SessionServerEvent::ElicitationResolved {
            request_id,
            action: "cancel".to_string(),
        });
    }
    state.append_to_log(resolved)?;
    Ok(())
}

pub(super) fn try_dispatch_next_if_idle(state: &mut TaskSessionState) {
    if state.prompts.active_prompt.is_some() || state.prompts.pending_exit_interruption.is_some() {
        return;
    }
    if !has_healthy_client(state) {
        return;
    }
    let Some(client) = state.acp.client.as_ref() else {
        return;
    };
    if client.prompt_in_flight() {
        return;
    }
    let Some(next) = state.prompts.queued.front().cloned() else {
        return;
    };
    match dispatch_queued_prompt(state, &next) {
        Ok(()) => {
            state.prompts.queued.pop_front();
            state.prompts.queue_persist_error_reported = false;
        }
        Err(error) if !state.prompts.queue_persist_error_reported => {
            state.prompts.queue_persist_error_reported = true;
            let _ = state.append_to_log(vec![SessionServerEvent::Error {
                message: error.to_string(),
            }]);
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

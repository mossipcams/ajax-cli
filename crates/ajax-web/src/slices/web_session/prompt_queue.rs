//! In-flight and queued prompt ownership for one session slot.

use super::acp_drain::PromptTerminal;
use super::QueuedPrompt;
use crate::adapters::web_session_store::prompt_ledger::PromptLedger;
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub(super) struct PendingExitInterruption {
    pub client_message_id: String,
    pub persist_error_reported: bool,
    pub interruption_error_reported: bool,
}

pub(crate) struct ActivePrompt {
    request_id: u64,
    pub(super) client_message_id: Option<String>,
    pub(super) terminal: Option<PromptTerminal>,
    pub(super) persist_error_reported: bool,
    pub(super) cancel_requested: bool,
}

impl ActivePrompt {
    pub(crate) fn new(request_id: u64, client_message_id: Option<String>) -> Self {
        Self {
            request_id,
            client_message_id,
            terminal: None,
            persist_error_reported: false,
            cancel_requested: false,
        }
    }

    pub(super) fn mark_cancel_requested(&mut self) {
        self.cancel_requested = true;
    }

    pub(super) fn drain_cancel_context(&self) -> (u64, bool) {
        (self.request_id, self.cancel_requested)
    }

    pub(super) fn capture_terminal(&mut self, terminal: PromptTerminal) -> bool {
        if terminal.request_id != self.request_id || self.terminal.is_some() {
            return false;
        }
        self.terminal = Some(terminal);
        true
    }

    #[cfg(test)]
    pub(super) fn has_pending_terminal(&self) -> bool {
        self.terminal.is_some()
    }

    #[cfg(test)]
    pub(super) fn request_id(&self) -> u64 {
        self.request_id
    }
}

pub(super) struct PromptQueue {
    /// Sidecar prompt ownership; dedupe authority separate from transcript JSONL.
    pub prompt_ledger: PromptLedger,
    /// ACP request identity and durable browser identity for the active prompt.
    pub active_prompt: Option<ActivePrompt>,
    pub queued: VecDeque<QueuedPrompt>,
    /// Suppresses repeated operator errors while a queued transition retries.
    pub queue_persist_error_reported: bool,
    /// Set when the sidecar ledger cannot be loaded safely; rejects new submits.
    pub ledger_unusable: Option<String>,
    /// Exit-interruption persist still pending after unexpected child death.
    pub pending_exit_interruption: Option<PendingExitInterruption>,
    /// Suppress duplicate ACP exit evidence during expected cancel/detach/shutdown.
    pub suppress_exit_evidence: bool,
    /// Prevents duplicate unexpected-exit reconciliation for one child death.
    pub child_exit_reconciled: bool,
}

impl PromptQueue {
    pub(super) fn busy(&self, client_prompt_in_flight: bool) -> bool {
        self.active_prompt.is_some() || !self.queued.is_empty() || client_prompt_in_flight
    }
}

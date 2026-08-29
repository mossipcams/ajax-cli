//! Per-task orchestration session command loop and owned state.

use super::normalize::StreamNormalizer;
use super::protocol::{SessionEventEnvelope, SessionSnapshot};
use super::task_session_spawn;
use super::transcript::TranscriptLog;
use super::{
    acp_drain::{
        drain_acp_events_with_prompt_cancel, normalize_session_events, PromptTerminal,
        PromptTerminalOutcome,
    },
    acp_usage::UsageDeduper,
    apply_cancel_to_queue, prompt_content, QueuedPrompt, ReportSessionActivity, SessionActivity,
    SessionActivityReporter, SessionServerEvent, MAX_QUEUED_PROMPTS,
};
use crate::adapters::web_session_acp::{AcpStdioClient, PromptCapabilityDescriptor};
use crate::adapters::web_session_store::{
    self,
    prompt_ledger::{PromptLedger, PromptPhase},
};
use ajax_core::models::AgentClient;
use std::{collections::VecDeque, path::PathBuf, time::Instant};
use tokio::sync::{mpsc, oneshot};

use super::context_continuity::ContextContinuity;
use super::task_session_exit::{
    self, is_acp_exit_error, persist_ledger_update, reconcile_unexpected_child_exit,
    retry_pending_exit_interruption, try_dispatch_next_if_idle, try_finalize_active_prompt,
    PendingExitInterruption,
};

const COMMAND_CAPACITY: usize = 32;
pub(crate) struct AttachSnapshot {
    pub generation: u64,
    pub snapshot: SessionSnapshot,
    pub replayed: Vec<SessionEventEnvelope>,
}

pub(crate) struct OutboundBatch {
    pub generation: u64,
    pub cursor: usize,
    pub snapshot: Option<SessionSnapshot>,
    pub events: Vec<SessionEventEnvelope>,
}

pub(crate) enum TaskSessionCommand {
    Acquire {
        worktree_path: PathBuf,
        model: String,
        agent: AgentClient,
        reply: oneshot::Sender<Result<(), String>>,
    },
    Release {
        reply: oneshot::Sender<()>,
    },
    SubmitPrompt {
        client_message_id: String,
        text: String,
        content_blocks: Vec<prompt_content::PromptContentBlockWire>,
        reply: oneshot::Sender<Result<(), String>>,
    },
    Cancel {
        keep_queue: bool,
        reply: oneshot::Sender<Result<(), String>>,
    },
    AnswerPermission {
        request_id: String,
        approved: bool,
        reason: Option<String>,
        reply: oneshot::Sender<Result<(), String>>,
    },
    AnswerElicitation {
        request_id: String,
        action: String,
        content: Option<serde_json::Value>,
        reply: oneshot::Sender<Result<(), String>>,
    },
    ApplyModel {
        worktree_path: PathBuf,
        model: String,
        reply: oneshot::Sender<Result<u64, String>>,
    },
    ApplyConfigOption {
        config_id: String,
        value: agent_client_protocol::schema::v1::SessionConfigOptionValue,
        reply: oneshot::Sender<Result<task_session_spawn::ApplyConfigOptionResult, String>>,
    },
    ResetHarness {
        worktree_path: PathBuf,
        model: String,
        agent: AgentClient,
        reply: oneshot::Sender<Result<u64, String>>,
    },
    RetryRestore {
        reply: oneshot::Sender<Result<(), String>>,
    },
    StartNewContext {
        reply: oneshot::Sender<Result<(), String>>,
    },
    AttachSnapshot {
        model: String,
        client_cursor: Option<usize>,
        reply: oneshot::Sender<AttachSnapshot>,
    },
    #[cfg(test)]
    ReadFrom {
        cursor: usize,
        reply: oneshot::Sender<(Vec<SessionServerEvent>, usize)>,
    },
    CollectOutbound {
        cursor: usize,
        generation: u64,
        reply: oneshot::Sender<OutboundBatch>,
    },
    #[cfg(test)]
    Record {
        event: SessionServerEvent,
        reply: oneshot::Sender<()>,
    },
    #[cfg(test)]
    ChildId {
        reply: oneshot::Sender<Option<u32>>,
    },
    #[cfg(test)]
    KillHostForTest {
        reply: oneshot::Sender<()>,
    },
    #[cfg(test)]
    Pump,
    EvictionSnapshot {
        reply: oneshot::Sender<EvictionSnapshot>,
    },
    Shutdown {
        /// When true, send ACP `session/close` before killing stdio (Drop / Switch).
        /// When false, detach so a later spawn can resume/load (idle eviction / restart).
        close: bool,
    },
}

pub(crate) struct EvictionSnapshot {
    pub evictable: bool,
    #[cfg_attr(not(test), allow(dead_code))]
    pub holders: usize,
}

pub(super) struct ActivePrompt {
    request_id: u64,
    pub(super) client_message_id: Option<String>,
    pub(super) terminal: Option<PromptTerminal>,
    pub(super) persist_error_reported: bool,
    pub(super) cancel_requested: bool,
}

impl ActivePrompt {
    pub(super) fn new(request_id: u64, client_message_id: Option<String>) -> Self {
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

pub(super) struct HolderCount(usize);

impl HolderCount {
    fn acquire(&mut self) {
        self.0 += 1;
    }

    fn release(&mut self) -> bool {
        if self.0 > 0 {
            self.0 -= 1;
        }
        self.0 == 0
    }

    fn reset_to_one(&mut self) {
        self.0 = 1;
    }
}

pub(crate) struct TaskSessionState {
    pub(super) qualified_handle: String,
    pub(super) state_dir: PathBuf,
    pub(super) client: Option<AcpStdioClient>,
    /// Normalized operator pin used for spawn and slot replacement.
    pub(super) model: String,
    /// Harness-reported model id for protocol snapshots ([#952](https://github.com/mossipcams/ajax-cli/issues/952)).
    pub(super) applied_model: String,
    pub(super) generation: u64,
    pub(super) holders: HolderCount,
    pub(super) log: TranscriptLog,
    pub(super) stream_normalizer: StreamNormalizer,
    pub(super) queued: VecDeque<QueuedPrompt>,
    pub(super) last_released: Option<Instant>,
    pub(super) acp_alive: bool,
    pub(super) agent: AgentClient,
    pub(super) worktree_path: Option<PathBuf>,
    pub(super) usage_deduper: UsageDeduper,
    /// Model-only snapshot after in-band apply (reset stays false).
    pub(super) pending_model_snapshot: Option<String>,
    /// Live advertised config options for connected picker binding.
    pub(super) session_config_options:
        Option<Vec<crate::adapters::web_session_acp::ConfigOptionDescriptor>>,
    pub(super) pending_config_snapshot:
        Option<Vec<crate::adapters::web_session_acp::ConfigOptionDescriptor>>,
    /// Live advertised slash commands for connected composer completion.
    pub(super) session_available_commands:
        Option<Vec<crate::adapters::web_session_acp::AvailableCommandDescriptor>>,
    pub(super) pending_commands_snapshot:
        Option<Vec<crate::adapters::web_session_acp::AvailableCommandDescriptor>>,
    pub(super) session_prompt_capabilities: Option<PromptCapabilityDescriptor>,
    pub(super) pending_capabilities_snapshot: Option<PromptCapabilityDescriptor>,
    /// Agent-reported session title from ACP `session_info_update`.
    pub(super) session_title: Option<String>,
    pub(super) pending_title_snapshot: bool,
    /// Set when transcript append fails; next collect_outbound emits transcriptError.
    pub(super) pending_transcript_error_snapshot: bool,
    /// Dedupes ACP run-state transitions for task evidence for this slot.
    /// Activity that failed to persist on a prior append; retried before new events.
    pending_activity_report: Option<SessionActivity>,
    activity_reporter: SessionActivityReporter,
    report_activity: Option<ReportSessionActivity>,
    /// Sidecar prompt ownership; dedupe authority separate from transcript JSONL.
    pub(super) prompt_ledger: PromptLedger,
    /// ACP request identity and durable browser identity for the active prompt.
    pub(super) active_prompt: Option<ActivePrompt>,
    /// Suppresses repeated operator errors while a queued transition retries.
    pub(super) queue_persist_error_reported: bool,
    /// Set when the sidecar ledger cannot be loaded safely; rejects new submits.
    pub(super) ledger_unusable: Option<String>,
    /// Exit-interruption persist still pending after unexpected child death.
    pub(super) pending_exit_interruption: Option<PendingExitInterruption>,
    /// Suppress duplicate ACP exit evidence during expected cancel/detach/shutdown.
    pub(super) suppress_exit_evidence: bool,
    /// Prevents duplicate unexpected-exit reconciliation for one child death.
    pub(super) child_exit_reconciled: bool,
    /// Host-owned ACP context continuity projected into protocol snapshots.
    pub(super) context_continuity: ContextContinuity,
    /// Set when transcript append fails; blocks new prompts until operator reset.
    pub(super) transcript_durability_fault: Option<String>,
}

impl TaskSessionState {
    pub(super) fn acquire_holder(&mut self) {
        self.holders.acquire();
    }

    pub(super) fn reset_holders_to_one(&mut self) {
        self.holders.reset_to_one();
    }

    pub(super) fn append_to_log(&mut self, events: Vec<SessionServerEvent>) -> Result<(), String> {
        if events.is_empty() {
            return Ok(());
        }
        self.flush_pending_activity_report();
        for event in &events {
            let Some(activity) = self.activity_reporter.activity_for_event(event) else {
                continue;
            };
            if self.try_report_activity(activity) {
                self.activity_reporter.commit(activity);
            } else {
                self.pending_activity_report = Some(activity);
            }
        }
        match web_session_store::append_events(&self.state_dir, &self.qualified_handle, &events) {
            Ok(()) => {
                self.log.append(events);
                Ok(())
            }
            Err(error) => {
                let message = format!("transcript persistence unavailable: {error}");
                self.transcript_durability_fault = Some(message.clone());
                self.pending_transcript_error_snapshot = true;
                Err(message)
            }
        }
    }

    fn flush_pending_activity_report(&mut self) {
        let Some(pending) = self.pending_activity_report else {
            return;
        };
        if self.try_report_activity(pending) {
            self.activity_reporter.commit(pending);
            self.pending_activity_report = None;
        }
    }

    fn try_report_activity(&self, activity: SessionActivity) -> bool {
        match &self.report_activity {
            Some(report) => {
                const MAX_ATTEMPTS: usize = 3;
                for attempt in 0..MAX_ATTEMPTS {
                    if report(&self.qualified_handle, activity) {
                        return true;
                    }
                    if attempt + 1 < MAX_ATTEMPTS {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                }
                false
            }
            None => true,
        }
    }

    fn is_idle(&self) -> bool {
        self.holders.0 == 0
    }

    fn has_restore_readiness(&self) -> bool {
        web_session_store::load::<SessionServerEvent>(&self.state_dir, &self.qualified_handle)
            .acp_session_id
            .is_some()
    }

    pub(super) fn busy(&self) -> bool {
        self.active_prompt.is_some()
            || !self.queued.is_empty()
            || self
                .client
                .as_ref()
                .is_some_and(|client| client.prompt_in_flight())
    }

    pub(super) fn pump(&mut self) {
        retry_pending_exit_interruption(self);
        let prompt_cancel = self
            .active_prompt
            .as_ref()
            .map(ActivePrompt::drain_cancel_context);
        let outcome = match self.client.as_mut() {
            Some(client) => {
                drain_acp_events_with_prompt_cancel(client, &mut self.usage_deduper, prompt_cancel)
            }
            None => return,
        };
        if let Some(model) = outcome.applied_model {
            self.applied_model = model;
            self.pending_model_snapshot = Some(self.applied_model.clone());
        }
        if let Some(options) = outcome.session_config_options {
            self.session_config_options = Some(options.clone());
            self.pending_config_snapshot = Some(options);
        }
        if let Some(commands) = outcome.session_available_commands {
            self.session_available_commands = Some(commands.clone());
            self.pending_commands_snapshot = Some(commands);
        }
        if let Some(title) = outcome.session_title_update {
            self.session_title = title;
            self.pending_title_snapshot = true;
        }
        for terminal in outcome.prompt_terminals {
            if let Some(active) = self.active_prompt.as_mut() {
                active.capture_terminal(terminal);
            }
        }
        let host_exit_from_drain = outcome.host_exited;
        let host_gone = host_exit_from_drain
            || self
                .client
                .as_mut()
                .is_some_and(|client| client.host_exited());
        let mut events = outcome.events;
        if self.suppress_exit_evidence || self.child_exit_reconciled {
            events.retain(|event| !is_acp_exit_error(event));
        }
        if !events.is_empty() {
            let normalized = normalize_session_events(&mut self.stream_normalizer, events);
            let _ = self.append_to_log(normalized);
        }
        if host_gone && !self.child_exit_reconciled {
            reconcile_unexpected_child_exit(self, host_exit_from_drain);
        } else {
            try_finalize_active_prompt(self);
            if host_gone {
                self.acp_alive = false;
            }
        }
        try_dispatch_next_if_idle(self);
    }

    #[cfg(test)]
    fn read_from(&self, cursor: usize) -> (Vec<SessionServerEvent>, usize) {
        self.log.read_from(cursor)
    }
}

pub(crate) fn spawn_task_session(
    qualified_handle: String,
    state_dir: PathBuf,
    report_activity: Option<ReportSessionActivity>,
) -> (
    mpsc::Sender<TaskSessionCommand>,
    tokio::task::JoinHandle<()>,
) {
    let (tx, mut rx) = mpsc::channel(COMMAND_CAPACITY);
    let handle = qualified_handle.clone();
    let (prompt_ledger, ledger_unusable) =
        match web_session_store::prompt_ledger::load(&state_dir, &handle) {
            Ok(ledger) => (ledger, None),
            Err(error) => (PromptLedger::default(), Some(error.to_string())),
        };
    let join = tokio::spawn(async move {
        let mut state = TaskSessionState {
            qualified_handle: handle,
            state_dir,
            client: None,
            model: String::new(),
            applied_model: String::new(),
            generation: 0,
            holders: HolderCount(0),
            log: TranscriptLog::default(),
            stream_normalizer: StreamNormalizer::default(),
            queued: VecDeque::new(),
            last_released: None,
            acp_alive: false,
            agent: AgentClient::Cursor,
            worktree_path: None,
            usage_deduper: UsageDeduper::default(),
            pending_model_snapshot: None,
            session_config_options: None,
            pending_config_snapshot: None,
            session_available_commands: None,
            pending_commands_snapshot: None,
            session_prompt_capabilities: None,
            pending_capabilities_snapshot: None,
            session_title: None,
            pending_title_snapshot: false,
            pending_transcript_error_snapshot: false,
            pending_activity_report: None,
            activity_reporter: SessionActivityReporter::default(),
            report_activity,
            prompt_ledger,
            active_prompt: None,
            queue_persist_error_reported: false,
            ledger_unusable,
            pending_exit_interruption: None,
            suppress_exit_evidence: false,
            child_exit_reconciled: false,
            context_continuity: ContextContinuity::default(),
            transcript_durability_fault: None,
        };
        let mut poll = tokio::time::interval(std::time::Duration::from_millis(50));
        poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut close_on_exit = false;

        loop {
            tokio::select! {
                cmd = rx.recv() => {
                    match cmd {
                        Some(TaskSessionCommand::Shutdown { close }) => {
                            close_on_exit = close;
                            break;
                        }
                        None => break,
                        Some(cmd) => handle_command(&mut state, cmd).await,
                    }
                }
                _ = poll.tick() => {
                    retry_pending_exit_interruption(&mut state);
                    if state.client.is_some() {
                        state.pump();
                    }
                }
            }
        }
        if close_on_exit {
            state.queued.clear();
            state.prompt_ledger.remove_queued();
            let _ = task_session_exit::persist_prompt_ledger(&state);
        }
        if let Some(mut client) = state.client.take() {
            state.suppress_exit_evidence = true;
            if !client.host_exited() {
                if let Some(active) = state.active_prompt.as_mut() {
                    active.mark_cancel_requested();
                }
                let _ = client.cancel();
            }
            let message = if close_on_exit {
                client.shutdown()
            } else {
                client.detach()
            };
            state.suppress_exit_evidence = false;
            if let Some(message) = message {
                let _ = state.append_to_log(vec![SessionServerEvent::Error { message }]);
            }
            state.child_exit_reconciled = true;
            state.acp_alive = false;
        }
    });
    (tx, join)
}

async fn handle_command(state: &mut TaskSessionState, command: TaskSessionCommand) {
    match command {
        TaskSessionCommand::Acquire {
            worktree_path,
            model,
            agent,
            reply,
        } => {
            let result = task_session_spawn::acquire(state, &worktree_path, &model, agent).await;
            let _ = reply.send(result);
        }
        TaskSessionCommand::Release { reply } => {
            if state.holders.release() {
                state.last_released = Some(Instant::now());
            }
            let _ = reply.send(());
        }
        TaskSessionCommand::SubmitPrompt {
            client_message_id,
            text,
            content_blocks,
            reply,
        } => {
            let result = submit_prompt(state, client_message_id, text, content_blocks);
            let _ = reply.send(result);
        }
        TaskSessionCommand::Cancel { keep_queue, reply } => {
            let result = cancel(state, keep_queue);
            let _ = reply.send(result);
        }
        TaskSessionCommand::AnswerPermission {
            request_id,
            approved,
            reason,
            reply,
        } => {
            let result = super::task_session_answers::answer_permission(
                state,
                &request_id,
                approved,
                reason.as_deref(),
            );
            let _ = reply.send(result);
        }
        TaskSessionCommand::AnswerElicitation {
            request_id,
            action,
            content,
            reply,
        } => {
            let result = super::task_session_answers::answer_elicitation(
                state,
                &request_id,
                &action,
                content.as_ref(),
            );
            let _ = reply.send(result);
        }
        TaskSessionCommand::ApplyModel {
            worktree_path,
            model,
            reply,
        } => {
            let result = task_session_spawn::apply_model(state, &worktree_path, &model).await;
            let _ = reply.send(result);
        }
        TaskSessionCommand::ApplyConfigOption {
            config_id,
            value,
            reply,
        } => {
            let result = task_session_spawn::apply_config_option(state, &config_id, value).await;
            let _ = reply.send(result);
        }
        TaskSessionCommand::ResetHarness {
            worktree_path,
            model,
            agent,
            reply,
        } => {
            let result =
                task_session_spawn::reset_harness_context(state, &worktree_path, &model, agent)
                    .await;
            let _ = reply.send(result);
        }
        TaskSessionCommand::RetryRestore { reply } => {
            let result = task_session_spawn::retry_restore(state).await;
            let _ = reply.send(result);
        }
        TaskSessionCommand::StartNewContext { reply } => {
            let result = task_session_spawn::start_new_context(state).await;
            let _ = reply.send(result);
        }
        TaskSessionCommand::AttachSnapshot {
            model,
            client_cursor,
            reply,
        } => {
            drop(model);
            let snapshot = super::task_session_outbound::attach_snapshot(state, client_cursor);
            let _ = reply.send(snapshot);
        }
        #[cfg(test)]
        TaskSessionCommand::ReadFrom { cursor, reply } => {
            state.pump();
            let result = state.read_from(cursor);
            let _ = reply.send(result);
        }
        TaskSessionCommand::CollectOutbound {
            cursor,
            generation,
            reply,
        } => {
            let batch = super::task_session_outbound::collect_outbound(state, cursor, generation);
            let _ = reply.send(batch);
        }
        #[cfg(test)]
        TaskSessionCommand::Record { event, reply } => {
            let _ = state.append_to_log(vec![event]);
            let _ = reply.send(());
        }
        #[cfg(test)]
        TaskSessionCommand::ChildId { reply } => {
            let id = state.client.as_ref().map(|client| client.child_id());
            let _ = reply.send(id);
        }
        #[cfg(test)]
        TaskSessionCommand::KillHostForTest { reply } => {
            if let Some(client) = state.client.as_mut() {
                client.kill_host_for_test();
            }
            let _ = reply.send(());
        }
        #[cfg(test)]
        TaskSessionCommand::Pump => state.pump(),
        TaskSessionCommand::EvictionSnapshot { reply } => {
            let _ = reply.send(EvictionSnapshot {
                evictable: state.is_idle() && !state.busy() && state.has_restore_readiness(),
                holders: state.holders.0,
            });
        }
        TaskSessionCommand::Shutdown { .. } => {}
    }
}

fn submit_prompt(
    state: &mut TaskSessionState,
    client_message_id: String,
    text: String,
    content_blocks: Vec<prompt_content::PromptContentBlockWire>,
) -> Result<(), String> {
    if let Some(reason) = &state.ledger_unusable {
        return Err(format!("prompt ownership unavailable: {reason}"));
    }
    if let Some(reason) = &state.transcript_durability_fault {
        return Err(reason.clone());
    }
    if state.context_continuity.prompts_blocked() {
        let message = state.context_continuity.error.clone().unwrap_or_else(|| {
            "ACP context unavailable — restore required before sending prompts".to_string()
        });
        return Err(message);
    }
    let caps = state
        .session_prompt_capabilities
        .clone()
        .unwrap_or_else(prompt_content::default_prompt_capabilities);
    let payload = prompt_content::build_prompt_payload(&text, &content_blocks, &caps)?;
    let Some(client) = state.client.as_ref() else {
        return Err("session slot missing".to_string());
    };
    if !state.acp_alive {
        return Err("ACP process exited — reconnect to send prompts".to_string());
    }
    if !client_message_id.is_empty() {
        if let Some(entry) = state.prompt_ledger.entry(&client_message_id) {
            match entry.phase {
                PromptPhase::Interrupted => {
                    return Err(format!(
                        "prompt {client_message_id} was interrupted and was not executed"
                    ));
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
    let in_flight = state.active_prompt.is_some() || client.prompt_in_flight();
    if in_flight {
        if state.queued.len() >= MAX_QUEUED_PROMPTS {
            return Err("prompt queue is full".to_string());
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
        state.queued.push_back(QueuedPrompt {
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
    let Some(client) = state.client.as_mut() else {
        return Err("session slot missing".to_string());
    };
    let request_id = match client.begin_prompt(&payload.blocks) {
        Ok(request_id) => request_id,
        Err(error) => {
            retain_begin_failure(
                state,
                (!client_message_id.is_empty()).then_some(client_message_id),
                error.clone(),
            );
            return Err(error);
        }
    };
    state.active_prompt = Some(ActivePrompt::new(
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
    next: &QueuedPrompt,
) -> Result<(), String> {
    if let Some(reason) = &state.transcript_durability_fault {
        return Err(reason.clone());
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
    let Some(client) = state.client.as_mut() else {
        return Err("session slot missing".to_string());
    };
    match client.begin_prompt(&next.blocks) {
        Ok(request_id) => {
            state.active_prompt = Some(ActivePrompt::new(
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
    state.active_prompt = Some(active);
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

fn cancel(state: &mut TaskSessionState, keep_queue: bool) -> Result<(), String> {
    if !keep_queue {
        persist_ledger_update(state, PromptLedger::remove_queued)?;
    }
    apply_cancel_to_queue(&mut state.queued, keep_queue);
    let Some(client) = state.client.as_mut() else {
        return Err("session slot missing".to_string());
    };
    let cancelled = client.cancel()?;
    if let Some(active) = state.active_prompt.as_mut() {
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

pub(crate) type TaskSessionSender = mpsc::Sender<TaskSessionCommand>;

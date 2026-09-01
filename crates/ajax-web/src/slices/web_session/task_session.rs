//! Per-task orchestration session command loop and owned state.

use super::acp_slot::AcpSlot;
use super::normalize::StreamNormalizer;
use super::prompt_queue::PromptQueue;
use super::protocol::{SessionEventEnvelope, SessionSnapshot};
use super::session_evidence::SessionEvidence;
use super::task_session_spawn;
use super::transcript::TranscriptLog;
use super::{
    acp_drain::{drain_acp_events_with_prompt_cancel, normalize_session_events},
    SessionError, SessionServerEvent,
};
use crate::adapters::web_session_store::{self, prompt_ledger::PromptLedger};
use ajax_core::models::AgentClient;
use std::{path::PathBuf, time::Instant};
use tokio::sync::{mpsc, oneshot};

use super::task_session_exit::{
    self, cancel, is_acp_exit_error, reconcile_unexpected_child_exit,
    retry_pending_exit_interruption, submit_prompt, try_dispatch_next_if_idle,
    try_finalize_active_prompt,
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
        reply: oneshot::Sender<Result<(), SessionError>>,
    },
    Release {
        reply: oneshot::Sender<()>,
    },
    SubmitPrompt {
        client_message_id: String,
        text: String,
        content_blocks: Vec<super::prompt_content::PromptContentBlockWire>,
        reply: oneshot::Sender<Result<(), SessionError>>,
    },
    Cancel {
        keep_queue: bool,
        reply: oneshot::Sender<Result<(), SessionError>>,
    },
    AnswerPermission {
        request_id: String,
        approved: bool,
        reason: Option<String>,
        reply: oneshot::Sender<Result<(), SessionError>>,
    },
    AnswerElicitation {
        request_id: String,
        action: String,
        content: Option<serde_json::Value>,
        reply: oneshot::Sender<Result<(), SessionError>>,
    },
    ApplyConfigOption {
        config_id: String,
        value: agent_client_protocol::schema::v1::SessionConfigOptionValue,
        reply: oneshot::Sender<Result<task_session_spawn::ApplyConfigOptionResult, SessionError>>,
    },
    ResetHarness {
        worktree_path: PathBuf,
        model: String,
        agent: AgentClient,
        reply: oneshot::Sender<Result<u64, SessionError>>,
    },
    ClearContext {
        worktree_path: PathBuf,
        reply: oneshot::Sender<Result<u64, SessionError>>,
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
    pub(super) acp: AcpSlot,
    pub(super) prompts: PromptQueue,
    pub(super) evidence: SessionEvidence,
    pub(super) generation: u64,
    pub(super) holders: HolderCount,
    pub(super) log: TranscriptLog,
    pub(super) stream_normalizer: StreamNormalizer,
    pub(super) last_released: Option<Instant>,
    pub(super) agent: AgentClient,
    pub(super) worktree_path: Option<PathBuf>,
}

impl TaskSessionState {
    pub(super) fn acquire_holder(&mut self) {
        self.holders.acquire();
    }

    pub(super) fn reset_holders_to_one(&mut self) {
        self.holders.reset_to_one();
    }

    pub(super) fn append_to_log(
        &mut self,
        events: Vec<SessionServerEvent>,
    ) -> Result<(), SessionError> {
        if events.is_empty() {
            return Ok(());
        }
        self.evidence
            .flush_pending_activity_report(&self.qualified_handle);
        let mut filtered = Vec::with_capacity(events.len());
        for event in events {
            if self
                .evidence
                .should_skip_duplicate_spawn_error(self.generation, &event)
            {
                continue;
            }
            self.evidence
                .report_activity_for_event(&self.qualified_handle, &event);
            filtered.push(event);
        }
        if filtered.is_empty() {
            return Ok(());
        }
        web_session_store::append_events(&self.state_dir, &self.qualified_handle, &filtered);
        self.log.append(filtered);
        Ok(())
    }

    fn is_idle(&self) -> bool {
        self.holders.0 == 0
    }

    pub(super) fn busy(&self) -> bool {
        let client_busy = self
            .acp
            .client
            .as_ref()
            .is_some_and(|client| client.prompt_in_flight());
        self.prompts.busy(client_busy)
    }

    pub(super) fn pump(&mut self) {
        retry_pending_exit_interruption(self);
        let prompt_cancel = self
            .prompts
            .active_prompt
            .as_ref()
            .map(super::prompt_queue::ActivePrompt::drain_cancel_context);
        let outcome = match self.acp.client.as_mut() {
            Some(client) => drain_acp_events_with_prompt_cancel(
                client,
                &mut self.acp.usage_deduper,
                prompt_cancel,
            ),
            None => return,
        };
        self.acp.apply_drain_outcome(&outcome);
        for terminal in outcome.prompt_terminals {
            if let Some(active) = self.prompts.active_prompt.as_mut() {
                active.capture_terminal(terminal);
            }
        }
        let host_exit_from_drain = outcome.host_exited;
        let host_gone = self.acp.host_gone(host_exit_from_drain);
        let mut events = outcome.events;
        if self.prompts.suppress_exit_evidence || self.prompts.child_exit_reconciled {
            events.retain(|event| !is_acp_exit_error(event));
        }
        if !events.is_empty() {
            let normalized = normalize_session_events(&mut self.stream_normalizer, events);
            let _ = self.append_to_log(normalized);
        }
        if host_gone && !self.prompts.child_exit_reconciled {
            reconcile_unexpected_child_exit(self, host_exit_from_drain);
        } else {
            try_finalize_active_prompt(self);
            if host_gone {
                self.acp.acp_alive = false;
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
    report_activity: Option<super::ReportSessionActivity>,
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
            acp: AcpSlot {
                client: None,
                model: String::new(),
                applied_model: String::new(),
                acp_alive: false,
                session_config_options: None,
                pending_config_snapshot: None,
                session_available_commands: None,
                pending_commands_snapshot: None,
                session_prompt_capabilities: None,
                pending_capabilities_snapshot: None,
                pending_model_snapshot: None,
                session_title: None,
                pending_title_snapshot: false,
                usage_deduper: super::acp_usage::UsageDeduper::default(),
            },
            prompts: PromptQueue {
                prompt_ledger,
                active_prompt: None,
                queued: std::collections::VecDeque::new(),
                queue_persist_error_reported: false,
                ledger_unusable,
                pending_exit_interruption: None,
                suppress_exit_evidence: false,
                child_exit_reconciled: false,
            },
            evidence: SessionEvidence {
                activity_reporter: super::SessionActivityReporter::default(),
                pending_activity_report: None,
                report_activity,
                activity_report_fault: None,
                pending_activity_report_error_snapshot: false,
                last_logged_spawn_error_id: None,
                transcript_durability_fault: None,
                pending_transcript_error_snapshot: false,
            },
            generation: 0,
            holders: HolderCount(0),
            log: TranscriptLog::default(),
            stream_normalizer: StreamNormalizer::default(),
            last_released: None,
            agent: AgentClient::Cursor,
            worktree_path: None,
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
                    if state.acp.client.is_some() {
                        state.pump();
                    }
                }
            }
        }
        if close_on_exit {
            state.prompts.queued.clear();
            state.prompts.prompt_ledger.remove_queued();
            let _ = task_session_exit::persist_prompt_ledger(&state);
        }
        if let Some(mut client) = state.acp.client.take() {
            state.prompts.suppress_exit_evidence = true;
            if !client.host_exited() {
                if let Some(active) = state.prompts.active_prompt.as_mut() {
                    active.mark_cancel_requested();
                }
                let _ = client.cancel();
            }
            let message = if close_on_exit {
                client.shutdown()
            } else {
                client.detach()
            };
            state.prompts.suppress_exit_evidence = false;
            if let Some(message) = message {
                let _ = state.append_to_log(vec![SessionServerEvent::Error { message }]);
            }
            state.prompts.child_exit_reconciled = true;
            state.acp.acp_alive = false;
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
        TaskSessionCommand::ClearContext {
            worktree_path,
            reply,
        } => {
            let result = task_session_spawn::clear_session_context(state, &worktree_path).await;
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
            let id = state.acp.client.as_ref().map(|client| client.child_id());
            let _ = reply.send(id);
        }
        #[cfg(test)]
        TaskSessionCommand::KillHostForTest { reply } => {
            if let Some(client) = state.acp.client.as_mut() {
                client.kill_host_for_test();
            }
            let _ = reply.send(());
        }
        #[cfg(test)]
        TaskSessionCommand::Pump => state.pump(),
        TaskSessionCommand::EvictionSnapshot { reply } => {
            let _ = reply.send(EvictionSnapshot {
                evictable: state.is_idle() && !state.busy(),
                holders: state.holders.0,
            });
        }
        TaskSessionCommand::Shutdown { .. } => {}
    }
}

pub(crate) type TaskSessionSender = mpsc::Sender<TaskSessionCommand>;

#[cfg(test)]
pub(crate) use super::prompt_queue::ActivePrompt;

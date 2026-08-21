//! Per-task orchestration session command loop and owned state.

use super::task_session_spawn;

use super::normalize::StreamNormalizer;
use super::protocol::{SessionEventEnvelope, SessionSnapshot};
use super::replay::{build_attach, pending_permission};
use super::transcript::TranscriptLog;
use super::{
    acp_drain::{
        drain_acp_events, normalize_session_events, parse_json_rpc_id, permission_response,
    },
    acp_usage::UsageDeduper,
    apply_cancel_to_queue, dispatch_prompt, PromptDispatch, SessionServerEvent,
};
use crate::adapters::web_session_acp::AcpStdioClient;
use crate::adapters::web_session_store;
use ajax_core::models::AgentClient;
use std::{collections::VecDeque, path::PathBuf, time::Instant};
use tokio::sync::{mpsc, oneshot};

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
    Shutdown,
}

pub(crate) struct EvictionSnapshot {
    pub evictable: bool,
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
    pub(super) queued: VecDeque<String>,
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
}

impl TaskSessionState {
    pub(super) fn acquire_holder(&mut self) {
        self.holders.acquire();
    }

    pub(super) fn reset_holders_to_one(&mut self) {
        self.holders.reset_to_one();
    }

    pub(super) fn append_to_log(&mut self, events: Vec<SessionServerEvent>) {
        if events.is_empty() {
            return;
        }
        self.log.append(events.clone());
        web_session_store::append_events(&self.state_dir, &self.qualified_handle, &events);
    }

    fn is_idle(&self) -> bool {
        self.holders.0 == 0
    }

    fn busy(&self) -> bool {
        !self.queued.is_empty()
            || self
                .client
                .as_ref()
                .is_some_and(|client| client.prompt_in_flight())
    }

    fn pump(&mut self) {
        let outcome = match self.client.as_mut() {
            Some(client) => drain_acp_events(client, &mut self.usage_deduper),
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
        if outcome.prompt_finished {
            if let Some(next) = self.queued.pop_front() {
                let begin_error = self
                    .client
                    .as_mut()
                    .and_then(|client| client.begin_prompt(&next).err());
                if let Some(error) = begin_error {
                    self.append_to_log(vec![SessionServerEvent::Error {
                        message: format!("queued prompt failed: {error}"),
                    }]);
                }
            }
        }
        let host_gone = outcome.host_exited
            || self
                .client
                .as_mut()
                .is_some_and(|client| client.host_exited());
        let host_exit_from_drain = outcome.host_exited;
        if !outcome.events.is_empty() {
            let normalized = normalize_session_events(&mut self.stream_normalizer, outcome.events);
            self.append_to_log(normalized);
        }
        if host_gone {
            let record_host_exit = self.acp_alive && !host_exit_from_drain;
            self.acp_alive = false;
            if record_host_exit {
                self.append_to_log(vec![SessionServerEvent::Error {
                    message: "ACP process exited".to_string(),
                }]);
            }
        }
    }

    #[cfg(test)]
    fn read_from(&self, cursor: usize) -> (Vec<SessionServerEvent>, usize) {
        self.log.read_from(cursor)
    }
}

pub(crate) fn spawn_task_session(
    qualified_handle: String,
    state_dir: PathBuf,
) -> (
    mpsc::Sender<TaskSessionCommand>,
    tokio::task::JoinHandle<()>,
) {
    let (tx, mut rx) = mpsc::channel(COMMAND_CAPACITY);
    let handle = qualified_handle.clone();
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
        };
        let mut poll = tokio::time::interval(std::time::Duration::from_millis(50));
        poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                cmd = rx.recv() => {
                    match cmd {
                        Some(TaskSessionCommand::Shutdown) | None => break,
                        Some(cmd) => handle_command(&mut state, cmd).await,
                    }
                }
                _ = poll.tick() => {
                    if state.client.is_some() {
                        state.pump();
                    }
                }
            }
        }
        if let Some(mut client) = state.client.take() {
            let _ = client.cancel();
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
            reply,
        } => {
            let result = submit_prompt(state, client_message_id, text);
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
            let result = answer_permission(state, &request_id, approved, reason.as_deref());
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
        TaskSessionCommand::AttachSnapshot {
            model,
            client_cursor,
            reply,
        } => {
            let snapshot = attach_snapshot(state, model, client_cursor);
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
            let batch = collect_outbound(state, cursor, generation);
            let _ = reply.send(batch);
        }
        #[cfg(test)]
        TaskSessionCommand::Record { event, reply } => {
            state.append_to_log(vec![event]);
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
                evictable: state.is_idle() && !state.busy(),
            });
        }
        TaskSessionCommand::Shutdown => {}
    }
}

fn submit_prompt(
    state: &mut TaskSessionState,
    client_message_id: String,
    text: String,
) -> Result<(), String> {
    let Some(client) = state.client.as_mut() else {
        return Err("session slot missing".to_string());
    };
    if !client_message_id.is_empty()
        && state.log.events.iter().any(|event| {
            matches!(
                event,
                SessionServerEvent::PromptAccepted { client_message_id: accepted }
                    if *accepted == client_message_id
            )
        })
    {
        state.append_to_log(vec![SessionServerEvent::PromptAccepted {
            client_message_id,
        }]);
        return Ok(());
    }
    let user_event = SessionServerEvent::Message {
        role: "user".to_string(),
        text: text.clone(),
        item_id: state.stream_normalizer.fresh_item_id(),
        message_id: None,
    };
    let in_flight = client.prompt_in_flight();
    match dispatch_prompt(in_flight, &mut state.queued, text.clone()) {
        PromptDispatch::Queued => {
            let mut events = vec![user_event];
            if !client_message_id.is_empty() {
                events.push(SessionServerEvent::PromptAccepted { client_message_id });
            }
            state.append_to_log(events);
            Ok(())
        }
        PromptDispatch::StartNow => {
            client.begin_prompt(&text).map(|_| ())?;
            let mut events = vec![user_event];
            if !client_message_id.is_empty() {
                events.push(SessionServerEvent::PromptAccepted { client_message_id });
            }
            state.append_to_log(events);
            Ok(())
        }
    }
}

fn cancel(state: &mut TaskSessionState, keep_queue: bool) -> Result<(), String> {
    let Some(client) = state.client.as_mut() else {
        return Err("session slot missing".to_string());
    };
    apply_cancel_to_queue(&mut state.queued, keep_queue);
    let cancelled = client.cancel()?;
    let resolved: Vec<SessionServerEvent> = cancelled
        .into_iter()
        .map(|request_id| SessionServerEvent::PermissionResolved {
            request_id,
            approved: false,
        })
        .collect();
    state.append_to_log(resolved);
    Ok(())
}

fn answer_permission(
    state: &mut TaskSessionState,
    request_id: &str,
    approved: bool,
    reason: Option<&str>,
) -> Result<(), String> {
    let Some(client) = state.client.as_mut() else {
        return Err("session slot missing".to_string());
    };
    let id = parse_json_rpc_id(request_id);
    let respond_result = client.respond_client_request(&id, permission_response(approved, reason));
    if respond_result.is_ok()
        || respond_result
            .as_ref()
            .err()
            .is_some_and(|message| message == "ACP permission request is no longer pending")
    {
        state.append_to_log(vec![SessionServerEvent::PermissionResolved {
            request_id: request_id.to_string(),
            approved,
        }]);
    }
    match respond_result {
        Ok(()) => Ok(()),
        Err(message) if message == "ACP permission request is no longer pending" => Ok(()),
        Err(message) => Err(message),
    }
}

fn attach_snapshot(
    state: &mut TaskSessionState,
    _model: String,
    client_cursor: Option<usize>,
) -> AttachSnapshot {
    let snapshot_model = snapshot_applied_model(state);
    let (snapshot, replayed) = build_attach(
        &state.log,
        snapshot_model,
        state.busy(),
        client_cursor,
        state.session_config_options.clone(),
    );
    AttachSnapshot {
        generation: state.generation,
        snapshot,
        replayed,
    }
}

fn snapshot_applied_model(state: &TaskSessionState) -> String {
    state.applied_model.clone()
}

fn collect_outbound(state: &mut TaskSessionState, cursor: usize, generation: u64) -> OutboundBatch {
    let current_generation = state.generation;
    let generation_changed = current_generation != generation;
    let read_from = if generation_changed {
        state.log.dropped
    } else {
        cursor
    };
    let snapshot = if generation_changed {
        Some(SessionSnapshot::new(
            state.log.absolute_next_cursor(),
            snapshot_applied_model(state),
            state.busy(),
            true,
            pending_permission(&state.log),
            state.session_config_options.clone(),
        ))
    } else if let Some(model) = state.pending_model_snapshot.take() {
        let config = state.pending_config_snapshot.take();
        Some(SessionSnapshot::new(
            state.log.absolute_next_cursor(),
            model,
            state.busy(),
            false,
            pending_permission(&state.log),
            config.or_else(|| state.session_config_options.clone()),
        ))
    } else {
        None
    };
    state.pump();
    let (events, next) = state.log.read_from_enveloped(read_from);
    OutboundBatch {
        generation: current_generation,
        cursor: next,
        snapshot,
        events,
    }
}

pub(crate) type TaskSessionSender = mpsc::Sender<TaskSessionCommand>;

pub(crate) async fn send_command<T>(
    tx: &TaskSessionSender,
    build: impl FnOnce(oneshot::Sender<T>) -> TaskSessionCommand,
) -> Result<T, String> {
    let (reply, rx) = oneshot::channel();
    tx.send(build(reply))
        .await
        .map_err(|_| "session task stopped".to_string())?;
    rx.await
        .map_err(|_| "session task dropped reply".to_string())
}

#[cfg(test)]
pub(crate) fn disk_read_from(
    state_dir: &std::path::Path,
    handle: &str,
    cursor: usize,
) -> (Vec<SessionServerEvent>, usize) {
    let stored: crate::adapters::web_session_store::StoredSession<SessionServerEvent> =
        web_session_store::load(state_dir, handle);
    if stored.events.is_empty() {
        (Vec::new(), cursor)
    } else {
        TranscriptLog::from_events(stored.events, stored.dropped).read_from(cursor)
    }
}

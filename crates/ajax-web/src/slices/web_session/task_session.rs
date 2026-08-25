//! Per-task orchestration session command loop and owned state.

use super::task_session_spawn;

use super::normalize::StreamNormalizer;
use super::protocol::{SessionChrome, SessionEventEnvelope, SessionSnapshot};
use super::replay::{build_attach, pending_elicitation, pending_permission};
use super::transcript::TranscriptLog;
use super::{
    acp_drain::{
        drain_acp_events, normalize_session_events, parse_json_rpc_id, permission_response,
    },
    acp_usage::UsageDeduper,
    apply_cancel_to_queue, prompt_content, QueuedPrompt, ReportSessionActivity, SessionActivity,
    SessionActivityReporter, SessionServerEvent, MAX_QUEUED_PROMPTS,
};
use crate::adapters::web_session_acp::{AcpStdioClient, PromptCapabilityDescriptor};
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
    /// Dedupes ACP run-state transitions for task evidence for this slot.
    /// Activity that failed to persist on a prior append; retried before new events.
    pending_activity_report: Option<SessionActivity>,
    activity_reporter: SessionActivityReporter,
    report_activity: Option<ReportSessionActivity>,
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
        self.log.append(events.clone());
        web_session_store::append_events(&self.state_dir, &self.qualified_handle, &events);
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
        if let Some(commands) = outcome.session_available_commands {
            self.session_available_commands = Some(commands.clone());
            self.pending_commands_snapshot = Some(commands);
        }
        if let Some(title) = outcome.session_title_update {
            self.session_title = title;
            self.pending_title_snapshot = true;
        }
        if outcome.prompt_finished {
            if let Some(next) = self.queued.pop_front() {
                let begin_error = self
                    .client
                    .as_mut()
                    .and_then(|client| client.begin_prompt(&next.blocks).err());
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
    report_activity: Option<ReportSessionActivity>,
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
            session_available_commands: None,
            pending_commands_snapshot: None,
            session_prompt_capabilities: None,
            pending_capabilities_snapshot: None,
            session_title: None,
            pending_title_snapshot: false,
            pending_activity_report: None,
            activity_reporter: SessionActivityReporter::default(),
            report_activity,
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
                    if state.client.is_some() {
                        state.pump();
                    }
                }
            }
        }
        if let Some(mut client) = state.client.take() {
            if !client.host_exited() {
                let _ = client.cancel();
            }
            let message = if close_on_exit {
                client.shutdown()
            } else {
                client.detach()
            };
            if let Some(message) = message {
                state.append_to_log(vec![SessionServerEvent::Error { message }]);
            }
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
            let result = answer_permission(state, &request_id, approved, reason.as_deref());
            let _ = reply.send(result);
        }
        TaskSessionCommand::AnswerElicitation {
            request_id,
            action,
            content,
            reply,
        } => {
            let result = answer_elicitation(state, &request_id, &action, content.as_ref());
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
        TaskSessionCommand::Shutdown { .. } => {}
    }
}

fn submit_prompt(
    state: &mut TaskSessionState,
    client_message_id: String,
    text: String,
    content_blocks: Vec<prompt_content::PromptContentBlockWire>,
) -> Result<(), String> {
    let caps = state
        .session_prompt_capabilities
        .clone()
        .unwrap_or_else(prompt_content::default_prompt_capabilities);
    let payload = prompt_content::build_prompt_payload(&text, &content_blocks, &caps)?;
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
        text: payload.transcript_text.clone(),
        content_blocks: Vec::new(),
        item_id: state.stream_normalizer.fresh_item_id(),
        message_id: None,
    };
    let in_flight = client.prompt_in_flight();
    if in_flight {
        if state.queued.len() >= MAX_QUEUED_PROMPTS {
            state.queued.pop_front();
        }
        state.queued.push_back(QueuedPrompt {
            transcript_text: payload.transcript_text,
            blocks: payload.blocks,
        });
        let mut events = vec![user_event];
        if !client_message_id.is_empty() {
            events.push(SessionServerEvent::PromptAccepted { client_message_id });
        }
        state.append_to_log(events);
        return Ok(());
    }
    client.begin_prompt(&payload.blocks).map(|_| ())?;
    let mut events = vec![user_event];
    if !client_message_id.is_empty() {
        events.push(SessionServerEvent::PromptAccepted { client_message_id });
    }
    state.append_to_log(events);
    Ok(())
}

fn cancel(state: &mut TaskSessionState, keep_queue: bool) -> Result<(), String> {
    let Some(client) = state.client.as_mut() else {
        return Err("session slot missing".to_string());
    };
    apply_cancel_to_queue(&mut state.queued, keep_queue);
    let cancelled = client.cancel()?;
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
    state.append_to_log(resolved);
    Ok(())
}

fn answer_elicitation(
    state: &mut TaskSessionState,
    request_id: &str,
    action: &str,
    content: Option<&serde_json::Value>,
) -> Result<(), String> {
    use crate::adapters::web_session_acp::sdk_elicitation::{
        accept_action, wire_content_from_json,
    };
    use agent_client_protocol::schema::v1::ElicitationAction;

    let Some(client) = state.client.as_mut() else {
        return Err("session slot missing".to_string());
    };
    let acp_action = match action {
        "accept" => {
            let payload = content.ok_or_else(|| {
                "elicitation accept requires content matching the requested schema".to_string()
            })?;
            accept_action(wire_content_from_json(payload)?)
        }
        "decline" => ElicitationAction::Decline,
        "cancel" => ElicitationAction::Cancel,
        other => return Err(format!("unsupported elicitation action: {other}")),
    };
    let respond_result = client.respond_elicitation(request_id, acp_action);
    if respond_result.is_ok()
        || respond_result
            .as_ref()
            .err()
            .is_some_and(|message| message == "ACP elicitation request is no longer pending")
    {
        state.append_to_log(vec![SessionServerEvent::ElicitationResolved {
            request_id: request_id.to_string(),
            action: action.to_string(),
        }]);
    }
    match respond_result {
        Ok(()) => Ok(()),
        Err(message) if message == "ACP elicitation request is no longer pending" => Ok(()),
        Err(message) => Err(message),
    }
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
    state.pump();
    let snapshot_model = snapshot_applied_model(state);
    let (snapshot, replayed) = build_attach(
        &state.log,
        snapshot_model,
        state.busy(),
        client_cursor,
        SessionChrome {
            session_config_options: state.session_config_options.clone(),
            available_commands: state.session_available_commands.clone(),
            prompt_capabilities: state.session_prompt_capabilities.clone(),
            session_title: state.session_title.clone(),
        },
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
            pending_elicitation(&state.log),
            SessionChrome {
                session_config_options: state.session_config_options.clone(),
                available_commands: state.session_available_commands.clone(),
                prompt_capabilities: state.session_prompt_capabilities.clone(),
                session_title: state.session_title.clone(),
            },
        ))
    } else if let Some(model) = state.pending_model_snapshot.take() {
        let config = state.pending_config_snapshot.take();
        let _ = state.pending_commands_snapshot.take();
        let _ = state.pending_capabilities_snapshot.take();
        state.pending_title_snapshot = false;
        Some(SessionSnapshot::new(
            state.log.absolute_next_cursor(),
            model,
            state.busy(),
            false,
            pending_permission(&state.log),
            pending_elicitation(&state.log),
            SessionChrome {
                session_config_options: config.or_else(|| state.session_config_options.clone()),
                available_commands: state.session_available_commands.clone(),
                prompt_capabilities: state.session_prompt_capabilities.clone(),
                session_title: state.session_title.clone(),
            },
        ))
    } else if state.pending_title_snapshot
        || state.pending_commands_snapshot.is_some()
        || state.pending_capabilities_snapshot.is_some()
    {
        let _ = state.pending_commands_snapshot.take();
        let _ = state.pending_capabilities_snapshot.take();
        state.pending_title_snapshot = false;
        Some(SessionSnapshot::new(
            state.log.absolute_next_cursor(),
            snapshot_applied_model(state),
            state.busy(),
            false,
            pending_permission(&state.log),
            pending_elicitation(&state.log),
            SessionChrome {
                session_config_options: state.session_config_options.clone(),
                available_commands: state.session_available_commands.clone(),
                prompt_capabilities: state.session_prompt_capabilities.clone(),
                session_title: state.session_title.clone(),
            },
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

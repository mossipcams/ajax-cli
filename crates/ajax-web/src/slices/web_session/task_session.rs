//! Per-task orchestration session command loop and owned state.

use super::task_session_spawn;

use super::transcript::TranscriptLog;
use super::{
    acp_drain::{
        coalesce_session_events, drain_acp_events, parse_json_rpc_id, permission_response,
    },
    apply_cancel_to_queue, dispatch_prompt, PromptDispatch, SessionServerEvent,
};
use crate::adapters::web_session_acp::AcpStdioClient;
use crate::adapters::web_session_store::{self, StoredSession};
use ajax_core::models::AgentClient;
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    time::Instant,
};
use tokio::sync::{mpsc, oneshot};

const COMMAND_CAPACITY: usize = 32;

pub(crate) struct AttachSnapshot {
    pub generation: u64,
    pub replayed: Vec<SessionServerEvent>,
    pub cursor: usize,
    pub ready: SessionServerEvent,
}

pub(crate) struct OutboundBatch {
    pub generation: u64,
    pub cursor: usize,
    pub ready: Option<SessionServerEvent>,
    pub events: Vec<SessionServerEvent>,
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
    Respawn {
        worktree_path: PathBuf,
        model: String,
        reply: oneshot::Sender<Result<u64, String>>,
    },
    AttachSnapshot {
        model: String,
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
    pub(super) model: String,
    pub(super) generation: u64,
    pub(super) holders: HolderCount,
    pub(super) log: TranscriptLog,
    pub(super) queued: VecDeque<String>,
    pub(super) last_released: Option<Instant>,
    pub(super) acp_alive: bool,
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
        let Some(client) = self.client.as_mut() else {
            return;
        };
        let (events, host_exited, prompt_finished) = drain_acp_events(client);
        if prompt_finished {
            if let Some(next) = self.queued.pop_front() {
                if let Err(error) = client.begin_prompt(&next) {
                    self.append_to_log(vec![SessionServerEvent::Error {
                        message: format!("queued prompt failed: {error}"),
                    }]);
                }
            }
        }
        if host_exited {
            self.acp_alive = false;
        }
        if !events.is_empty() {
            self.append_to_log(coalesce_session_events(events));
        }
    }

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
            generation: 0,
            holders: HolderCount(0),
            log: TranscriptLog::default(),
            queued: VecDeque::new(),
            last_released: None,
            acp_alive: false,
            agent: AgentClient::Cursor,
            worktree_path: None,
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
                    if state.client.is_some()
                        && (state.busy() || !state.is_idle() || !state.queued.is_empty())
                    {
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
        TaskSessionCommand::Respawn {
            worktree_path,
            model,
            reply,
        } => {
            let result = task_session_spawn::respawn(state, &worktree_path, &model).await;
            let _ = reply.send(result);
        }
        TaskSessionCommand::AttachSnapshot { model, reply } => {
            let snapshot = attach_snapshot(state, model);
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
    client.respond_client_request(&id, permission_response(approved, reason))?;
    state.append_to_log(vec![SessionServerEvent::PermissionResolved {
        request_id: request_id.to_string(),
        approved,
    }]);
    Ok(())
}

fn attach_snapshot(state: &mut TaskSessionState, model: String) -> AttachSnapshot {
    let (replayed, cursor) = state.read_from(0);
    AttachSnapshot {
        generation: state.generation,
        replayed,
        cursor,
        ready: SessionServerEvent::Ready {
            model: if state.model.is_empty() {
                model
            } else {
                state.model.clone()
            },
            busy: state.busy(),
        },
    }
}

fn collect_outbound(state: &mut TaskSessionState, cursor: usize, generation: u64) -> OutboundBatch {
    let current_generation = state.generation;
    let (cursor, ready) = if current_generation == generation {
        (cursor, None)
    } else {
        (
            0,
            Some(SessionServerEvent::Ready {
                model: if state.model.is_empty() {
                    "auto".to_string()
                } else {
                    state.model.clone()
                },
                busy: state.busy(),
            }),
        )
    };
    state.pump();
    let (events, next) = state.read_from(cursor);
    OutboundBatch {
        generation: current_generation,
        cursor: next,
        ready,
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

pub(crate) fn disk_read_from(
    state_dir: &Path,
    handle: &str,
    cursor: usize,
) -> (Vec<SessionServerEvent>, usize) {
    let stored: StoredSession<SessionServerEvent> = web_session_store::load(state_dir, handle);
    if stored.events.is_empty() {
        (Vec::new(), cursor)
    } else {
        TranscriptLog::from_events(stored.events, stored.dropped).read_from(cursor)
    }
}

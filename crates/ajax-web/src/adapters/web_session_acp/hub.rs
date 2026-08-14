//! Task-scoped ACP host sessions keyed by qualified handle.

use super::client::{AcpClientEvent, AcpStdioClient};
use super::store::{self, MAX_LOG_EVENTS};
use crate::slices::web_session::{
    apply_cancel_to_queue, dispatch_prompt, map_acp_client_request, map_acp_session_update,
    PromptDispatch, SessionServerEvent,
};
use ajax_core::models::AgentClient;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};

struct HolderCount(usize);

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
}

/// Per-task transcript bound. Long sessions trim from the front rather than
/// growing without limit. Cap is owned by `store::MAX_LOG_EVENTS`.
/// Slots with no sockets keep their ACP child alive so a reload can resume.
/// This bounds how many such children may linger.
pub(crate) const MAX_IDLE_SESSIONS: usize = 8;

/// Append-only transcript. Sockets hold absolute cursors into it, which is what
/// lets a reload replay and two devices both receive every event — the ACP
/// receiver itself is single-consumer and would otherwise split the stream.
#[derive(Default)]
pub(crate) struct TranscriptLog {
    pub(crate) events: Vec<SessionServerEvent>,
    /// Events trimmed off the front, so cursors stay absolute across trimming.
    pub(crate) dropped: usize,
}

impl TranscriptLog {
    fn from_events(events: Vec<SessionServerEvent>, dropped: usize) -> Self {
        Self { events, dropped }
    }

    pub(crate) fn append(&mut self, events: Vec<SessionServerEvent>) {
        self.events.extend(events);
        if self.events.len() > MAX_LOG_EVENTS {
            let excess = self.events.len() - MAX_LOG_EVENTS;
            self.events.drain(..excess);
            self.dropped += excess;
        }
    }

    /// Events at or after `cursor`, plus the cursor to read from next. A cursor
    /// left behind by trimming resumes at the oldest event still held.
    /// Resolved permission requests are omitted so reconnect does not flash
    /// already-answered prompts.
    pub(crate) fn read_from(&self, cursor: usize) -> (Vec<SessionServerEvent>, usize) {
        let next = self.dropped + self.events.len();
        let start = cursor.saturating_sub(self.dropped).min(self.events.len());
        let resolved: HashSet<String> = self
            .events
            .iter()
            .filter_map(|event| match event {
                SessionServerEvent::PermissionResolved { request_id, .. } => {
                    Some(request_id.clone())
                }
                _ => None,
            })
            .collect();
        let events = self.events[start..]
            .iter()
            .filter(|event| {
                !matches!(
                    event,
                    SessionServerEvent::PermissionRequest { request_id, .. }
                        if resolved.contains(request_id)
                )
            })
            .cloned()
            .collect();
        (events, next)
    }
}

struct SessionSlot {
    client: Arc<Mutex<AcpStdioClient>>,
    /// Normalized model id (`auto` when using Cursor's default).
    model: String,
    /// Bumped when the ACP child is replaced so live sockets replay the log.
    generation: u64,
    holders: HolderCount,
    log: TranscriptLog,
    /// Prompts waiting behind the single in-flight ACP turn.
    queued: VecDeque<String>,
    last_released: Option<Instant>,
    /// Cleared when pump observes process exit. Acquire must respawn before reuse.
    acp_alive: bool,
    /// Harness whose ACP process this slot runs; respawn must reuse it.
    agent: AgentClient,
}

impl SessionSlot {
    fn add_holder(&mut self) {
        self.holders.acquire();
        self.last_released = None;
    }

    fn release_holder(&mut self) {
        if self.holders.release() {
            self.last_released = Some(Instant::now());
        }
    }

    fn is_idle(&self) -> bool {
        self.holders.0 == 0
    }

    fn append_to_log(&mut self, state_dir: &Path, handle: &str, events: Vec<SessionServerEvent>) {
        if events.is_empty() {
            return;
        }
        self.log.append(events.clone());
        store::append_events(state_dir, handle, &events);
    }
}

pub struct WebSessionHub {
    sessions: Mutex<HashMap<String, SessionSlot>>,
    state_dir: PathBuf,
}

impl WebSessionHub {
    pub fn new(state_dir: PathBuf) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            state_dir,
        }
    }

    pub fn acquire(
        &self,
        qualified_handle: &str,
        worktree_path: &Path,
        model: &str,
        agent: AgentClient,
    ) -> Result<(), String> {
        enum AcquirePlan {
            ReplaceExisting { resume_id: Option<String> },
            InsertNew,
        }

        let plan = {
            let mut sessions = self.sessions.lock().unwrap();
            if let Some(slot) = sessions.get_mut(qualified_handle) {
                let host_exited = {
                    let mut client = slot.client.lock().unwrap();
                    client.host_exited()
                };
                if slot_must_replace(slot.acp_alive, &slot.model, model, host_exited) {
                    let resume_id =
                        replace_resume_id(&slot.model, model, &self.state_dir, qualified_handle);
                    begin_cancel_slot_client(slot);
                    // Pin before spawn so idle LRU cannot drop this slot unlocked.
                    slot.add_holder();
                    AcquirePlan::ReplaceExisting { resume_id }
                } else {
                    slot.add_holder();
                    return Ok(());
                }
            } else {
                evict_idle_over_limit(&mut sessions);
                AcquirePlan::InsertNew
            }
        };

        let stored = store::load(&self.state_dir, qualified_handle);
        let resume_id = match &plan {
            AcquirePlan::ReplaceExisting { resume_id } => resume_id.clone(),
            AcquirePlan::InsertNew => stored.acp_session_id.clone(),
        };
        let (client, report) = match AcpStdioClient::spawn(
            agent,
            worktree_path,
            spawn_model_arg(model),
            resume_id.as_deref(),
        ) {
            Ok(spawned) => spawned,
            Err(error) => {
                if matches!(plan, AcquirePlan::ReplaceExisting { .. }) {
                    let mut sessions = self.sessions.lock().unwrap();
                    if let Some(slot) = sessions.get_mut(qualified_handle) {
                        slot.release_holder();
                    }
                }
                return Err(error);
            }
        };

        let mut sessions = self.sessions.lock().unwrap();
        match plan {
            AcquirePlan::ReplaceExisting { .. } => {
                let Some(slot) = sessions.get_mut(qualified_handle) else {
                    return Err("session slot missing".to_string());
                };
                install_replaced_client(
                    slot,
                    client,
                    &report,
                    model,
                    &self.state_dir,
                    qualified_handle,
                )?;
            }
            AcquirePlan::InsertNew => {
                if let Some(slot) = sessions.get_mut(qualified_handle) {
                    let host_exited = {
                        let mut guard = slot.client.lock().unwrap();
                        guard.host_exited()
                    };
                    if !slot_must_replace(slot.acp_alive, &slot.model, model, host_exited) {
                        drop(client);
                        slot.add_holder();
                        return Ok(());
                    }
                    install_replaced_client(
                        slot,
                        client,
                        &report,
                        model,
                        &self.state_dir,
                        qualified_handle,
                    )?;
                    slot.add_holder();
                    return Ok(());
                }
                let mut log = TranscriptLog::from_events(stored.events, stored.dropped);
                if context_reset_needed(&report, &log) {
                    let note = context_reset_note();
                    log.append(vec![note.clone()]);
                    store::append_events(
                        &self.state_dir,
                        qualified_handle,
                        std::slice::from_ref(&note),
                    );
                }
                store::save_meta(
                    &self.state_dir,
                    qualified_handle,
                    Some(client.session_id()),
                    model,
                );
                sessions.insert(
                    qualified_handle.to_string(),
                    SessionSlot {
                        client: Arc::new(Mutex::new(client)),
                        model: model.to_string(),
                        generation: 0,
                        holders: HolderCount(1),
                        log,
                        queued: VecDeque::new(),
                        last_released: None,
                        acp_alive: true,
                        agent,
                    },
                );
            }
        }
        Ok(())
    }

    /// Replace the ACP child for an existing slot, keeping the transcript.
    /// Returns the new generation so the calling socket can reset its cursor.
    pub fn respawn(
        &self,
        qualified_handle: &str,
        worktree_path: &Path,
        model: &str,
    ) -> Result<u64, String> {
        let (resume_id, agent) = {
            let mut sessions = self.sessions.lock().unwrap();
            let Some(slot) = sessions.get_mut(qualified_handle) else {
                return Err("session slot missing".to_string());
            };
            let agent = slot.agent;
            let host_exited = {
                let mut client = slot.client.lock().unwrap();
                client.host_exited()
            };
            if !slot_must_replace(slot.acp_alive, &slot.model, model, host_exited) {
                return Ok(slot.generation);
            }
            let resume_id =
                replace_resume_id(&slot.model, model, &self.state_dir, qualified_handle);
            begin_cancel_slot_client(slot);
            (resume_id, agent)
        };

        let (client, report) = AcpStdioClient::spawn(
            agent,
            worktree_path,
            spawn_model_arg(model),
            resume_id.as_deref(),
        )?;

        let mut sessions = self.sessions.lock().unwrap();
        let Some(slot) = sessions.get_mut(qualified_handle) else {
            drop(client);
            return Err("session slot missing".to_string());
        };
        let host_exited = {
            let mut guard = slot.client.lock().unwrap();
            guard.host_exited()
        };
        if !slot_must_replace(slot.acp_alive, &slot.model, model, host_exited) {
            drop(client);
            return Ok(slot.generation);
        }
        install_replaced_client(
            slot,
            client,
            &report,
            model,
            &self.state_dir,
            qualified_handle,
        )?;
        Ok(slot.generation)
    }

    pub fn model(&self, handle: &str) -> Option<String> {
        let sessions = self.sessions.lock().unwrap();
        sessions.get(handle).map(|slot| slot.model.clone())
    }

    pub fn generation(&self, handle: &str) -> u64 {
        let sessions = self.sessions.lock().unwrap();
        sessions
            .get(handle)
            .map(|slot| slot.generation)
            .unwrap_or(0)
    }

    /// The slot deliberately outlives its last socket: dropping it here would
    /// kill the `agent acp` child, so a browser reload would terminate work in
    /// progress and lose the transcript. Idle slots are reclaimed on the next
    /// `acquire` instead.
    pub fn release(&self, handle: &str) {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(slot) = sessions.get_mut(handle) {
            slot.release_holder();
        }
    }

    /// Forget this task's slot so the next acquire spawns from scratch. Used
    /// when the task moves to another harness: the live child belongs to the
    /// old one and must not keep serving the task.
    pub fn drop_session(&self, handle: &str) {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(slot) = sessions.remove(handle) {
            let mut client = slot.client.lock().unwrap();
            let _ = client.begin_cancel();
        }
    }

    #[cfg(test)]
    pub(crate) fn child_id(&self, handle: &str) -> Option<u32> {
        let sessions = self.sessions.lock().unwrap();
        sessions
            .get(handle)
            .map(|slot| slot.client.lock().unwrap().child_id())
    }

    #[cfg(test)]
    pub(crate) fn kill_host_for_test(&self, handle: &str) {
        let sessions = self.sessions.lock().unwrap();
        if let Some(slot) = sessions.get(handle) {
            slot.client.lock().unwrap().kill_host_for_test();
        }
    }

    /// Enqueue or start a prompt. Records the operator message after the
    /// prompt is accepted (started or queued) — ACP does not echo user turns.
    pub fn submit_prompt(&self, handle: &str, text: String) -> Result<(), String> {
        let mut sessions = self.sessions.lock().unwrap();
        let Some(slot) = sessions.get_mut(handle) else {
            return Err("session slot missing".to_string());
        };
        let user_event = SessionServerEvent::Message {
            role: "user".to_string(),
            text: text.clone(),
        };
        let client = Arc::clone(&slot.client);
        let in_flight = client.lock().unwrap().prompt_in_flight();
        match dispatch_prompt(in_flight, &mut slot.queued, text.clone()) {
            PromptDispatch::Queued => {
                slot.append_to_log(&self.state_dir, handle, vec![user_event]);
                Ok(())
            }
            PromptDispatch::StartNow => {
                client.lock().unwrap().begin_prompt(&text).map(|_| ())?;
                slot.append_to_log(&self.state_dir, handle, vec![user_event]);
                Ok(())
            }
        }
    }

    /// Cancel the in-flight turn and optionally retain queued follow-ups.
    pub fn cancel(&self, handle: &str, keep_queue: bool) -> Result<(), String> {
        let mut sessions = self.sessions.lock().unwrap();
        let Some(slot) = sessions.get_mut(handle) else {
            return Err("session slot missing".to_string());
        };
        let client = Arc::clone(&slot.client);
        apply_cancel_to_queue(&mut slot.queued, keep_queue);
        let result = client.lock().unwrap().begin_cancel().map(|_| ());
        result
    }

    /// Answer a permission request and record the decision so reload replay
    /// does not resurrect an already-decided prompt.
    pub fn answer_permission(
        &self,
        handle: &str,
        request_id: &str,
        approved: bool,
        reason: Option<&str>,
    ) -> Result<(), String> {
        let mut sessions = self.sessions.lock().unwrap();
        let Some(slot) = sessions.get_mut(handle) else {
            return Err("session slot missing".to_string());
        };
        let resolved = SessionServerEvent::PermissionResolved {
            request_id: request_id.to_string(),
            approved,
        };
        slot.append_to_log(&self.state_dir, handle, vec![resolved]);
        let client = Arc::clone(&slot.client);
        let id = parse_json_rpc_id(request_id);
        let result = client
            .lock()
            .unwrap()
            .respond_client_request(&id, permission_response(approved, reason));
        result
    }

    /// Move whatever the ACP client has produced into the slot's transcript.
    /// Draining in one place under the sessions lock is what gives every socket
    /// the same totally-ordered log instead of a race for `try_recv`.
    pub fn pump(&self, handle: &str) {
        let mut sessions = self.sessions.lock().unwrap();
        let Some(slot) = sessions.get_mut(handle) else {
            return;
        };
        let (events, host_exited) = {
            let mut client = slot.client.lock().unwrap();
            let (mut events, host_exited, prompt_finished) = drain_acp_events(&client);
            if prompt_finished {
                if let Some(next) = slot.queued.pop_front() {
                    if let Err(error) = client.begin_prompt(&next) {
                        events.push(SessionServerEvent::Error {
                            message: format!("queued prompt failed: {error}"),
                        });
                    }
                }
            }
            (events, host_exited)
        };
        if host_exited {
            slot.acp_alive = false;
        }
        if !events.is_empty() {
            slot.append_to_log(&self.state_dir, handle, events);
        }
    }

    /// Append an event the ACP client will never produce. The agent does not
    /// echo the operator's own prompts, so without this a replayed transcript
    /// would carry the agent's half of the conversation and none of yours.
    pub fn record(&self, handle: &str, event: SessionServerEvent) {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(slot) = sessions.get_mut(handle) {
            slot.append_to_log(&self.state_dir, handle, vec![event]);
        }
    }

    pub fn read_from(&self, handle: &str, cursor: usize) -> (Vec<SessionServerEvent>, usize) {
        let sessions = self.sessions.lock().unwrap();
        match sessions.get(handle) {
            Some(slot) => slot.log.read_from(cursor),
            None => {
                let stored = store::load(&self.state_dir, handle);
                if stored.events.is_empty() {
                    (Vec::new(), cursor)
                } else {
                    TranscriptLog::from_events(stored.events, stored.dropped).read_from(cursor)
                }
            }
        }
    }
}

/// Reclaim the least recently released idle slots once too many linger.
fn evict_idle_over_limit(sessions: &mut HashMap<String, SessionSlot>) {
    let mut idle: Vec<(String, Instant)> = sessions
        .iter()
        .filter(|(_, slot)| {
            slot.is_idle()
                && slot.queued.is_empty()
                && !slot.client.lock().unwrap().prompt_in_flight()
        })
        .map(|(handle, slot)| {
            (
                handle.clone(),
                slot.last_released.unwrap_or_else(Instant::now),
            )
        })
        .collect();
    if idle.len() < MAX_IDLE_SESSIONS {
        return;
    }
    idle.sort_by_key(|(_, released)| *released);
    for (handle, _) in idle.iter().take(idle.len() - MAX_IDLE_SESSIONS + 1) {
        sessions.remove(handle);
    }
}

fn spawn_model_arg(model: &str) -> Option<&str> {
    if model.is_empty() || model == "auto" {
        None
    } else {
        Some(model)
    }
}

pub(crate) fn slot_must_replace(
    acp_alive: bool,
    slot_model: &str,
    want_model: &str,
    host_exited: bool,
) -> bool {
    !acp_alive || host_exited || slot_model != want_model
}

fn context_reset_needed(report: &super::client::SpawnReport, log: &TranscriptLog) -> bool {
    !report.resumed && !log.events.is_empty()
}

fn context_reset_note() -> SessionServerEvent {
    SessionServerEvent::Message {
        role: "agent".to_string(),
        text: "Model context reset after restart. Prior turns are still visible here.".to_string(),
    }
}

fn begin_cancel_slot_client(slot: &SessionSlot) {
    let mut client = slot.client.lock().unwrap();
    let _ = client.begin_cancel();
}

fn replace_resume_id(
    slot_model: &str,
    want_model: &str,
    state_dir: &Path,
    handle: &str,
) -> Option<String> {
    if slot_model == want_model {
        store::load(state_dir, handle).acp_session_id
    } else {
        None
    }
}

/// Best-effort cancel already done; spawn happened outside the sessions lock.
fn install_replaced_client(
    slot: &mut SessionSlot,
    new_client: AcpStdioClient,
    report: &super::client::SpawnReport,
    model: &str,
    state_dir: &Path,
    handle: &str,
) -> Result<(), String> {
    if context_reset_needed(report, &slot.log) {
        let note = context_reset_note();
        slot.append_to_log(state_dir, handle, vec![note]);
    }
    store::save_meta(state_dir, handle, Some(new_client.session_id()), model);
    {
        let mut guard = slot.client.lock().unwrap();
        *guard = new_client;
    }
    slot.model = model.to_string();
    slot.generation = slot.generation.saturating_add(1);
    slot.acp_alive = true;
    Ok(())
}

pub fn drain_acp_events(client: &AcpStdioClient) -> (Vec<SessionServerEvent>, bool, bool) {
    let mut events = Vec::new();
    let mut host_exited = false;
    let mut prompt_finished = false;
    while let Some(event) = client.poll_event() {
        match event {
            AcpClientEvent::SessionUpdate(params) => {
                events.extend(map_acp_session_update(&params));
            }
            AcpClientEvent::ClientRequest { id, method, params } => {
                if let Some(mut mapped) = map_acp_client_request(&method, &params) {
                    if let SessionServerEvent::PermissionRequest { request_id, .. } = &mut mapped {
                        // Prefer the JSON-RPC request id so permission replies match.
                        if let Some(rpc_id) = id.as_str() {
                            *request_id = rpc_id.to_string();
                        } else if let Some(rpc_id) = id.as_u64() {
                            *request_id = rpc_id.to_string();
                        } else if let Some(rpc_id) = id.as_i64() {
                            *request_id = rpc_id.to_string();
                        }
                    }
                    events.push(mapped);
                }
            }
            AcpClientEvent::RequestFinished { result, method, .. } => {
                if method == "session/prompt" {
                    prompt_finished = true;
                }
                if let Some(mapped) = map_request_finished(method, result) {
                    events.push(mapped);
                }
            }
            AcpClientEvent::Error(message) => {
                events.push(SessionServerEvent::Error { message });
            }
            AcpClientEvent::Exited => {
                host_exited = true;
                events.push(SessionServerEvent::Error {
                    message: "ACP process exited".to_string(),
                });
            }
        }
    }
    (events, host_exited, prompt_finished)
}

/// A finished `session/prompt` is the only signal the browser gets that the
/// agent stopped working, so it must reach the client even when the turn
/// succeeded. Other completed requests carry nothing the chat can show.
pub(crate) fn map_request_finished(
    method: &'static str,
    result: Result<Value, String>,
) -> Option<SessionServerEvent> {
    match result {
        Ok(value) if method == "session/prompt" => Some(SessionServerEvent::TurnEnd {
            stop_reason: value
                .get("stopReason")
                .and_then(Value::as_str)
                .map(str::to_string),
        }),
        Ok(_) => None,
        Err(message) => Some(SessionServerEvent::Error { message }),
    }
}

pub fn permission_response(approved: bool, reason: Option<&str>) -> Value {
    json!({
        "approved": approved,
        "reason": reason,
    })
}

fn parse_json_rpc_id(raw: &str) -> Value {
    if let Ok(n) = raw.parse::<u64>() {
        return Value::Number(n.into());
    }
    if let Ok(n) = raw.parse::<i64>() {
        return Value::Number(n.into());
    }
    Value::String(raw.to_string())
}

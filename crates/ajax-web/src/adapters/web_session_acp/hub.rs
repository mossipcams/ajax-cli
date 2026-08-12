//! Task-scoped ACP host sessions keyed by qualified handle.

use super::client::{AcpClientEvent, AcpStdioClient};
use super::store::{self, MAX_LOG_EVENTS};
use crate::slices::web_session::{
    map_acp_client_request, map_acp_session_update, SessionServerEvent,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
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
const MAX_IDLE_SESSIONS: usize = 8;

/// Append-only transcript. Sockets hold absolute cursors into it, which is what
/// lets a reload replay and two devices both receive every event — the ACP
/// receiver itself is single-consumer and would otherwise split the stream.
#[derive(Default)]
struct TranscriptLog {
    events: Vec<SessionServerEvent>,
    /// Events trimmed off the front, so cursors stay absolute across trimming.
    dropped: usize,
}

impl TranscriptLog {
    fn from_events(events: Vec<SessionServerEvent>) -> Self {
        Self { events, dropped: 0 }
    }

    fn append(&mut self, events: Vec<SessionServerEvent>) {
        self.events.extend(events);
        if self.events.len() > MAX_LOG_EVENTS {
            let excess = self.events.len() - MAX_LOG_EVENTS;
            self.events.drain(..excess);
            self.dropped += excess;
        }
    }

    /// Events at or after `cursor`, plus the cursor to read from next. A cursor
    /// left behind by trimming resumes at the oldest event still held.
    fn read_from(&self, cursor: usize) -> (Vec<SessionServerEvent>, usize) {
        let next = self.dropped + self.events.len();
        let start = cursor.saturating_sub(self.dropped).min(self.events.len());
        (self.events[start..].to_vec(), next)
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
    last_released: Option<Instant>,
    /// Cleared when pump observes process exit. Acquire must respawn before reuse.
    acp_alive: bool,
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
    ) -> Result<Arc<Mutex<AcpStdioClient>>, String> {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(slot) = sessions.get_mut(qualified_handle) {
            let host_exited = {
                let mut client = slot.client.lock().unwrap();
                client.host_exited()
            };
            if slot_must_replace(slot.acp_alive, &slot.model, model, host_exited) {
                replace_slot_client(
                    slot,
                    worktree_path,
                    model,
                    &self.state_dir,
                    qualified_handle,
                )?;
            }
            slot.add_holder();
            return Ok(Arc::clone(&slot.client));
        }
        evict_idle_over_limit(&mut sessions);
        let stored = store::load(&self.state_dir, qualified_handle);
        let (client, report) = AcpStdioClient::spawn(
            worktree_path,
            spawn_model_arg(model),
            stored.acp_session_id.as_deref(),
        )?;
        let mut log = TranscriptLog::from_events(stored.events);
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
        let client = Arc::new(Mutex::new(client));
        sessions.insert(
            qualified_handle.to_string(),
            SessionSlot {
                client: Arc::clone(&client),
                model: model.to_string(),
                generation: 0,
                holders: HolderCount(1),
                log,
                last_released: None,
                acp_alive: true,
            },
        );
        Ok(client)
    }

    /// Replace the ACP child for an existing slot, keeping the transcript.
    /// Returns the new generation so the calling socket can reset its cursor.
    pub fn respawn(
        &self,
        qualified_handle: &str,
        worktree_path: &Path,
        model: &str,
    ) -> Result<(Arc<Mutex<AcpStdioClient>>, u64), String> {
        let mut sessions = self.sessions.lock().unwrap();
        let Some(slot) = sessions.get_mut(qualified_handle) else {
            return Err("session slot missing".to_string());
        };
        if slot_must_replace(slot.acp_alive, &slot.model, model, {
            let mut client = slot.client.lock().unwrap();
            client.host_exited()
        }) {
            replace_slot_client(
                slot,
                worktree_path,
                model,
                &self.state_dir,
                qualified_handle,
            )?;
        }
        Ok((Arc::clone(&slot.client), slot.generation))
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

    /// Move whatever the ACP client has produced into the slot's transcript.
    /// Draining in one place under the sessions lock is what gives every socket
    /// the same totally-ordered log instead of a race for `try_recv`.
    pub fn pump(&self, handle: &str) {
        let mut sessions = self.sessions.lock().unwrap();
        let Some(slot) = sessions.get_mut(handle) else {
            return;
        };
        let (events, host_exited) = {
            let client = slot.client.lock().unwrap();
            drain_acp_events(&client)
        };
        if host_exited {
            slot.acp_alive = false;
        }
        if !events.is_empty() {
            slot.log.append(events.clone());
            store::append_events(&self.state_dir, handle, &events);
        }
    }

    /// Append an event the ACP client will never produce. The agent does not
    /// echo the operator's own prompts, so without this a replayed transcript
    /// would carry the agent's half of the conversation and none of yours.
    pub fn record(&self, handle: &str, event: SessionServerEvent) {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(slot) = sessions.get_mut(handle) {
            slot.log.append(vec![event.clone()]);
            store::append_events(&self.state_dir, handle, std::slice::from_ref(&event));
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
                    TranscriptLog::from_events(stored.events).read_from(cursor)
                }
            }
        }
    }
}

/// Reclaim the least recently released idle slots once too many linger.
fn evict_idle_over_limit(sessions: &mut HashMap<String, SessionSlot>) {
    let mut idle: Vec<(String, Instant)> = sessions
        .iter()
        .filter(|(_, slot)| slot.is_idle())
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

fn slot_must_replace(
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

/// Best-effort cancel, then drop and replace the ACP child. Transcript stays.
fn replace_slot_client(
    slot: &mut SessionSlot,
    worktree_path: &Path,
    model: &str,
    state_dir: &Path,
    handle: &str,
) -> Result<(), String> {
    {
        let mut client = slot.client.lock().unwrap();
        let _ = client.begin_cancel();
    }
    // Same model: try ACP session/load. Model change starts a new conversation.
    let resume_id = if slot.model == model {
        store::load(state_dir, handle).acp_session_id
    } else {
        None
    };
    let (new_client, report) =
        AcpStdioClient::spawn(worktree_path, spawn_model_arg(model), resume_id.as_deref())?;
    if context_reset_needed(&report, &slot.log) {
        let note = context_reset_note();
        slot.log.append(vec![note.clone()]);
        store::append_events(state_dir, handle, std::slice::from_ref(&note));
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

pub fn drain_acp_events(client: &AcpStdioClient) -> (Vec<SessionServerEvent>, bool) {
    let mut events = Vec::new();
    let mut host_exited = false;
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
                events.extend(map_request_finished(method, result));
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
    (events, host_exited)
}

/// A finished `session/prompt` is the only signal the browser gets that the
/// agent stopped working, so it must reach the client even when the turn
/// succeeded. Other completed requests carry nothing the chat can show.
fn map_request_finished(
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ajax-web-session-hub-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn note(text: &str) -> SessionServerEvent {
        SessionServerEvent::Message {
            role: "agent".to_string(),
            text: text.to_string(),
        }
    }

    #[test]
    fn slot_must_replace_when_host_is_dead_or_model_changes() {
        assert!(!slot_must_replace(true, "auto", "auto", false));
        assert!(slot_must_replace(false, "auto", "auto", false));
        assert!(slot_must_replace(true, "auto", "auto", true));
        assert!(slot_must_replace(true, "auto", "composer-2.5", false));
    }

    #[test]
    fn hub_release_is_noop_when_handle_missing() {
        let hub = WebSessionHub::new(scratch_dir("release"));
        hub.release("web/fix-login");
        assert!(hub.sessions.lock().unwrap().is_empty());
    }

    /// A reconnecting socket starts at cursor 0, which is what makes a reload
    /// resume the conversation instead of showing an empty thread.
    #[test]
    fn a_fresh_cursor_replays_the_whole_transcript() {
        let mut log = TranscriptLog::default();
        log.append(vec![note("one"), note("two")]);
        let (events, next) = log.read_from(0);
        assert_eq!(events, vec![note("one"), note("two")]);
        assert_eq!(next, 2);
    }

    /// Two sockets each hold their own cursor, so both receive every event —
    /// the bug this replaces handed each of them a random half.
    #[test]
    fn two_cursors_each_receive_every_event() {
        let mut log = TranscriptLog::default();
        log.append(vec![note("one")]);
        let (first_a, cursor_a) = log.read_from(0);
        let (first_b, cursor_b) = log.read_from(0);
        assert_eq!(first_a, first_b);

        log.append(vec![note("two")]);
        let (next_a, _) = log.read_from(cursor_a);
        let (next_b, _) = log.read_from(cursor_b);
        assert_eq!(next_a, vec![note("two")]);
        assert_eq!(next_b, vec![note("two")]);
    }

    #[test]
    fn a_caught_up_cursor_reads_nothing() {
        let mut log = TranscriptLog::default();
        log.append(vec![note("one")]);
        let (_, cursor) = log.read_from(0);
        assert!(log.read_from(cursor).0.is_empty());
    }

    #[test]
    fn trimming_keeps_cursors_absolute_and_resumes_at_the_oldest_kept_event() {
        let mut log = TranscriptLog::default();
        log.append(
            (0..MAX_LOG_EVENTS + 10)
                .map(|i| note(&i.to_string()))
                .collect(),
        );
        assert_eq!(log.events.len(), MAX_LOG_EVENTS);
        assert_eq!(log.dropped, 10);

        // A cursor stranded before the trim point resumes at the oldest event
        // still held rather than panicking or re-sending everything.
        let (events, next) = log.read_from(0);
        assert_eq!(events.len(), MAX_LOG_EVENTS);
        assert_eq!(events[0], note("10"));
        assert_eq!(next, MAX_LOG_EVENTS + 10);
    }

    #[test]
    fn reading_an_unknown_handle_loads_from_disk_when_present() {
        let dir = scratch_dir("disk-read");
        let handle = "web/fix-login";
        let events = vec![note("persisted")];
        store::append_events(&dir, handle, &events);
        let hub = WebSessionHub::new(dir.clone());
        let (loaded, next) = hub.read_from(handle, 0);
        assert_eq!(loaded, events);
        assert_eq!(next, 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn reading_an_unknown_handle_leaves_the_cursor_untouched_when_disk_empty() {
        let hub = WebSessionHub::new(scratch_dir("unknown"));
        assert_eq!(hub.read_from("web/none", 7), (Vec::new(), 7));
    }

    #[test]
    fn releasing_the_last_holder_marks_the_slot_idle_without_dropping_it() {
        let mut holders = HolderCount(1);
        assert!(holders.release());
        // The slot survives so the ACP child outlives a reload; eviction is
        // the next acquire's job, not release's.
        assert_eq!(holders.0, 0);
    }

    #[test]
    fn holder_count_retains_across_one_release_when_two_acquires() {
        let mut holders = HolderCount(2);
        assert!(!holders.release());
        assert!(holders.release());
    }

    #[test]
    fn finished_prompt_reports_turn_end_with_stop_reason() {
        let event = map_request_finished("session/prompt", Ok(json!({ "stopReason": "end_turn" })));
        assert_eq!(
            event,
            Some(SessionServerEvent::TurnEnd {
                stop_reason: Some("end_turn".to_string()),
            })
        );
    }

    #[test]
    fn finished_non_prompt_request_reports_nothing() {
        assert_eq!(map_request_finished("session/cancel", Ok(json!({}))), None);
    }

    #[test]
    fn failed_request_reports_error() {
        let event = map_request_finished("session/prompt", Err("boom".to_string()));
        assert_eq!(
            event,
            Some(SessionServerEvent::Error {
                message: "boom".to_string(),
            })
        );
    }

    #[test]
    fn drain_maps_session_update_notifications() {
        let update = serde_json::json!({
            "sessionId": "sess",
            "update": {
                "sessionUpdate": "agent_message_chunk",
                "content": { "type": "text", "text": "hello" }
            }
        });
        let events = map_acp_session_update(&update);
        assert_eq!(events.len(), 1);
    }
}

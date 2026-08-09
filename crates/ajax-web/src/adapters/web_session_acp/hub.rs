//! Task-scoped ACP host sessions keyed by qualified handle.

use super::client::{AcpClientEvent, AcpStdioClient};
use crate::slices::web_session::{
    map_acp_client_request, map_acp_session_update, SessionServerEvent,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::Path,
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
/// growing without limit.
const MAX_LOG_EVENTS: usize = 2000;
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
    holders: HolderCount,
    log: TranscriptLog,
    last_released: Option<Instant>,
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
}

impl Default for WebSessionHub {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }
}

impl WebSessionHub {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn acquire(
        &self,
        qualified_handle: &str,
        worktree_path: &Path,
    ) -> Result<Arc<Mutex<AcpStdioClient>>, String> {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(slot) = sessions.get_mut(qualified_handle) {
            slot.add_holder();
            return Ok(Arc::clone(&slot.client));
        }
        evict_idle_over_limit(&mut sessions);
        let client = Arc::new(Mutex::new(AcpStdioClient::spawn(worktree_path)?));
        sessions.insert(
            qualified_handle.to_string(),
            SessionSlot {
                client: Arc::clone(&client),
                holders: HolderCount(1),
                log: TranscriptLog::default(),
                last_released: None,
            },
        );
        Ok(client)
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
        let events = {
            let client = slot.client.lock().unwrap();
            drain_acp_events(&client)
        };
        if !events.is_empty() {
            slot.log.append(events);
        }
    }

    /// Append an event the ACP client will never produce. The agent does not
    /// echo the operator's own prompts, so without this a replayed transcript
    /// would carry the agent's half of the conversation and none of yours.
    pub fn record(&self, handle: &str, event: SessionServerEvent) {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(slot) = sessions.get_mut(handle) {
            slot.log.append(vec![event]);
        }
    }

    pub fn read_from(&self, handle: &str, cursor: usize) -> (Vec<SessionServerEvent>, usize) {
        let sessions = self.sessions.lock().unwrap();
        match sessions.get(handle) {
            Some(slot) => slot.log.read_from(cursor),
            None => (Vec::new(), cursor),
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

pub fn drain_acp_events(client: &AcpStdioClient) -> Vec<SessionServerEvent> {
    let mut events = Vec::new();
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
                events.push(SessionServerEvent::Error {
                    message: "ACP process exited".to_string(),
                });
            }
        }
    }
    events
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

    fn note(text: &str) -> SessionServerEvent {
        SessionServerEvent::Message {
            role: "agent".to_string(),
            text: text.to_string(),
        }
    }

    #[test]
    fn hub_release_is_noop_when_handle_missing() {
        let hub = WebSessionHub::new();
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
    fn reading_an_unknown_handle_leaves_the_cursor_untouched() {
        let hub = WebSessionHub::new();
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

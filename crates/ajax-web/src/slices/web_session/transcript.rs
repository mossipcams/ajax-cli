//! In-memory transcript cursor and replay filtering.

use super::SessionServerEvent;
use crate::adapters::web_session_store::MAX_LOG_EVENTS;
use std::collections::HashSet;

/// Per-task transcript bound. Long sessions trim from the front rather than
/// growing without limit.
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
    pub(crate) fn from_events(events: Vec<SessionServerEvent>, dropped: usize) -> Self {
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

/// Host commentary, not agent output: the browser marks an `agent` message as a
/// live turn, so a note in that role would leave the thread reading "Working"
/// with nothing running.
pub(crate) fn context_reset_note() -> SessionServerEvent {
    SessionServerEvent::Message {
        role: "note".to_string(),
        text: "Model context reset after restart. Prior turns are still visible here.".to_string(),
        message_id: None,
    }
}

pub(crate) fn context_reset_needed(resumed: bool, log: &TranscriptLog) -> bool {
    !resumed && !log.events.is_empty()
}

/// True when the log already ends with this note. Each restart would otherwise
/// stack another identical copy on the transcript.
pub(crate) fn already_noted(log: &TranscriptLog, note: &SessionServerEvent) -> bool {
    log.events.last() == Some(note)
}

pub(crate) fn slot_must_replace(
    acp_alive: bool,
    slot_model: &str,
    want_model: &str,
    host_exited: bool,
) -> bool {
    !acp_alive || host_exited || slot_model != want_model
}

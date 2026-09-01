//! In-memory transcript cursor and replay filtering.

use super::protocol::SessionEventEnvelope;
use super::SessionServerEvent;
use crate::adapters::web_session_store::MAX_LOG_EVENTS;
use std::{collections::HashSet, time::Duration};

/// Per-task transcript bound. Long sessions trim from the front rather than
/// growing without limit.
pub(crate) const MAX_IDLE_SESSIONS: usize = 8;

/// How long a disconnected slot keeps its live ACP child before idle-LRU may
/// reclaim it. Sized for PWA / Safari background reconnect (order of minutes).
pub(crate) const IDLE_RELEASE_GRACE: Duration = Duration::from_secs(15 * 60);

pub(crate) fn idle_release_grace() -> Duration {
    #[cfg(test)]
    if let Some(grace) = test_idle_release_grace_override() {
        return grace;
    }
    IDLE_RELEASE_GRACE
}

#[cfg(test)]
static TEST_IDLE_RELEASE_GRACE: std::sync::Mutex<Option<Duration>> = std::sync::Mutex::new(None);

#[cfg(test)]
static TEST_IDLE_RELEASE_GRACE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
fn test_idle_release_grace_override() -> Option<Duration> {
    *TEST_IDLE_RELEASE_GRACE.lock().unwrap()
}

#[cfg(test)]
pub(crate) fn with_test_idle_release_grace<F, R>(grace: Duration, f: F) -> R
where
    F: FnOnce() -> R,
{
    let _guard = TEST_IDLE_RELEASE_GRACE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let mut slot = TEST_IDLE_RELEASE_GRACE.lock().unwrap();
    let previous = *slot;
    *slot = Some(grace);
    drop(slot);
    let result = f();
    *TEST_IDLE_RELEASE_GRACE.lock().unwrap() = previous;
    result
}

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

    pub(crate) fn absolute_next_cursor(&self) -> usize {
        self.dropped + self.events.len()
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
    #[cfg(test)]
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

    /// Like [`read_from`](Self::read_from), but each row keeps its absolute log
    /// index even when resolved permission requests are filtered out.
    pub(crate) fn read_from_enveloped(&self, cursor: usize) -> (Vec<SessionEventEnvelope>, usize) {
        let next = self.absolute_next_cursor();
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
        let envelopes = self.events[start..]
            .iter()
            .enumerate()
            .filter_map(|(index, event)| {
                if matches!(
                    event,
                    SessionServerEvent::PermissionRequest { request_id, .. }
                        if resolved.contains(request_id)
                ) {
                    return None;
                }
                let absolute_cursor = self.dropped + start + index;
                Some(SessionEventEnvelope::new(absolute_cursor, event.clone()))
            })
            .collect();
        (envelopes, next)
    }
}

/// Host commentary, not agent output: the browser marks an `agent` message as a
/// live turn, so a note in that role would leave the thread reading "Working"
/// with nothing running.
pub(crate) fn context_reset_note() -> SessionServerEvent {
    SessionServerEvent::Message {
        role: "note".to_string(),
        text: "Model context reset after restart. Prior turns are still visible here.".to_string(),
        content_blocks: Vec::new(),
        item_id: "context-reset".to_string(),
        message_id: None,
    }
}

pub(crate) fn harness_switch_note(item_id: String) -> SessionServerEvent {
    SessionServerEvent::Message {
        role: "note".to_string(),
        text: "Client switched harness. Context reset.".to_string(),
        content_blocks: Vec::new(),
        item_id,
        message_id: None,
    }
}

pub(crate) fn context_cleared_note(item_id: String) -> SessionServerEvent {
    SessionServerEvent::Message {
        role: "note".to_string(),
        text: "Context cleared.".to_string(),
        content_blocks: Vec::new(),
        item_id,
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

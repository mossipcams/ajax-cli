use super::protocol::SessionEventEnvelope;
use super::SessionServerEvent;
use crate::adapters::web_session_store::MAX_LOG_EVENTS;
use std::{collections::HashSet, time::Duration};

pub(crate) const MAX_IDLE_SESSIONS: usize = 8;

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

#[derive(Default)]
pub(crate) struct TranscriptLog {
    pub(crate) events: Vec<SessionServerEvent>,
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

    pub(crate) fn superseded_cursors_for(&self, batch: &[SessionServerEvent]) -> Vec<usize> {
        let mut cursors = Vec::new();
        for event in batch {
            let SessionServerEvent::Message { item_id, .. } = event else {
                continue;
            };
            if item_id.is_empty() || is_superseded_marker(event) {
                continue;
            }
            for (index, stored) in self.events.iter().rev().enumerate() {
                let SessionServerEvent::Message {
                    item_id: stored_item,
                    ..
                } = stored
                else {
                    continue;
                };
                if stored_item != item_id {
                    continue;
                }
                if !is_superseded_marker(stored) {
                    cursors.push(self.dropped + self.events.len() - 1 - index);
                }
                break;
            }
        }
        cursors.sort_unstable();
        cursors.dedup();
        cursors
    }

    pub(crate) fn marker_for(&self, cursor: usize) -> Option<SessionServerEvent> {
        let mut marker = self.events.get(cursor.checked_sub(self.dropped)?)?.clone();
        let SessionServerEvent::Message {
            text,
            content_blocks,
            message_id,
            ..
        } = &mut marker
        else {
            return None;
        };
        text.clear();
        content_blocks.clear();
        *message_id = None;
        Some(marker)
    }

    pub(crate) fn mark_superseded(&mut self, cursors: &[usize]) {
        for &cursor in cursors {
            if let Some(marker) = self.marker_for(cursor) {
                self.events[cursor - self.dropped] = marker;
            }
        }
    }

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
                !is_superseded_marker(event)
                    && !matches!(
                        event,
                        SessionServerEvent::PermissionRequest { request_id, .. }
                            if resolved.contains(request_id)
                    )
            })
            .cloned()
            .collect();
        (events, next)
    }

    pub(crate) fn read_from_enveloped(&self, cursor: usize) -> (Vec<SessionEventEnvelope>, usize) {
        let next = self.absolute_next_cursor();
        let start = cursor.saturating_sub(self.dropped).min(self.events.len());
        let usage_reset = self
            .events
            .iter()
            .rposition(|event| matches!(event, SessionServerEvent::UsageReset));
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
                if is_superseded_marker(event) {
                    return None;
                }
                if usage_reset.is_some_and(|reset| start + index < reset)
                    && matches!(
                        event,
                        SessionServerEvent::Usage { .. } | SessionServerEvent::TurnUsage { .. }
                    )
                {
                    return None;
                }
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

pub(crate) fn is_superseded_marker(event: &SessionServerEvent) -> bool {
    matches!(
        event,
        SessionServerEvent::Message { text, content_blocks, item_id, .. }
            if text.is_empty() && content_blocks.is_empty() && !item_id.is_empty()
    )
}

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

pub(crate) fn already_noted(log: &TranscriptLog, note: &SessionServerEvent) -> bool {
    log.events.last() == Some(note)
}

pub(crate) fn slot_must_replace(acp_alive: bool, host_exited: bool) -> bool {
    !acp_alive || host_exited
}

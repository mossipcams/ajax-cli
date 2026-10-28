use super::protocol::{
    PendingElicitation, PendingPermission, SessionChrome, SessionEventEnvelope, SessionSnapshot,
};
use super::transcript::TranscriptLog;
use std::collections::{HashMap, HashSet};

use super::SessionServerEvent;

pub(crate) const TAIL_EVENT_LIMIT: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReplayPlan {
    pub reset: bool,
    pub from: usize,
}

pub(crate) fn plan_replay(client_cursor: Option<usize>, log: &TranscriptLog) -> ReplayPlan {
    let next = log.absolute_next_cursor();
    let dropped = log.dropped;
    let Some(client) = client_cursor else {
        return ReplayPlan {
            reset: true,
            from: 0,
        };
    };
    if client < dropped || client > next {
        ReplayPlan {
            reset: true,
            from: dropped,
        }
    } else {
        ReplayPlan {
            reset: false,
            from: client,
        }
    }
}

pub(crate) struct TranscriptPage {
    pub events: Vec<SessionEventEnvelope>,
    pub from: usize,
    pub floor: usize,
}

pub(crate) fn build_attach(
    log: &TranscriptLog,
    model: String,
    busy: bool,
    client_cursor: Option<usize>,
    chrome: SessionChrome,
) -> (SessionSnapshot, Vec<SessionEventEnvelope>) {
    let plan = plan_replay(client_cursor, log);
    let (replayed, next) = log.read_from_enveloped(plan.from);
    let snapshot = SessionSnapshot::new(
        next,
        model,
        busy,
        plan.reset,
        pending_permission(log),
        pending_elicitation(log),
        chrome,
    );
    let snapshot = if plan.reset {
        attach_inline_tail(snapshot, &replayed, next, log.dropped)
    } else {
        snapshot
    };
    (snapshot, replayed)
}

pub(crate) fn attach_inline_tail(
    snapshot: SessionSnapshot,
    replayed: &[SessionEventEnvelope],
    next: usize,
    floor: usize,
) -> SessionSnapshot {
    let start = tail_start_index(
        replayed,
        snapshot.pending_permission.as_ref(),
        snapshot.pending_elicitation.as_ref(),
    );
    let transcript_from = Some(replayed.get(start).map(|env| env.cursor).unwrap_or(next));
    snapshot
        .with_transcript(
            compact_superseded_messages(replayed[start..].to_vec()),
            transcript_from,
        )
        .with_transcript_floor(floor)
}

fn tail_start_index(
    replayed: &[SessionEventEnvelope],
    pending_perm: Option<&PendingPermission>,
    pending_eli: Option<&PendingElicitation>,
) -> usize {
    let mut start = replayed.len().saturating_sub(TAIL_EVENT_LIMIT);
    if let Some(pending) = pending_perm {
        if let Some(index) = replayed.iter().position(|env| {
            matches!(
                &env.payload,
                SessionServerEvent::PermissionRequest {
                    request_id, ..
                } if *request_id == pending.request_id
            )
        }) {
            start = start.min(index);
        }
    }
    if let Some(pending) = pending_eli {
        if let Some(index) = replayed.iter().position(|env| {
            matches!(
                &env.payload,
                SessionServerEvent::ElicitationRequest {
                    request_id, ..
                } if *request_id == pending.request_id
            )
        }) {
            start = start.min(index);
        }
    }
    if let Some(cursor) =
        user_message_boundary(replayed, start, TAIL_EVENT_LIMIT).map(|idx| replayed[idx].cursor)
    {
        if let Some(index) = replayed.iter().position(|env| env.cursor == cursor) {
            return index;
        }
    }
    start
}

pub(crate) fn read_before(log: &TranscriptLog, before: usize, limit: usize) -> TranscriptPage {
    let end = before.clamp(log.dropped, log.absolute_next_cursor());
    let (envelopes, _) = log.read_from_enveloped(log.dropped);
    let mut start = end.saturating_sub(limit).max(log.dropped);
    if limit > 0 {
        let bound_index = envelopes
            .iter()
            .rposition(|env| env.cursor < start)
            .map_or(0, |i| i + 1);
        if let Some(idx) = user_message_boundary(&envelopes, bound_index, limit) {
            start = envelopes[idx].cursor;
        }
    }
    let i0 = envelopes
        .iter()
        .rposition(|env| env.cursor < start)
        .map_or(0, |i| i + 1);
    let i1 = envelopes.partition_point(|env| env.cursor < end);
    let events = compact_superseded_messages(envelopes[i0..i1].to_vec());
    let from = if events.is_empty() && limit > 0 {
        start
    } else {
        events.first().map(|env| env.cursor).unwrap_or(end)
    };
    TranscriptPage {
        from,
        floor: log.dropped,
        events,
    }
}

fn user_message_boundary(
    replayed: &[SessionEventEnvelope],
    start: usize,
    bound: usize,
) -> Option<usize> {
    let mut seen = 0usize;
    for (idx, env) in replayed[..start].iter().enumerate().rev() {
        seen += 1;
        if seen > bound {
            break;
        }
        if matches!(&env.payload, SessionServerEvent::Message { ref role, .. } if role == "user") {
            return Some(idx);
        }
    }
    None
}

fn compact_superseded_messages(events: Vec<SessionEventEnvelope>) -> Vec<SessionEventEnvelope> {
    let mut out: Vec<SessionEventEnvelope> = Vec::with_capacity(events.len());
    let mut first_seen: HashMap<String, usize> = HashMap::new();
    for env in events {
        if let SessionServerEvent::Message { item_id, .. } = &env.payload {
            if !item_id.is_empty() {
                if let Some(&pos) = first_seen.get(item_id) {
                    out[pos].payload = env.payload;
                    continue;
                }
                first_seen.insert(item_id.clone(), out.len());
            }
        }
        out.push(env);
    }
    out
}

pub(crate) fn pending_permission(log: &TranscriptLog) -> Option<PendingPermission> {
    let resolved: HashSet<&str> = log
        .events
        .iter()
        .filter_map(|event| match event {
            SessionServerEvent::PermissionResolved { request_id, .. } => Some(request_id.as_str()),
            _ => None,
        })
        .collect();
    log.events.iter().rev().find_map(|event| match event {
        SessionServerEvent::PermissionRequest {
            request_id,
            title,
            detail,
        } if !resolved.contains(request_id.as_str()) => Some(PendingPermission {
            request_id: request_id.clone(),
            title: title.clone(),
            detail: detail.clone(),
        }),
        _ => None,
    })
}

pub(crate) fn pending_elicitation(log: &TranscriptLog) -> Option<PendingElicitation> {
    let mut open: Option<PendingElicitation> = None;
    for event in &log.events {
        match event {
            SessionServerEvent::ElicitationRequest {
                request_id,
                message,
                schema,
            } => {
                open = Some(PendingElicitation {
                    request_id: request_id.clone(),
                    message: message.clone(),
                    schema: schema.clone(),
                });
            }
            SessionServerEvent::ElicitationResolved { request_id, .. }
                if open
                    .as_ref()
                    .is_some_and(|pending| pending.request_id == *request_id) =>
            {
                open = None;
            }
            _ => {}
        }
    }
    open
}

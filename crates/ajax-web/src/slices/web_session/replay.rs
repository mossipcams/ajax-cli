use super::protocol::{
    PendingElicitation, PendingPermission, SessionChrome, SessionEventEnvelope, SessionSnapshot,
};
use super::transcript::TranscriptLog;
use std::collections::HashMap;

use super::SessionServerEvent;

/// Maximum number of newest transcript events carried in a reset attach
/// snapshot's `transcript`; older history stays reachable via the cursor.
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

/// One page of older retained transcript events, returned for a cursor walk
/// back toward the retention floor.
pub(crate) struct TranscriptPage {
    pub events: Vec<SessionEventEnvelope>,
    /// Cursor of the first event in `events`. For an empty page with
    /// `limit > 0` this is the lowered (post-snap) lower bound, strictly less
    /// than `before` whenever `before > floor`; with `limit == 0` it is
    /// `end`, which makes no progress. Pass as the next page's `before`
    /// to keep walking backwards.
    pub from: usize,
    /// Retention floor; no older page exists once a walk reaches it.
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

/// Attach the inline transcript tail to a reset snapshot: the newest
/// `TAIL_EVENT_LIMIT` events of `replayed`, extended backwards to include the
/// request event of any open permission or elicitation carried on the
/// snapshot, then snapped back to a user-message boundary. Sets
/// `transcript_from` to the first tail cursor (or `next` when empty) and
/// `transcript_floor` to `floor`.
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

/// Index into `replayed` where a reset snapshot's transcript tail begins:
/// the newest `TAIL_EVENT_LIMIT` events, extended backwards to include the
/// request event of any currently open permission or elicitation.
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
    // Snap the tail start back to the nearest preceding user-role message so
    // the client never starts mid-turn, bounded by one more TAIL_EVENT_LIMIT.
    if let Some(cursor) =
        user_message_boundary(replayed, start, TAIL_EVENT_LIMIT).map(|idx| replayed[idx].cursor)
    {
        if let Some(index) = replayed.iter().position(|env| env.cursor == cursor) {
            return index;
        }
    }
    start
}

/// One page of older retained transcript events before `before` (exclusive),
/// with the page start snapped back to a user-message boundary. Envelopes come
/// from the same filtered read as attach replay, so cursors stay absolute and
/// permission/usage filtering matches `build_attach`. A non-empty page sets
/// `from` to its first event cursor; an empty page with `limit > 0` sets it to
/// the lowered (post-snap) lower bound, strictly less than `before`
/// whenever `before > floor`, so repeated pages terminate at the floor.
/// A page requested with `limit == 0` is always empty and makes no progress
/// (`from == end`); callers walking back must use `limit >= 1`.
pub(crate) fn read_before(log: &TranscriptLog, before: usize, limit: usize) -> TranscriptPage {
    let end = before.clamp(log.dropped, log.absolute_next_cursor());
    let (envelopes, _) = log.read_from_enveloped(log.dropped);
    // First index whose envelope cursor is >= `start`; the page is the slice
    // [i0, i1) in ascending cursor order.
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
    // An empty page with `limit > 0` must still make progress: report the
    // computed post-snap lower bound as `from`, never `end`. That guarantees
    // `from < before` whenever `before > log.dropped`, so a caller walking
    // history backwards (`before = page.from`) advances toward the floor.
    // With `limit == 0` no progress is possible and `from == end`; callers
    // must use `limit >= 1` when walking back.
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

/// Index of the nearest user-role message envelope within `bound` positions
/// before `start` in `replayed`, if any; both tails and older pages use it so
/// a page never starts mid-turn.
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

/// Collapse streamed message versions: for every non-empty `item_id` that
/// appears more than once, keep a single envelope at the first occurrence's
/// position and cursor carrying the last occurrence's payload; drop all other
/// occurrences. Empty `item_id` messages and non-message events pass through
/// unchanged and in order.
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
    let mut open: Option<PendingPermission> = None;
    for event in &log.events {
        match event {
            SessionServerEvent::PermissionRequest {
                request_id,
                title,
                detail,
            } => {
                open = Some(PendingPermission {
                    request_id: request_id.clone(),
                    title: title.clone(),
                    detail: detail.clone(),
                });
            }
            SessionServerEvent::PermissionResolved { request_id, .. }
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

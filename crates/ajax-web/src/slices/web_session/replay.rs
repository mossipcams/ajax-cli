//! Cursor validation and replay planning for protocol v2 attach.

use super::protocol::{
    PendingElicitation, PendingPermission, SessionChrome, SessionEventEnvelope, SessionSnapshot,
};
use super::transcript::TranscriptLog;
use super::SessionServerEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReplayPlan {
    pub reset: bool,
    pub from: usize,
}

/// Decide whether a browser-supplied cursor can resume incrementally.
pub(crate) fn plan_replay(client_cursor: Option<usize>, log: &TranscriptLog) -> ReplayPlan {
    let next = log.absolute_next_cursor();
    let dropped = log.dropped;
    let Some(client) = client_cursor else {
        return ReplayPlan {
            reset: false,
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
    (snapshot, replayed)
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
            SessionServerEvent::PermissionResolved { request_id, .. } => {
                if open
                    .as_ref()
                    .is_some_and(|pending| pending.request_id == *request_id)
                {
                    open = None;
                }
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
            SessionServerEvent::ElicitationResolved { request_id, .. } => {
                if open
                    .as_ref()
                    .is_some_and(|pending| pending.request_id == *request_id)
                {
                    open = None;
                }
            }
            _ => {}
        }
    }
    open
}

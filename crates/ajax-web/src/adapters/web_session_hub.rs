//! Process-local Ajax Web Session hub: shared ACP lifetime, parked operator
//! requests, and cross-session attention fan-out.
//!
//! ponytail: non-primary sessions retain the existing fixed grace TTL; ACP-primary
//! sessions live until explicit task teardown so reconnects preserve the peer.

use super::web_session_rpc::{
    spawn_default_agent_acp, AgentAcpError, AgentAcpEvent, AgentAcpProcess, OperatorRequestKind,
};
use crate::slices::web_session::{
    AttentionKind, AttentionResponse, FailedAttentionAction, PermissionOutcome,
    ReviewAttentionAction, WebSessionServerEvent, WEB_SESSION_PROTOCOL_VERSION,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{sync_channel, Receiver, SyncSender},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

/// Keep ACP alive briefly after the last UI subscriber leaves when nothing is parked.
pub(crate) const WEB_SESSION_HUB_GRACE: Duration = Duration::from_secs(30);

const SUBSCRIBER_QUEUE: usize = 64;

#[derive(Clone, Debug)]
pub(crate) enum HubClientEvent {
    Local(AgentAcpEvent),
    Attention(WebSessionServerEvent),
}

struct PendingOperatorRequest {
    json_rpc_id: Value,
    kind: OperatorRequestKind,
}

struct TaskSlot {
    handle: String,
    worktree: PathBuf,
    peer: Mutex<Option<AgentAcpProcess>>,
    pending: Mutex<HashMap<String, PendingOperatorRequest>>,
    last_user_prompt: Mutex<Option<String>>,
    local_subscribers: Mutex<HashMap<u64, SyncSender<HubClientEvent>>>,
    subscriber_count: Mutex<usize>,
    grace_deadline: Mutex<Option<Instant>>,
    acp_primary: Mutex<bool>,
    session_id: Mutex<Option<String>>,
}

pub(crate) struct WebSessionHub {
    next_subscriber: AtomicU64,
    slots: Mutex<HashMap<String, Arc<TaskSlot>>>,
    /// Every live web-session socket (any task) receives cross-session attention.
    bus: Mutex<HashMap<u64, SyncSender<HubClientEvent>>>,
}

pub(crate) struct HubSubscription {
    pub(crate) subscriber_id: u64,
    pub(crate) handle: String,
    events: Arc<Mutex<Receiver<HubClientEvent>>>,
    hub: Arc<WebSessionHub>,
}

impl Drop for HubSubscription {
    fn drop(&mut self) {
        self.hub.detach(&self.handle, self.subscriber_id);
    }
}

impl WebSessionHub {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            next_subscriber: AtomicU64::new(1),
            slots: Mutex::new(HashMap::new()),
            bus: Mutex::new(HashMap::new()),
        })
    }

    pub(crate) fn attach(
        self: &Arc<Self>,
        handle: &str,
        worktree: PathBuf,
        acp_primary: bool,
    ) -> Result<HubSubscription, AgentAcpError> {
        let slot = self.get_or_create_slot(handle, worktree, acp_primary)?;
        let subscriber_id = self.next_subscriber.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = sync_channel(SUBSCRIBER_QUEUE);
        slot.local_subscribers
            .lock()
            .expect("local subscribers")
            .insert(subscriber_id, tx.clone());
        *slot.subscriber_count.lock().expect("subscriber count") += 1;
        *slot.grace_deadline.lock().expect("grace") = None;
        *slot.acp_primary.lock().expect("acp primary") |= acp_primary;
        self.bus.lock().expect("bus").insert(subscriber_id, tx);

        // Replay parked requests so reconnecting clients see them.
        for (request_id, pending) in slot.pending.lock().expect("pending").iter() {
            let event = attention_required_event(
                &slot.handle,
                request_id,
                pending.kind,
                &operator_summary_for_kind(pending.kind),
            );
            let _ = slot
                .local_subscribers
                .lock()
                .expect("local subscribers")
                .get(&subscriber_id)
                .map(|sender| sender.try_send(HubClientEvent::Attention(event)));
        }

        Ok(HubSubscription {
            subscriber_id,
            handle: handle.to_string(),
            events: Arc::new(Mutex::new(rx)),
            hub: Arc::clone(self),
        })
    }

    fn detach(&self, handle: &str, subscriber_id: u64) {
        self.bus.lock().expect("bus").remove(&subscriber_id);
        let slots = self.slots.lock().expect("slots");
        let Some(slot) = slots.get(handle) else {
            return;
        };
        slot.local_subscribers
            .lock()
            .expect("local subscribers")
            .remove(&subscriber_id);
        let mut count = slot.subscriber_count.lock().expect("subscriber count");
        *count = count.saturating_sub(1);
        if *count == 0 {
            let pending_empty = slot.pending.lock().expect("pending").is_empty();
            if pending_empty && !*slot.acp_primary.lock().expect("acp primary") {
                *slot.grace_deadline.lock().expect("grace") =
                    Some(Instant::now() + WEB_SESSION_HUB_GRACE);
            }
        }
    }

    fn get_or_create_slot(
        &self,
        handle: &str,
        worktree: PathBuf,
        acp_primary: bool,
    ) -> Result<Arc<TaskSlot>, AgentAcpError> {
        self.reap_expired();
        let mut slots = self.slots.lock().expect("slots");
        if let Some(existing) = slots.get(handle) {
            *existing.acp_primary.lock().expect("acp primary") |= acp_primary;
            let mut peer_guard = existing.peer.lock().expect("peer");
            if peer_guard.is_none() {
                let mut peer = spawn_default_agent_acp(&existing.worktree)?;
                let previous_session_id = existing.session_id.lock().expect("session id").clone();
                let session_id = peer
                    .handshake_with_session(&existing.worktree, previous_session_id.as_deref())?;
                *existing.session_id.lock().expect("session id") = Some(session_id);
                *peer_guard = Some(peer);
            }
            drop(peer_guard);
            return Ok(Arc::clone(existing));
        }
        let mut peer = spawn_default_agent_acp(&worktree)?;
        let session_id = peer.handshake(&worktree)?;
        let slot = Arc::new(TaskSlot {
            handle: handle.to_string(),
            worktree,
            peer: Mutex::new(Some(peer)),
            pending: Mutex::new(HashMap::new()),
            last_user_prompt: Mutex::new(None),
            local_subscribers: Mutex::new(HashMap::new()),
            subscriber_count: Mutex::new(0),
            grace_deadline: Mutex::new(None),
            acp_primary: Mutex::new(acp_primary),
            session_id: Mutex::new(Some(session_id)),
        });
        slots.insert(handle.to_string(), Arc::clone(&slot));
        Ok(slot)
    }

    fn reap_expired(&self) {
        let mut slots = self.slots.lock().expect("slots");
        let now = Instant::now();
        slots.retain(|_, slot| {
            if *slot.acp_primary.lock().expect("acp primary") {
                return true;
            }
            let subs = *slot.subscriber_count.lock().expect("subscriber count");
            if subs > 0 {
                return true;
            }
            if !slot.pending.lock().expect("pending").is_empty() {
                return true;
            }
            match *slot.grace_deadline.lock().expect("grace") {
                Some(deadline) if now >= deadline => false,
                Some(_) => true,
                None => false,
            }
        });
    }

    pub(crate) fn poll_peer_into_subscribers(&self, handle: &str) {
        let slot = {
            let slots = self.slots.lock().expect("slots");
            slots.get(handle).map(Arc::clone)
        };
        let Some(slot) = slot else {
            return;
        };
        let mut peer_guard = slot.peer.lock().expect("peer");
        let Some(peer) = peer_guard.as_mut() else {
            return;
        };
        while let Some(event) = peer.poll_event() {
            match event {
                AgentAcpEvent::OperatorRequest {
                    request_id,
                    json_rpc_id,
                    summary,
                    kind,
                    ..
                } => {
                    slot.pending.lock().expect("pending").insert(
                        request_id.clone(),
                        PendingOperatorRequest { json_rpc_id, kind },
                    );
                    let attention =
                        attention_required_event(&slot.handle, &request_id, kind, &summary);
                    self.broadcast_attention(attention);
                }
                AgentAcpEvent::Exited => {
                    self.fanout_local(&slot, HubClientEvent::Local(AgentAcpEvent::Exited));
                    *peer_guard = None;
                    break;
                }
                other => self.fanout_local(&slot, HubClientEvent::Local(other)),
            }
        }
    }

    pub(crate) fn release(&self, handle: &str) {
        if let Some(slot) = self.slots.lock().expect("slots").remove(handle) {
            let _ = slot.peer.lock().expect("peer").take();
        }
    }

    fn fanout_local(&self, slot: &TaskSlot, event: HubClientEvent) {
        let subscribers = slot.local_subscribers.lock().expect("local subscribers");
        for sender in subscribers.values() {
            let _ = sender.try_send(event.clone());
        }
    }

    fn broadcast_attention(&self, event: WebSessionServerEvent) {
        let bus = self.bus.lock().expect("bus");
        for sender in bus.values() {
            let _ = sender.try_send(HubClientEvent::Attention(event.clone()));
        }
    }

    pub(crate) fn send_prompt(&self, handle: &str, message: &str) -> Result<(), AgentAcpError> {
        let slot = self.slot(handle)?;
        *slot.last_user_prompt.lock().expect("last prompt") = Some(message.to_string());
        let mut peer = slot.peer.lock().expect("peer");
        let peer = peer.as_mut().ok_or(AgentAcpError::SessionClosed)?;
        peer.send_prompt(message)?;
        Ok(())
    }

    pub(crate) fn send_cancel(&self, handle: &str) -> Result<(), AgentAcpError> {
        let slot = self.slot(handle)?;
        let mut peer = slot.peer.lock().expect("peer");
        let peer = peer.as_mut().ok_or(AgentAcpError::SessionClosed)?;
        peer.send_cancel()
    }

    pub(crate) fn pending_snapshot(&self, handle: &str) -> Vec<WebSessionServerEvent> {
        let slot = {
            let slots = self.slots.lock().expect("slots");
            slots.get(handle).map(Arc::clone)
        };
        let Some(slot) = slot else {
            return Vec::new();
        };
        let pending = slot.pending.lock().expect("pending");
        let events: Vec<_> = pending
            .iter()
            .map(|(request_id, pending)| {
                attention_required_event(
                    handle,
                    request_id,
                    pending.kind,
                    &operator_summary_for_kind(pending.kind),
                )
            })
            .collect();
        drop(pending);
        events
    }

    pub(crate) fn respond_attention(
        &self,
        target_handle: &str,
        request_id: &str,
        response: AttentionResponse,
    ) -> Result<WebSessionServerEvent, AttentionRespondError> {
        match response {
            AttentionResponse::Review {
                action: ReviewAttentionAction::Open,
            } => {
                // UI navigates; clear is optional for cockpit-derived banners.
                return Ok(WebSessionServerEvent::AttentionCleared {
                    version: WEB_SESSION_PROTOCOL_VERSION,
                    handle: target_handle.to_string(),
                    request_id: request_id.to_string(),
                });
            }
            AttentionResponse::Failed { action } => {
                return self.respond_failed(target_handle, request_id, action);
            }
            AttentionResponse::Permission { outcome } => {
                self.complete_parked(
                    target_handle,
                    request_id,
                    OperatorRequestKind::Permission,
                    permission_acp_result(outcome),
                )?;
            }
            AttentionResponse::Question { text } => {
                self.complete_parked(
                    target_handle,
                    request_id,
                    OperatorRequestKind::Question,
                    question_acp_result(&text),
                )?;
            }
        }
        let cleared = WebSessionServerEvent::AttentionCleared {
            version: WEB_SESSION_PROTOCOL_VERSION,
            handle: target_handle.to_string(),
            request_id: request_id.to_string(),
        };
        self.broadcast_attention(cleared.clone());
        Ok(cleared)
    }

    fn respond_failed(
        &self,
        target_handle: &str,
        request_id: &str,
        action: FailedAttentionAction,
    ) -> Result<WebSessionServerEvent, AttentionRespondError> {
        let slot = self
            .slot(target_handle)
            .map_err(|_| AttentionRespondError::HubGone)?;
        match action {
            FailedAttentionAction::Stop => {
                let mut peer = slot.peer.lock().expect("peer");
                let peer = peer.as_mut().ok_or(AttentionRespondError::HubGone)?;
                peer.send_cancel()
                    .map_err(|error| AttentionRespondError::Protocol(error.to_string()))?;
            }
            FailedAttentionAction::Retry => {
                let prompt = slot
                    .last_user_prompt
                    .lock()
                    .expect("last prompt")
                    .clone()
                    .ok_or(AttentionRespondError::NoRetryPrompt)?;
                {
                    let mut peer = slot.peer.lock().expect("peer");
                    let peer = peer.as_mut().ok_or(AttentionRespondError::HubGone)?;
                    let _ = peer.send_cancel();
                    peer.send_prompt(&prompt)
                        .map_err(|error| AttentionRespondError::Protocol(error.to_string()))?;
                }
            }
        }
        let cleared = WebSessionServerEvent::AttentionCleared {
            version: WEB_SESSION_PROTOCOL_VERSION,
            handle: target_handle.to_string(),
            request_id: request_id.to_string(),
        };
        self.broadcast_attention(cleared.clone());
        Ok(cleared)
    }

    fn complete_parked(
        &self,
        target_handle: &str,
        request_id: &str,
        expected_kind: OperatorRequestKind,
        result: Value,
    ) -> Result<(), AttentionRespondError> {
        let slot = self
            .slot(target_handle)
            .map_err(|_| AttentionRespondError::HubGone)?;
        let pending = {
            let mut map = slot.pending.lock().expect("pending");
            map.remove(request_id)
        };
        let Some(pending) = pending else {
            return Err(AttentionRespondError::StaleRequest);
        };
        if pending.kind != expected_kind {
            slot.pending
                .lock()
                .expect("pending")
                .insert(request_id.to_string(), pending);
            return Err(AttentionRespondError::KindMismatch);
        }
        let mut peer = slot.peer.lock().expect("peer");
        let peer = peer.as_mut().ok_or(AttentionRespondError::HubGone)?;
        peer.respond_json_rpc(&pending.json_rpc_id, result)
            .map_err(|error| AttentionRespondError::Protocol(error.to_string()))?;
        Ok(())
    }

    fn slot(&self, handle: &str) -> Result<Arc<TaskSlot>, AgentAcpError> {
        self.slots
            .lock()
            .expect("slots")
            .get(handle)
            .map(Arc::clone)
            .ok_or(AgentAcpError::SessionClosed)
    }

    pub(crate) fn try_recv(sub: &HubSubscription) -> Option<HubClientEvent> {
        sub.events.lock().expect("events").try_recv().ok()
    }

    /// Publish a cockpit-derived attention event (review / failed without ACP id).
    pub(crate) fn publish_attention(&self, event: WebSessionServerEvent) {
        self.broadcast_attention(event);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AttentionRespondError {
    HubGone,
    StaleRequest,
    KindMismatch,
    NoRetryPrompt,
    Protocol(String),
}

impl AttentionRespondError {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::HubGone => "hub_gone",
            Self::StaleRequest => "stale_request",
            Self::KindMismatch => "kind_mismatch",
            Self::NoRetryPrompt => "no_retry_prompt",
            Self::Protocol(_) => "respond_failed",
        }
    }

    pub(crate) fn message(&self) -> String {
        match self {
            Self::HubGone => "originating session hub is gone".to_string(),
            Self::StaleRequest => "attention request is no longer pending".to_string(),
            Self::KindMismatch => "attention response kind mismatch".to_string(),
            Self::NoRetryPrompt => "no prior prompt available to retry".to_string(),
            Self::Protocol(message) => message.clone(),
        }
    }
}

fn attention_required_event(
    handle: &str,
    request_id: &str,
    kind: OperatorRequestKind,
    summary: &str,
) -> WebSessionServerEvent {
    let (kind, title, options) = match kind {
        OperatorRequestKind::Permission => (
            AttentionKind::Permission,
            "Permission needed".to_string(),
            Some(vec!["allow-once".to_string(), "reject".to_string()]),
        ),
        OperatorRequestKind::Question => (AttentionKind::Question, "Question".to_string(), None),
    };
    WebSessionServerEvent::AttentionRequired {
        version: WEB_SESSION_PROTOCOL_VERSION,
        handle: handle.to_string(),
        request_id: request_id.to_string(),
        kind,
        title,
        summary: summary.to_string(),
        options,
    }
}

fn operator_summary_for_kind(kind: OperatorRequestKind) -> String {
    match kind {
        OperatorRequestKind::Permission => "Permission required".to_string(),
        OperatorRequestKind::Question => "Agent question".to_string(),
    }
}

fn permission_acp_result(outcome: PermissionOutcome) -> Value {
    match outcome {
        PermissionOutcome::AllowOnce => json!({
            "outcome": { "outcome": "selected", "optionId": "allow-once" }
        }),
        PermissionOutcome::Reject => json!({
            "outcome": { "outcome": "rejected" }
        }),
    }
}

fn question_acp_result(text: &str) -> Value {
    json!({
        "outcome": {
            "outcome": "answered",
            "text": text,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_and_question_acp_payloads_are_stable() {
        assert_eq!(
            permission_acp_result(PermissionOutcome::AllowOnce)["outcome"]["optionId"],
            "allow-once"
        );
        assert_eq!(question_acp_result("ship it")["outcome"]["text"], "ship it");
    }

    #[test]
    fn hub_new_is_empty() {
        let hub = WebSessionHub::new();
        assert!(hub.pending_snapshot("web/fix-login").is_empty());
    }

    #[test]
    fn release_is_explicit_and_safe_for_missing_task() {
        WebSessionHub::new().release("web/missing");
    }
}

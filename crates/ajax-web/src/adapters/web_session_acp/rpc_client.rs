//! The transport-neutral client core shared by every RPC-style harness transport: it owns the session, the event queue and the prompt request ids.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use agent_client_protocol::schema::v1::{ContentBlock, ResourceLink, TextContent};

use super::client::AcpClientEvent;
use super::rpc_session::{RpcSession, RpcStep};

pub type RunFinishedHook = fn(&mut RpcSession) -> Option<AcpClientEvent>;

pub struct RpcClientCore {
    inner: Mutex<CoreInner>,
}

struct CoreInner {
    session: Option<RpcSession>,
    /// Remainder of a `RpcStep::Events` batch not yet consumed by callers.
    event_queue: VecDeque<AcpClientEvent>,
    /// The request id assigned by the most recent `begin_prompt`, cleared on finish.
    active_request_id: Option<u64>,
    /// Monotonically increasing counter for request ids (starts at 1).
    next_request_id: u64,
    /// True once `Exited` was observed or `shutdown` removed the session.
    host_exited: bool,
    on_run_finished: Option<RunFinishedHook>,
}

impl RpcClientCore {
    pub fn new(session: RpcSession, on_run_finished: Option<RunFinishedHook>) -> Self {
        Self {
            inner: Mutex::new(CoreInner {
                session: Some(session),
                event_queue: VecDeque::new(),
                active_request_id: None,
                next_request_id: 1,
                host_exited: false,
                on_run_finished,
            }),
        }
    }

    /// The session id assigned during handshake. Returns an empty string if the
    /// session has been shut down.
    pub fn session_id(&self) -> String {
        let g = self.inner.lock().expect("rpc_client mutex poisoned");
        g.session
            .as_ref()
            .map(|s| s.session_id().to_string())
            .unwrap_or_default()
    }

    /// Build the prompt text from content blocks and begin a prompt run.
    ///
    /// Text blocks are joined with `"\n\n"`; ResourceLink blocks contribute their URI as a line;
    /// all other block kinds produce a short placeholder. Returns the request id for tracking.
    pub fn begin_prompt(&self, blocks: &[ContentBlock]) -> Result<u64, String> {
        let mut parts: Vec<String> = Vec::new();

        for block in blocks {
            match block {
                ContentBlock::Text(tc @ TextContent { .. }) => {
                    parts.push(tc.text.clone());
                }
                ContentBlock::ResourceLink(rl @ ResourceLink { .. }) => {
                    parts.push(rl.uri.clone());
                }
                // ponytail: images are not forwarded yet — only a placeholder is emitted
                _ => {
                    parts.push("[image omitted]".to_string());
                }
            }
        }

        let message = parts.join("\n\n");
        if message.is_empty() {
            return Err("empty prompt content: no blocks supplied".to_string());
        }

        let mut g = self.inner.lock().expect("rpc_client mutex poisoned");
        let session = g
            .session
            .as_mut()
            .ok_or_else(|| "session not available (shut down)".to_string())?;
        session
            .begin_prompt(&message)
            .map_err(|e| format!("begin_prompt failed: {e}"))?;

        let id = g.next_request_id;
        g.next_request_id += 1;
        g.active_request_id = Some(id);

        Ok(id)
    }

    /// Abort the active prompt run. Returns `Err` when no run is in flight.
    pub fn cancel(&self) -> Result<(), String> {
        let mut g = self.inner.lock().expect("rpc_client mutex poisoned");
        let session = g
            .session
            .as_mut()
            .ok_or_else(|| "session not available (shut down)".to_string())?;
        session.abort().map_err(|e| format!("cancel failed: {e}"))
    }

    /// True while a prompt run is active.
    pub fn prompt_in_flight(&self) -> bool {
        let g = self.inner.lock().expect("rpc_client mutex poisoned");
        g.session.as_ref().is_some_and(|s| s.run_active())
    }

    /// Non-blocking: returns the next queued event, or `None` if the queue is empty and no
    /// new step is immediately available. Uses `Duration::ZERO`.
    pub fn poll_event(&self) -> Option<AcpClientEvent> {
        self.wait_event(Duration::ZERO)
    }

    /// Block up to `timeout` for the next event. Returns `None` on timeout with no events.
    pub fn wait_event(&self, timeout: Duration) -> Option<AcpClientEvent> {
        let mut g = self.inner.lock().expect("rpc_client mutex poisoned");

        // Drain the remainder of a previous Events batch first.
        if let Some(ev) = g.event_queue.pop_front() {
            return Some(ev);
        }

        let session = g.session.as_mut()?;

        match session.next_step(timeout) {
            RpcStep::Events(evs) => {
                for ev in evs {
                    g.event_queue.push_back(ev);
                }
                g.event_queue.pop_front()
            }
            RpcStep::RunFinished { aborted } => {
                let id = g.active_request_id.take().unwrap_or(0);
                let stop_reason = if aborted { "cancelled" } else { "end_turn" };

                let on_run_finished = g.on_run_finished;
                let usage = match (on_run_finished, g.session.as_mut()) {
                    (Some(hook), Some(session)) => hook(session),
                    _ => None,
                };

                let finished = AcpClientEvent::RequestFinished {
                    id,
                    method: "session/prompt",
                    result: Ok(serde_json::json!({ "stopReason": stop_reason })),
                };

                match usage {
                    Some(usage_event) => {
                        g.event_queue.push_back(finished);
                        Some(usage_event)
                    }
                    None => Some(finished),
                }
            }
            RpcStep::PromptRejected(e) => {
                let _id = g.active_request_id.take();
                Some(AcpClientEvent::RequestFinished {
                    id: _id.unwrap_or(0),
                    method: "session/prompt",
                    result: Err(e),
                })
            }
            RpcStep::Error(t) => Some(AcpClientEvent::Error(t)),
            RpcStep::Exited => {
                g.host_exited = true;
                Some(AcpClientEvent::Exited)
            }
            RpcStep::Idle => None,
        }
    }

    /// Take the session out of its `Option`, dropping it (which closes stdin and reaps the
    /// child). Returns the session id on first call, `None` thereafter.
    pub fn shutdown(&self) -> Option<String> {
        let mut g = self.inner.lock().expect("rpc_client mutex poisoned");
        if let Some(session) = g.session.take() {
            let id = session.session_id().to_string();
            g.host_exited = true;
            Some(id)
        } else {
            None
        }
    }

    /// True once `Exited` was observed from the event stream or `shutdown` removed the session.
    pub fn host_exited(&self) -> bool {
        let g = self.inner.lock().expect("rpc_client mutex poisoned");
        g.host_exited || g.session.is_none()
    }

    pub fn with_session<R>(&self, f: impl FnOnce(&mut RpcSession) -> R) -> Result<R, String> {
        let mut g = self.inner.lock().expect("rpc_client mutex poisoned");
        let session = g
            .session
            .as_mut()
            .ok_or_else(|| "session not available (shut down)".to_string())?;
        Ok(f(session))
    }
}

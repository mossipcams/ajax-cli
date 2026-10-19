//! The transport-neutral client core shared by every RPC-style harness transport: it owns the session, the event queue and the prompt request ids.

use super::agent_event::{AgentEvent, NativeCommand, NativeEvent, NativeImage};
use super::rpc_session::{RpcSession, RpcStep};
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

pub type RunFinishedHook = fn(&mut RpcSession) -> Option<AgentEvent>;

pub struct RpcClientCore {
    inner: Mutex<CoreInner>,
    /// Slash commands advertised at handshake; handed out once, like an event.
    commands: Mutex<Option<Vec<NativeCommand>>>,
}

struct CoreInner {
    session: Option<RpcSession>,
    /// Remainder of a `RpcStep::Events` batch not yet consumed by callers.
    event_queue: VecDeque<AgentEvent>,
    /// The request id assigned by the most recent `begin_prompt`, cleared on finish.
    active_request_id: Option<u64>,
    /// Monotonically increasing counter for request ids (starts at 1).
    next_request_id: u64,
    /// Error text raised while a run was active (#1275), held back from the event
    /// stream. Folded into the run's `TurnFinished` as `Err` on a non-aborted finish,
    /// otherwise forwarded as a `NativeEvent::Error` so it is never dropped.
    held_run_error: Option<String>,
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
                held_run_error: None,
                host_exited: false,
                on_run_finished,
            }),
            commands: Mutex::new(None),
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

    /// Record the commands the harness advertised at handshake (handed out once).
    pub fn set_available_commands(&self, commands: Vec<NativeCommand>) {
        *self
            .commands
            .lock()
            .expect("rpc_client commands mutex poisoned") = Some(commands);
    }

    /// Slash commands advertised at handshake, returned by the first call only
    /// (one-shot, like an event) so every later drain yields an empty list.
    pub fn take_available_commands(&self) -> Vec<NativeCommand> {
        self.commands
            .lock()
            .expect("rpc_client commands mutex poisoned")
            .take()
            .unwrap_or_default()
    }

    /// Begin a prompt run from a plain-text message.
    ///
    /// Returns the request id for tracking.
    pub fn begin_prompt(&self, message: &str) -> Result<u64, String> {
        self.begin_prompt_with_images(message, &[])
    }

    /// Like [`Self::begin_prompt`], attaching images as `images:
    /// [{data, mimeType}]` when any are given.
    pub fn begin_prompt_with_images(
        &self,
        message: &str,
        images: &[NativeImage],
    ) -> Result<u64, String> {
        if message.is_empty() && images.is_empty() {
            return Err("empty prompt content: no blocks supplied".to_string());
        }

        let mut g = self.inner.lock().expect("rpc_client mutex poisoned");
        let session = g
            .session
            .as_mut()
            .ok_or_else(|| "session not available (shut down)".to_string())?;
        session
            .begin_prompt_with_images(message, images)
            .map_err(|e| format!("begin_prompt failed: {e}"))?;

        let id = g.next_request_id;
        g.next_request_id += 1;
        g.active_request_id = Some(id);

        Ok(id)
    }

    /// Begin a built-in command run (e.g. `compact`) in place of a prompt.
    ///
    /// Returns the request id for tracking.
    #[allow(dead_code)] // Called by the Pi client's /compact and /name paths (slice B of #1360).
    pub fn begin_command(&self, command: &str, fields: Value) -> Result<u64, String> {
        let mut g = self.inner.lock().expect("rpc_client mutex poisoned");
        let session = g
            .session
            .as_mut()
            .ok_or_else(|| "session not available (shut down)".to_string())?;
        session
            .begin_command(command, fields)
            .map_err(|e| format!("begin_command failed: {e}"))?;

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

    /// Write one response line carrying a caller-chosen `id` (e.g. Pi's
    /// `extension_ui_response`, which must echo the id of its
    /// `extension_ui_request`). No auto-generated id is allocated.
    // Issue #1306 part 2b consumes this; the stack needs it before any caller exists.
    #[allow(dead_code)]
    pub fn respond(
        &self,
        command_type: &str,
        id: &str,
        fields: serde_json::Value,
    ) -> Result<(), String> {
        self.with_session(|s| s.respond(command_type, id, fields))
            .and_then(Result::<_, String>::into)
    }

    /// True while a prompt run is active.
    pub fn prompt_in_flight(&self) -> bool {
        let g = self.inner.lock().expect("rpc_client mutex poisoned");
        g.session.as_ref().is_some_and(|s| s.run_active())
    }

    /// Non-blocking: returns the next queued event, or `None` if the queue is empty and no
    /// new step is immediately available. Uses `Duration::ZERO`.
    pub fn poll_event(&self) -> Option<AgentEvent> {
        self.wait_event(Duration::ZERO)
    }

    /// Block up to `timeout` for the next event. Returns `None` on timeout with no events.
    ///
    /// A [`NativeEvent::Error`] raised while a prompt run is active is held back (#1275)
    /// and folded into that run's `TurnFinished` as `Err`, so the prompt settles as
    /// failed and the error is shown once. An aborted run, or one that ends another way,
    /// forwards the held error as a plain event instead.
    pub fn wait_event(&self, timeout: Duration) -> Option<AgentEvent> {
        let mut guard = self.inner.lock().expect("rpc_client mutex poisoned");
        let g = &mut *guard;

        // Drain the remainder of a previous Events batch first.
        if let Some(ev) = g.event_queue.pop_front() {
            return Some(ev);
        }

        loop {
            let session = g.session.as_mut()?;

            match session.next_step(timeout) {
                RpcStep::Events(evs) => {
                    for ev in evs {
                        match ev {
                            AgentEvent::Native(NativeEvent::Error(text))
                                if g.active_request_id.is_some() =>
                            {
                                g.held_run_error = Some(match g.held_run_error.take() {
                                    Some(earlier) => format!("{earlier}\n{text}"),
                                    None => text,
                                });
                            }
                            other => g.event_queue.push_back(other),
                        }
                    }
                    if let Some(ev) = g.event_queue.pop_front() {
                        return Some(ev);
                    }
                    // The whole batch was a held-back error: keep waiting for the run's outcome.
                }
                RpcStep::RunFinished { aborted } => {
                    let id = g.active_request_id.take().unwrap_or(0);
                    let (forwarded, result) = match (aborted, g.held_run_error.take()) {
                        (false, Some(text)) => (None, Err(text)),
                        (true, Some(text)) => (Some(text), Ok("cancelled".to_string())),
                        (false, None) => (None, Ok("end_turn".to_string())),
                        (true, None) => (None, Ok("cancelled".to_string())),
                    };

                    let on_run_finished = g.on_run_finished;
                    let usage = match (on_run_finished, g.session.as_mut()) {
                        (Some(hook), Some(session)) => hook(session),
                        _ => None,
                    };

                    let finished = AgentEvent::Native(NativeEvent::TurnFinished {
                        request_id: id,
                        result,
                    });

                    g.event_queue
                        .extend(forwarded.map(|text| AgentEvent::Native(NativeEvent::Error(text))));
                    g.event_queue.extend(usage);
                    g.event_queue.push_back(finished);
                    return g.event_queue.pop_front();
                }
                RpcStep::PromptRejected(e) => {
                    let id = g.active_request_id.take();
                    return forward_held_error(
                        g,
                        AgentEvent::Native(NativeEvent::TurnFinished {
                            request_id: id.unwrap_or(0),
                            result: Err(e),
                        }),
                    );
                }
                RpcStep::Error(t) => {
                    return forward_held_error(g, AgentEvent::Native(NativeEvent::Error(t)));
                }
                RpcStep::Exited => {
                    g.host_exited = true;
                    return forward_held_error(g, AgentEvent::Native(NativeEvent::Exited));
                }
                RpcStep::Idle => return None,
            }
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

    /// OS process id of the live child while a session exists; `None` after shutdown.
    pub fn child_pid(&self) -> Option<u32> {
        let g = self.inner.lock().expect("rpc_client mutex poisoned");
        g.session.as_ref().map(|session| session.child_pid())
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

/// Queue the error held for a run that is ending without a failed `TurnFinished` ahead of
/// `event`, so it is forwarded rather than dropped (#1275), and return the next event.
fn forward_held_error(g: &mut CoreInner, event: AgentEvent) -> Option<AgentEvent> {
    if let Some(text) = g.held_run_error.take() {
        g.event_queue
            .push_back(AgentEvent::Native(NativeEvent::Error(text)));
    }
    g.event_queue.push_back(event);
    g.event_queue.pop_front()
}

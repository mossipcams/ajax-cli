use super::agent_event::{AgentEvent, NativeCommand, NativeEvent, NativeImage};
use super::rpc_session::{RpcSession, RpcStep};
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

pub type RunFinishedHook = fn(&mut RpcSession) -> Option<AgentEvent>;

pub struct RpcClientCore {
    inner: Mutex<CoreInner>,
    commands: Mutex<Option<Vec<NativeCommand>>>,
}

struct CoreInner {
    session: Option<RpcSession>,
    event_queue: VecDeque<AgentEvent>,
    active_request_id: Option<u64>,
    next_request_id: u64,
    held_run_error: Option<String>,
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

    #[allow(dead_code)]
    pub(crate) fn adopt(
        session: RpcSession,
        active_request_id: u64,
        on_run_finished: Option<RunFinishedHook>,
    ) -> Self {
        let core = Self::new(session, on_run_finished);
        {
            let mut g = core.inner.lock().expect("rpc_client mutex poisoned");
            g.active_request_id = Some(active_request_id);
            g.next_request_id = active_request_id + 1;
        }
        core
    }

    pub fn session_id(&self) -> String {
        let g = self.inner.lock().expect("rpc_client mutex poisoned");
        g.session
            .as_ref()
            .map(|s| s.session_id().to_string())
            .unwrap_or_default()
    }

    pub fn set_available_commands(&self, commands: Vec<NativeCommand>) {
        *self
            .commands
            .lock()
            .expect("rpc_client commands mutex poisoned") = Some(commands);
    }

    pub fn take_available_commands(&self) -> Vec<NativeCommand> {
        self.commands
            .lock()
            .expect("rpc_client commands mutex poisoned")
            .take()
            .unwrap_or_default()
    }

    pub fn begin_prompt(&self, message: &str) -> Result<u64, String> {
        self.begin_prompt_with_images(message, &[])
    }

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

    #[allow(dead_code)]
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

    pub fn cancel(&self) -> Result<(), String> {
        let mut g = self.inner.lock().expect("rpc_client mutex poisoned");
        let session = g
            .session
            .as_mut()
            .ok_or_else(|| "session not available (shut down)".to_string())?;
        session.abort().map_err(|e| format!("cancel failed: {e}"))
    }

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

    pub fn prompt_in_flight(&self) -> bool {
        let g = self.inner.lock().expect("rpc_client mutex poisoned");
        g.session.as_ref().is_some_and(|s| s.run_active())
    }

    pub fn poll_event(&self) -> Option<AgentEvent> {
        self.wait_event(Duration::ZERO)
    }

    pub fn wait_event(&self, timeout: Duration) -> Option<AgentEvent> {
        let mut guard = self.inner.lock().expect("rpc_client mutex poisoned");
        let g = &mut *guard;

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

    pub fn host_exited(&self) -> bool {
        let g = self.inner.lock().expect("rpc_client mutex poisoned");
        g.host_exited || g.session.is_none()
    }

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

fn forward_held_error(g: &mut CoreInner, event: AgentEvent) -> Option<AgentEvent> {
    if let Some(text) = g.held_run_error.take() {
        g.event_queue
            .push_back(AgentEvent::Native(NativeEvent::Error(text)));
    }
    g.event_queue.push_back(event);
    g.event_queue.pop_front()
}

#[cfg(test)]
mod tests {
    use std::io::{self, Write};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use serde_json::json;

    use super::super::agent_event::{AgentEvent, NativeEvent};
    use super::super::claude_sdk_map::map_sdk_message;
    use super::super::jsonl_process::JsonlProcess;
    use super::super::rpc_session::RpcSession;
    use super::RpcClientCore;

    #[derive(Clone, Default)]
    struct CapturedWriter(Arc<Mutex<Vec<u8>>>);

    impl CapturedWriter {
        fn bytes(&self) -> Vec<u8> {
            self.0.lock().expect("capture lock").clone()
        }
    }

    impl Write for CapturedWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().expect("capture lock").extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn adopted_claude_client(
        request_id: u64,
    ) -> (std::io::PipeWriter, CapturedWriter, RpcClientCore) {
        let (reader, peer_stdout) = std::io::pipe().expect("pipe");
        let writer = CapturedWriter::default();
        let process =
            JsonlProcess::attach(Box::new(reader), Box::new(writer.clone()), 4242).expect("attach");
        let session = RpcSession::adopt(map_sdk_message, process, "claude-session-1".to_owned());
        let core = RpcClientCore::adopt(session, request_id, None);
        (peer_stdout, writer, core)
    }

    fn events_until_turn_finished(core: &RpcClientCore) -> Vec<AgentEvent> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut events = Vec::new();
        while Instant::now() < deadline {
            let Some(event) = core.wait_event(Duration::from_millis(50)) else {
                continue;
            };
            let finished = matches!(event, AgentEvent::Native(NativeEvent::TurnFinished { .. }));
            events.push(event);
            if finished {
                return events;
            }
        }
        panic!("turn did not finish; events: {events:?}");
    }

    #[test]
    fn adopted_claude_client_finishes_the_turn_with_the_given_request_id() {
        let (mut peer, writer, core) = adopted_claude_client(7);
        assert!(core.prompt_in_flight(), "adopted run must be mid-turn");

        let assistant = json!({
            "type": "assistant",
            "message": {"model": "<synthetic>", "role": "assistant",
                "content": [{"type": "text", "text": "hi"}]}
        });
        let result = json!({"type": "result", "subtype": "success", "is_error": false});
        peer.write_all(format!("{assistant}\n{result}\n").as_bytes())
            .expect("write records to the adopted shim");

        let events = events_until_turn_finished(&core);
        assert!(
            events.iter().any(|event| matches!(
                event,
                AgentEvent::Native(NativeEvent::AgentText(text)) if text == "hi"
            )),
            "events: {events:?}"
        );
        assert!(
            matches!(
                events.last(),
                Some(AgentEvent::Native(NativeEvent::TurnFinished {
                    request_id: 7,
                    result: Ok(reason),
                })) if reason == "end_turn"
            ),
            "events: {events:?}"
        );
        assert!(!core.prompt_in_flight());
        assert!(
            writer.bytes().is_empty(),
            "adopt must never write a prompt or handshake record"
        );
    }

    #[test]
    fn dropping_an_attached_client_sends_shutdown_and_leaves_the_peer_open() {
        let (mut peer, writer, core) = adopted_claude_client(3);
        assert_eq!(core.child_pid(), Some(4242));
        drop(core);

        assert_eq!(writer.bytes(), b"SHUTDOWN\n");
        peer.write_all(b"{}\n")
            .expect("the shim side must stay open after an attached drop");
    }
}

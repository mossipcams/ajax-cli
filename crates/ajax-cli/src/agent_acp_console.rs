use std::io::{self, BufRead, Write};

use agent_client_protocol::schema::{v1, v2};
use base64::{engine::general_purpose::STANDARD, Engine};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::CliError;

pub const ELICITATION_VALIDATION_ERROR: &str = "Expected a JSON object with no fields.";
pub const AUTHENTICATION_VALIDATION_ERROR: &str = "Enter a number for an authentication method.";

pub enum ConsoleEvent {
    PromptLine(String),
    Interrupt,
    InputError(String),
}

pub struct AgentAcpConsole<W> {
    events: UnboundedReceiver<ConsoleEvent>,
    output: W,
}

impl<W: Write> AgentAcpConsole<W> {
    pub fn new(events: UnboundedReceiver<ConsoleEvent>, output: W) -> Self {
        Self { events, output }
    }

    pub async fn next_event(&mut self) -> Option<ConsoleEvent> {
        self.events.recv().await
    }

    pub fn render_update(&mut self, update: &v2::SessionUpdate) -> Result<(), CliError> {
        match update {
            v2::SessionUpdate::AgentMessageChunk(chunk) => {
                if let v2::ContentBlock::Text(text) = &chunk.content {
                    self.output
                        .write_all(text.text.as_bytes())
                        .map_err(console_output_error)?;
                    self.output.flush().map_err(console_output_error)?;
                }
            }
            v2::SessionUpdate::TerminalOutputChunk(chunk) => {
                let bytes = STANDARD.decode(&chunk.data).map_err(|error| {
                    CliError::CommandFailed(format!("ACP terminal output decode failed: {error}"))
                })?;
                self.output
                    .write_all(&bytes)
                    .map_err(console_output_error)?;
                self.output.flush().map_err(console_output_error)?;
            }
            _ => {}
        }
        Ok(())
    }

    pub fn render_update_v1(&mut self, update: &v1::SessionUpdate) -> Result<(), CliError> {
        match update {
            v1::SessionUpdate::AgentMessageChunk(chunk) => {
                if let v1::ContentBlock::Text(text) = &chunk.content {
                    self.output
                        .write_all(text.text.as_bytes())
                        .map_err(console_output_error)?;
                    self.output.flush().map_err(console_output_error)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub fn render_permission_prompt_v1(
        &mut self,
        request: &v1::RequestPermissionRequest,
    ) -> Result<(), CliError> {
        let title = request
            .tool_call
            .fields
            .title
            .as_deref()
            .filter(|title| !title.is_empty())
            .unwrap_or("Permission required");
        self.output
            .write_all(title.as_bytes())
            .map_err(console_output_error)?;
        self.output.write_all(b"\n").map_err(console_output_error)?;
        for (index, option) in request.options.iter().enumerate() {
            self.output
                .write_all(format!("{}. {}\n", index + 1, option.name).as_bytes())
                .map_err(console_output_error)?;
        }
        self.output.flush().map_err(console_output_error)?;
        Ok(())
    }

    pub fn render_permission_prompt(
        &mut self,
        request: &v2::RequestPermissionRequest,
    ) -> Result<(), CliError> {
        self.output
            .write_all(request.title.as_bytes())
            .map_err(console_output_error)?;
        self.output.write_all(b"\n").map_err(console_output_error)?;
        if let Some(description) = &request.description {
            self.output
                .write_all(description.as_bytes())
                .map_err(console_output_error)?;
            self.output.write_all(b"\n").map_err(console_output_error)?;
        }
        for (index, option) in request.options.iter().enumerate() {
            self.output
                .write_all(format!("{}. {}\n", index + 1, option.name).as_bytes())
                .map_err(console_output_error)?;
        }
        self.output.flush().map_err(console_output_error)?;
        Ok(())
    }

    pub fn render_elicitation_prompt(
        &mut self,
        message: &str,
        fields: &[(String, &str)],
    ) -> Result<(), CliError> {
        self.output
            .write_all(message.as_bytes())
            .map_err(console_output_error)?;
        self.output.write_all(b"\n").map_err(console_output_error)?;
        for (name, type_label) in fields {
            self.output
                .write_all(format!("{name}: {type_label}\n").as_bytes())
                .map_err(console_output_error)?;
        }
        self.output.flush().map_err(console_output_error)?;
        Ok(())
    }

    pub fn render_elicitation_validation_error(&mut self, detail: &str) -> Result<(), CliError> {
        self.output
            .write_all(detail.as_bytes())
            .map_err(console_output_error)?;
        self.output.write_all(b"\n").map_err(console_output_error)?;
        self.output.flush().map_err(console_output_error)?;
        Ok(())
    }

    pub fn render_authentication_prompt(&mut self, method_names: &[&str]) -> Result<(), CliError> {
        for (index, name) in method_names.iter().enumerate() {
            self.output
                .write_all(format!("{}. {}\n", index + 1, name).as_bytes())
                .map_err(console_output_error)?;
        }
        self.output.flush().map_err(console_output_error)?;
        Ok(())
    }

    pub fn render_authentication_validation_error(&mut self) -> Result<(), CliError> {
        self.output
            .write_all(AUTHENTICATION_VALIDATION_ERROR.as_bytes())
            .map_err(console_output_error)?;
        self.output.write_all(b"\n").map_err(console_output_error)?;
        self.output.flush().map_err(console_output_error)?;
        Ok(())
    }
}

#[allow(dead_code)]
impl AgentAcpConsole<io::Sink> {
    pub fn closed() -> Self {
        let (_tx, events) = tokio::sync::mpsc::unbounded_channel();
        Self::new(events, io::sink())
    }
}

impl AgentAcpConsole<io::Stdout> {
    pub fn spawn_stdio() -> Self {
        let (events_tx, events_rx) = tokio::sync::mpsc::unbounded_channel();
        let stdin_tx = events_tx.clone();
        std::thread::spawn(move || {
            let stdin = io::stdin();
            let mut lines = stdin.lock().lines();
            loop {
                match lines.next() {
                    None => break,
                    Some(Ok(line)) => {
                        if stdin_tx.send(ConsoleEvent::PromptLine(line)).is_err() {
                            break;
                        }
                    }
                    Some(Err(error)) => {
                        let _ = stdin_tx.send(ConsoleEvent::InputError(error.to_string()));
                        break;
                    }
                }
            }
        });
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                let _ = events_tx.send(ConsoleEvent::Interrupt);
            }
        });
        Self::new(events_rx, io::stdout())
    }
}

fn console_output_error(error: io::Error) -> CliError {
    CliError::CommandFailed(format!("ACP console output failed: {error}"))
}

#[cfg(test)]
mod tests {
    use std::{
        io::{self, Write},
        path::{Path, PathBuf},
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc, Mutex,
        },
        time::Duration,
    };

    use agent_client_protocol::{
        schema::{v2, ProtocolVersion},
        Agent, Channel, Client, ConnectTo, ConnectionTo, Error,
    };
    use ajax_core::acp_status::{AcpActionKind, AcpSessionState, AcpStopReason};
    use base64::Engine;

    use super::{AgentAcpConsole, ConsoleEvent};
    use crate::{
        agent_acp::run_session_lifecycle,
        agent_acp_snapshot::{read_snapshot, AcpRuntimeSnapshot},
    };

    static CONSOLE_TEST_SUFFIX: AtomicU64 = AtomicU64::new(0);

    struct ConsoleTempRoot(PathBuf);

    impl ConsoleTempRoot {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "ajax-acp-console-{}-{}",
                std::process::id(),
                CONSOLE_TEST_SUFFIX.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).expect("create console test root");
            Self(path)
        }
    }

    impl Drop for ConsoleTempRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[derive(Clone, Default)]
    struct SharedWriter(Arc<Mutex<Vec<u8>>>);

    impl Write for SharedWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().write(buf)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.0.lock().unwrap().flush()
        }
    }

    fn unix_millis() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    }

    fn console_snapshot(state_root: &Path, task_id: &str) -> AcpRuntimeSnapshot {
        read_snapshot(state_root, task_id, unix_millis())
            .unwrap()
            .unwrap()
    }

    fn idle_cancelled() -> v2::StateUpdate {
        v2::StateUpdate::Idle(v2::IdleStateUpdate::new().stop_reason(v2::StopReason::Cancelled))
    }

    async fn wait_for_snapshot<F>(state_root: &Path, task_id: &str, mut matches: F)
    where
        F: FnMut(&AcpRuntimeSnapshot) -> bool,
    {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(snapshot) = read_snapshot(state_root, task_id, unix_millis())
                    .ok()
                    .flatten()
                {
                    if matches(&snapshot) {
                        break;
                    }
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("expected snapshot was not published");
    }

    fn console_test_agent(
        session_id: String,
        agent_message: String,
        terminal_b64: String,
        prompts: Arc<Mutex<Vec<v2::PromptRequest>>>,
        cancels: Arc<Mutex<Vec<String>>>,
        close_after_cancel: tokio::sync::oneshot::Sender<()>,
        hold_prompt_response: Option<tokio::sync::oneshot::Receiver<()>>,
    ) -> impl agent_client_protocol::ConnectTo<Client> + 'static {
        let new_session_id = session_id.clone();
        let prompt_session_id = session_id.clone();
        let output_session_id = session_id.clone();
        let cancel_session_id = session_id.clone();
        let idle_session_id = session_id;
        let close_after_cancel = Arc::new(Mutex::new(Some(close_after_cancel)));
        let prompt_gate = Arc::new(Mutex::new(hold_prompt_response));

        Agent
            .v2()
            .on_receive_request(
                async |request: v2::InitializeRequest, responder, _cx| {
                    assert_eq!(request.protocol_version, ProtocolVersion::V2);
                    responder.respond(v2::InitializeResponse::new(
                        ProtocolVersion::V2,
                        v2::Implementation::new("console-test-agent", "1"),
                    ))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |_request: v2::NewSessionRequest,
                            responder: agent_client_protocol::Responder<v2::NewSessionResponse>,
                            _cx| {
                    responder.respond(v2::NewSessionResponse::new(new_session_id.clone()))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |request: v2::PromptRequest,
                            responder: agent_client_protocol::Responder<v2::PromptResponse>,
                            cx: ConnectionTo<Client>| {
                    prompts.lock().unwrap().push(request.clone());
                    cx.send_notification(v2::UpdateSessionNotification::new(
                        output_session_id.clone(),
                        v2::SessionUpdate::AgentMessageChunk(v2::ContentChunk::new(
                            v2::ContentBlock::Text(v2::TextContent::new(agent_message.clone())),
                            "msg-agent",
                        )),
                    ))?;
                    cx.send_notification(v2::UpdateSessionNotification::new(
                        output_session_id.clone(),
                        v2::SessionUpdate::TerminalOutputChunk(v2::TerminalOutputChunk::new(
                            "term-1",
                            terminal_b64.clone(),
                        )),
                    ))?;
                    assert_eq!(request.session_id.0.as_ref(), prompt_session_id.as_str());
                    let release = prompt_gate.lock().unwrap().take();
                    if let Some(release) = release {
                        let _ = release.await;
                    }
                    responder.respond(v2::PromptResponse::new())?;
                    Ok(())
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_notification(
                async move |notification: v2::CancelSessionNotification,
                            cx: ConnectionTo<Client>| {
                    cancels
                        .lock()
                        .unwrap()
                        .push(notification.session_id.0.to_string());
                    assert_eq!(
                        notification.session_id.0.as_ref(),
                        cancel_session_id.as_str()
                    );
                    cx.send_notification(v2::UpdateSessionNotification::new(
                        idle_session_id.clone(),
                        v2::SessionUpdate::StateUpdate(idle_cancelled()),
                    ))?;
                    if let Some(sender) = close_after_cancel.lock().unwrap().take() {
                        let _ = sender.send(());
                    }
                    Ok(())
                },
                agent_client_protocol::on_receive_notification!(),
            )
    }

    #[tokio::test(flavor = "current_thread")]
    async fn prompt_output_cancel_routes_terminal_io_and_idle_before_eof() {
        let root = ConsoleTempRoot::new();
        let task_id = "repo/console";
        let cwd = root.0.join("console-worktree");
        let session_id = "console-session";
        let prompt_line = "ship it now";
        let agent_message = "agent says hi";
        let terminal_bytes = b"term bytes\n";
        let terminal_b64 = base64::engine::general_purpose::STANDARD.encode(terminal_bytes);

        let prompts = Arc::new(Mutex::new(Vec::<v2::PromptRequest>::new()));
        let cancels = Arc::new(Mutex::new(Vec::<String>::new()));
        let (close_agent_tx, close_agent_rx) = tokio::sync::oneshot::channel::<()>();

        let agent = console_test_agent(
            session_id.to_owned(),
            agent_message.to_owned(),
            terminal_b64,
            Arc::clone(&prompts),
            Arc::clone(&cancels),
            close_agent_tx,
            None,
        );

        let (client_transport, agent_transport) = Channel::duplex();
        let agent_task = tokio::spawn(agent.connect_to(agent_transport));

        let (events_tx, events_rx) = tokio::sync::mpsc::unbounded_channel();
        let writer = SharedWriter::default();
        let output = writer.clone();
        let console = AgentAcpConsole::new(events_rx, writer);
        let lifecycle_root = root.0.clone();
        let lifecycle_cwd = cwd.clone();
        let lifecycle_task = tokio::spawn(async move {
            run_session_lifecycle(
                client_transport,
                task_id,
                &lifecycle_root,
                &lifecycle_cwd,
                console,
            )
            .await
        });

        events_tx
            .send(ConsoleEvent::PromptLine(prompt_line.to_owned()))
            .expect("prompt event");
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                {
                    let recorded = prompts.lock().unwrap();
                    if recorded.len() == 1 {
                        let prompt = &recorded[0];
                        assert_eq!(prompt.session_id.0.as_ref(), session_id);
                        assert_eq!(prompt.prompt.len(), 1);
                        assert_eq!(
                            prompt.prompt[0],
                            v2::ContentBlock::Text(v2::TextContent::new(prompt_line))
                        );
                        break;
                    }
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("prompt was not sent");

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let bytes = output.0.lock().unwrap().clone();
                let expected = [agent_message.as_bytes(), terminal_bytes.as_slice()].concat();
                if bytes == expected {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("console output was not rendered in order");

        events_tx
            .send(ConsoleEvent::Interrupt)
            .expect("interrupt event");
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if !cancels.lock().unwrap().is_empty() {
                    assert_eq!(cancels.lock().unwrap()[0], session_id);
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("cancel was not sent");

        let _ = close_agent_rx.await;
        agent_task.abort();
        let _ = agent_task.await;

        let result = tokio::time::timeout(Duration::from_secs(2), lifecycle_task)
            .await
            .expect("lifecycle did not stop after EOF")
            .expect("lifecycle task panicked");
        result.expect("idle/cancelled EOF should be graceful");

        let snapshot = console_snapshot(&root.0, task_id);
        assert_eq!(snapshot.session_id.as_deref(), Some(session_id));
        assert_eq!(
            snapshot.observation.state,
            AcpSessionState::Idle(Some(AcpStopReason::Cancelled))
        );
    }

    struct FailingWriter;

    impl Write for FailingWriter {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("console output failed"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn prompt_output_cancel_renders_while_prompt_response_pending() {
        let root = ConsoleTempRoot::new();
        let task_id = "repo/console-pending";
        let cwd = root.0.join("console-pending-worktree");
        let session_id = "console-pending-session";
        let agent_message = "rendered before ack";
        let terminal_bytes = b"pending bytes\n";
        let terminal_b64 = base64::engine::general_purpose::STANDARD.encode(terminal_bytes);

        let prompts = Arc::new(Mutex::new(Vec::<v2::PromptRequest>::new()));
        let cancels = Arc::new(Mutex::new(Vec::<String>::new()));
        let (release_prompt_tx, release_prompt_rx) = tokio::sync::oneshot::channel::<()>();
        let (close_agent_tx, close_agent_rx) = tokio::sync::oneshot::channel::<()>();

        let agent = console_test_agent(
            session_id.to_owned(),
            agent_message.to_owned(),
            terminal_b64,
            Arc::clone(&prompts),
            Arc::clone(&cancels),
            close_agent_tx,
            Some(release_prompt_rx),
        );

        let (client_transport, agent_transport) = Channel::duplex();
        let agent_task = tokio::spawn(agent.connect_to(agent_transport));

        let (events_tx, events_rx) = tokio::sync::mpsc::unbounded_channel();
        let output = SharedWriter::default();
        let writer = output.clone();
        let console = AgentAcpConsole::new(events_rx, output);
        let lifecycle_root = root.0.clone();
        let lifecycle_cwd = cwd.clone();
        let lifecycle_task = tokio::spawn(async move {
            run_session_lifecycle(
                client_transport,
                task_id,
                &lifecycle_root,
                &lifecycle_cwd,
                console,
            )
            .await
        });

        events_tx
            .send(ConsoleEvent::PromptLine("hold response".to_owned()))
            .expect("prompt event");

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let bytes = writer.0.lock().unwrap().clone();
                let expected = [agent_message.as_bytes(), terminal_bytes.as_slice()].concat();
                if bytes == expected {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("output must render before prompt response");

        let _ = release_prompt_tx.send(());
        events_tx
            .send(ConsoleEvent::Interrupt)
            .expect("interrupt event");

        let _ = close_agent_rx.await;
        agent_task.abort();
        let _ = agent_task.await;

        let result = tokio::time::timeout(Duration::from_secs(2), lifecycle_task)
            .await
            .expect("lifecycle did not stop")
            .expect("lifecycle task panicked");
        result.expect("pending prompt lifecycle should succeed");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn prompt_output_cancel_publishes_failed_snapshot_on_console_output_error() {
        let root = ConsoleTempRoot::new();
        let task_id = "repo/console-fail";
        let cwd = root.0.join("console-fail-worktree");
        let session_id = "console-fail-session";

        let prompts = Arc::new(Mutex::new(Vec::<v2::PromptRequest>::new()));
        let cancels = Arc::new(Mutex::new(Vec::<String>::new()));
        let (close_agent_tx, _close_agent_rx) = tokio::sync::oneshot::channel::<()>();

        let agent = console_test_agent(
            session_id.to_owned(),
            "will not render".to_owned(),
            base64::engine::general_purpose::STANDARD.encode(b"unused"),
            Arc::clone(&prompts),
            Arc::clone(&cancels),
            close_agent_tx,
            None,
        );

        let (client_transport, agent_transport) = Channel::duplex();
        let _agent_task = tokio::spawn(agent.connect_to(agent_transport));

        let (events_tx, events_rx) = tokio::sync::mpsc::unbounded_channel();
        let console = AgentAcpConsole::new(events_rx, FailingWriter);
        let lifecycle_root = root.0.clone();
        let lifecycle_cwd = cwd.clone();
        let lifecycle_task = tokio::spawn(async move {
            run_session_lifecycle(
                client_transport,
                task_id,
                &lifecycle_root,
                &lifecycle_cwd,
                console,
            )
            .await
        });

        events_tx
            .send(ConsoleEvent::PromptLine("trigger output".to_owned()))
            .expect("prompt event");

        let result = tokio::time::timeout(Duration::from_secs(2), lifecycle_task)
            .await
            .expect("lifecycle did not stop")
            .expect("lifecycle task panicked");

        let error = result.expect_err("console output failure should fail lifecycle");
        assert!(
            error.to_string().contains("ACP console output failed"),
            "{error}"
        );

        let snapshot = console_snapshot(&root.0, task_id);
        assert_eq!(snapshot.session_id.as_deref(), Some(session_id));
        assert_eq!(snapshot.observation.state, AcpSessionState::Failed);
        assert_eq!(
            snapshot.observation.detail.as_deref(),
            Some("ACP console output failed: console output failed")
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn prompt_output_cancel_publishes_failed_snapshot_on_console_input_error() {
        let root = ConsoleTempRoot::new();
        let task_id = "repo/console-input-fail";
        let cwd = root.0.join("console-input-fail-worktree");
        let session_id = "console-input-fail-session";

        let prompts = Arc::new(Mutex::new(Vec::<v2::PromptRequest>::new()));
        let cancels = Arc::new(Mutex::new(Vec::<String>::new()));
        let (close_agent_tx, close_agent_rx) = tokio::sync::oneshot::channel::<()>();

        let agent = console_test_agent(
            session_id.to_owned(),
            "unused".to_owned(),
            base64::engine::general_purpose::STANDARD.encode(b"unused"),
            Arc::clone(&prompts),
            Arc::clone(&cancels),
            close_agent_tx,
            None,
        );

        let (client_transport, agent_transport) = Channel::duplex();
        let agent_task = tokio::spawn(agent.connect_to(agent_transport));

        let (events_tx, events_rx) = tokio::sync::mpsc::unbounded_channel();
        let console = AgentAcpConsole::new(events_rx, io::sink());
        let lifecycle_root = root.0.clone();
        let lifecycle_cwd = cwd.clone();
        let lifecycle_task = tokio::spawn(async move {
            run_session_lifecycle(
                client_transport,
                task_id,
                &lifecycle_root,
                &lifecycle_cwd,
                console,
            )
            .await
        });

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if read_snapshot(&root.0, task_id, unix_millis())
                    .ok()
                    .flatten()
                    .is_some()
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("session snapshot was not published");

        events_tx
            .send(ConsoleEvent::InputError("stdin read failed".to_owned()))
            .expect("input error event");

        let result = tokio::time::timeout(Duration::from_secs(2), lifecycle_task)
            .await
            .expect("lifecycle did not stop")
            .expect("lifecycle task panicked");
        let error = result.expect_err("console input failure should fail lifecycle");
        assert!(
            error.to_string().contains("ACP console input failed"),
            "{error}"
        );

        let snapshot = console_snapshot(&root.0, task_id);
        assert_eq!(snapshot.session_id.as_deref(), Some(session_id));
        assert_eq!(snapshot.observation.state, AcpSessionState::Failed);
        assert_eq!(
            snapshot.observation.detail.as_deref(),
            Some("ACP console input failed: stdin read failed")
        );

        let _ = close_agent_rx.await;
        agent_task.abort();
        let _ = agent_task.await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn requests_permission_maps_exact_id_and_cancels_unknown() {
        let root = ConsoleTempRoot::new();
        let task_id = "repo/permission";
        let cwd = root.0.join("permission-worktree");
        let session_id = "permission-session";
        let first_title = "Allow file edit?";
        let first_description = "The agent wants to edit src/main.rs";
        let first_label_a = "Allow once";
        let first_label_b = "Deny";
        let first_opaque_a = "opaque-perm-id-alpha";
        let first_opaque_b = "opaque-perm-id-beta";
        let second_title = "Install dependency?";
        let second_label = "Approve";
        let second_opaque = "opaque-perm-id-gamma";

        let (running_tx, running_rx) = tokio::sync::oneshot::channel::<()>();
        let (responses_tx, mut responses_rx) =
            tokio::sync::mpsc::unbounded_channel::<v2::RequestPermissionResponse>();
        let (client_transport, agent_transport) = Channel::duplex();
        let agent_session_id = session_id.to_owned();
        let agent = Agent
            .v2()
            .on_receive_request(
                async |request: v2::InitializeRequest, responder, _cx| {
                    assert_eq!(request.protocol_version, ProtocolVersion::V2);
                    responder.respond(v2::InitializeResponse::new(
                        ProtocolVersion::V2,
                        v2::Implementation::new("permission-test-agent", "1"),
                    ))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |_request: v2::NewSessionRequest,
                            responder: agent_client_protocol::Responder<v2::NewSessionResponse>,
                            _cx| {
                    responder.respond(v2::NewSessionResponse::new(agent_session_id.clone()))
                },
                agent_client_protocol::on_receive_request!(),
            );
        let agent_task = tokio::spawn(async move {
            let session_id = session_id.to_owned();
            let permission_responses_tx = responses_tx;
            agent
                .connect_with(
                    agent_transport,
                    async move |connection: ConnectionTo<Client>| {
                        let _ = running_rx.await;

                        let first = connection
                            .send_request(
                                v2::RequestPermissionRequest::new(
                                    session_id.clone(),
                                    first_title,
                                    vec![
                                        v2::PermissionOption::new(
                                            first_opaque_a,
                                            first_label_a,
                                            v2::PermissionOptionKind::AllowOnce,
                                        ),
                                        v2::PermissionOption::new(
                                            first_opaque_b,
                                            first_label_b,
                                            v2::PermissionOptionKind::RejectOnce,
                                        ),
                                    ],
                                )
                                .description(first_description),
                            )
                            .block_task()
                            .await?;
                        permission_responses_tx
                            .send(first)
                            .expect("first permission response channel open");

                        let second = connection
                            .send_request(v2::RequestPermissionRequest::new(
                                session_id.clone(),
                                second_title,
                                vec![v2::PermissionOption::new(
                                    second_opaque,
                                    second_label,
                                    v2::PermissionOptionKind::AllowOnce,
                                )],
                            ))
                            .block_task()
                            .await?;
                        permission_responses_tx
                            .send(second)
                            .expect("second permission response channel open");

                        connection.send_notification(v2::UpdateSessionNotification::new(
                            session_id,
                            v2::SessionUpdate::StateUpdate(idle_cancelled()),
                        ))?;
                        tokio::task::yield_now().await;
                        Ok(())
                    },
                )
                .await
        });

        let (events_tx, events_rx) = tokio::sync::mpsc::unbounded_channel();
        let writer = SharedWriter::default();
        let output = writer.clone();
        let console = AgentAcpConsole::new(events_rx, writer);
        let lifecycle_root = root.0.clone();
        let lifecycle_cwd = cwd.clone();
        let lifecycle_task = tokio::spawn(async move {
            run_session_lifecycle(
                client_transport,
                task_id,
                &lifecycle_root,
                &lifecycle_cwd,
                console,
            )
            .await
        });

        wait_for_snapshot(&root.0, task_id, |snapshot| {
            matches!(snapshot.observation.state, AcpSessionState::Running)
        })
        .await;
        let _ = running_tx.send(());

        wait_for_snapshot(&root.0, task_id, |snapshot| {
            matches!(
                snapshot.observation.state,
                AcpSessionState::RequiresAction(AcpActionKind::Permission)
            ) && snapshot
                .observation
                .detail
                .as_deref()
                .is_some_and(|detail| {
                    detail.contains(first_title)
                        && detail.contains(first_label_a)
                        && detail.contains(first_label_b)
                })
        })
        .await;

        let assert_no_opaque_ids = |output: &str| {
            for opaque in [first_opaque_a, first_opaque_b, second_opaque] {
                assert!(
                    !output.contains(opaque),
                    "terminal output leaked opaque id: {opaque}"
                );
            }
        };

        let rendered = String::from_utf8(output.0.lock().unwrap().clone()).expect("utf8 output");
        assert!(rendered.contains(first_title));
        assert!(rendered.contains(first_description));
        assert!(rendered.contains(first_label_a));
        assert!(rendered.contains(first_label_b));
        assert_no_opaque_ids(&rendered);

        events_tx
            .send(ConsoleEvent::PromptLine("2".to_owned()))
            .expect("first permission choice");

        let first_response = tokio::time::timeout(Duration::from_secs(2), responses_rx.recv())
            .await
            .expect("first permission response timed out")
            .expect("first permission response channel closed");
        match first_response.outcome {
            v2::RequestPermissionOutcome::Selected(selected) => {
                assert_eq!(selected.option_id.0.as_ref(), first_opaque_b);
            }
            other => panic!("expected selected second option, got {other:?}"),
        }

        wait_for_snapshot(&root.0, task_id, |snapshot| {
            matches!(
                snapshot.observation.state,
                AcpSessionState::RequiresAction(AcpActionKind::Permission)
            ) && snapshot
                .observation
                .detail
                .as_deref()
                .is_some_and(|detail| {
                    detail.contains(second_title) && detail.contains(second_label)
                })
        })
        .await;

        events_tx
            .send(ConsoleEvent::PromptLine("maybe".to_owned()))
            .expect("unknown permission choice");

        let second_response = tokio::time::timeout(Duration::from_secs(2), responses_rx.recv())
            .await
            .expect("second permission response timed out")
            .expect("second permission response channel closed");
        assert!(matches!(
            second_response.outcome,
            v2::RequestPermissionOutcome::Cancelled
        ));

        wait_for_snapshot(&root.0, task_id, |snapshot| {
            matches!(
                snapshot.observation.state,
                AcpSessionState::Idle(Some(AcpStopReason::Cancelled))
            )
        })
        .await;

        tokio::time::timeout(Duration::from_secs(2), agent_task)
            .await
            .expect("agent did not stop")
            .expect("agent task panicked")
            .expect("agent connect_with should succeed");

        let result = tokio::time::timeout(Duration::from_secs(2), lifecycle_task)
            .await
            .expect("lifecycle did not stop after EOF")
            .expect("lifecycle task panicked");
        result.expect("permission lifecycle should succeed");

        assert_no_opaque_ids(
            &String::from_utf8(output.0.lock().unwrap().clone()).expect("utf8 output"),
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn requests_elicitation_transport_handles_empty_form_and_declines_url() {
        let root = ConsoleTempRoot::new();
        let task_id = "repo/elicitation-transport";
        let cwd = root.0.join("elicitation-worktree");
        let session_id = "elicitation-session";
        let form_message = "Confirm to continue";
        let url_payload = "https://elicitation-url-decline.example/setup";
        let url_elicitation_id = "opaque-elicit-url-id";

        let (running_tx, running_rx) = tokio::sync::oneshot::channel::<()>();
        let (responses_tx, mut responses_rx) =
            tokio::sync::mpsc::unbounded_channel::<v2::CreateElicitationResponse>();
        let (client_transport, agent_transport) = Channel::duplex();
        let agent_session_id = session_id.to_owned();
        let agent = Agent
            .v2()
            .on_receive_request(
                async |request: v2::InitializeRequest, responder, _cx| {
                    assert_eq!(request.protocol_version, ProtocolVersion::V2);
                    let elicitation = request
                        .capabilities
                        .elicitation
                        .as_ref()
                        .expect("form elicitation capability");
                    assert!(elicitation.form.is_some());
                    assert!(elicitation.url.is_none());
                    responder.respond(v2::InitializeResponse::new(
                        ProtocolVersion::V2,
                        v2::Implementation::new("elicitation-test-agent", "1"),
                    ))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |_request: v2::NewSessionRequest,
                            responder: agent_client_protocol::Responder<v2::NewSessionResponse>,
                            _cx| {
                    responder.respond(v2::NewSessionResponse::new(agent_session_id.clone()))
                },
                agent_client_protocol::on_receive_request!(),
            );
        let agent_task = tokio::spawn(async move {
            let session_id = session_id.to_owned();
            let elicitation_responses_tx = responses_tx;
            agent
                .connect_with(
                    agent_transport,
                    async move |connection: ConnectionTo<Client>| {
                        let _ = running_rx.await;

                        let form_response = connection
                            .send_request(v2::CreateElicitationRequest::new(
                                v2::ElicitationFormMode::new(
                                    v2::ElicitationSessionScope::new(session_id.clone()),
                                    v2::ElicitationSchema::new(),
                                ),
                                form_message,
                            ))
                            .block_task()
                            .await?;
                        elicitation_responses_tx
                            .send(form_response)
                            .expect("form elicitation response channel open");

                        let url_response = connection
                            .send_request(v2::CreateElicitationRequest::new(
                                v2::ElicitationUrlMode::new(
                                    v2::ElicitationSessionScope::new(session_id.clone()),
                                    url_elicitation_id,
                                    url_payload,
                                ),
                                "Open browser to continue",
                            ))
                            .block_task()
                            .await?;
                        elicitation_responses_tx
                            .send(url_response)
                            .expect("url elicitation response channel open");

                        connection.send_notification(v2::UpdateSessionNotification::new(
                            session_id,
                            v2::SessionUpdate::StateUpdate(idle_cancelled()),
                        ))?;
                        tokio::task::yield_now().await;
                        Ok(())
                    },
                )
                .await
        });

        let (events_tx, events_rx) = tokio::sync::mpsc::unbounded_channel();
        let writer = SharedWriter::default();
        let output = writer.clone();
        let console = AgentAcpConsole::new(events_rx, writer);
        let lifecycle_root = root.0.clone();
        let lifecycle_cwd = cwd.clone();
        let lifecycle_task = tokio::spawn(async move {
            run_session_lifecycle(
                client_transport,
                task_id,
                &lifecycle_root,
                &lifecycle_cwd,
                console,
            )
            .await
        });

        wait_for_snapshot(&root.0, task_id, |snapshot| {
            matches!(snapshot.observation.state, AcpSessionState::Running)
        })
        .await;
        let _ = running_tx.send(());

        wait_for_snapshot(&root.0, task_id, |snapshot| {
            matches!(
                snapshot.observation.state,
                AcpSessionState::RequiresAction(AcpActionKind::Input)
            ) && snapshot
                .observation
                .detail
                .as_deref()
                .is_some_and(|detail| detail == form_message)
        })
        .await;

        let rendered = String::from_utf8(output.0.lock().unwrap().clone()).expect("utf8 output");
        assert!(rendered.contains(form_message));
        assert!(!rendered.contains(url_payload));
        assert!(!rendered.contains(url_elicitation_id));

        events_tx
            .send(ConsoleEvent::PromptLine("not-json".to_owned()))
            .expect("invalid elicitation input");

        let validation_error = super::ELICITATION_VALIDATION_ERROR;
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let bytes = output.0.lock().unwrap().clone();
                if String::from_utf8_lossy(&bytes).contains(validation_error) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("validation error was not rendered");
        assert!(responses_rx.try_recv().is_err());

        events_tx
            .send(ConsoleEvent::PromptLine("{}".to_owned()))
            .expect("valid elicitation input");

        let form_response = tokio::time::timeout(Duration::from_secs(2), responses_rx.recv())
            .await
            .expect("form elicitation response timed out")
            .expect("form elicitation response channel closed");
        match form_response.action {
            v2::ElicitationAction::Accept(action) => {
                assert_eq!(
                    action.content.as_ref(),
                    Some(&std::collections::BTreeMap::new())
                );
            }
            other => panic!("expected accept for empty form, got {other:?}"),
        }

        let url_response = tokio::time::timeout(Duration::from_secs(2), responses_rx.recv())
            .await
            .expect("url elicitation response timed out")
            .expect("url elicitation response channel closed");
        assert!(matches!(
            url_response.action,
            v2::ElicitationAction::Decline
        ));

        let final_output =
            String::from_utf8(output.0.lock().unwrap().clone()).expect("utf8 output");
        assert!(!final_output.contains(url_payload));
        assert!(!final_output.contains(url_elicitation_id));

        wait_for_snapshot(&root.0, task_id, |snapshot| {
            matches!(
                snapshot.observation.state,
                AcpSessionState::Idle(Some(AcpStopReason::Cancelled))
            )
        })
        .await;

        tokio::time::timeout(Duration::from_secs(2), agent_task)
            .await
            .expect("agent did not stop")
            .expect("agent task panicked")
            .expect("agent connect_with should succeed");

        let result = tokio::time::timeout(Duration::from_secs(2), lifecycle_task)
            .await
            .expect("lifecycle did not stop after EOF")
            .expect("lifecycle task panicked");
        result.expect("elicitation lifecycle should succeed");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn requests_elicitation_values_handles_string_integer_form() {
        let root = ConsoleTempRoot::new();
        let task_id = "repo/elicitation-values";
        let cwd = root.0.join("elicitation-values-worktree");
        let session_id = "elicitation-values-session";
        let form_message = "Enter deployment settings";
        let cluster_name = "cluster";
        let replicas_name = "replicas";
        let secret = "SECRET-TYPED-VALUE-42";

        let form_schema = v2::ElicitationSchema::new()
            .string(cluster_name, true)
            .property(replicas_name, v2::IntegerPropertySchema::new(), true);

        let (running_tx, running_rx) = tokio::sync::oneshot::channel::<()>();
        let (responses_tx, mut responses_rx) =
            tokio::sync::mpsc::unbounded_channel::<v2::CreateElicitationResponse>();
        let (client_transport, agent_transport) = Channel::duplex();
        let agent_session_id = session_id.to_owned();
        let agent = Agent
            .v2()
            .on_receive_request(
                async |request: v2::InitializeRequest, responder, _cx| {
                    assert_eq!(request.protocol_version, ProtocolVersion::V2);
                    responder.respond(v2::InitializeResponse::new(
                        ProtocolVersion::V2,
                        v2::Implementation::new("elicitation-values-agent", "1"),
                    ))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |_request: v2::NewSessionRequest,
                            responder: agent_client_protocol::Responder<v2::NewSessionResponse>,
                            _cx| {
                    responder.respond(v2::NewSessionResponse::new(agent_session_id.clone()))
                },
                agent_client_protocol::on_receive_request!(),
            );
        let agent_task = tokio::spawn(async move {
            let session_id = session_id.to_owned();
            let elicitation_responses_tx = responses_tx;
            agent
                .connect_with(
                    agent_transport,
                    async move |connection: ConnectionTo<Client>| {
                        let _ = running_rx.await;

                        let form_response = connection
                            .send_request(v2::CreateElicitationRequest::new(
                                v2::ElicitationFormMode::new(
                                    v2::ElicitationSessionScope::new(session_id.clone()),
                                    form_schema,
                                ),
                                form_message,
                            ))
                            .block_task()
                            .await?;
                        elicitation_responses_tx
                            .send(form_response)
                            .expect("elicitation response channel open");

                        connection.send_notification(v2::UpdateSessionNotification::new(
                            session_id,
                            v2::SessionUpdate::StateUpdate(idle_cancelled()),
                        ))?;
                        tokio::task::yield_now().await;
                        Ok(())
                    },
                )
                .await
        });

        let (events_tx, events_rx) = tokio::sync::mpsc::unbounded_channel();
        let writer = SharedWriter::default();
        let output = writer.clone();
        let console = AgentAcpConsole::new(events_rx, writer);
        let lifecycle_root = root.0.clone();
        let lifecycle_cwd = cwd.clone();
        let lifecycle_task = tokio::spawn(async move {
            run_session_lifecycle(
                client_transport,
                task_id,
                &lifecycle_root,
                &lifecycle_cwd,
                console,
            )
            .await
        });

        wait_for_snapshot(&root.0, task_id, |snapshot| {
            matches!(snapshot.observation.state, AcpSessionState::Running)
        })
        .await;
        let _ = running_tx.send(());

        wait_for_snapshot(&root.0, task_id, |snapshot| {
            matches!(
                snapshot.observation.state,
                AcpSessionState::RequiresAction(AcpActionKind::Input)
            ) && snapshot
                .observation
                .detail
                .as_deref()
                .is_some_and(|detail| {
                    detail.contains(form_message)
                        && detail.contains(&format!("{cluster_name}: string"))
                        && detail.contains(&format!("{replicas_name}: integer"))
                })
        })
        .await;

        let rendered = String::from_utf8(output.0.lock().unwrap().clone()).expect("utf8 output");
        assert!(rendered.contains(form_message));
        assert!(rendered.contains(&format!("{cluster_name}: string")));
        assert!(rendered.contains(&format!("{replicas_name}: integer")));

        events_tx
            .send(ConsoleEvent::PromptLine(format!(
                r#"{{"{cluster_name}":"prod","{replicas_name}":"{secret}"}}"#
            )))
            .expect("type-mismatched elicitation input");

        let expected_type_error = format!("{replicas_name}: expected integer");
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let bytes = output.0.lock().unwrap().clone();
                let text = String::from_utf8_lossy(&bytes);
                if text.contains(&expected_type_error) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("type validation error was not rendered");
        assert!(responses_rx.try_recv().is_err());
        assert!(!String::from_utf8_lossy(&output.0.lock().unwrap().clone()).contains(secret));

        wait_for_snapshot(&root.0, task_id, |snapshot| {
            matches!(
                snapshot.observation.state,
                AcpSessionState::RequiresAction(AcpActionKind::Input)
            ) && snapshot
                .observation
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains(form_message))
        })
        .await;

        events_tx
            .send(ConsoleEvent::PromptLine(
                r#"{"cluster":"prod","replicas":3}"#.to_owned(),
            ))
            .expect("valid elicitation input");

        let form_response = tokio::time::timeout(Duration::from_secs(2), responses_rx.recv())
            .await
            .expect("elicitation response timed out")
            .expect("elicitation response channel closed");
        match form_response.action {
            v2::ElicitationAction::Accept(action) => {
                let content = action.content.expect("typed content");
                assert_eq!(
                    content.get(cluster_name),
                    Some(&v2::ElicitationContentValue::String("prod".to_owned()))
                );
                assert_eq!(
                    content.get(replicas_name),
                    Some(&v2::ElicitationContentValue::Integer(3))
                );
            }
            other => panic!("expected accept for typed form, got {other:?}"),
        }

        wait_for_snapshot(&root.0, task_id, |snapshot| {
            matches!(
                snapshot.observation.state,
                AcpSessionState::Idle(Some(AcpStopReason::Cancelled))
            )
        })
        .await;

        tokio::time::timeout(Duration::from_secs(2), agent_task)
            .await
            .expect("agent did not stop")
            .expect("agent task panicked")
            .expect("agent connect_with should succeed");

        let result = tokio::time::timeout(Duration::from_secs(2), lifecycle_task)
            .await
            .expect("lifecycle did not stop after EOF")
            .expect("lifecycle task panicked");
        result.expect("elicitation values lifecycle should succeed");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn requests_authentication_success_retries_new_session_after_login() {
        let root = ConsoleTempRoot::new();
        let task_id = "repo/authentication-success";
        let cwd = root.0.join("authentication-worktree");
        let first_name = "API Key";
        let second_name = "Device Code";
        let second_id = "opaque-agent-auth-beta";
        let session_id = "authenticated-session";
        let secrets = [
            "opaque-unsupported-auth-id",
            "Browser OAuth",
            "unsupported-description-secret",
            "opaque-agent-auth-alpha",
            second_id,
            "peer-auth-required-secret",
        ];

        let new_calls = Arc::new(AtomicU64::new(0));
        let requests = Arc::new(Mutex::new(Vec::<String>::new()));
        let (client_transport, agent_transport) = Channel::duplex();
        let agent = Agent.v2().on_receive_request(
            async |request: v2::InitializeRequest, responder, _cx| {
                assert_eq!(request.protocol_version, ProtocolVersion::V2);
                responder.respond(
                    v2::InitializeResponse::new(
                        ProtocolVersion::V2,
                        v2::Implementation::new("authentication-success-agent", "1"),
                    )
                    .auth_methods(vec![
                        v2::AuthMethod::Other(v2::OtherAuthMethod::new(
                            "_oauth",
                            "opaque-unsupported-auth-id",
                            "Browser OAuth",
                            std::collections::BTreeMap::from([(
                                "description".to_owned(),
                                serde_json::Value::String(
                                    "unsupported-description-secret".to_owned(),
                                ),
                            )]),
                        )),
                        v2::AuthMethod::Agent(v2::AuthMethodAgent::new(
                            "opaque-agent-auth-alpha",
                            "API Key",
                        )),
                        v2::AuthMethod::Agent(v2::AuthMethodAgent::new(
                            "opaque-agent-auth-beta",
                            "Device Code",
                        )),
                    ]),
                )
            },
            agent_client_protocol::on_receive_request!(),
        );
        let agent = {
            let requests = Arc::clone(&requests);
            agent.on_receive_request(
                async move |request: v2::LoginAuthRequest,
                            responder: agent_client_protocol::Responder<v2::LoginAuthResponse>,
                            _cx| {
                    requests
                        .lock()
                        .unwrap()
                        .push(format!("login:{}", request.method_id.0));
                    responder.respond(v2::LoginAuthResponse::new())
                },
                agent_client_protocol::on_receive_request!(),
            )
        };
        let agent = {
            let requests = Arc::clone(&requests);
            let new_calls = Arc::clone(&new_calls);
            agent.on_receive_request(
                async move |_request: v2::NewSessionRequest,
                            responder: agent_client_protocol::Responder<v2::NewSessionResponse>,
                            _cx| {
                    requests.lock().unwrap().push("new".to_owned());
                    if new_calls.fetch_add(1, Ordering::Relaxed) == 0 {
                        responder.respond_with_error(
                            Error::auth_required().data("peer-auth-required-secret"),
                        )
                    } else {
                        responder.respond(v2::NewSessionResponse::new(session_id))
                    }
                },
                agent_client_protocol::on_receive_request!(),
            )
        };
        let agent_task = tokio::spawn(agent.connect_to(agent_transport));

        let (events_tx, events_rx) = tokio::sync::mpsc::unbounded_channel();
        let output = SharedWriter::default();
        let console = AgentAcpConsole::new(events_rx, output.clone());
        let lifecycle_task = tokio::spawn({
            let root = root.0.clone();
            async move { run_session_lifecycle(client_transport, task_id, &root, &cwd, console).await }
        });

        wait_for_snapshot(&root.0, task_id, |snapshot| {
            matches!(
                snapshot.observation.state,
                AcpSessionState::RequiresAction(AcpActionKind::Authentication)
            ) && snapshot.session_id.is_none()
        })
        .await;

        let pending = console_snapshot(&root.0, task_id);
        let detail = pending
            .observation
            .detail
            .as_deref()
            .expect("authentication detail");
        assert!(detail.contains(&format!("1. {first_name}")));
        assert!(detail.contains(&format!("2. {second_name}")));
        for secret in secrets {
            assert!(!detail.contains(secret), "detail leaked {secret}");
        }

        let rendered = String::from_utf8(output.0.lock().unwrap().clone()).expect("utf8");
        assert!(rendered.contains(&format!("1. {first_name}")));
        assert!(rendered.contains(&format!("2. {second_name}")));
        for secret in secrets {
            assert!(!rendered.contains(secret), "output leaked {secret}");
        }

        events_tx
            .send(ConsoleEvent::PromptLine("0".to_owned()))
            .expect("invalid choice");
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let rendered = String::from_utf8(output.0.lock().unwrap().clone()).expect("utf8");
                if rendered.contains(super::AUTHENTICATION_VALIDATION_ERROR) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("authentication validation line was not rendered");
        assert_eq!(new_calls.load(Ordering::Relaxed), 1);
        assert!(!requests
            .lock()
            .unwrap()
            .iter()
            .any(|entry| entry.starts_with("login:")));

        events_tx
            .send(ConsoleEvent::PromptLine("2".to_owned()))
            .expect("valid choice");
        wait_for_snapshot(&root.0, task_id, |snapshot| {
            snapshot.session_id.as_deref() == Some(session_id)
                && matches!(snapshot.observation.state, AcpSessionState::Running)
        })
        .await;

        assert_eq!(
            *requests.lock().unwrap(),
            vec![
                "new".to_owned(),
                format!("login:{second_id}"),
                "new".to_owned(),
            ]
        );
        assert_eq!(new_calls.load(Ordering::Relaxed), 2);

        agent_task.abort();
        let _ = agent_task.await;
        let _ = lifecycle_task.await;
    }

    #[derive(Clone, Copy)]
    enum AuthFailureCase {
        UnsupportedOnly,
        LoginFailed,
        ClosedConsole,
        Interrupt,
        InputError,
        PostAuthStartFailed,
    }

    async fn run_auth_failure_case(case: AuthFailureCase) {
        const PEER: &str = "peer-s";
        const INPUT: &str = "in-s";
        const META: [&str; 4] = [
            "opaque-unsupported-auth-id",
            "Browser OAuth",
            "unsupported-description-secret",
            "opaque-agent-auth-alpha",
        ];
        let (tag, detail, expect_new, expect_login) = match case {
            AuthFailureCase::UnsupportedOnly => (
                "u",
                "No supported authentication methods are available.",
                1u64,
                0u64,
            ),
            AuthFailureCase::LoginFailed => ("l", "Authentication failed.", 1, 1),
            AuthFailureCase::ClosedConsole
            | AuthFailureCase::Interrupt
            | AuthFailureCase::InputError => ("c", "Authentication cancelled.", 1, 0),
            AuthFailureCase::PostAuthStartFailed => {
                ("p", "Session start failed after authentication.", 2, 1)
            }
        };
        let tag = match case {
            AuthFailureCase::ClosedConsole => "cl",
            AuthFailureCase::Interrupt => "i",
            AuthFailureCase::InputError => "e",
            _ => tag,
        };
        let root = ConsoleTempRoot::new();
        let task_id = format!("repo/auth-fail-{tag}");
        let cwd = root.0.join(format!("auth-fail-{tag}"));
        let new_calls = Arc::new(AtomicU64::new(0));
        let login_calls = Arc::new(AtomicU64::new(0));
        let (client_transport, agent_transport) = Channel::duplex();
        let agent = Agent.v2().on_receive_request(
            async move |request: v2::InitializeRequest, responder, _cx| {
                assert_eq!(request.protocol_version, ProtocolVersion::V2);
                let auth_methods = if matches!(case, AuthFailureCase::UnsupportedOnly) {
                    vec![v2::AuthMethod::Other(v2::OtherAuthMethod::new(
                        "_oauth",
                        META[0],
                        META[1],
                        std::collections::BTreeMap::from([(
                            "description".to_owned(),
                            serde_json::Value::String(META[2].to_owned()),
                        )]),
                    ))]
                } else {
                    vec![v2::AuthMethod::Agent(v2::AuthMethodAgent::new(
                        META[3], "API Key",
                    ))]
                };
                responder.respond(
                    v2::InitializeResponse::new(
                        ProtocolVersion::V2,
                        v2::Implementation::new("authentication-failure-agent", "1"),
                    )
                    .auth_methods(auth_methods),
                )
            },
            agent_client_protocol::on_receive_request!(),
        );
        let agent = {
            let login_calls = Arc::clone(&login_calls);
            agent.on_receive_request(
                async move |request: v2::LoginAuthRequest,
                            responder: agent_client_protocol::Responder<v2::LoginAuthResponse>,
                            _cx| {
                    login_calls.fetch_add(1, Ordering::Relaxed);
                    assert_eq!(request.method_id.0.as_ref(), META[3]);
                    match case {
                        AuthFailureCase::LoginFailed => {
                            responder.respond_with_error(Error::internal_error().data(PEER))
                        }
                        AuthFailureCase::PostAuthStartFailed => {
                            responder.respond(v2::LoginAuthResponse::new())
                        }
                        _ => panic!("unexpected auth/login"),
                    }
                },
                agent_client_protocol::on_receive_request!(),
            )
        };
        let agent = {
            let new_calls = Arc::clone(&new_calls);
            agent.on_receive_request(
                async move |_request: v2::NewSessionRequest,
                            responder: agent_client_protocol::Responder<v2::NewSessionResponse>,
                            _cx| {
                    let attempt = new_calls.fetch_add(1, Ordering::Relaxed) + 1;
                    if matches!(case, AuthFailureCase::PostAuthStartFailed) && attempt == 2 {
                        responder.respond_with_error(Error::internal_error().data(PEER))
                    } else {
                        responder.respond_with_error(Error::auth_required().data(PEER))
                    }
                },
                agent_client_protocol::on_receive_request!(),
            )
        };
        let agent_task = tokio::spawn(agent.connect_to(agent_transport));
        let output = SharedWriter::default();
        let (sender, events_rx) = tokio::sync::mpsc::unbounded_channel();
        let events_tx = if matches!(case, AuthFailureCase::ClosedConsole) {
            drop(sender);
            None
        } else {
            Some(sender)
        };
        let console = AgentAcpConsole::new(events_rx, output.clone());
        let lifecycle_task = tokio::spawn({
            let root = root.0.clone();
            let cwd = cwd.clone();
            let task_id = task_id.clone();
            async move { run_session_lifecycle(client_transport, &task_id, &root, &cwd, console).await }
        });
        if let Some(event) = match case {
            AuthFailureCase::LoginFailed | AuthFailureCase::PostAuthStartFailed => {
                Some(ConsoleEvent::PromptLine("1".to_owned()))
            }
            AuthFailureCase::Interrupt => Some(ConsoleEvent::Interrupt),
            AuthFailureCase::InputError => Some(ConsoleEvent::InputError(INPUT.to_owned())),
            AuthFailureCase::UnsupportedOnly | AuthFailureCase::ClosedConsole => None,
        } {
            wait_for_snapshot(&root.0, &task_id, |snapshot| {
                matches!(
                    snapshot.observation.state,
                    AcpSessionState::RequiresAction(AcpActionKind::Authentication)
                )
            })
            .await;
            events_tx
                .as_ref()
                .expect("interactive console")
                .send(event)
                .expect("console event");
        }
        wait_for_snapshot(&root.0, &task_id, |snapshot| {
            snapshot.observation.state == AcpSessionState::Failed
        })
        .await;
        tokio::time::timeout(Duration::from_secs(2), lifecycle_task)
            .await
            .expect("lifecycle did not stop")
            .expect("lifecycle task panicked")
            .expect_err("authentication failure should return an error");
        let snapshot = console_snapshot(&root.0, &task_id);
        assert_eq!(snapshot.session_id, None);
        assert_eq!(snapshot.observation.state, AcpSessionState::Failed);
        assert_eq!(snapshot.observation.detail.as_deref(), Some(detail));
        assert_eq!(new_calls.load(Ordering::Relaxed), expect_new);
        assert_eq!(login_calls.load(Ordering::Relaxed), expect_login);
        let rendered = String::from_utf8(output.0.lock().unwrap().clone()).expect("utf8");
        for text in [
            snapshot.observation.detail.as_deref().unwrap_or(""),
            &rendered,
        ] {
            for secret in META.into_iter().chain([PEER, INPUT]) {
                assert!(!text.contains(secret), "leaked {secret}");
            }
        }
        agent_task.abort();
        let _ = agent_task.await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn requests_authentication_failure_cases() {
        for case in [
            AuthFailureCase::UnsupportedOnly,
            AuthFailureCase::LoginFailed,
            AuthFailureCase::ClosedConsole,
            AuthFailureCase::Interrupt,
            AuthFailureCase::InputError,
            AuthFailureCase::PostAuthStartFailed,
        ] {
            run_auth_failure_case(case).await;
        }
    }
}

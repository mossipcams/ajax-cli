//! PTY-backed tmux attach for the browser task terminal bridge.

use crate::slices::terminal::TerminalAttachPlan;
use axum::extract::ws::{Message, WebSocket};
use portable_pty::{native_pty_system, Child, CommandBuilder, PtySize};
use serde::Deserialize;
use std::{
    io::{Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::mpsc;
use tracing::warn;

const TERMINAL_CHILD_CLEANUP_WAIT_TIMEOUT: Duration = Duration::from_secs(2);

pub const MAX_INPUT_FRAME_BYTES: usize = 4096;
const PTY_READ_BUFFER_BYTES: usize = 8192;

trait TerminalChild {
    fn kill_child(&mut self) -> std::io::Result<()>;
    fn wait_child(&mut self) -> std::io::Result<()>;
}

impl TerminalChild for Box<dyn Child + Send + Sync> {
    fn kill_child(&mut self) -> std::io::Result<()> {
        self.kill()
    }

    fn wait_child(&mut self) -> std::io::Result<()> {
        self.wait().map(|_| ())
    }
}

fn cleanup_spawned_child<C: TerminalChild>(mut child: C) {
    let _ = child.kill_child();
    let _ = child.wait_child();
}

async fn cleanup_spawned_child_async<C: TerminalChild + Send + 'static>(child: C) {
    cleanup_spawned_child_async_with_timeout(child, TERMINAL_CHILD_CLEANUP_WAIT_TIMEOUT).await;
}

async fn cleanup_spawned_child_async_with_timeout<C: TerminalChild + Send + 'static>(
    child: C,
    wait_timeout: Duration,
) {
    let wait_task = tokio::task::spawn_blocking(move || cleanup_spawned_child(child));
    match tokio::time::timeout(wait_timeout, wait_task).await {
        Ok(Ok(())) => {}
        Ok(Err(join_error)) => {
            warn!("terminal child cleanup task failed: {join_error}");
        }
        Err(_) => {
            warn!(
                "terminal child cleanup timed out after {:?}; continuing websocket close",
                wait_timeout
            );
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TmuxAttachCommandPlan {
    pub program: String,
    pub args: Vec<String>,
}

pub fn tmux_attach_target(session: &str, worktrunk_window: &str) -> String {
    format!("{session}:{worktrunk_window}")
}

pub fn build_tmux_attach_command_plan(plan: &TerminalAttachPlan) -> TmuxAttachCommandPlan {
    let target = tmux_attach_target(&plan.tmux_session, &plan.worktrunk_window);
    TmuxAttachCommandPlan {
        program: "tmux".to_string(),
        args: vec!["attach-session".to_string(), "-t".to_string(), target],
    }
}

/// A single tmux invocation used to stand up or tear down the isolated client
/// session. Kept as a plain data plan so the wiring is unit-testable without a
/// live tmux server.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TmuxCommand {
    pub program: String,
    pub args: Vec<String>,
}

impl TmuxCommand {
    fn new<const N: usize>(args: [&str; N]) -> Self {
        TmuxCommand {
            program: "tmux".to_string(),
            args: args.iter().map(|arg| arg.to_string()).collect(),
        }
    }
}

/// Attach a mobile client to its *own* grouped tmux session instead of the
/// shared task session.
///
/// `tmux attach-session` sizes a window to the smallest attached client, so a
/// phone in portrait would shrink the agent window for every other client and
/// SIGWINCH-storm the pane on each keyboard open/close. A grouped session
/// (`new-session -t <shared>`) shares the shared session's window set but keeps
/// an independent size, so the phone can be tiny without disturbing anyone. The
/// ephemeral session is killed on disconnect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IsolatedAttachPlan {
    /// The ephemeral grouped session name, e.g. `ajax-web-fix-login-m1a2b3c4`.
    pub ephemeral_session: String,
    /// Commands to run before attaching (create the grouped session).
    pub setup: Vec<TmuxCommand>,
    /// The attach command spawned inside the outer PTY.
    pub attach: TmuxAttachCommandPlan,
    /// Commands to run on disconnect (remove the grouped session).
    pub teardown: Vec<TmuxCommand>,
}

/// Prefix that marks a session as an ephemeral per-client grouped session.
/// The reaper uses this to distinguish them from real task sessions.
pub const EPHEMERAL_SESSION_INFIX: &str = "-m";

pub fn build_isolated_attach_plan(plan: &TerminalAttachPlan) -> IsolatedAttachPlan {
    build_isolated_attach_plan_with_token(plan, &random_session_token())
}

fn build_isolated_attach_plan_with_token(
    plan: &TerminalAttachPlan,
    token: &str,
) -> IsolatedAttachPlan {
    let ephemeral = format!("{}{EPHEMERAL_SESSION_INFIX}{token}", plan.tmux_session);
    // Reuse the shared attach builder against the ephemeral session so the
    // "never attach through the browser handle" and worktrunk-window guarantees
    // are preserved for the isolated client too.
    let ephemeral_plan = TerminalAttachPlan {
        qualified_handle: plan.qualified_handle.clone(),
        tmux_session: ephemeral.clone(),
        worktrunk_window: plan.worktrunk_window.clone(),
    };
    IsolatedAttachPlan {
        setup: vec![TmuxCommand::new([
            "new-session",
            "-d",
            "-s",
            &ephemeral,
            "-t",
            &plan.tmux_session,
        ])],
        attach: build_tmux_attach_command_plan(&ephemeral_plan),
        teardown: vec![TmuxCommand::new(["kill-session", "-t", &ephemeral])],
        ephemeral_session: ephemeral,
    }
}

/// 12 lowercase-hex chars of randomness for the ephemeral session suffix.
fn random_session_token() -> String {
    let mut bytes = [0_u8; 6];
    // A failed RNG here only weakens uniqueness of a short-lived session name;
    // fall back to a time-derived token rather than aborting the attach.
    if getrandom::fill(&mut bytes).is_err() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0);
        bytes.copy_from_slice(&nanos.to_le_bytes()[..6]);
    }
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut token = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        token.push(HEX[(byte >> 4) as usize] as char);
        token.push(HEX[(byte & 0x0f) as usize] as char);
    }
    token
}

fn run_tmux_command_blocking(command: &TmuxCommand) -> std::io::Result<std::process::Output> {
    std::process::Command::new(&command.program)
        .args(&command.args)
        .output()
}

/// True when `name` looks like an ephemeral per-client grouped session
/// (`<shared>-m<12 lowercase hex>`). Requires the full 12-hex token so real
/// task sessions such as `ajax-web-main` are never matched.
pub fn is_ephemeral_session_name(name: &str) -> bool {
    match name.rfind(EPHEMERAL_SESSION_INFIX) {
        Some(index) if index > 0 => {
            let token = &name[index + EPHEMERAL_SESSION_INFIX.len()..];
            token.len() == 12
                && token
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        }
        _ => false,
    }
}

/// Select the ephemeral grouped sessions to kill from a list of live session
/// names. A crashed bridge can leave its per-client session behind; the web
/// server reaps them on startup so they don't accumulate.
pub fn ephemeral_sessions_to_reap(names: &[String]) -> Vec<String> {
    names
        .iter()
        .filter(|name| is_ephemeral_session_name(name))
        .cloned()
        .collect()
}

/// Best-effort: list tmux sessions and kill any orphaned ephemeral grouped
/// sessions. Never fails the caller; if tmux is absent or has no server there
/// is nothing to reap.
pub fn reap_orphan_terminal_sessions() {
    let listing = match run_tmux_command_blocking(&TmuxCommand::new([
        "list-sessions",
        "-F",
        "#{session_name}",
    ])) {
        Ok(output) if output.status.success() => output.stdout,
        _ => return,
    };
    let names: Vec<String> = String::from_utf8_lossy(&listing)
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();
    for session in ephemeral_sessions_to_reap(&names) {
        let _ = run_tmux_command_blocking(&TmuxCommand::new(["kill-session", "-t", &session]));
    }
}

#[derive(Debug, Deserialize)]
struct TerminalInputFrame {
    #[serde(rename = "type")]
    frame_type: String,
    data: Option<String>,
    #[serde(default)]
    cols: u16,
    #[serde(default)]
    rows: u16,
}

#[derive(Debug, serde::Serialize)]
struct TerminalOutputFrame<'a> {
    #[serde(rename = "type")]
    frame_type: &'static str,
    data: &'a str,
}

pub async fn bridge_task_terminal_socket(mut socket: WebSocket, plan: TerminalAttachPlan) {
    let isolated = build_isolated_attach_plan(&plan);

    // Stand up the isolated grouped session before attaching so the phone's
    // dimensions never shrink the shared window for other clients. If this
    // fails the shared session is likely gone; report and bail rather than
    // attaching to nothing.
    for command in &isolated.setup {
        match run_tmux_command_blocking(command) {
            Ok(output) if output.status.success() => {}
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let _ = socket
                    .send(Message::Text(
                        serde_json::json!({
                            "type": "error",
                            "error": format!("failed to create terminal session: {}", stderr.trim()),
                        })
                        .to_string()
                        .into(),
                    ))
                    .await;
                let _ = socket.send(Message::Close(None)).await;
                return;
            }
            Err(error) => {
                let _ = socket
                    .send(Message::Text(
                        serde_json::json!({
                            "type": "error",
                            "error": format!("failed to create terminal session: {error}"),
                        })
                        .to_string()
                        .into(),
                    ))
                    .await;
                let _ = socket.send(Message::Close(None)).await;
                return;
            }
        }
    }

    let command_plan = isolated.attach.clone();
    let pty_system = native_pty_system();
    let pty_pair = match pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        Ok(pair) => pair,
        Err(error) => {
            let _ = socket
                .send(Message::Text(
                    serde_json::json!({
                        "type": "error",
                        "error": format!("failed to open PTY: {error}"),
                    })
                    .to_string()
                    .into(),
                ))
                .await;
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
    };

    let mut command = CommandBuilder::new(&command_plan.program);
    for arg in &command_plan.args {
        command.arg(arg);
    }

    let child = match pty_pair.slave.spawn_command(command) {
        Ok(child) => child,
        Err(error) => {
            let _ = socket
                .send(Message::Text(
                    serde_json::json!({
                        "type": "error",
                        "error": format!("failed to spawn tmux attach: {error}"),
                    })
                    .to_string()
                    .into(),
                ))
                .await;
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
    };

    let mut reader = match pty_pair.master.try_clone_reader() {
        Ok(reader) => reader,
        Err(error) => {
            cleanup_spawned_child_async(child).await;
            let _ = socket
                .send(Message::Text(
                    serde_json::json!({
                        "type": "error",
                        "error": format!("failed to clone PTY reader: {error}"),
                    })
                    .to_string()
                    .into(),
                ))
                .await;
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
    };
    let mut writer = match pty_pair.master.take_writer() {
        Ok(writer) => writer,
        Err(error) => {
            cleanup_spawned_child_async(child).await;
            let _ = socket
                .send(Message::Text(
                    serde_json::json!({
                        "type": "error",
                        "error": format!("failed to open PTY writer: {error}"),
                    })
                    .to_string()
                    .into(),
                ))
                .await;
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
    };

    let (output_tx, mut output_rx) = mpsc::channel::<Vec<u8>>(32);
    let running = Arc::new(AtomicBool::new(true));
    let reader_running = Arc::clone(&running);
    let _reader_task = tokio::task::spawn_blocking(move || {
        let mut buffer = [0_u8; PTY_READ_BUFFER_BYTES];
        while reader_running.load(Ordering::Relaxed) {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    if output_tx.blocking_send(buffer[..count].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    loop {
        tokio::select! {
            output = output_rx.recv() => {
                match output {
                    Some(bytes) => {
                        let encoded = base64::Engine::encode(
                            &base64::engine::general_purpose::STANDARD,
                            bytes,
                        );
                        let frame = TerminalOutputFrame {
                            frame_type: "output",
                            data: &encoded,
                        };
                        let payload = match serde_json::to_string(&frame) {
                            Ok(payload) => payload,
                            Err(_) => break,
                        };
                        if socket.send(Message::Text(payload.into())).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        match handle_input_frame(&text, &mut writer) {
                            Ok(Some(size)) => {
                                let _ = pty_pair.master.resize(size);
                            }
                            Err(_) => break,
                            Ok(None) => {}
                        }
                    }
                    Some(Ok(Message::Binary(bytes))) => {
                        if bytes.len() > MAX_INPUT_FRAME_BYTES {
                            break;
                        }
                        if writer.write_all(&bytes).is_err() {
                            break;
                        }
                        let _ = writer.flush();
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(payload))) => {
                        if socket.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Err(_)) => break,
                }
            }
        }
    }

    running.store(false, Ordering::Relaxed);
    cleanup_spawned_child_async(child).await;

    // Remove the ephemeral grouped session now that the client is gone. Killing
    // a grouped session detaches only this client and never destroys the shared
    // session's windows unless it was the last member.
    let teardown = isolated.teardown.clone();
    let _ = tokio::task::spawn_blocking(move || {
        for command in &teardown {
            let _ = run_tmux_command_blocking(command);
        }
    })
    .await;

    let _ = socket.send(Message::Close(None)).await;
}

fn handle_input_frame(
    text: &str,
    writer: &mut Box<dyn Write + Send>,
) -> std::io::Result<Option<PtySize>> {
    let frame: TerminalInputFrame = match serde_json::from_str(text) {
        Ok(frame) => frame,
        Err(_) => return Ok(None),
    };

    match frame.frame_type.as_str() {
        "input" => {
            let data = frame.data.ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "input frame missing data")
            })?;
            if data.len() > MAX_INPUT_FRAME_BYTES {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "input frame too large",
                ));
            }
            writer.write_all(data.as_bytes())?;
            writer.flush()?;
            Ok(None)
        }
        "resize" if frame.cols > 0 && frame.rows > 0 => Ok(Some(PtySize {
            rows: frame.rows,
            cols: frame.cols,
            pixel_width: 0,
            pixel_height: 0,
        })),
        _ => Ok(None),
    }
}

#[cfg(test)]
pub(crate) async fn simulate_terminal_disconnect_cleanup_for_tests(wait_timeout: Duration) {
    #[derive(Clone, Debug)]
    struct Child {
        killed: Arc<std::sync::Mutex<bool>>,
        wait_gate: Arc<std::sync::Mutex<std::sync::mpsc::Receiver<()>>>,
    }

    impl Default for Child {
        fn default() -> Self {
            let (_, receiver) = std::sync::mpsc::channel();
            Self {
                killed: Arc::new(std::sync::Mutex::new(false)),
                wait_gate: Arc::new(std::sync::Mutex::new(receiver)),
            }
        }
    }

    impl TerminalChild for Child {
        fn kill_child(&mut self) -> std::io::Result<()> {
            *self.killed.lock().unwrap() = true;
            Ok(())
        }

        fn wait_child(&mut self) -> std::io::Result<()> {
            let receiver = self.wait_gate.lock().unwrap();
            let _ = receiver.recv();
            Ok(())
        }
    }

    cleanup_spawned_child_async_with_timeout(Child::default(), wait_timeout).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn tmux_attach_command_plan_uses_registered_session_and_worktrunk_target() {
        let plan = TerminalAttachPlan {
            qualified_handle: "web/fix-login".to_string(),
            tmux_session: "ajax-web-fix-login".to_string(),
            worktrunk_window: "worktrunk".to_string(),
        };

        let command_plan = build_tmux_attach_command_plan(&plan);

        assert_eq!(command_plan.program, "tmux");
        assert_eq!(
            command_plan.args,
            vec![
                "attach-session".to_string(),
                "-t".to_string(),
                "ajax-web-fix-login:worktrunk".to_string(),
            ]
        );
        assert!(!command_plan
            .args
            .iter()
            .any(|arg| arg.contains("web/fix-login")));
    }

    #[test]
    fn tmux_attach_target_never_uses_browser_handle() {
        let plan = TerminalAttachPlan {
            qualified_handle: "web/evil-handle".to_string(),
            tmux_session: "ajax-web-fix-login".to_string(),
            worktrunk_window: "worktrunk".to_string(),
        };

        let command_plan = build_tmux_attach_command_plan(&plan);

        assert_eq!(command_plan.args[2], "ajax-web-fix-login:worktrunk");
        assert!(!command_plan
            .args
            .iter()
            .any(|arg| arg.contains("evil-handle")));
    }

    #[test]
    fn isolated_attach_plan_creates_grouped_session_then_attaches() {
        let plan = TerminalAttachPlan {
            qualified_handle: "web/fix-login".to_string(),
            tmux_session: "ajax-web-fix-login".to_string(),
            worktrunk_window: "worktrunk".to_string(),
        };

        let isolated = build_isolated_attach_plan_with_token(&plan, "1a2b3c");
        let ephemeral = "ajax-web-fix-login-m1a2b3c";

        assert_eq!(isolated.ephemeral_session, ephemeral);
        // A grouped session shares the shared session's windows but keeps an
        // independent size, so the phone never shrinks the shared window.
        assert_eq!(
            isolated.setup,
            vec![TmuxCommand {
                program: "tmux".to_string(),
                args: vec![
                    "new-session".to_string(),
                    "-d".to_string(),
                    "-s".to_string(),
                    ephemeral.to_string(),
                    "-t".to_string(),
                    "ajax-web-fix-login".to_string(),
                ],
            }]
        );
        // Attach targets the ephemeral session's worktrunk window, never the
        // browser handle and never the shared session directly.
        assert_eq!(
            isolated.attach.args,
            vec![
                "attach-session".to_string(),
                "-t".to_string(),
                format!("{ephemeral}:worktrunk"),
            ]
        );
        assert!(!isolated
            .attach
            .args
            .iter()
            .any(|arg| arg == "ajax-web-fix-login:worktrunk"));
        assert!(!isolated
            .attach
            .args
            .iter()
            .any(|arg| arg.contains("web/fix-login")));
    }

    #[test]
    fn reaper_targets_only_ephemeral_grouped_sessions() {
        let names = vec![
            "ajax-web-x".to_string(),
            "ajax-web-x-m0123456789ab".to_string(),
            "ajax-web-main".to_string(),
            "other".to_string(),
            // Wrong token length must not match a real session ending in -m...
            "ajax-web-x-mabc".to_string(),
        ];

        let targets = ephemeral_sessions_to_reap(&names);

        assert_eq!(targets, vec!["ajax-web-x-m0123456789ab".to_string()]);
    }

    #[test]
    fn isolated_attach_cleanup_kills_ephemeral_session() {
        let plan = TerminalAttachPlan {
            qualified_handle: "web/fix-login".to_string(),
            tmux_session: "ajax-web-fix-login".to_string(),
            worktrunk_window: "worktrunk".to_string(),
        };

        let isolated = build_isolated_attach_plan_with_token(&plan, "1a2b3c");

        assert_eq!(
            isolated.teardown,
            vec![TmuxCommand {
                program: "tmux".to_string(),
                args: vec![
                    "kill-session".to_string(),
                    "-t".to_string(),
                    "ajax-web-fix-login-m1a2b3c".to_string(),
                ],
            }]
        );
    }

    #[test]
    fn isolated_attach_sessions_are_unique_per_call_and_never_the_shared_session() {
        let plan = TerminalAttachPlan {
            qualified_handle: "web/fix-login".to_string(),
            tmux_session: "ajax-web-fix-login".to_string(),
            worktrunk_window: "worktrunk".to_string(),
        };

        let first = build_isolated_attach_plan(&plan).ephemeral_session;
        let second = build_isolated_attach_plan(&plan).ephemeral_session;

        assert_ne!(first, second);
        assert_ne!(first, "ajax-web-fix-login");
        assert!(first.starts_with("ajax-web-fix-login-m"));
    }

    #[test]
    fn handle_input_frame_accepts_resize_without_data() {
        let mut writer: Box<dyn Write + Send> = Box::new(Vec::<u8>::new());

        let size = handle_input_frame(r#"{"type":"resize","cols":132,"rows":40}"#, &mut writer)
            .expect("resize frame should parse")
            .expect("resize frame should return a pty size");

        assert_eq!(size.cols, 132);
        assert_eq!(size.rows, 40);
    }

    #[derive(Clone, Debug, Default)]
    struct CleanupRecorder {
        killed: Arc<Mutex<bool>>,
        waited: Arc<Mutex<bool>>,
    }

    impl TerminalChild for CleanupRecorder {
        fn kill_child(&mut self) -> std::io::Result<()> {
            *self.killed.lock().unwrap() = true;
            Ok(())
        }

        fn wait_child(&mut self) -> std::io::Result<()> {
            *self.waited.lock().unwrap() = true;
            Ok(())
        }
    }

    #[test]
    fn cleanup_spawned_child_kills_and_waits() {
        let child = CleanupRecorder::default();
        let killed = Arc::clone(&child.killed);
        let waited = Arc::clone(&child.waited);

        cleanup_spawned_child(child);

        assert!(*killed.lock().unwrap());
        assert!(*waited.lock().unwrap());
    }

    #[derive(Clone, Debug)]
    struct BlockingWaitChild {
        killed: Arc<Mutex<bool>>,
        release_rx: Arc<Mutex<std::sync::mpsc::Receiver<()>>>,
    }

    impl TerminalChild for BlockingWaitChild {
        fn kill_child(&mut self) -> std::io::Result<()> {
            *self.killed.lock().unwrap() = true;
            Ok(())
        }

        fn wait_child(&mut self) -> std::io::Result<()> {
            let receiver = self.release_rx.lock().unwrap();
            let _ = receiver.recv();
            Ok(())
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn terminal_cleanup_runs_wait_on_blocking_task() {
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let child = BlockingWaitChild {
            killed: Arc::new(Mutex::new(false)),
            release_rx: Arc::new(Mutex::new(release_rx)),
        };
        let killed = Arc::clone(&child.killed);
        let progress = Arc::new(AtomicBool::new(false));
        let progress_for_task = Arc::clone(&progress);

        let cleanup = tokio::spawn(async move {
            cleanup_spawned_child_async(child).await;
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        tokio::spawn(async move {
            progress_for_task.store(true, Ordering::Relaxed);
        })
        .await
        .expect("concurrent async task should run while cleanup waits");

        assert!(
            progress.load(Ordering::Relaxed),
            "tokio worker should stay responsive while child wait runs on a blocking thread"
        );
        assert!(*killed.lock().unwrap());

        release_tx.send(()).expect("release blocked child wait");
        cleanup.await.expect("cleanup task should finish");
    }

    #[derive(Clone, Debug)]
    struct NeverCompletingWaitChild {
        killed: Arc<Mutex<bool>>,
        wait_gate: Arc<Mutex<std::sync::mpsc::Receiver<()>>>,
    }

    impl Default for NeverCompletingWaitChild {
        fn default() -> Self {
            let (_, receiver) = std::sync::mpsc::channel();
            Self {
                killed: Arc::new(Mutex::new(false)),
                wait_gate: Arc::new(Mutex::new(receiver)),
            }
        }
    }

    impl TerminalChild for NeverCompletingWaitChild {
        fn kill_child(&mut self) -> std::io::Result<()> {
            *self.killed.lock().unwrap() = true;
            Ok(())
        }

        fn wait_child(&mut self) -> std::io::Result<()> {
            let receiver = self.wait_gate.lock().unwrap();
            let _ = receiver.recv();
            Ok(())
        }
    }

    #[tokio::test]
    async fn terminal_cleanup_does_not_wait_forever_after_kill() {
        let child = NeverCompletingWaitChild::default();
        let killed = Arc::clone(&child.killed);
        let timeout = Duration::from_millis(50);

        let started = std::time::Instant::now();
        cleanup_spawned_child_async_with_timeout(child, timeout).await;
        let elapsed = started.elapsed();

        assert!(*killed.lock().unwrap());
        assert!(
            elapsed < Duration::from_millis(250),
            "cleanup should time out instead of waiting forever, took {elapsed:?}"
        );
    }
}

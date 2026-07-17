//! PTY-backed tmux attach for the browser task terminal bridge.

/// Transport input for a browser task terminal attach: the task handle, its
/// tmux session, and its task window. Owned by the PTY adapter that consumes it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalAttachPlan {
    pub qualified_handle: String,
    pub tmux_session: String,
    pub task_window: String,
}

use axum::extract::ws::{Message, WebSocket};
use portable_pty::{native_pty_system, Child, CommandBuilder, PtySize};
use serde::Deserialize;
use std::{
    io::{Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

const TERMINAL_CHILD_CLEANUP_WAIT_TIMEOUT: Duration = Duration::from_secs(2);
const RESIZE_WAIT_TIMEOUT: Duration = Duration::from_millis(500);

pub const MAX_INPUT_FRAME_BYTES: usize = 4096;
const PTY_READ_BUFFER_BYTES: usize = 8192;
const TERMINAL_OUTPUT_FLUSH_MS: u64 = 16;
const TERMINAL_OUTPUT_MAX_BYTES: usize = 16 * 1024;
const BROWSER_TMUX_TERM: &str = "xterm-256color";
const SCROLLBACK_HOSTILE_SEQUENCES: &[&[u8]] = &[
    b"\x1b[?47h",
    b"\x1b[?47l",
    b"\x1b[?1047h",
    b"\x1b[?1047l",
    b"\x1b[?1049h",
    b"\x1b[?1049l",
    b"\x1b[?1000h",
    b"\x1b[?1000l",
    b"\x1b[?1001h",
    b"\x1b[?1001l",
    b"\x1b[?1002h",
    b"\x1b[?1002l",
    b"\x1b[?1003h",
    b"\x1b[?1003l",
    b"\x1b[?1004h",
    b"\x1b[?1004l",
    b"\x1b[?1005h",
    b"\x1b[?1005l",
    b"\x1b[?1006h",
    b"\x1b[?1006l",
    b"\x1b[?1007h",
    b"\x1b[?1007l",
    b"\x1b[3J",
];

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
            eprintln!("Ajax web terminal child cleanup task failed: {join_error}");
        }
        Err(_) => {
            eprintln!(
                "Ajax web terminal child cleanup timed out after {wait_timeout:?}; continuing websocket close"
            );
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TmuxAttachCommandPlan {
    pub program: String,
    pub args: Vec<String>,
}

pub fn tmux_attach_target(session: &str, task_window: &str) -> String {
    format!("{session}:{task_window}")
}

pub fn build_tmux_attach_command_plan(plan: &TerminalAttachPlan) -> TmuxAttachCommandPlan {
    let target = tmux_attach_target(&plan.tmux_session, &plan.task_window);
    TmuxAttachCommandPlan {
        program: "tmux".to_string(),
        args: vec!["attach-session".to_string(), "-t".to_string(), target],
    }
}

fn build_tmux_attach_command(command_plan: &TmuxAttachCommandPlan) -> CommandBuilder {
    let mut command = CommandBuilder::new(&command_plan.program);
    for arg in &command_plan.args {
        command.arg(arg);
    }
    command.env("TERM", BROWSER_TMUX_TERM);
    command
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
    /// Existing task-pane history to seed before live PTY output.
    pub history: TmuxCommand,
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
    let history_target = tmux_attach_target(&ephemeral, &plan.task_window);
    // Reuse the shared attach builder against the ephemeral session so the
    // "never attach through the browser handle" and task-window guarantees
    // are preserved for the isolated client too.
    let ephemeral_plan = TerminalAttachPlan {
        qualified_handle: plan.qualified_handle.clone(),
        tmux_session: ephemeral.clone(),
        task_window: plan.task_window.clone(),
    };
    IsolatedAttachPlan {
        setup: vec![
            TmuxCommand::new([
                "new-session",
                "-d",
                "-s",
                &ephemeral,
                "-t",
                &plan.tmux_session,
            ]),
            // Quieter status redraw on the browser-only grouped session; never
            // touch the shared task session's options.
            TmuxCommand::new(["set-option", "-t", &ephemeral, "status-interval", "5"]),
            TmuxCommand::new(["set-option", "-t", &ephemeral, "visual-activity", "off"]),
            TmuxCommand::new(["set-option", "-t", &ephemeral, "visual-bell", "off"]),
        ],
        history: TmuxCommand::new([
            "capture-pane",
            "-p",
            "-e",
            "-t",
            &history_target,
            "-S",
            // ponytail: matches the mobile Ghostty cap; raise both caps if deeper history matters.
            "-2000",
            "-E",
            "-1",
        ]),
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
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
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

/// Outcome of a parsed *text* input frame. Reported by `handle_input_frame`
/// and folded into `FrameOutcome` by `process_client_frame`. Only `InputWritten`
/// advances operator acknowledgment: resize and ignored frames do not.
#[derive(Debug)]
pub enum TextFrameOutcome {
    /// An `input` frame whose data was written to the PTY writer.
    InputWritten,
    /// A `resize` frame with positive cols/rows.
    Resize(PtySize),
    /// Anything else (parse failure, unsupported type, resize with zero size).
    Ignored,
}

/// Outcome of routing a single client WebSocket frame through the helper used
/// by both socket loops. `Resize` carries the requested PTY size for the
/// caller to apply; `Abort` requests the loop terminate; `Handled` is a no-op
/// keeper (the frame was consumed, ignored, or successfully written).
#[derive(Debug)]
pub enum FrameOutcome {
    Handled,
    Resize(PtySize),
    Abort,
}

/// Decode a JSON text frame, write any input bytes to `writer`, and report
/// whether it was an input write, a resize, or ignored. Errors abort the loop.
pub fn handle_input_frame(
    text: &str,
    writer: &mut impl Write,
) -> std::io::Result<TextFrameOutcome> {
    let frame: TerminalInputFrame = match serde_json::from_str(text) {
        Ok(frame) => frame,
        Err(_) => return Ok(TextFrameOutcome::Ignored),
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
            Ok(TextFrameOutcome::InputWritten)
        }
        "resize" if frame.cols > 0 && frame.rows > 0 => Ok(TextFrameOutcome::Resize(PtySize {
            rows: frame.rows,
            cols: frame.cols,
            pixel_width: 0,
            pixel_height: 0,
        })),
        _ => Ok(TextFrameOutcome::Ignored),
    }
}

/// Route a single client WebSocket frame through the shared input path used by
/// both socket loops: oversized binary or write error aborts; validated input
/// frames fire `on_operator_input` exactly once; resize is returned to the
/// caller; everything else is ignored. Only `Message::Text` and `Binary` are
/// expected here; other frame kinds fall back to `Handled` so the loop owns
/// their side effects (ping/pong/close) directly.
pub fn process_client_frame(
    frame: &Message,
    writer: &mut impl Write,
    on_operator_input: &Arc<dyn Fn() + Send + Sync>,
) -> FrameOutcome {
    match frame {
        Message::Binary(bytes) => {
            if bytes.len() > MAX_INPUT_FRAME_BYTES {
                return FrameOutcome::Abort;
            }
            if writer.write_all(bytes).is_err() {
                return FrameOutcome::Abort;
            }
            let _ = writer.flush();
            on_operator_input();
            FrameOutcome::Handled
        }
        Message::Text(text) => match handle_input_frame(text, writer) {
            Ok(TextFrameOutcome::InputWritten) => {
                on_operator_input();
                FrameOutcome::Handled
            }
            Ok(TextFrameOutcome::Resize(size)) => FrameOutcome::Resize(size),
            Ok(TextFrameOutcome::Ignored) => FrameOutcome::Handled,
            Err(_) => FrameOutcome::Abort,
        },
        _ => FrameOutcome::Handled,
    }
}

fn filter_scrollback_hostile_sequences(carry: &mut Vec<u8>, chunk: &[u8]) -> Vec<u8> {
    let mut buf = std::mem::take(carry);
    buf.extend_from_slice(chunk);

    let mut output = Vec::with_capacity(buf.len());
    let mut index = 0;
    while index < buf.len() {
        let rest = &buf[index..];
        if let Some(sequence) = SCROLLBACK_HOSTILE_SEQUENCES
            .iter()
            .find(|sequence| rest.starts_with(sequence))
        {
            index += sequence.len();
            continue;
        }
        if SCROLLBACK_HOSTILE_SEQUENCES
            .iter()
            .any(|sequence| sequence.len() > rest.len() && sequence.starts_with(rest))
        {
            carry.extend_from_slice(rest);
            return output;
        }
        output.push(buf[index]);
        index += 1;
    }

    output
}

/// Non-empty drained batch bytes ready for `Message::Binary` (no JSON/base64 wrap).
fn output_frame_bytes(bytes: Vec<u8>) -> Option<Vec<u8>> {
    if bytes.is_empty() {
        None
    } else {
        Some(bytes)
    }
}

/// Captured-history seed bytes for xterm: bare LF becomes CRLF so each row
/// starts at column zero. Live PTY output must keep using `output_frame_bytes`.
fn captured_history_frame_bytes(bytes: Vec<u8>) -> Option<Vec<u8>> {
    let mut normalized = Vec::with_capacity(bytes.len());
    for &byte in &bytes {
        if byte == b'\n' && normalized.last().copied() != Some(b'\r') {
            normalized.push(b'\r');
        }
        normalized.push(byte);
    }
    output_frame_bytes(normalized)
}

/// `seed=0` in a WS URL query opts out of the history seed; anything else
/// (absent query, other params, seed=1) keeps the default seed.
pub fn seed_history_from_query(query: Option<&str>) -> bool {
    query
        .map(|query| query.split('&').all(|pair| pair != "seed=0"))
        .unwrap_or(true)
}

/// How long the bridge may keep waiting for the client's first resize frame
/// before seeding anyway. Returns None when the deadline passed.
fn remaining_resize_wait(started: Instant, now: Instant) -> Option<Duration> {
    let elapsed = now.saturating_duration_since(started);
    if elapsed >= RESIZE_WAIT_TIMEOUT {
        None
    } else {
        Some(RESIZE_WAIT_TIMEOUT - elapsed)
    }
}

/// Report a bridge setup failure to the browser and close the socket.
async fn send_error_and_close(socket: &mut WebSocket, error: String) {
    let _ = socket
        .send(Message::Text(
            serde_json::json!({ "type": "error", "error": error })
                .to_string()
                .into(),
        ))
        .await;
    let _ = socket.send(Message::Close(None)).await;
}

pub async fn bridge_task_terminal_socket(
    mut socket: WebSocket,
    plan: TerminalAttachPlan,
    seed_history: bool,
    on_operator_input: Arc<dyn Fn() + Send + Sync>,
) {
    let isolated = build_isolated_attach_plan(&plan);

    // Stand up the isolated grouped session before attaching so the phone's
    // dimensions never shrink the shared window for other clients. If this
    // fails the shared session is likely gone; report and bail rather than
    // attaching to nothing.
    for command in &isolated.setup {
        let failure = match run_tmux_command_blocking(command) {
            Ok(output) if output.status.success() => continue,
            Ok(output) => String::from_utf8_lossy(&output.stderr).trim().to_string(),
            Err(error) => error.to_string(),
        };
        send_error_and_close(
            &mut socket,
            format!("failed to create terminal session: {failure}"),
        )
        .await;
        return;
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
            send_error_and_close(&mut socket, format!("failed to open PTY: {error}")).await;
            return;
        }
    };

    let command = build_tmux_attach_command(&command_plan);

    let child = match pty_pair.slave.spawn_command(command) {
        Ok(child) => child,
        Err(error) => {
            send_error_and_close(&mut socket, format!("failed to spawn tmux attach: {error}"))
                .await;
            return;
        }
    };

    let mut reader = match pty_pair.master.try_clone_reader() {
        Ok(reader) => reader,
        Err(error) => {
            cleanup_spawned_child_async(child).await;
            send_error_and_close(&mut socket, format!("failed to clone PTY reader: {error}")).await;
            return;
        }
    };
    let mut writer = match pty_pair.master.take_writer() {
        Ok(writer) => writer,
        Err(error) => {
            cleanup_spawned_child_async(child).await;
            send_error_and_close(&mut socket, format!("failed to open PTY writer: {error}")).await;
            return;
        }
    };

    let resize_wait_started = Instant::now();
    let mut resize_applied = false;
    let mut pre_loop_abort = false;
    while let Some(remaining) = remaining_resize_wait(resize_wait_started, Instant::now()) {
        match tokio::time::timeout(remaining, socket.recv()).await {
            Err(_) => break,
            Ok(None) => {
                pre_loop_abort = true;
                break;
            }
            Ok(Some(Err(_))) => {
                pre_loop_abort = true;
                break;
            }
            Ok(Some(Ok(Message::Close(_)))) => {
                pre_loop_abort = true;
                break;
            }
            Ok(Some(Ok(Message::Text(text)))) => {
                match process_client_frame(&Message::Text(text), &mut writer, &on_operator_input) {
                    FrameOutcome::Resize(size) => {
                        let _ = pty_pair.master.resize(size);
                        resize_applied = true;
                        break;
                    }
                    FrameOutcome::Abort => {
                        pre_loop_abort = true;
                        break;
                    }
                    FrameOutcome::Handled => {}
                }
            }
            Ok(Some(Ok(Message::Binary(bytes)))) => {
                match process_client_frame(&Message::Binary(bytes), &mut writer, &on_operator_input)
                {
                    FrameOutcome::Resize(size) => {
                        let _ = pty_pair.master.resize(size);
                        resize_applied = true;
                        break;
                    }
                    FrameOutcome::Abort => {
                        pre_loop_abort = true;
                        break;
                    }
                    FrameOutcome::Handled => {}
                }
            }
            Ok(Some(Ok(Message::Ping(payload)))) => {
                if socket.send(Message::Pong(payload)).await.is_err() {
                    pre_loop_abort = true;
                    break;
                }
            }
            Ok(Some(Ok(Message::Pong(_)))) => {}
        }
    }

    if pre_loop_abort {
        cleanup_spawned_child_async(child).await;
        let teardown = isolated.teardown.clone();
        let _ = tokio::task::spawn_blocking(move || {
            for command in &teardown {
                let _ = run_tmux_command_blocking(command);
            }
        })
        .await;
        let _ = socket.send(Message::Close(None)).await;
        return;
    }

    if resize_applied {
        // Fixed beat so tmux processes the WINCH and reflows history before capture.
        // ponytail: replace with an event-driven readiness check if this ever proves flaky.
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Seed history after attach starts so output produced during capture is
    // already queued in the PTY, then forward that live stream afterward.
    if seed_history {
        if let Ok(output) = run_tmux_command_blocking(&isolated.history) {
            if output.status.success() {
                if let Some(payload) = captured_history_frame_bytes(output.stdout) {
                    if socket.send(Message::Binary(payload.into())).await.is_err() {
                        cleanup_spawned_child_async(child).await;
                        for command in &isolated.teardown {
                            let _ = run_tmux_command_blocking(command);
                        }
                        return;
                    }
                }
            }
        }
    }

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

    let mut scrollback_filter_carry = Vec::new();
    let mut output_batch: Vec<u8> = Vec::new();
    let mut flush_deadline: Option<tokio::time::Instant> = None;

    loop {
        let flush_wait = match flush_deadline {
            Some(deadline) => tokio::time::sleep_until(deadline),
            None => tokio::time::sleep(Duration::from_secs(86400 * 365)),
        };
        tokio::pin!(flush_wait);

        tokio::select! {
            _ = &mut flush_wait, if flush_deadline.is_some() => {
                flush_deadline = None;
                let drained = std::mem::take(&mut output_batch);
                if let Some(payload) = output_frame_bytes(drained) {
                    if socket.send(Message::Binary(payload.into())).await.is_err() {
                        break;
                    }
                }
            }
            output = output_rx.recv() => {
                match output {
                    Some(bytes) => {
                        let filtered =
                            filter_scrollback_hostile_sequences(&mut scrollback_filter_carry, &bytes);
                        if filtered.is_empty() {
                            continue;
                        }
                        output_batch.extend_from_slice(&filtered);
                        if output_batch.len() >= TERMINAL_OUTPUT_MAX_BYTES {
                            flush_deadline = None;
                            let drained = std::mem::take(&mut output_batch);
                            if let Some(payload) = output_frame_bytes(drained) {
                                if socket.send(Message::Binary(payload.into())).await.is_err() {
                                    break;
                                }
                            }
                        } else if flush_deadline.is_none() {
                            flush_deadline = Some(
                                tokio::time::Instant::now()
                                    + Duration::from_millis(TERMINAL_OUTPUT_FLUSH_MS),
                            );
                        }
                    }
                    None => {
                        let drained = std::mem::take(&mut output_batch);
                        if let Some(payload) = output_frame_bytes(drained) {
                            let _ = socket.send(Message::Binary(payload.into())).await;
                        }
                        break;
                    }
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => match process_client_frame(
                        &Message::Text(text),
                        &mut writer,
                        &on_operator_input,
                    ) {
                        FrameOutcome::Resize(size) => {
                            let _ = pty_pair.master.resize(size);
                        }
                        FrameOutcome::Abort => break,
                        FrameOutcome::Handled => {}
                    },
                    Some(Ok(Message::Binary(bytes))) => match process_client_frame(
                        &Message::Binary(bytes),
                        &mut writer,
                        &on_operator_input,
                    ) {
                        FrameOutcome::Resize(size) => {
                            let _ = pty_pair.master.resize(size);
                        }
                        FrameOutcome::Abort => break,
                        FrameOutcome::Handled => {}
                    },
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

#[cfg(test)]
pub(crate) async fn simulate_terminal_disconnect_cleanup_for_tests(wait_timeout: Duration) {
    let (child, _release) = tests::MockChild::gated();
    cleanup_spawned_child_async_with_timeout(child, wait_timeout).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn attach_plan(handle: &str) -> TerminalAttachPlan {
        TerminalAttachPlan {
            qualified_handle: handle.to_string(),
            tmux_session: "ajax-web-fix-login".to_string(),
            task_window: "task".to_string(),
        }
    }

    /// One configurable stand-in for a spawned PTY child: records kill/wait
    /// calls and, when gated, blocks `wait_child` until the returned sender is
    /// dropped or signalled.
    #[derive(Clone)]
    pub(crate) struct MockChild {
        killed: Arc<Mutex<bool>>,
        waited: Arc<Mutex<bool>>,
        wait_gate: Option<Arc<Mutex<std::sync::mpsc::Receiver<()>>>>,
    }

    impl MockChild {
        fn instant() -> Self {
            Self {
                killed: Arc::new(Mutex::new(false)),
                waited: Arc::new(Mutex::new(false)),
                wait_gate: None,
            }
        }

        pub(crate) fn gated() -> (Self, std::sync::mpsc::Sender<()>) {
            let (release, receiver) = std::sync::mpsc::channel();
            let mut child = Self::instant();
            child.wait_gate = Some(Arc::new(Mutex::new(receiver)));
            (child, release)
        }
    }

    impl TerminalChild for MockChild {
        fn kill_child(&mut self) -> std::io::Result<()> {
            *self.killed.lock().unwrap() = true;
            Ok(())
        }

        fn wait_child(&mut self) -> std::io::Result<()> {
            if let Some(gate) = &self.wait_gate {
                let receiver = gate.lock().unwrap();
                let _ = receiver.recv();
            }
            *self.waited.lock().unwrap() = true;
            Ok(())
        }
    }

    #[test]
    fn tmux_attach_command_plan_uses_registered_session_and_task_target() {
        let plan = attach_plan("web/fix-login");

        let command_plan = build_tmux_attach_command_plan(&plan);

        assert_eq!(command_plan.program, "tmux");
        assert_eq!(
            command_plan.args,
            vec![
                "attach-session".to_string(),
                "-t".to_string(),
                "ajax-web-fix-login:task".to_string(),
            ]
        );
        assert!(!command_plan
            .args
            .iter()
            .any(|arg| arg.contains("web/fix-login")));
    }

    #[test]
    fn tmux_attach_target_never_uses_browser_handle() {
        let plan = attach_plan("web/evil-handle");

        let command_plan = build_tmux_attach_command_plan(&plan);

        assert_eq!(command_plan.args[2], "ajax-web-fix-login:task");
        assert!(!command_plan
            .args
            .iter()
            .any(|arg| arg.contains("evil-handle")));
    }

    #[test]
    fn tmux_attach_command_uses_clear_capable_terminal_type() {
        let plan = attach_plan("web/fix-login");
        let command_plan = build_tmux_attach_command_plan(&plan);

        let command = build_tmux_attach_command(&command_plan);

        assert_eq!(
            command.get_env("TERM"),
            Some(std::ffi::OsStr::new("xterm-256color"))
        );
    }

    #[test]
    fn isolated_attach_plan_creates_grouped_session_then_attaches() {
        let plan = attach_plan("web/fix-login");

        let isolated = build_isolated_attach_plan_with_token(&plan, "1a2b3c");
        let ephemeral = "ajax-web-fix-login-m1a2b3c";

        assert_eq!(isolated.ephemeral_session, ephemeral);
        // A grouped session shares the shared session's windows but keeps an
        // independent size, so the phone never shrinks the shared window.
        // Quieter status options target the ephemeral session only.
        assert_eq!(
            isolated.setup,
            vec![
                TmuxCommand {
                    program: "tmux".to_string(),
                    args: vec![
                        "new-session".to_string(),
                        "-d".to_string(),
                        "-s".to_string(),
                        ephemeral.to_string(),
                        "-t".to_string(),
                        "ajax-web-fix-login".to_string(),
                    ],
                },
                TmuxCommand {
                    program: "tmux".to_string(),
                    args: vec![
                        "set-option".to_string(),
                        "-t".to_string(),
                        ephemeral.to_string(),
                        "status-interval".to_string(),
                        "5".to_string(),
                    ],
                },
                TmuxCommand {
                    program: "tmux".to_string(),
                    args: vec![
                        "set-option".to_string(),
                        "-t".to_string(),
                        ephemeral.to_string(),
                        "visual-activity".to_string(),
                        "off".to_string(),
                    ],
                },
                TmuxCommand {
                    program: "tmux".to_string(),
                    args: vec![
                        "set-option".to_string(),
                        "-t".to_string(),
                        ephemeral.to_string(),
                        "visual-bell".to_string(),
                        "off".to_string(),
                    ],
                },
            ]
        );
        let set_option_targets: Vec<&str> = isolated
            .setup
            .iter()
            .filter(|cmd| cmd.args.first().map(String::as_str) == Some("set-option"))
            .filter_map(|cmd| {
                cmd.args
                    .windows(2)
                    .find(|pair| pair[0] == "-t")
                    .map(|pair| pair[1].as_str())
            })
            .collect();
        assert_eq!(set_option_targets, vec![ephemeral, ephemeral, ephemeral]);
        assert!(!set_option_targets.contains(&"ajax-web-fix-login"));
        // Attach targets the ephemeral session's task window, never the
        // browser handle and never the shared session directly.
        assert_eq!(
            isolated.attach.args,
            vec![
                "attach-session".to_string(),
                "-t".to_string(),
                format!("{ephemeral}:task"),
            ]
        );
        assert!(!isolated
            .attach
            .args
            .iter()
            .any(|arg| arg == "ajax-web-fix-login:task"));
        assert!(!isolated
            .attach
            .args
            .iter()
            .any(|arg| arg.contains("web/fix-login")));
    }

    #[test]
    fn seed_history_query_parsing() {
        assert!(seed_history_from_query(None));
        assert!(seed_history_from_query(Some("")));
        assert!(!seed_history_from_query(Some("seed=0")));
        assert!(!seed_history_from_query(Some("a=b&seed=0")));
        assert!(seed_history_from_query(Some("seed=1")));
        assert!(seed_history_from_query(Some("seed=00")));
    }

    #[test]
    fn remaining_resize_wait_deadline() {
        let started = Instant::now();
        assert_eq!(
            remaining_resize_wait(started, started),
            Some(Duration::from_millis(500))
        );
        assert_eq!(
            remaining_resize_wait(started, started + Duration::from_millis(499)),
            Some(Duration::from_millis(1))
        );
        assert_eq!(
            remaining_resize_wait(started, started + Duration::from_millis(500)),
            None
        );
        assert_eq!(
            remaining_resize_wait(started, started + Duration::from_millis(501)),
            None
        );
    }

    #[test]
    fn isolated_attach_plan_seeds_browser_scrollback_from_task_window() {
        let plan = attach_plan("web/fix-login");

        let isolated = build_isolated_attach_plan_with_token(&plan, "1a2b3c");

        assert_eq!(isolated.history.program, "tmux");
        assert_eq!(
            isolated.history.args,
            vec![
                "capture-pane",
                "-p",
                "-e",
                "-t",
                "ajax-web-fix-login-m1a2b3c:task",
                "-S",
                "-2000",
                "-E",
                "-1",
            ]
        );
        assert!(!isolated
            .history
            .args
            .iter()
            .any(|arg| arg.contains("web/fix-login")));
    }

    #[test]
    fn history_capture_preserves_display_wrapping() {
        let plan = attach_plan("web/fix-login");
        let isolated = build_isolated_attach_plan_with_token(&plan, "1a2b3c");
        // Display-row capture must match the browser's wrap width; -J joins
        // logical lines and re-wraps badly after seed.
        assert!(!isolated.history.args.contains(&"-J".to_string()));
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
        let plan = attach_plan("web/fix-login");

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
        let plan = attach_plan("web/fix-login");

        let first = build_isolated_attach_plan(&plan).ephemeral_session;
        let second = build_isolated_attach_plan(&plan).ephemeral_session;

        assert_ne!(first, second);
        assert_ne!(first, "ajax-web-fix-login");
        assert!(first.starts_with("ajax-web-fix-login-m"));
    }

    #[test]
    fn terminal_output_flush_constants_match_targets() {
        assert_eq!(TERMINAL_OUTPUT_FLUSH_MS, 16);
        assert_eq!(TERMINAL_OUTPUT_MAX_BYTES, 16 * 1024);
    }

    #[test]
    fn terminal_output_frame_bytes_returns_raw_bytes_for_binary_send() {
        let bytes = output_frame_bytes(b"hello".to_vec()).expect("non-empty bytes");
        assert_eq!(bytes, b"hello");
        // Live path sends Message::Binary(raw); must not base64-wrap or JSON-wrap.
        assert!(!String::from_utf8_lossy(&bytes).contains("\"type\""));
        assert!(!String::from_utf8_lossy(&bytes).contains("output"));
        assert!(output_frame_bytes(Vec::new()).is_none());
    }

    #[test]
    fn captured_history_frame_bytes_converts_lf_to_crlf_without_doubling_crlf() {
        // Mixed ANSI, bare LF, CRLF, consecutive bare LF, and lone CR.
        let input = b"\x1b[31mred\x1b[0m\ncrlf\r\n\n\rkeep".to_vec();
        let out = captured_history_frame_bytes(input).expect("non-empty history");
        assert_eq!(out, b"\x1b[31mred\x1b[0m\r\ncrlf\r\n\r\n\rkeep");
        // Bare LF -> CRLF; existing CRLF stays one CRLF; consecutive lines start at col 0.
        assert_eq!(&out[12..14], b"\r\n");
        assert_eq!(&out[18..20], b"\r\n");
        assert_eq!(&out[20..22], b"\r\n");
        assert_eq!(out[22], b'\r');
        assert!(captured_history_frame_bytes(Vec::new()).is_none());
    }

    #[test]
    fn filter_scrollback_hostile_sequences_strips_targets_and_carries_split_sequences() {
        let mut carry = Vec::new();
        let output = filter_scrollback_hostile_sequences(
            &mut carry,
            b"\x1b[?1049h\x1b[55;1Hdialog\x1b[3J\x1b[?1006h\x1b[?1049l",
        );
        assert_eq!(output, b"\x1b[55;1Hdialog");
        assert!(carry.is_empty());

        let mut carry = Vec::new();
        assert_eq!(
            filter_scrollback_hostile_sequences(&mut carry, b"\x1b[2J\x1b[J\x1b[12;4Hhi"),
            b"\x1b[2J\x1b[J\x1b[12;4Hhi"
        );
        assert!(carry.is_empty());

        let mut carry = Vec::new();
        assert_eq!(
            filter_scrollback_hostile_sequences(&mut carry, b"pre\x1b[?104"),
            b"pre"
        );
        assert!(!carry.is_empty());
        assert_eq!(
            filter_scrollback_hostile_sequences(&mut carry, b"9hpost"),
            b"post"
        );
        assert!(carry.is_empty());

        let mut carry = Vec::new();
        assert_eq!(
            filter_scrollback_hostile_sequences(&mut carry, b"\x1b[?104"),
            b""
        );
        assert_eq!(
            filter_scrollback_hostile_sequences(&mut carry, b"8hX"),
            b"\x1b[?1048hX"
        );
        assert!(carry.is_empty());
    }

    #[test]
    fn filter_strips_hostile_sequences_fed_one_byte_at_a_time_without_prefix_leaks() {
        // A PTY read can split an escape sequence at any byte. Feeding the
        // stream byte-by-byte is the worst case: every hostile sequence must
        // still vanish completely and every normal byte must still come out.
        let stream: &[u8] = b"a\x1b[?1049h\x1b[2Jb\x1b[?1002lc\x1b[3J\x1b[31md";
        let mut carry = Vec::new();
        let mut output = Vec::new();
        for byte in stream {
            output.extend(filter_scrollback_hostile_sequences(&mut carry, &[*byte]));
        }
        assert_eq!(output, b"a\x1b[2Jbc\x1b[31md");
        assert!(carry.is_empty());
    }

    fn counting_sink() -> (
        std::sync::Arc<std::sync::atomic::AtomicUsize>,
        std::sync::Arc<dyn Fn() + Send + Sync>,
    ) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let counter = std::sync::Arc::new(AtomicUsize::new(0));
        let counter_clone = std::sync::Arc::clone(&counter);
        let sink: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });
        (counter, sink)
    }

    #[test]
    fn handle_input_frame_accepts_resize_without_data() {
        let mut writer: Vec<u8> = Vec::new();

        let outcome = handle_input_frame(r#"{"type":"resize","cols":132,"rows":40}"#, &mut writer)
            .expect("resize frame should parse");
        let size = match outcome {
            TextFrameOutcome::Resize(size) => size,
            _ => panic!("resize frame should return a pty size"),
        };

        assert_eq!(size.cols, 132);
        assert_eq!(size.rows, 40);
    }

    #[test]
    fn process_client_frame_fires_sink_once_for_binary_input_within_limit() {
        let (counter, sink) = counting_sink();
        let mut writer: Vec<u8> = Vec::new();
        let frame = Message::Binary(axum::body::Bytes::from(b"hello".to_vec()));

        let outcome = process_client_frame(&frame, &mut writer, &sink);

        assert!(matches!(outcome, FrameOutcome::Handled), "{outcome:?}");
        assert_eq!(writer, b"hello");
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn process_client_frame_fires_sink_once_for_text_input_frame() {
        let (counter, sink) = counting_sink();
        let mut writer: Vec<u8> = Vec::new();
        let frame = Message::Text(r#"{"type":"input","data":"x"}"#.into());

        let outcome = process_client_frame(&frame, &mut writer, &sink);

        assert!(matches!(outcome, FrameOutcome::Handled), "{outcome:?}");
        assert_eq!(writer, b"x");
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn process_client_frame_resize_does_not_fire_sink() {
        let (counter, sink) = counting_sink();
        let mut writer: Vec<u8> = Vec::new();
        let frame = Message::Text(r#"{"type":"resize","cols":80,"rows":24}"#.into());

        let outcome = process_client_frame(&frame, &mut writer, &sink);

        match outcome {
            FrameOutcome::Resize(size) => {
                assert_eq!(size.cols, 80);
                assert_eq!(size.rows, 24);
            }
            _ => panic!("expected resize outcome, got {outcome:?}"),
        }
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(writer.is_empty());
    }

    #[test]
    fn process_client_frame_malformed_text_does_not_fire_sink() {
        let (counter, sink) = counting_sink();
        let mut writer: Vec<u8> = Vec::new();
        let frame = Message::Text("not json".into());

        let outcome = process_client_frame(&frame, &mut writer, &sink);

        assert!(matches!(outcome, FrameOutcome::Handled), "{outcome:?}");
        assert!(writer.is_empty());
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn process_client_frame_oversized_binary_aborts_without_firing_sink() {
        let (counter, sink) = counting_sink();
        let mut writer: Vec<u8> = Vec::new();
        let big = vec![b'a'; MAX_INPUT_FRAME_BYTES + 1];
        let frame = Message::Binary(axum::body::Bytes::from(big));

        let outcome = process_client_frame(&frame, &mut writer, &sink);

        assert!(matches!(outcome, FrameOutcome::Abort), "{outcome:?}");
        assert!(writer.is_empty());
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn cleanup_spawned_child_kills_and_waits() {
        let child = MockChild::instant();
        let killed = Arc::clone(&child.killed);
        let waited = Arc::clone(&child.waited);

        cleanup_spawned_child(child);

        assert!(*killed.lock().unwrap());
        assert!(*waited.lock().unwrap());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn terminal_cleanup_runs_wait_on_blocking_task() {
        let (child, release_tx) = MockChild::gated();
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

    #[tokio::test]
    async fn terminal_cleanup_does_not_wait_forever_after_kill() {
        // The release sender is held for the whole test, so wait_child never
        // completes on its own; only the cleanup timeout can end it.
        let (child, _release) = MockChild::gated();
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

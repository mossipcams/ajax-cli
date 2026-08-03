//! Terminal WebSocket frame handling and PTY bridge loop.

use super::*;
use axum::extract::ws::{Message, WebSocket};
use portable_pty::{native_pty_system, PtySize};
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

pub(crate) fn filter_scrollback_hostile_sequences(carry: &mut Vec<u8>, chunk: &[u8]) -> Vec<u8> {
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
pub(crate) fn output_frame_bytes(bytes: Vec<u8>) -> Option<Vec<u8>> {
    if bytes.is_empty() {
        None
    } else {
        Some(bytes)
    }
}

/// Captured-history seed bytes for xterm: bare LF becomes CRLF so each row
/// starts at column zero. Live PTY output must keep using `output_frame_bytes`.
pub(crate) fn captured_history_frame_bytes(bytes: Vec<u8>) -> Option<Vec<u8>> {
    let mut normalized = Vec::with_capacity(bytes.len());
    for &byte in &bytes {
        if byte == b'\n' && normalized.last().copied() != Some(b'\r') {
            normalized.push(b'\r');
        }
        normalized.push(byte);
    }
    if normalized.is_empty() {
        return None;
    }
    Some(normalized)
}

/// `seed=0` in a WS URL query opts out of the history seed; anything else
/// (absent query, other params, seed=1) keeps the default seed.
pub fn seed_history_from_query(query: Option<&str>) -> bool {
    query
        .map(|query| query.split('&').all(|pair| pair != "seed=0"))
        .unwrap_or(true)
}

/// Allowlist for a `client=` token in the WS URL query. Only
/// `[A-Za-z0-9_-]{1,64}` is accepted so the token never injects tmux name
/// metacharacters; the token is hashed anyway but the gate keeps bad input
/// from even reaching the hash. Anything else (absent, empty, too long,
/// characters outside the allowlist) returns `None` so the bridge falls back
/// to a random per-call [`build_isolated_attach_plan`].
pub fn client_id_from_query(query: Option<&str>) -> Option<String> {
    let query = query?;
    for pair in query.split('&') {
        if let Some(rest) = pair.strip_prefix("client=") {
            if rest.is_empty() {
                continue;
            }
            if rest.len() > 64 {
                continue;
            }
            if !rest
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
            {
                continue;
            }
            return Some(rest.to_string());
        }
    }
    None
}

/// Select the isolated attach plan for a bridge connection. A present, validated
/// client id routes through [`build_isolated_attach_plan_for_client`] so a
/// reconnecting tab reuses its tmux viewport; `None` keeps the historical
/// random-per-call [`build_isolated_attach_plan`] path.
pub fn isolated_plan_for_bridge(
    plan: &TerminalAttachPlan,
    client_id: Option<&str>,
) -> IsolatedAttachPlan {
    match client_id {
        Some(id) => build_isolated_attach_plan_for_client(plan, id),
        None => build_isolated_attach_plan(plan),
    }
}

/// Fixed reflow beat before history capture is only worth paying when we will
/// actually seed history *and* a client resize already fired a WINCH that tmux
/// still needs to reflow. Unseeded auto-reconnect must skip the 100ms sleep.
pub fn should_wait_reflow_before_seed(seed_history: bool, resize_applied: bool) -> bool {
    seed_history && resize_applied
}

/// How long the bridge may keep waiting for the client's first resize frame
/// before seeding anyway. Returns None when the deadline passed.
pub(crate) fn remaining_resize_wait(started: Instant, now: Instant) -> Option<Duration> {
    let elapsed = now.saturating_duration_since(started);
    if elapsed >= RESIZE_WAIT_TIMEOUT {
        None
    } else {
        Some(RESIZE_WAIT_TIMEOUT - elapsed)
    }
}

/// Remaining quiet time after the last client resize before seeding. Returns
/// `None` once the settle window has elapsed.
pub(crate) fn resize_settle_deadline(last_resize_at: Instant, now: Instant) -> Option<Duration> {
    let elapsed = now.saturating_duration_since(last_resize_at);
    if elapsed >= RESIZE_SETTLE_QUIET {
        None
    } else {
        Some(RESIZE_SETTLE_QUIET - elapsed)
    }
}

/// Report a bridge setup failure to the browser and close the socket.
pub(crate) async fn send_error_and_close(socket: &mut WebSocket, error: String) {
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
    client_id: Option<String>,
    on_operator_input: Arc<dyn Fn() + Send + Sync>,
) {
    // ponytail: reap only detached ephemerals (attached==0). Ceiling: one
    // list-sessions per connect. Upgrade: periodic background reaper if connect
    // rate is too low to keep up.
    let _ = tokio::task::spawn_blocking(reap_detached_ephemeral_terminal_sessions).await;

    let isolated = isolated_plan_for_bridge(&plan, client_id.as_deref());

    // Stand up the isolated grouped session before attaching so the phone's
    // dimensions never shrink the shared window for other clients. If this
    // fails the shared session is likely gone; report and bail rather than
    // attaching to nothing.
    for command in &isolated.setup {
        let failure = match run_tmux_command_blocking(command) {
            Ok(output) if output.status.success() => continue,
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if should_ignore_setup_failure(command, stderr.trim()) {
                    continue;
                }
                stderr.trim().to_string()
            }
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
    let mut last_resize_at: Option<Instant> = None;
    let mut pre_loop_abort = false;
    while let Some(overall_remaining) = remaining_resize_wait(resize_wait_started, Instant::now()) {
        let now = Instant::now();
        if let Some(last) = last_resize_at {
            if resize_settle_deadline(last, now).is_none() {
                break;
            }
        }
        let wait = match last_resize_at {
            Some(last) => overall_remaining
                .min(resize_settle_deadline(last, now).expect("settle still armed")),
            None => overall_remaining,
        };
        match tokio::time::timeout(wait, socket.recv()).await {
            Err(_) => {
                let now = Instant::now();
                if let Some(last) = last_resize_at {
                    if resize_settle_deadline(last, now).is_none() {
                        break;
                    }
                    continue;
                }
                break;
            }
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
                        last_resize_at = Some(Instant::now());
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
                        last_resize_at = Some(Instant::now());
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

    if should_wait_reflow_before_seed(seed_history, resize_applied) {
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

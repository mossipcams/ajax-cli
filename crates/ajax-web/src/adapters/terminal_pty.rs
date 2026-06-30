//! PTY-backed tmux attach for the browser task terminal bridge.

use crate::slices::terminal::TerminalAttachPlan;
use axum::extract::ws::{Message, WebSocket};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::Deserialize;
use std::{
    io::{Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::mpsc;

pub const MAX_INPUT_FRAME_BYTES: usize = 4096;
const PTY_READ_BUFFER_BYTES: usize = 8192;

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

#[derive(Debug, Deserialize)]
struct TerminalInputFrame {
    #[serde(rename = "type")]
    frame_type: String,
    data: String,
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
    let command_plan = build_tmux_attach_command_plan(&plan);
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

    let mut child = match pty_pair.slave.spawn_command(command) {
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
    let _ = child.kill();
    let _ = child.wait();
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
            if frame.data.len() > MAX_INPUT_FRAME_BYTES {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "input frame too large",
                ));
            }
            writer.write_all(frame.data.as_bytes())?;
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
mod tests {
    use super::*;

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
}

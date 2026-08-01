//! Supervised local STT provider boundary and Moonshine command adapter.
//!
//! Transport-agnostic: no WebSocket, PTY, or task-registry coupling. Model-
//! specific process launch stays behind [`MoonshineProvider`].

use serde::Deserialize;
use std::{
    io::{self, BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        mpsc::{sync_channel, Receiver, SyncSender, TryRecvError, TrySendError},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

/// Maximum PCM16 payload bytes in one sidecar audio frame.
pub const MAX_SIDECAR_AUDIO_PCM_BYTES: usize = 640;

const SIDECAR_FRAME_KIND_START: u8 = 0;
const SIDECAR_FRAME_KIND_AUDIO: u8 = 1;
const SIDECAR_FRAME_KIND_FINALIZE: u8 = 2;
const SIDECAR_EVENT_QUEUE_BOUND: usize = 64;

/// Session parameters handed to a provider when speech capture begins.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderSessionConfig {
    pub session_id: String,
    pub sample_rate: u32,
    pub channels: u32,
    pub language: String,
    pub phrase_end_silence_ms: u64,
}

/// Provider availability for health probes and UI gating.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderHealth {
    Available,
    Unavailable(String),
}

/// Recoverable provider failures. Never panic for missing or unusable commands.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderError {
    Unavailable(String),
    StartupFailed(String),
    AudioBufferOverflow,
    SessionClosed,
    Protocol(String),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(message) => write!(formatter, "stt provider unavailable: {message}"),
            Self::StartupFailed(message) => {
                write!(formatter, "stt provider startup failed: {message}")
            }
            Self::AudioBufferOverflow => write!(formatter, "stt audio buffer overflow"),
            Self::SessionClosed => write!(formatter, "stt provider session already closed"),
            Self::Protocol(message) => write!(formatter, "stt provider protocol error: {message}"),
        }
    }
}

impl std::error::Error for ProviderError {}

/// Typed provider-side transcript and activity events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderEvent {
    Partial { sequence: u32, text: String },
    Final { sequence: u32, text: String },
    SpeechStarted,
    SpeechEnded,
    Error { message: String },
}

impl ProviderEvent {
    pub fn sequence(&self) -> Option<u32> {
        match self {
            Self::Partial { sequence, .. } | Self::Final { sequence, .. } => Some(*sequence),
            Self::SpeechStarted | Self::SpeechEnded | Self::Error { .. } => None,
        }
    }
}

/// Encode one sidecar audio frame: kind `1`, big-endian sequence, raw PCM16.
pub fn encode_sidecar_audio_frame(sequence: u32, pcm: &[u8]) -> Result<Vec<u8>, ProviderError> {
    if pcm.len() > MAX_SIDECAR_AUDIO_PCM_BYTES {
        return Err(ProviderError::AudioBufferOverflow);
    }
    // Length-prefixed like the start frame: without it the sidecar cannot tell
    // where this frame's PCM ends and the next frame begins.
    let len = u32::try_from(pcm.len())
        .map_err(|_| ProviderError::Protocol("audio frame payload too large".to_string()))?;
    let mut frame = Vec::with_capacity(1 + 4 + 4 + pcm.len());
    frame.push(SIDECAR_FRAME_KIND_AUDIO);
    frame.extend_from_slice(&sequence.to_be_bytes());
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(pcm);
    Ok(frame)
}

fn encode_sidecar_start_frame(config: &ProviderSessionConfig) -> Result<Vec<u8>, ProviderError> {
    let payload = serde_json::json!({
        "sessionId": config.session_id,
        "sampleRate": config.sample_rate,
        "channels": config.channels,
        "language": config.language,
        "phraseEndSilenceMs": config.phrase_end_silence_ms,
    });
    let body = serde_json::to_vec(&payload).map_err(|error| {
        ProviderError::Protocol(format!("failed to encode start frame: {error}"))
    })?;
    let len = u32::try_from(body.len())
        .map_err(|_| ProviderError::Protocol("start frame payload too large".to_string()))?;
    let mut frame = Vec::with_capacity(1 + 4 + body.len());
    frame.push(SIDECAR_FRAME_KIND_START);
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(&body);
    Ok(frame)
}

fn encode_sidecar_finalize_frame() -> Vec<u8> {
    vec![SIDECAR_FRAME_KIND_FINALIZE]
}

#[derive(Debug, Deserialize)]
struct SidecarEventLine {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    sequence: Option<u32>,
    #[serde(default)]
    text: Option<String>,
}

/// Parse one newline-delimited sidecar JSON event line.
pub fn parse_sidecar_event_line(line: &[u8]) -> Result<ProviderEvent, ProviderError> {
    let parsed: SidecarEventLine = serde_json::from_slice(line)
        .map_err(|error| ProviderError::Protocol(format!("invalid sidecar event JSON: {error}")))?;
    match parsed.event_type.as_str() {
        "stt.partial" => Ok(ProviderEvent::Partial {
            sequence: parsed.sequence.ok_or_else(|| {
                ProviderError::Protocol("stt.partial missing sequence".to_string())
            })?,
            text: parsed
                .text
                .ok_or_else(|| ProviderError::Protocol("stt.partial missing text".to_string()))?,
        }),
        "stt.final" => Ok(ProviderEvent::Final {
            sequence: parsed
                .sequence
                .ok_or_else(|| ProviderError::Protocol("stt.final missing sequence".to_string()))?,
            text: parsed
                .text
                .ok_or_else(|| ProviderError::Protocol("stt.final missing text".to_string()))?,
        }),
        "stt.speech_started" => Ok(ProviderEvent::SpeechStarted),
        "stt.speech_ended" => Ok(ProviderEvent::SpeechEnded),
        other => Err(ProviderError::Protocol(format!(
            "unknown sidecar event type `{other}`"
        ))),
    }
}

fn spawn_frame_writer(stdin: ChildStdin, rx: Receiver<Vec<u8>>) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut stdin = stdin;
        while let Ok(frame) = rx.recv() {
            if stdin.write_all(&frame).is_err() {
                break;
            }
            if stdin.flush().is_err() {
                break;
            }
        }
    })
}

fn spawn_event_reader(
    stdout: impl Read + Send + 'static,
    tx: SyncSender<ProviderEvent>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = Vec::new();
        loop {
            line.clear();
            match reader.read_until(b'\n', &mut line) {
                Ok(0) => break,
                Ok(_) => {
                    while line
                        .last()
                        .is_some_and(|byte| matches!(byte, b'\n' | b'\r'))
                    {
                        line.pop();
                    }
                    if line.is_empty() {
                        continue;
                    }
                    let event = match parse_sidecar_event_line(&line) {
                        Ok(event) => event,
                        Err(error) => ProviderEvent::Error {
                            message: error.to_string(),
                        },
                    };
                    if tx.send(event).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    })
}

fn split_command_line(command: &str) -> Result<(String, Vec<String>), ProviderError> {
    let mut parts = command.split_whitespace();
    let program = parts
        .next()
        .ok_or_else(|| ProviderError::StartupFailed("provider command is empty".to_string()))?;
    Ok((program.to_string(), parts.map(str::to_string).collect()))
}

fn spawn_provider_command(command: &str) -> Result<Child, ProviderError> {
    let (program, args) = split_command_line(command)?;
    Command::new(&program)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| map_spawn_error(&program, error))
}

fn map_spawn_error(program: &str, error: io::Error) -> ProviderError {
    ProviderError::StartupFailed(format!(
        "failed to start STT provider command `{program}`: {error}"
    ))
}

/// Supervised local Moonshine Small Streaming sidecar adapter.
pub struct MoonshineProvider {
    command: Option<String>,
    max_buffered_audio_ms: u64,
    phrase_end_silence_ms: u64,
    shut_down: bool,
}

impl MoonshineProvider {
    pub fn new(
        command: Option<String>,
        max_buffered_audio_ms: u64,
        phrase_end_silence_ms: u64,
    ) -> Self {
        Self {
            command,
            max_buffered_audio_ms,
            phrase_end_silence_ms,
            shut_down: false,
        }
    }

    pub fn health(&self) -> ProviderHealth {
        if self.shut_down {
            return ProviderHealth::Unavailable("provider shut down".to_string());
        }
        match &self.command {
            Some(_) => ProviderHealth::Available,
            None => ProviderHealth::Unavailable("no STT provider command configured".to_string()),
        }
    }

    pub fn start_session(
        &self,
        mut config: ProviderSessionConfig,
    ) -> Result<MoonshineSession, ProviderError> {
        if self.shut_down {
            return Err(ProviderError::Unavailable("provider shut down".to_string()));
        }
        config.phrase_end_silence_ms = self.phrase_end_silence_ms;
        let command = self.command.as_ref().ok_or_else(|| {
            ProviderError::Unavailable("no STT provider command configured".to_string())
        })?;
        let mut child = spawn_provider_command(command)?;
        let stdin = child.stdin.take().ok_or_else(|| {
            ProviderError::StartupFailed("provider stdin pipe unavailable".to_string())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            ProviderError::StartupFailed("provider stdout pipe unavailable".to_string())
        })?;
        let (tx, rx) = sync_channel(SIDECAR_EVENT_QUEUE_BOUND);
        let reader = spawn_event_reader(stdout, tx);
        let frame_capacity = (self.max_buffered_audio_ms / 20).max(1) as usize;
        let (frame_tx, frame_rx) = sync_channel(frame_capacity);
        let writer = spawn_frame_writer(stdin, frame_rx);
        let mut session = MoonshineSession {
            session_id: config.session_id.clone(),
            child: Some(child),
            frame_tx: Some(frame_tx),
            events: rx,
            reader: Some(reader),
            writer: Some(writer),
            next_sequence: 0,
            finalizing: false,
            closed: false,
            sidecar_ended: false,
        };
        session.write_frame(&encode_sidecar_start_frame(&config)?)?;
        Ok(session)
    }

    pub fn shutdown(&mut self) {
        self.shut_down = true;
    }
}

/// One Moonshine-backed speech session with a finite outbound frame queue.
pub struct MoonshineSession {
    session_id: String,
    child: Option<Child>,
    frame_tx: Option<SyncSender<Vec<u8>>>,
    events: Receiver<ProviderEvent>,
    reader: Option<JoinHandle<()>>,
    writer: Option<JoinHandle<()>>,
    next_sequence: u32,
    /// True after an idempotent finalize signal; child stays up for event drain.
    finalizing: bool,
    /// True only after cancel/Drop tears down the supervised child.
    closed: bool,
    /// Latches one `stt sidecar exited` error after the event reader disconnects.
    sidecar_ended: bool,
}

impl MoonshineSession {
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn push_audio(&mut self, pcm: Vec<u8>) -> Result<(), ProviderError> {
        if self.closed || self.finalizing {
            return Err(ProviderError::SessionClosed);
        }
        let frame = encode_sidecar_audio_frame(self.next_sequence, &pcm)?;
        self.write_frame(&frame)?;
        self.next_sequence = self.next_sequence.wrapping_add(1);
        Ok(())
    }

    pub fn poll_event(&mut self) -> Option<ProviderEvent> {
        match self.events.try_recv() {
            Ok(event) => Some(event),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                if self.sidecar_ended {
                    None
                } else {
                    self.sidecar_ended = true;
                    Some(ProviderEvent::Error {
                        message: "stt sidecar exited".to_string(),
                    })
                }
            }
        }
    }

    pub fn finalize(&mut self) -> Result<(), ProviderError> {
        if self.closed || self.finalizing {
            return Ok(());
        }
        self.write_frame(&encode_sidecar_finalize_frame())?;
        self.finalizing = true;
        Ok(())
    }

    pub fn cancel(&mut self) {
        if self.closed {
            return;
        }
        while self.events.try_recv().is_ok() {}
        self.frame_tx.take();
        self.stop_child();
        self.finalizing = false;
        self.closed = true;
    }

    fn write_frame(&mut self, frame: &[u8]) -> Result<(), ProviderError> {
        let tx = self.frame_tx.as_ref().ok_or(ProviderError::SessionClosed)?;
        tx.try_send(frame.to_vec()).map_err(|error| match error {
            TrySendError::Full(_) => ProviderError::AudioBufferOverflow,
            TrySendError::Disconnected(_) => ProviderError::SessionClosed,
        })
    }

    fn stop_child(&mut self) {
        self.frame_tx.take();
        // Kill before joining the writer: a live-but-wedged sidecar leaves the
        // writer blocked inside write_all on a full pipe, and dropping the sender
        // cannot wake it. Killing closes the pipe so the write fails and it exits.
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(handle) = self.writer.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.reader.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for MoonshineSession {
    fn drop(&mut self) {
        self.cancel();
    }
}

const MAX_STT_CONTROL_BYTES: usize = 8_192;
const MAX_STT_BINARY_BYTES: usize = 4 + crate::slices::stt::MAX_AUDIO_FRAME_BYTES;
const STT_EVENT_POLL_MS: u64 = 20;

fn provider_event_to_server(
    session_id: &str,
    event: ProviderEvent,
) -> crate::slices::stt::SttServerEvent {
    use crate::slices::stt::{SttServerEvent, STT_PROTOCOL_VERSION};
    match event {
        ProviderEvent::Partial { sequence, text } => SttServerEvent::Partial {
            version: STT_PROTOCOL_VERSION,
            session_id: session_id.to_string(),
            sequence,
            text,
        },
        ProviderEvent::Final { sequence, text } => SttServerEvent::Final {
            version: STT_PROTOCOL_VERSION,
            session_id: session_id.to_string(),
            sequence,
            text,
        },
        ProviderEvent::SpeechStarted => SttServerEvent::SpeechStarted {
            version: STT_PROTOCOL_VERSION,
            session_id: session_id.to_string(),
        },
        ProviderEvent::SpeechEnded => SttServerEvent::SpeechEnded {
            version: STT_PROTOCOL_VERSION,
            session_id: session_id.to_string(),
        },
        ProviderEvent::Error { message } => SttServerEvent::Error {
            version: STT_PROTOCOL_VERSION,
            session_id: session_id.to_string(),
            code: "provider_error".to_string(),
            message,
        },
    }
}

async fn send_stt_event(
    socket: &mut axum::extract::ws::WebSocket,
    event: &crate::slices::stt::SttServerEvent,
) -> bool {
    match serde_json::to_string(event) {
        Ok(payload) => socket
            .send(axum::extract::ws::Message::Text(payload.into()))
            .await
            .is_ok(),
        Err(_) => false,
    }
}

async fn send_stt_error(
    socket: &mut axum::extract::ws::WebSocket,
    session_id: &str,
    code: &str,
    message: impl Into<String>,
) -> bool {
    send_stt_event(
        socket,
        &crate::slices::stt::SttServerEvent::Error {
            version: crate::slices::stt::STT_PROTOCOL_VERSION,
            session_id: session_id.to_string(),
            code: code.to_string(),
            message: message.into(),
        },
    )
    .await
}

fn drain_provider_events(
    session: &mut MoonshineSession,
    session_id: &str,
) -> Vec<crate::slices::stt::SttServerEvent> {
    let mut events = Vec::new();
    while let Some(event) = session.poll_event() {
        events.push(provider_event_to_server(session_id, event));
    }
    events
}

/// Authenticated STT WebSocket loop. Separate from the PTY terminal bridge.
pub async fn bridge_task_stt_socket(
    mut socket: axum::extract::ws::WebSocket,
    provider: Arc<Mutex<MoonshineProvider>>,
    finalization_timeout_ms: u64,
    phrase_end_silence_ms: u64,
    pause_grace_period_ms: u64,
    language: String,
) {
    use crate::slices::stt::{
        decode_audio_frame, AudioFrameError, SttClientMessage, SttServerEvent, STT_PROTOCOL_VERSION,
    };
    use axum::extract::ws::Message;
    use std::time::Instant;

    let mut provider_session: Option<MoonshineSession> = None;
    let mut active_session_id: Option<String> = None;
    let mut finalize_deadline: Option<std::time::Instant> = None;

    loop {
        if let (Some(session), Some(session_id)) =
            (provider_session.as_mut(), active_session_id.as_deref())
        {
            let events = drain_provider_events(session, session_id);
            let drained_final = events
                .iter()
                .any(|event| matches!(event, SttServerEvent::Final { .. }));
            for event in events {
                if !send_stt_event(&mut socket, &event).await {
                    if let Some(mut session) = provider_session.take() {
                        session.cancel();
                    }
                    return;
                }
            }
            if let Some(deadline) = finalize_deadline {
                if drained_final || Instant::now() >= deadline {
                    let closed_session_id = session_id.to_string();
                    if !send_stt_event(
                        &mut socket,
                        &SttServerEvent::Closed {
                            version: STT_PROTOCOL_VERSION,
                            session_id: closed_session_id,
                        },
                    )
                    .await
                    {
                        if let Some(mut session) = provider_session.take() {
                            session.cancel();
                        }
                        return;
                    }
                    if let Some(mut session) = provider_session.take() {
                        session.cancel();
                    }
                    active_session_id = None;
                    finalize_deadline = None;
                }
            }
        }

        let received =
            tokio::time::timeout(Duration::from_millis(STT_EVENT_POLL_MS), socket.recv()).await;

        let message = match received {
            Err(_) => continue,
            Ok(None) | Ok(Some(Err(_))) => {
                if let Some(mut session) = provider_session.take() {
                    session.cancel();
                }
                return;
            }
            Ok(Some(Ok(Message::Close(_)))) => {
                if let Some(mut session) = provider_session.take() {
                    session.cancel();
                }
                let _ = socket.send(Message::Close(None)).await;
                return;
            }
            Ok(Some(Ok(Message::Ping(payload)))) => {
                if socket.send(Message::Pong(payload)).await.is_err() {
                    if let Some(mut session) = provider_session.take() {
                        session.cancel();
                    }
                    return;
                }
                continue;
            }
            Ok(Some(Ok(Message::Pong(_)))) => continue,
            Ok(Some(Ok(Message::Text(text)))) => {
                if text.len() > MAX_STT_CONTROL_BYTES {
                    let sid = active_session_id.as_deref().unwrap_or("");
                    let _ = send_stt_error(
                        &mut socket,
                        sid,
                        "control_too_large",
                        "STT control frame too large",
                    )
                    .await;
                    if let Some(mut session) = provider_session.take() {
                        session.cancel();
                    }
                    let _ = socket.send(Message::Close(None)).await;
                    return;
                }
                match serde_json::from_str::<SttClientMessage>(&text) {
                    Ok(control) => control,
                    Err(_) => {
                        let sid = active_session_id.as_deref().unwrap_or("");
                        let _ = send_stt_error(
                            &mut socket,
                            sid,
                            "invalid_control",
                            "Malformed STT control frame",
                        )
                        .await;
                        continue;
                    }
                }
            }
            Ok(Some(Ok(Message::Binary(bytes)))) => {
                let Some(session_id) = active_session_id.clone() else {
                    let _ = send_stt_error(
                        &mut socket,
                        "",
                        "session_inactive",
                        "Audio received before stt.start",
                    )
                    .await;
                    continue;
                };
                if bytes.len() > MAX_STT_BINARY_BYTES {
                    let _ = send_stt_error(
                        &mut socket,
                        &session_id,
                        "audio_too_large",
                        "STT audio frame too large",
                    )
                    .await;
                    continue;
                }
                let pcm = match decode_audio_frame(&bytes) {
                    Ok((_sequence, pcm))
                        if pcm.len() <= crate::slices::stt::MAX_AUDIO_FRAME_BYTES =>
                    {
                        pcm.to_vec()
                    }
                    Ok(_) => {
                        let _ = send_stt_error(
                            &mut socket,
                            &session_id,
                            "audio_too_large",
                            "STT audio payload too large",
                        )
                        .await;
                        continue;
                    }
                    Err(AudioFrameError::Truncated) => {
                        let _ = send_stt_error(
                            &mut socket,
                            &session_id,
                            "audio_truncated",
                            "STT audio frame truncated",
                        )
                        .await;
                        continue;
                    }
                    Err(AudioFrameError::TooLarge) => {
                        let _ = send_stt_error(
                            &mut socket,
                            &session_id,
                            "audio_too_large",
                            "STT audio frame too large",
                        )
                        .await;
                        continue;
                    }
                };
                let Some(session) = provider_session.as_mut() else {
                    continue;
                };
                if let Err(error) = session.push_audio(pcm) {
                    let _ = send_stt_error(
                        &mut socket,
                        &session_id,
                        "audio_rejected",
                        error.to_string(),
                    )
                    .await;
                }
                continue;
            }
        };

        match message {
            SttClientMessage::Start {
                version,
                session_id,
                encoding: _,
                sample_rate,
                channels,
            } => {
                if version != STT_PROTOCOL_VERSION {
                    let _ = send_stt_error(
                        &mut socket,
                        &session_id,
                        "version_mismatch",
                        format!("unsupported STT protocol version {version}"),
                    )
                    .await;
                    continue;
                }
                if active_session_id.is_some() {
                    let _ = send_stt_error(
                        &mut socket,
                        &session_id,
                        "duplicate_start",
                        "STT session already active on this socket",
                    )
                    .await;
                    continue;
                }
                let started = {
                    let provider = provider
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    provider.start_session(ProviderSessionConfig {
                        session_id: session_id.clone(),
                        sample_rate,
                        channels,
                        language: language.clone(),
                        phrase_end_silence_ms,
                    })
                };
                match started {
                    Ok(session) => {
                        provider_session = Some(session);
                        active_session_id = Some(session_id.clone());
                        finalize_deadline = None;
                        if !send_stt_event(
                            &mut socket,
                            &SttServerEvent::Ready {
                                version: STT_PROTOCOL_VERSION,
                                session_id,
                                pause_grace_period_ms,
                                finalization_timeout_ms,
                            },
                        )
                        .await
                        {
                            if let Some(mut session) = provider_session.take() {
                                session.cancel();
                            }
                            return;
                        }
                    }
                    Err(error) => {
                        let code = match error {
                            ProviderError::Unavailable(_) => "provider_unavailable",
                            ProviderError::StartupFailed(_) => "provider_startup_failed",
                            _ => "provider_error",
                        };
                        let _ =
                            send_stt_error(&mut socket, &session_id, code, error.to_string()).await;
                    }
                }
            }
            SttClientMessage::Stop {
                version,
                session_id,
            } => {
                if version != STT_PROTOCOL_VERSION
                    || active_session_id.as_deref() != Some(session_id.as_str())
                {
                    let _ = send_stt_error(
                        &mut socket,
                        &session_id,
                        "stale_session",
                        "stt.stop session mismatch",
                    )
                    .await;
                    continue;
                }
                let Some(session) = provider_session.as_mut() else {
                    continue;
                };
                if let Err(error) = session.finalize() {
                    let _ = send_stt_error(
                        &mut socket,
                        &session_id,
                        "finalize_failed",
                        error.to_string(),
                    )
                    .await;
                }
                finalize_deadline =
                    Some(Instant::now() + Duration::from_millis(finalization_timeout_ms.max(1)));
            }
            SttClientMessage::Cancel {
                version,
                session_id,
            } => {
                if version != STT_PROTOCOL_VERSION
                    || active_session_id.as_deref() != Some(session_id.as_str())
                {
                    let _ = send_stt_error(
                        &mut socket,
                        &session_id,
                        "stale_session",
                        "stt.cancel session mismatch",
                    )
                    .await;
                    continue;
                }
                if let Some(mut session) = provider_session.take() {
                    session.cancel();
                }
                active_session_id = None;
                finalize_deadline = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_config() -> ProviderSessionConfig {
        ProviderSessionConfig {
            session_id: "session-1".to_string(),
            sample_rate: 16_000,
            channels: 1,
            language: "en-US".to_string(),
            phrase_end_silence_ms: 700,
        }
    }

    #[test]
    fn missing_provider_command_reports_unavailable_without_panicking() {
        let provider = MoonshineProvider::new(None, 2_000, 700);

        assert!(matches!(provider.health(), ProviderHealth::Unavailable(_)));
        assert!(matches!(
            provider.start_session(session_config()),
            Err(ProviderError::Unavailable(_))
        ));
    }

    #[test]
    fn provider_startup_failure_is_recoverable() {
        let provider = MoonshineProvider::new(
            Some("/definitely/missing/ajax-moonshine-provider".to_string()),
            2_000,
            700,
        );

        assert!(matches!(
            provider.start_session(session_config()),
            Err(ProviderError::StartupFailed(_))
        ));
    }

    #[test]
    fn push_audio_rejects_overflow_when_channel_is_full() {
        let provider = MoonshineProvider::new(Some("cat".to_string()), 20, 700);
        let mut session = provider.start_session(session_config()).expect("session");
        let pcm = vec![0u8; MAX_SIDECAR_AUDIO_PCM_BYTES];
        let mut overflow = false;
        for _ in 0..256 {
            match session.push_audio(pcm.clone()) {
                Ok(()) => continue,
                Err(ProviderError::AudioBufferOverflow) => {
                    overflow = true;
                    break;
                }
                Err(other) => panic!("unexpected error: {other:?}"),
            }
        }
        assert!(
            overflow,
            "expected AudioBufferOverflow when channel is full"
        );
        session.cancel();
    }

    #[test]
    fn cancel_does_not_hang_when_the_sidecar_never_reads_stdin() {
        // `sleep` never drains stdin, so the writer thread ends up blocked inside
        // write_all on a full pipe. Teardown must kill the child before joining it.
        let provider = MoonshineProvider::new(Some("sleep 30".to_string()), 20, 700);
        let mut session = provider.start_session(session_config()).expect("session");
        let pcm = vec![0u8; MAX_SIDECAR_AUDIO_PCM_BYTES];
        // Keep pushing past `Full` so the writer drains into the pipe until the OS
        // pipe buffer itself fills and write_all blocks. Breaking on the first
        // `Full` would stop after a few frames and never wedge the writer.
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if session.push_audio(pcm.clone()).is_err() {
                thread::sleep(Duration::from_millis(1));
            }
        }

        let (done_tx, done_rx) = sync_channel(1);
        thread::spawn(move || {
            session.cancel();
            let _ = done_tx.send(());
        });

        assert!(
            done_rx.recv_timeout(Duration::from_secs(10)).is_ok(),
            "cancel() must not block on a sidecar that never reads stdin"
        );
    }

    #[test]
    fn sidecar_exit_surfaces_one_error_then_none() {
        let provider = MoonshineProvider::new(Some("true".to_string()), 2_000, 700);
        let mut session = provider.start_session(session_config()).expect("session");
        thread::sleep(Duration::from_millis(50));
        assert_eq!(
            session.poll_event(),
            Some(ProviderEvent::Error {
                message: "stt sidecar exited".to_string(),
            })
        );
        assert_eq!(session.poll_event(), None);
        assert_eq!(session.poll_event(), None);
        session.cancel();
    }

    #[test]
    fn provider_events_are_sequence_aware() {
        let event = ProviderEvent::Final {
            sequence: 12,
            text: "Inspect the adapter.".to_string(),
        };

        assert_eq!(event.sequence(), Some(12));
    }

    #[test]
    fn sidecar_audio_frames_preserve_sequence_without_json_base64() {
        let frame = encode_sidecar_audio_frame(42, &[1, 2, 3]).expect("encode frame");

        assert_eq!(&frame[..5], &[1, 0, 0, 0, 42]);
        // Length prefix keeps consecutive audio frames delimitable on the pipe.
        assert_eq!(&frame[5..9], &[0, 0, 0, 3]);
        assert_eq!(&frame[9..], &[1, 2, 3]);
    }

    #[test]
    fn consecutive_sidecar_audio_frames_are_delimitable() {
        let mut stream = encode_sidecar_audio_frame(0, &[7; 4]).expect("first");
        stream.extend(encode_sidecar_audio_frame(1, &[9; 2]).expect("second"));

        // Walk the stream the way a sidecar must: kind, sequence, length, payload.
        let mut cursor = 0usize;
        let mut decoded = Vec::new();
        while cursor < stream.len() {
            assert_eq!(stream[cursor], 1);
            let sequence = u32::from_be_bytes(stream[cursor + 1..cursor + 5].try_into().unwrap());
            let len =
                u32::from_be_bytes(stream[cursor + 5..cursor + 9].try_into().unwrap()) as usize;
            decoded.push((sequence, stream[cursor + 9..cursor + 9 + len].to_vec()));
            cursor += 9 + len;
        }

        assert_eq!(decoded, vec![(0, vec![7; 4]), (1, vec![9; 2])]);
    }

    #[test]
    fn sidecar_start_frame_carries_phrase_end_silence_configuration() {
        let mut config = session_config();
        config.phrase_end_silence_ms = 700;
        let frame = encode_sidecar_start_frame(&config).expect("start frame");
        let body_len = u32::from_be_bytes(frame[1..5].try_into().expect("length")) as usize;
        let body: serde_json::Value = serde_json::from_slice(&frame[5..5 + body_len]).unwrap();

        assert_eq!(body["phraseEndSilenceMs"], 700);
    }

    #[test]
    fn sidecar_start_frame_carries_server_configured_language() {
        let mut config = session_config();
        config.language = "en-GB".to_string();
        let frame = encode_sidecar_start_frame(&config).expect("start frame");
        let body_len = u32::from_be_bytes(frame[1..5].try_into().expect("length")) as usize;
        let body: serde_json::Value = serde_json::from_slice(&frame[5..5 + body_len]).unwrap();

        assert_eq!(body["language"], "en-GB");
    }

    #[test]
    fn sidecar_event_lines_parse_final_and_speech_activity() {
        assert_eq!(
            parse_sidecar_event_line(
                br#"{"type":"stt.final","sequence":12,"text":"Inspect the adapter."}"#,
            )
            .expect("final event"),
            ProviderEvent::Final {
                sequence: 12,
                text: "Inspect the adapter.".to_string(),
            }
        );
        assert_eq!(
            parse_sidecar_event_line(br#"{"type":"stt.speech_started"}"#).expect("speech event"),
            ProviderEvent::SpeechStarted
        );
    }

    #[test]
    fn finalize_leaves_the_session_open_to_drain_final_events() {
        let provider = MoonshineProvider::new(Some("cat".to_string()), 2_000, 700);
        let mut session = provider.start_session(session_config()).expect("session");

        session.finalize().expect("finalize signal");

        assert!(!session.closed);
        session.cancel();
    }
}

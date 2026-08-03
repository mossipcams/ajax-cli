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
};

/// Maximum PCM16 payload bytes in one sidecar audio frame.
pub const MAX_SIDECAR_AUDIO_PCM_BYTES: usize = 640;

const SIDECAR_FRAME_KIND_START: u8 = 0;
const SIDECAR_FRAME_KIND_AUDIO: u8 = 1;
const SIDECAR_FRAME_KIND_FINALIZE: u8 = 2;
const SIDECAR_FRAME_KIND_CANCEL: u8 = 3;
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
    /// Sidecar can accept audio for this session (model loaded).
    Ready,
    Partial {
        sequence: u32,
        text: String,
    },
    Final {
        sequence: u32,
        text: String,
    },
    SpeechStarted,
    SpeechEnded,
    /// Successful session completion after finalize; not an error.
    Completed,
    Error {
        message: String,
    },
}

impl ProviderEvent {
    pub fn sequence(&self) -> Option<u32> {
        match self {
            Self::Partial { sequence, .. } | Self::Final { sequence, .. } => Some(*sequence),
            Self::Ready
            | Self::SpeechStarted
            | Self::SpeechEnded
            | Self::Completed
            | Self::Error { .. } => None,
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

fn encode_sidecar_cancel_frame() -> Vec<u8> {
    vec![SIDECAR_FRAME_KIND_CANCEL]
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
        "stt.ready" => Ok(ProviderEvent::Ready),
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
        "stt.completed" => Ok(ProviderEvent::Completed),
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
/// Persistent Moonshine worker process. Model stays loaded across sessions.
struct PersistentWorker {
    child: Child,
    frame_tx: SyncSender<Vec<u8>>,
    events: Arc<Mutex<Receiver<ProviderEvent>>>,
    reader: Option<JoinHandle<()>>,
    writer: Option<JoinHandle<()>>,
}

impl PersistentWorker {
    fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

/// Supervised Moonshine Small Streaming provider with a persistent worker.
pub struct MoonshineProvider {
    command: Option<String>,
    max_buffered_audio_ms: u64,
    phrase_end_silence_ms: u64,
    shut_down: bool,
    worker: Option<PersistentWorker>,
    /// How many times a worker process was spawned (reuse detection for tests).
    worker_spawns: u32,
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
            worker: None,
            worker_spawns: 0,
        }
    }

    pub fn worker_spawns(&self) -> u32 {
        self.worker_spawns
    }

    pub fn health(&mut self) -> ProviderHealth {
        if self.shut_down {
            return ProviderHealth::Unavailable("provider shut down".to_string());
        }
        match &self.command {
            None => ProviderHealth::Unavailable("no STT provider command configured".to_string()),
            Some(_) => {
                if let Some(worker) = self.worker.as_mut() {
                    if !worker.is_alive() {
                        return ProviderHealth::Unavailable(
                            "STT worker process exited".to_string(),
                        );
                    }
                }
                ProviderHealth::Available
            }
        }
    }

    fn spawn_worker(&mut self) -> Result<(), ProviderError> {
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
        self.worker = Some(PersistentWorker {
            child,
            frame_tx,
            events: Arc::new(Mutex::new(rx)),
            reader: Some(reader),
            writer: Some(writer),
        });
        self.worker_spawns = self.worker_spawns.saturating_add(1);
        Ok(())
    }

    fn ensure_worker(&mut self) -> Result<(), ProviderError> {
        let needs_spawn = match self.worker.as_mut() {
            None => true,
            Some(worker) => !worker.is_alive(),
        };
        if needs_spawn {
            drop(self.worker.take());
            self.spawn_worker()?;
        }
        Ok(())
    }

    pub fn start_session(
        &mut self,
        mut config: ProviderSessionConfig,
    ) -> Result<MoonshineSession, ProviderError> {
        if self.shut_down {
            return Err(ProviderError::Unavailable("provider shut down".to_string()));
        }
        config.phrase_end_silence_ms = self.phrase_end_silence_ms;
        self.command.as_ref().ok_or_else(|| {
            ProviderError::Unavailable("no STT provider command configured".to_string())
        })?;
        self.ensure_worker()?;
        let worker = self
            .worker
            .as_ref()
            .ok_or_else(|| ProviderError::StartupFailed("worker missing".to_string()))?;
        let mut session = MoonshineSession {
            session_id: config.session_id.clone(),
            frame_tx: Some(worker.frame_tx.clone()),
            events: Arc::clone(&worker.events),
            next_sequence: 0,
            finalizing: false,
            closed: false,
            sidecar_ended: false,
            completed: false,
        };
        session.write_frame(&encode_sidecar_start_frame(&config)?)?;
        Ok(session)
    }

    pub fn shutdown(&mut self) {
        self.shut_down = true;
        drop(self.worker.take());
    }
}

/// One recognition session on the persistent Moonshine worker.
pub struct MoonshineSession {
    session_id: String,
    frame_tx: Option<SyncSender<Vec<u8>>>,
    events: Arc<Mutex<Receiver<ProviderEvent>>>,
    next_sequence: u32,
    /// True after an idempotent finalize signal; worker stays up for event drain.
    finalizing: bool,
    /// True after cancel tears down this session (worker remains).
    closed: bool,
    /// Latches terminal reader disconnect handling (error or clean completion).
    sidecar_ended: bool,
    /// True after an explicit `stt.completed` event; exit is not an error.
    completed: bool,
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
        let events = self
            .events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match events.try_recv() {
            Ok(event) => {
                drop(events);
                if matches!(event, ProviderEvent::Completed) {
                    self.completed = true;
                }
                Some(event)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                drop(events);
                if self.sidecar_ended {
                    None
                } else {
                    self.sidecar_ended = true;
                    if self.completed {
                        None
                    } else {
                        Some(ProviderEvent::Error {
                            message: "stt sidecar exited".to_string(),
                        })
                    }
                }
            }
        }
    }

    pub fn is_completed(&self) -> bool {
        self.completed
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
        // Best-effort cancel frame; never kill the persistent worker here.
        let _ = self
            .frame_tx
            .as_ref()
            .map(|tx| tx.try_send(encode_sidecar_cancel_frame()));
        if let Ok(events) = self.events.lock() {
            while events.try_recv().is_ok() {}
        }
        self.frame_tx.take();
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
}

impl Drop for MoonshineSession {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl Drop for PersistentWorker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        // Do not join reader/writer: a wedged write_all can outlive kill briefly
        // and hang tests/shutdown. Detach by dropping the JoinHandles.
        let _ = self.writer.take();
        let _ = self.reader.take();
    }
}

mod bridge;
pub use bridge::bridge_task_stt_socket;
#[cfg(test)]
pub(crate) use bridge::readiness_deadline_expired;

#[cfg(test)]
mod tests;

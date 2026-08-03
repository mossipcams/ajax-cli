//! Authenticated STT WebSocket bridge over the Moonshine provider.
//!
//! Kept separate from the transport-agnostic provider so the provider module
//! stays under the Rust LOC gate and free of Axum/WebSocket imports.

use super::{
    MoonshineProvider, MoonshineSession, ProviderError, ProviderEvent, ProviderSessionConfig,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const MAX_STT_CONTROL_BYTES: usize = 8_192;
const MAX_STT_BINARY_BYTES: usize = 4 + crate::slices::stt::MAX_AUDIO_FRAME_BYTES;
const STT_EVENT_POLL_MS: u64 = 20;
/// How long after `stt.start` the sidecar may take to emit `stt.ready`.
/// Cold model load can be slow; a missing Ready (legacy sidecar) must not hang forever.
const STT_READY_TIMEOUT_MS: u64 = 60_000;

fn provider_event_to_server(
    session_id: &str,
    event: ProviderEvent,
) -> Option<crate::slices::stt::SttServerEvent> {
    use crate::slices::stt::{SttServerEvent, STT_PROTOCOL_VERSION};
    match event {
        ProviderEvent::Ready => Some(SttServerEvent::Ready {
            version: STT_PROTOCOL_VERSION,
            session_id: session_id.to_string(),
            // Timing filled by the bridge from host config when forwarding Ready.
            pause_grace_period_ms: 0,
            finalization_timeout_ms: 0,
        }),
        ProviderEvent::Partial { sequence, text } => Some(SttServerEvent::Partial {
            version: STT_PROTOCOL_VERSION,
            session_id: session_id.to_string(),
            sequence,
            text,
        }),
        ProviderEvent::Final { sequence, text } => Some(SttServerEvent::Final {
            version: STT_PROTOCOL_VERSION,
            session_id: session_id.to_string(),
            sequence,
            text,
        }),
        ProviderEvent::SpeechStarted => Some(SttServerEvent::SpeechStarted {
            version: STT_PROTOCOL_VERSION,
            session_id: session_id.to_string(),
        }),
        ProviderEvent::SpeechEnded => Some(SttServerEvent::SpeechEnded {
            version: STT_PROTOCOL_VERSION,
            session_id: session_id.to_string(),
        }),
        ProviderEvent::Completed => None,
        ProviderEvent::Error { message } => Some(SttServerEvent::Error {
            version: STT_PROTOCOL_VERSION,
            session_id: session_id.to_string(),
            code: "provider_error".to_string(),
            message,
        }),
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
    pause_grace_period_ms: u64,
    finalization_timeout_ms: u64,
) -> (Vec<crate::slices::stt::SttServerEvent>, bool) {
    use crate::slices::stt::{SttServerEvent, STT_PROTOCOL_VERSION};
    let mut events = Vec::new();
    let mut completed = false;
    while let Some(event) = session.poll_event() {
        if matches!(event, ProviderEvent::Completed) {
            completed = true;
            continue;
        }
        if let Some(mut server_event) = provider_event_to_server(session_id, event) {
            if let SttServerEvent::Ready {
                pause_grace_period_ms: grace,
                finalization_timeout_ms: timeout,
                ..
            } = &mut server_event
            {
                *grace = pause_grace_period_ms;
                *timeout = finalization_timeout_ms;
            }
            // Ready must not be invented by spawn; only sidecar Ready reaches here.
            let _ = STT_PROTOCOL_VERSION;
            events.push(server_event);
        }
    }
    (events, completed || session.is_completed())
}

pub(crate) fn readiness_deadline_expired(
    session_ready: bool,
    ready_deadline: Option<std::time::Instant>,
    now: std::time::Instant,
) -> bool {
    !session_ready && ready_deadline.is_some_and(|deadline| now >= deadline)
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
    let mut ready_deadline: Option<std::time::Instant> = None;
    let mut session_ready = false;

    loop {
        if let (Some(session), Some(session_id)) =
            (provider_session.as_mut(), active_session_id.as_deref())
        {
            let (events, completed) = drain_provider_events(
                session,
                session_id,
                pause_grace_period_ms,
                finalization_timeout_ms,
            );
            for event in events {
                if matches!(event, SttServerEvent::Ready { .. }) {
                    session_ready = true;
                    ready_deadline = None;
                }
                if !send_stt_event(&mut socket, &event).await {
                    if let Some(mut session) = provider_session.take() {
                        session.cancel();
                    }
                    return;
                }
            }
            if readiness_deadline_expired(session_ready, ready_deadline, Instant::now()) {
                let sid = session_id.to_string();
                let _ = send_stt_error(
                    &mut socket,
                    &sid,
                    "provider_not_ready",
                    "STT worker did not become ready. Run ./scripts/setup-stt.sh and restart ajax web (worker must emit stt.ready).",
                )
                .await;
                if let Some(mut session) = provider_session.take() {
                    session.cancel();
                }
                active_session_id = None;
                finalize_deadline = None;
                ready_deadline = None;
                session_ready = false;
                continue;
            }
            if completed {
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
                ready_deadline = None;
                session_ready = false;
            } else if let Some(deadline) = finalize_deadline {
                if Instant::now() >= deadline {
                    let sid = session_id.to_string();
                    let _ = send_stt_error(
                        &mut socket,
                        &sid,
                        "finalization_timeout",
                        "STT finalization timed out",
                    )
                    .await;
                    if let Some(mut session) = provider_session.take() {
                        session.cancel();
                    }
                    active_session_id = None;
                    finalize_deadline = None;
                    ready_deadline = None;
                    session_ready = false;
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
                    let mut provider = provider
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
                        session_ready = false;
                        ready_deadline = Some(
                            Instant::now() + Duration::from_millis(STT_READY_TIMEOUT_MS.max(1)),
                        );
                        // stt.ready is forwarded only after the sidecar emits Ready
                        // (model loaded and audio accepted), not on process spawn.
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
                ready_deadline = None;
                session_ready = false;
            }
        }
    }
}

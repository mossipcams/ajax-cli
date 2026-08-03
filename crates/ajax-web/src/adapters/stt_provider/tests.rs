use super::*;
use std::time::Duration;

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
    let mut provider = MoonshineProvider::new(None, 2_000, 700);

    assert!(matches!(provider.health(), ProviderHealth::Unavailable(_)));
    assert!(matches!(
        provider.start_session(session_config()),
        Err(ProviderError::Unavailable(_))
    ));
}

#[test]
fn provider_startup_failure_is_recoverable() {
    let mut provider = MoonshineProvider::new(
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
    let mut provider = MoonshineProvider::new(Some("cat".to_string()), 20, 700);
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
    // write_all on a full pipe. Session cancel must not join that writer;
    // provider shutdown kills the persistent worker to unblock it.
    let mut provider = MoonshineProvider::new(Some("sleep 30".to_string()), 20, 700);
    let mut session = provider.start_session(session_config()).expect("session");
    let pcm = vec![0u8; MAX_SIDECAR_AUDIO_PCM_BYTES];
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
    provider.shutdown();
}

#[test]
fn sidecar_exit_surfaces_one_error_then_none() {
    let mut provider = MoonshineProvider::new(Some("true".to_string()), 2_000, 700);
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
fn expected_completion_does_not_surface_sidecar_exit_error() {
    let script = std::env::temp_dir().join(format!(
        "ajax_stt_complete_sidecar_{}.sh",
        std::process::id()
    ));
    std::fs::write(
        &script,
        concat!(
            "#!/bin/sh\n",
            "printf '%s\\n' '{\"type\":\"stt.ready\"}'\n",
            "printf '%s\\n' '{\"type\":\"stt.completed\"}'\n",
            // Keep stdin open briefly so the parent can finish writing the start frame.
            "sleep 0.2\n",
        ),
    )
    .expect("write script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&script).expect("meta").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script, perms).expect("chmod");
    }
    let mut provider =
        MoonshineProvider::new(Some(script.to_string_lossy().into_owned()), 2_000, 700);
    let mut session = provider.start_session(session_config()).expect("session");

    let mut saw_ready = false;
    let mut saw_completed = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < deadline && !(saw_ready && saw_completed) {
        match session.poll_event() {
            Some(ProviderEvent::Ready) => saw_ready = true,
            Some(ProviderEvent::Completed) => saw_completed = true,
            Some(ProviderEvent::Error { message }) => {
                panic!("unexpected error before completion: {message}");
            }
            Some(other) => panic!("unexpected event: {other:?}"),
            None => thread::sleep(Duration::from_millis(10)),
        }
    }
    assert!(saw_ready, "expected stt.ready from completing sidecar");
    assert!(
        saw_completed,
        "expected stt.completed from completing sidecar"
    );
    assert!(session.is_completed());

    // Reader disconnect after completed must not become an error.
    let drain_deadline = std::time::Instant::now() + Duration::from_secs(1);
    while std::time::Instant::now() < drain_deadline {
        match session.poll_event() {
            Some(ProviderEvent::Error { message }) => {
                panic!("delayed exit must not error after completion: {message}");
            }
            Some(_) => {}
            None => break,
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(session.poll_event(), None);
    session.cancel();
    let _ = std::fs::remove_file(&script);
}

#[test]
fn sidecar_event_lines_parse_ready_completed_final_and_speech_activity() {
    assert_eq!(
        parse_sidecar_event_line(br#"{"type":"stt.ready"}"#).expect("ready"),
        ProviderEvent::Ready
    );
    assert_eq!(
        parse_sidecar_event_line(br#"{"type":"stt.completed"}"#).expect("completed"),
        ProviderEvent::Completed
    );
    assert_eq!(
        parse_sidecar_event_line(br#"{"type":"stt.final","sequence":3,"text":"hello there"}"#)
            .expect("final"),
        ProviderEvent::Final {
            sequence: 3,
            text: "hello there".to_string(),
        }
    );
    assert_eq!(
        parse_sidecar_event_line(br#"{"type":"stt.speech_started"}"#).expect("started"),
        ProviderEvent::SpeechStarted
    );
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
        let len = u32::from_be_bytes(stream[cursor + 5..cursor + 9].try_into().unwrap()) as usize;
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
fn readiness_deadline_expires_only_before_ready() {
    let now = std::time::Instant::now();
    let past = now.checked_sub(Duration::from_secs(1)).unwrap_or(now);
    let future = now + Duration::from_secs(30);
    assert!(readiness_deadline_expired(false, Some(past), now));
    assert!(!readiness_deadline_expired(true, Some(past), now));
    assert!(!readiness_deadline_expired(false, Some(future), now));
    assert!(!readiness_deadline_expired(false, None, now));
}

#[test]
fn finalize_leaves_the_session_open_to_drain_final_events() {
    let mut provider = MoonshineProvider::new(Some("cat".to_string()), 2_000, 700);
    let mut session = provider.start_session(session_config()).expect("session");

    session.finalize().expect("finalize signal");

    assert!(!session.closed);
    session.cancel();
}

#[test]
fn second_session_reuses_the_persistent_worker_process() {
    let mut provider = MoonshineProvider::new(Some("cat".to_string()), 2_000, 700);
    let mut first = provider.start_session(session_config()).expect("first");
    assert_eq!(provider.worker_spawns(), 1);
    first.cancel();
    let mut second = provider.start_session(session_config()).expect("second");
    assert_eq!(
        provider.worker_spawns(),
        1,
        "second session must reuse the loaded worker"
    );
    second.cancel();
    provider.shutdown();
}

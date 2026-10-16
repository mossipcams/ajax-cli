use super::test_support::BlockingSessionDirectory;
use super::ws_bridge::{outbound_frames, should_skip_replayed_envelopes};
use super::ws_bridge_tests::{fake_acp_fixture, scratch_dir};
use super::{
    apply_client_message, SessionClientMessage, SessionEventEnvelope, SessionServerEvent,
    SessionSnapshot,
};
use crate::adapters::web_session_acp::with_test_acp_program;
use ajax_core::models::AgentClient;
use std::path::Path;

/// One seeded prompt inside the ACP fixture scope.
fn tail_seed(directory: &BlockingSessionDirectory, dir: &Path, handle: &str) {
    let mut generation = directory.generation(handle);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let prompt = SessionClientMessage::Prompt {
        text: "tail-nonce".to_string(),
        content_blocks: vec![],
        client_message_id: "prompt-tail".to_string(),
    };
    rt.block_on(apply_client_message(
        directory.inner(),
        handle,
        dir,
        prompt,
        &mut generation,
        None,
    ))
    .expect("prompt");
    directory.pump(handle);
}

#[test]
fn tail_snapshot_reset_attach_skips_envelope_replay() {
    let dir = scratch_dir("tail-snapshot-reset");
    let handle = "web/tail-snapshot-reset";
    let directory = BlockingSessionDirectory::new(dir.clone());
    with_test_acp_program(&fake_acp_fixture(), || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        tail_seed(&directory, &dir, handle);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let attach = rt.block_on(directory.inner().attach_snapshot(
            handle,
            "auto".to_string(),
            None,
        ));
        assert!(!attach.replayed.is_empty(), "log has retained events");
        assert!(attach.snapshot.transcript.is_some());
        assert!(!should_skip_replayed_envelopes(false, &attach.snapshot));
        assert!(should_skip_replayed_envelopes(true, &attach.snapshot));

        let snapshot_cursor = attach.snapshot.cursor;
        tail_seed(&directory, &dir, handle);
        let generation = directory.generation(handle);
        let batch = rt.block_on(directory.inner().collect_outbound(
            handle,
            snapshot_cursor,
            generation,
        ));
        assert!(
            !batch.events.is_empty(),
            "live event recorded after the attach must flow"
        );
        assert_eq!(batch.events[0].cursor, snapshot_cursor);
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn tail_snapshot_resumable_attach_keeps_envelope_replay() {
    let dir = scratch_dir("tail-snapshot-resume");
    let handle = "web/tail-snapshot-resume";
    let directory = BlockingSessionDirectory::new(dir.clone());
    with_test_acp_program(&fake_acp_fixture(), || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        tail_seed(&directory, &dir, handle);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let attach = rt.block_on(directory.inner().attach_snapshot(
            handle,
            "auto".to_string(),
            Some(1),
        ));
        assert!(attach.snapshot.transcript.is_none());
        assert!(!should_skip_replayed_envelopes(true, &attach.snapshot));
        assert!(!attach.replayed.is_empty());
        assert!(
            attach.replayed.iter().all(|env| env.cursor >= 1),
            "after cursor only"
        );
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn legacy_attach_keeps_full_envelope_replay() {
    let dir = scratch_dir("tail-snapshot-legacy");
    let handle = "web/tail-snapshot-legacy";
    let directory = BlockingSessionDirectory::new(dir.clone());
    with_test_acp_program(&fake_acp_fixture(), || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        tail_seed(&directory, &dir, handle);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let attach = rt.block_on(directory.inner().attach_snapshot(
            handle,
            "auto".to_string(),
            None,
        ));
        assert!(attach.snapshot.transcript.is_some());
        assert!(!should_skip_replayed_envelopes(false, &attach.snapshot));
        assert!(!attach.replayed.is_empty());
        assert_eq!(attach.replayed[0].cursor, 0);
    });

    let _ = std::fs::remove_dir_all(dir);
}

fn outbound_frame_snapshot(with_transcript: bool) -> SessionSnapshot {
    let chrome = super::protocol::SessionChrome {
        session_config_options: None,
        available_commands: None,
        prompt_capabilities: None,
        session_title: None,
    };
    let mut snapshot = SessionSnapshot::new(
        0,
        "auto".to_string(),
        false,
        with_transcript,
        None,
        None,
        chrome,
    );
    if with_transcript {
        snapshot = snapshot.with_transcript(
            vec![SessionEventEnvelope::new(
                0,
                SessionServerEvent::Ready {
                    model: "auto".to_string(),
                    busy: false,
                },
            )],
            Some(0),
        );
    }
    snapshot
}

fn outbound_frame_events(count: usize) -> Vec<SessionEventEnvelope> {
    (0..count)
        .map(|cursor| {
            SessionEventEnvelope::new(
                cursor,
                SessionServerEvent::Ready {
                    model: "auto".to_string(),
                    busy: false,
                },
            )
        })
        .collect()
}

#[test]
fn outbound_frames_drops_envelopes_for_tail_client_with_transcript_snapshot() {
    let snapshot = outbound_frame_snapshot(true);
    let events = outbound_frame_events(3);
    let (out_snapshot, out_events) = outbound_frames(Some(snapshot.clone()), events.clone(), true);
    assert!(
        out_events.is_empty(),
        "tail client: transcript snapshot already carries the history"
    );
    assert_eq!(
        out_snapshot.as_ref(),
        Some(&snapshot),
        "snapshot must pass through unchanged"
    );
}

#[test]
fn outbound_frames_keeps_envelopes_for_legacy_client_with_transcript_snapshot() {
    let snapshot = outbound_frame_snapshot(true);
    let events = outbound_frame_events(3);
    let (out_snapshot, out_events) = outbound_frames(Some(snapshot.clone()), events.clone(), false);
    assert_eq!(out_events, events, "legacy client keeps the full replay");
    assert_eq!(
        out_snapshot.as_ref(),
        Some(&snapshot),
        "snapshot must pass through unchanged"
    );
}

#[test]
fn outbound_frames_keeps_envelopes_when_tail_snapshot_has_no_transcript() {
    let snapshot = outbound_frame_snapshot(false);
    let events = outbound_frame_events(2);
    let (out_snapshot, out_events) = outbound_frames(Some(snapshot.clone()), events.clone(), true);
    assert_eq!(
        out_events, events,
        "resumable (no inline transcript) snapshot: envelopes must flow"
    );
    assert_eq!(
        out_snapshot.as_ref(),
        Some(&snapshot),
        "snapshot must pass through unchanged"
    );
}

#[test]
fn outbound_frames_keeps_envelopes_without_snapshot() {
    let events = outbound_frame_events(2);
    let (out_snapshot, out_events) = outbound_frames(None, events.clone(), true);
    assert_eq!(out_events, events, "no snapshot: live envelopes must flow");
    assert!(
        out_snapshot.is_none(),
        "snapshot must pass through unchanged"
    );
}

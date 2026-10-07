//! Transcript and metadata persistence failures must reach the operator.

use super::test_support::{fake_acp_fixture, note, scratch_dir, BlockingSessionDirectory};
use super::SessionServerEvent;
use crate::adapters::web_session_acp::with_test_acp_program;
use crate::adapters::web_session_store;
use ajax_core::models::AgentClient;
use std::{fs, path::Path};

/// A directory where the transcript file belongs: every read and append fails.
fn make_transcript_unreadable(dir: &Path, handle: &str) {
    let path = web_session_store::session_path(dir, handle);
    let _ = fs::remove_file(&path);
    fs::create_dir_all(&path).expect("block transcript path");
}

/// A directory where the rewrite temp file belongs: reads work, rewrites fail.
fn make_metadata_unwritable(dir: &Path, handle: &str) {
    let tmp = web_session_store::session_path(dir, handle).with_extension("jsonl.tmp");
    fs::create_dir_all(&tmp).expect("block rewrite temp path");
}

fn attach_transcript_error(directory: &BlockingSessionDirectory, handle: &str) -> Option<String> {
    directory
        .runtime_handle()
        .block_on(
            directory
                .inner()
                .attach_snapshot(handle, "auto".to_string(), None),
        )
        .snapshot
        .transcript_error
}

#[test]
fn issue_1219_transcript_append_failure_is_not_reported_as_success() {
    let dir = scratch_dir("append-fail-1219");
    let handle = "web/append-fail-1219";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        let (before, cursor) = directory.read_from(handle, 0);
        make_transcript_unreadable(&dir, handle);

        directory.record(handle, note("lost on disk"));

        let (after, next) = directory.read_from(handle, 0);
        assert_eq!(
            (after, next),
            (before, cursor),
            "an event that was not saved must not advance the live transcript"
        );
        let error = attach_transcript_error(&directory, handle)
            .expect("append failure must surface as transcriptError");
        assert!(error.contains("transcript could not be saved"), "{error}");
        assert!(
            directory
                .submit_prompt(handle, "hello".to_string())
                .is_err(),
            "a transcript that cannot be saved must reject new prompts"
        );
    });
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn issue_1220_unsaved_resume_id_fails_the_attach() {
    let dir = scratch_dir("resume-id-fail-1220");
    let handle = "web/resume-id-fail-1220";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();
    web_session_store::append_events(&dir, handle, &[note("seed")]);
    make_metadata_unwritable(&dir, handle);

    with_test_acp_program(&script, || {
        let error = directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect_err("a session whose id was not saved is not resumable")
            .to_string();
        assert!(error.contains("session id could not be saved"), "{error}");
    });
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn issue_1221_clear_context_fails_when_resume_id_stays_on_disk() {
    let dir = scratch_dir("clear-fail-1221");
    let handle = "web/clear-fail-1221";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    // Detached: no live slot, only the stored id.
    web_session_store::save_meta(&dir, handle, Some("sess-old"), "auto");
    make_metadata_unwritable(&dir, handle);
    assert!(
        directory.start_fresh(handle, &dir).is_err(),
        "clearing must not succeed while the old resume id is still stored"
    );
    assert_eq!(
        web_session_store::load::<SessionServerEvent>(&dir, handle)
            .acp_session_id
            .as_deref(),
        Some("sess-old")
    );

    // Live: same failure through the session actor.
    let live = "web/clear-fail-1221-live";
    with_test_acp_program(&script, || {
        directory
            .acquire(live, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        make_metadata_unwritable(&dir, live);
        assert!(
            directory.start_fresh(live, &dir).is_err(),
            "a live clear must report the metadata failure"
        );
    });
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn issue_1228_unreadable_metadata_does_not_start_a_fresh_session() {
    let dir = scratch_dir("unreadable-1228");
    let handle = "web/unreadable-1228";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();
    make_transcript_unreadable(&dir, handle);

    let error = attach_transcript_error(&directory, handle)
        .expect("a detached attach must say the stored session is unreadable");
    assert!(error.contains("unreadable"), "{error}");

    with_test_acp_program(&script, || {
        let error = directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect_err("unreadable metadata must not fall back to session/new")
            .to_string();
        assert!(error.contains("stored session is unreadable"), "{error}");
    });
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn issue_1240_interior_corruption_is_surfaced_on_attach() {
    let dir = scratch_dir("corrupt-1240");
    let handle = "web/corrupt-1240";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();
    web_session_store::append_events(&dir, handle, &[note("before")]);
    let path = web_session_store::session_path(&dir, handle);
    let mut raw = fs::read_to_string(&path).expect("seeded transcript");
    raw.push_str("{\"kind\":\"event\",\"event\":{\"type\":\"mess\n");
    fs::write(&path, raw).expect("corrupt a row");
    web_session_store::append_events(&dir, handle, &[note("after")]);

    let detached = attach_transcript_error(&directory, handle)
        .expect("a detached attach must warn about unreadable rows");
    assert!(detached.contains("1 stored transcript rows"), "{detached}");

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("corruption warns; it does not block the session");
        let live = attach_transcript_error(&directory, handle)
            .expect("a live attach must keep warning after the metadata rewrite");
        assert!(live.contains("1 stored transcript rows"), "{live}");
        directory
            .submit_prompt(handle, "hello".to_string())
            .expect("corruption must not block prompts");
    });
    let _ = fs::remove_dir_all(dir);
}

use super::test_support::BlockingSessionDirectory;
use super::{apply_client_message, ApplyClientMessageOutcome, SessionClientMessage};
use crate::adapters::web_session_acp::{with_test_acp_extra_args, with_test_acp_program};
use ajax_core::models::AgentClient;
use std::path::PathBuf;

fn scratch_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ajax-web-bridge-{label}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn fake_acp_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_acp.js")
}

#[test]
fn retry_restore_client_message_parses_and_restores_unavailable_context() {
    assert!(serde_json::from_str::<SessionClientMessage>(r#"{"type":"retry_restore"}"#).is_ok());

    let dir = scratch_dir("bridge-retry-restore");
    let handle = "web/bridge-retry-restore";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();
    let rt = directory.runtime_handle();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("first acquire");
        directory.kill_host_for_test(handle);

        with_test_acp_extra_args(&["--load-fail"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("unavailable attach");
        });

        let mut generation = directory.generation(handle);
        with_test_acp_extra_args(&["--load-fail"], || {
            let failed = rt.block_on(apply_client_message(
                directory.inner(),
                handle,
                &dir,
                SessionClientMessage::RetryRestore,
                &mut generation,
                None,
            ));
            assert!(failed.is_err(), "bridge retry must surface restore failure");
        });

        let outcome = rt
            .block_on(apply_client_message(
                directory.inner(),
                handle,
                &dir,
                SessionClientMessage::RetryRestore,
                &mut generation,
                None,
            ))
            .expect("bridge retry must succeed when restore succeeds");
        assert_eq!(outcome, ApplyClientMessageOutcome::Applied);

        let attach = rt.block_on(directory.inner().attach_snapshot(
            handle,
            "auto".to_string(),
            None,
        ));
        assert_eq!(
            attach.snapshot.context_state,
            super::context_continuity::ContextState::Restored
        );
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn start_new_context_client_message_parses_and_replaces_unavailable_context() {
    assert!(
        serde_json::from_str::<SessionClientMessage>(r#"{"type":"start_new_context"}"#).is_ok()
    );

    let dir = scratch_dir("bridge-start-new");
    let handle = "web/bridge-start-new";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();
    let rt = directory.runtime_handle();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("first acquire");
        directory.kill_host_for_test(handle);

        with_test_acp_extra_args(&["--load-fail"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("unavailable attach");
        });

        let unavailable = rt.block_on(directory.inner().attach_snapshot(
            handle,
            "auto".to_string(),
            None,
        ));
        let epoch_before = unavailable.snapshot.context_epoch;

        let mut generation = directory.generation(handle);
        let _fail = crate::adapters::web_session_store::ForceSaveMetaFailGuard::enable();
        let failed = rt.block_on(apply_client_message(
            directory.inner(),
            handle,
            &dir,
            SessionClientMessage::StartNewContext,
            &mut generation,
            None,
        ));
        drop(_fail);
        assert!(
            failed.is_err(),
            "bridge start new must surface persist failure"
        );

        let outcome = rt
            .block_on(apply_client_message(
                directory.inner(),
                handle,
                &dir,
                SessionClientMessage::StartNewContext,
                &mut generation,
                None,
            ))
            .expect("bridge start new must succeed when persist succeeds");
        assert_eq!(outcome, ApplyClientMessageOutcome::Applied);

        let attach = rt.block_on(directory.inner().attach_snapshot(
            handle,
            "auto".to_string(),
            None,
        ));
        assert_eq!(
            attach.snapshot.context_state,
            super::context_continuity::ContextState::Live
        );
        assert_eq!(attach.snapshot.context_epoch, epoch_before + 1);
    });

    let _ = std::fs::remove_dir_all(dir);
}

use super::context_continuity::ContextState;
use super::test_support::{
    fake_acp_fixture, has_message, log_contains_text, read_fake_acp_methods,
    read_fake_context_memory, remember_context, scratch_dir, BlockingSessionDirectory,
    CONTEXT_RESET_NOTE,
};
use super::SessionServerEvent;
use crate::adapters::web_session_acp::{with_test_acp_extra_args, with_test_acp_program};
use crate::adapters::web_session_store;
use ajax_core::models::AgentClient;

#[test]
fn restore_failure_retains_transcript_id_and_projects_unavailable_snapshot() {
    let dir = scratch_dir("restore-unavail-attach");
    let handle = "web/restore-unavail-attach";
    let script = fake_acp_fixture();
    let directory = BlockingSessionDirectory::new(dir.clone());
    let rt = directory.runtime_handle();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("first acquire");
        directory.record(
            handle,
            SessionServerEvent::Message {
                role: "user".to_string(),
                text: "seed".to_string(),
                content_blocks: Vec::new(),
                item_id: "seed-user".to_string(),
                message_id: None,
            },
        );
        let before = web_session_store::load::<SessionServerEvent>(&dir, handle);
        assert!(
            before.acp_session_id.is_some(),
            "first acquire must persist session id"
        );
        directory.kill_host_for_test(handle);

        with_test_acp_extra_args(&["--load-fail"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("restore failure must remain attachable");
        });

        let after = web_session_store::load::<SessionServerEvent>(&dir, handle);
        assert_eq!(
            after.acp_session_id, before.acp_session_id,
            "restore failure must retain stored session id"
        );
        let (events, _) = directory.read_from(handle, 0);
        assert!(
            has_message(&events, "user", "seed"),
            "restore failure must retain transcript"
        );
        assert!(
            !has_message(&events, "note", CONTEXT_RESET_NOTE),
            "restore failure must not silently reset context"
        );

        let attach = rt.block_on(directory.inner().attach_snapshot(
            handle,
            "auto".to_string(),
            None,
        ));
        assert_eq!(attach.snapshot.context_state, ContextState::Unavailable);
        assert!(
            attach.snapshot.context_error.is_some(),
            "unavailable snapshot must carry contextError"
        );

        let prompt = directory.submit_prompt_with_id(handle, "blocked-1".into(), "nope".into());
        assert!(prompt.is_err(), "unavailable context must reject prompts");
        assert!(
            directory.child_id(handle).is_none(),
            "restore failure must not install a live client"
        );
        let (events, _) = directory.read_from(handle, 0);
        assert!(
            !events.iter().any(|event| matches!(
                event,
                SessionServerEvent::PromptAccepted { client_message_id }
                    if client_message_id == "blocked-1"
            )),
            "rejected prompt must not be acknowledged"
        );
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn restore_failure_collect_outbound_includes_unavailable_snapshot() {
    let dir = scratch_dir("restore-unavail-outbound");
    let handle = "web/restore-unavail-outbound";
    let script = fake_acp_fixture();
    let directory = BlockingSessionDirectory::new(dir.clone());

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("first acquire");
        directory.record(
            handle,
            SessionServerEvent::Message {
                role: "user".to_string(),
                text: "seed".to_string(),
                content_blocks: Vec::new(),
                item_id: "seed-user".to_string(),
                message_id: None,
            },
        );
        directory.kill_host_for_test(handle);

        with_test_acp_extra_args(&["--load-fail"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("restore failure must remain attachable");
        });

        let batch = directory
            .runtime_handle()
            .block_on(directory.inner().collect_outbound(handle, 0, 0));
        let snapshot = batch
            .snapshot
            .expect("generation change must include snapshot");
        assert_eq!(snapshot.context_state, ContextState::Unavailable);
        assert!(snapshot.context_error.is_some());
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn retry_restore_uses_same_stored_id_failure_preserves_unavailable_success_restores_epoch() {
    let dir = scratch_dir("retry-restore");
    let handle = "web/retry-restore";
    let script = fake_acp_fixture();
    let directory = BlockingSessionDirectory::new(dir.clone());
    let rt = directory.runtime_handle();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("first acquire");
        directory.record(
            handle,
            SessionServerEvent::Message {
                role: "user".to_string(),
                text: "seed".to_string(),
                content_blocks: Vec::new(),
                item_id: "seed-user".to_string(),
                message_id: None,
            },
        );
        let before = web_session_store::load::<SessionServerEvent>(&dir, handle);
        assert!(
            before.acp_session_id.is_some(),
            "first acquire must persist session id"
        );
        directory.kill_host_for_test(handle);

        with_test_acp_extra_args(&["--load-fail"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("restore failure must remain attachable");
        });

        let unavailable = rt.block_on(directory.inner().attach_snapshot(
            handle,
            "auto".to_string(),
            None,
        ));
        assert_eq!(
            unavailable.snapshot.context_state,
            ContextState::Unavailable
        );
        let epoch_before = unavailable.snapshot.context_epoch;

        with_test_acp_extra_args(&["--load-fail"], || {
            let failed = rt.block_on(directory.inner().retry_restore(handle));
            assert!(failed.is_err(), "failed retry must report restore error");
        });

        let after_failed_retry = web_session_store::load::<SessionServerEvent>(&dir, handle);
        assert_eq!(
            after_failed_retry.acp_session_id, before.acp_session_id,
            "failed retry must preserve stored session id"
        );
        let still_unavailable = rt.block_on(directory.inner().attach_snapshot(
            handle,
            "auto".to_string(),
            None,
        ));
        assert_eq!(
            still_unavailable.snapshot.context_state,
            ContextState::Unavailable
        );
        assert_eq!(
            still_unavailable.snapshot.context_epoch, epoch_before,
            "failed retry must not advance context epoch"
        );
        assert!(
            directory.child_id(handle).is_none(),
            "failed retry must not install a live client"
        );

        rt.block_on(directory.inner().retry_restore(handle))
            .expect("successful retry must restore context");

        let after_success = web_session_store::load::<SessionServerEvent>(&dir, handle);
        assert_eq!(
            after_success.acp_session_id, before.acp_session_id,
            "successful retry must keep the same stored session id"
        );
        let restored = rt.block_on(directory.inner().attach_snapshot(
            handle,
            "auto".to_string(),
            None,
        ));
        assert_eq!(restored.snapshot.context_state, ContextState::Restored);
        assert_eq!(
            restored.snapshot.context_epoch, epoch_before,
            "successful retry must not advance context epoch"
        );
        assert!(
            directory.child_id(handle).is_some(),
            "successful retry must install a live client"
        );
        directory
            .submit_prompt_with_id(handle, "after-restore".into(), "hello".into())
            .expect("restored context must accept prompts");
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn g1_load_fail_enters_attachable_unavailable_state() {
    let dir = scratch_dir("load-fail");
    let script = fake_acp_fixture();
    let handle = "web/g1-load-fail";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let rt = directory.runtime_handle();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("first acquire");
        directory.record(
            handle,
            SessionServerEvent::Message {
                role: "user".to_string(),
                text: "seed".to_string(),
                content_blocks: Vec::new(),
                item_id: "seed-user".to_string(),
                message_id: None,
            },
        );
        let before = web_session_store::load::<SessionServerEvent>(&dir, handle);
        directory.kill_host_for_test(handle);

        with_test_acp_extra_args(&["--load-fail"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("load-fail must remain attachable with unavailable context");
        });

        assert!(
            !log_contains_text(&directory, handle, CONTEXT_RESET_NOTE),
            "load-fail must not silently start fresh context"
        );
        let after = web_session_store::load::<SessionServerEvent>(&dir, handle);
        assert_eq!(after.acp_session_id, before.acp_session_id);
        let attach = rt.block_on(directory.inner().attach_snapshot(
            handle,
            "auto".to_string(),
            None,
        ));
        assert_eq!(
            attach.snapshot.context_state,
            super::context_continuity::ContextState::Unavailable
        );
        assert!(attach.snapshot.context_error.is_some());
        assert!(directory
            .submit_prompt(handle, "blocked".to_string())
            .is_err());
    });

    let _ = std::fs::remove_dir_all(dir);
}
#[test]
fn directory_retry_restore_delegates_to_unavailable_session_actor() {
    let dir = scratch_dir("directory-retry-restore");
    let script = fake_acp_fixture();
    let handle = "web/directory-retry-restore";
    let directory = BlockingSessionDirectory::new(dir.clone());
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
            assert!(
                rt.block_on(directory.inner().retry_restore(handle))
                    .is_err(),
                "directory retry must surface restore failure"
            );
        });
        let attach = rt.block_on(directory.inner().attach_snapshot(
            handle,
            "auto".to_string(),
            None,
        ));
        assert_eq!(
            attach.snapshot.context_state,
            super::context_continuity::ContextState::Unavailable
        );

        rt.block_on(directory.inner().retry_restore(handle))
            .expect("directory retry must restore when ACP succeeds");
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

/// T19: forced restore failure must block instead of silently creating fresh context.
#[test]
fn forced_restore_failure_blocks_without_creating_fresh_context() {
    use super::context_continuity::ContextState;

    let dir = scratch_dir("t19-restore-block");
    let handle = "web/t19-restore-block";
    let script = fake_acp_fixture();
    let nonce = format!("restore-block-nonce-{}", std::process::id());

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--resume", "--remember-context"], || {
            let directory = BlockingSessionDirectory::new(dir.clone());
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            remember_context(&directory, handle, &dir, &nonce);
            directory.kill_host_for_test(handle);
        });

        with_test_acp_extra_args(
            &[
                "--resume",
                "--resume-fail",
                "--load-fail",
                "--record-methods",
                "--remember-context",
            ],
            || {
                let directory = BlockingSessionDirectory::new(dir.clone());
                let session_before = directory
                    .stored_acp_session_id(&dir, handle)
                    .expect("stored session id after kill");
                directory
                    .acquire(handle, &dir, "auto", AgentClient::Cursor)
                    .expect("restore failure must remain attachable");

                let attach = directory.attach_snapshot(handle, "auto");
                assert_eq!(
                    attach.snapshot.context_state,
                    ContextState::Unavailable,
                    "forced restore failure must block prompting"
                );
                assert!(
                    directory.submit_prompt(handle, "blocked".into()).is_err(),
                    "unavailable context must reject prompts"
                );
                assert_eq!(
                    directory.stored_acp_session_id(&dir, handle),
                    Some(session_before),
                    "restore failure must retain stored session id"
                );
                assert_eq!(
                    read_fake_context_memory(&dir).as_deref(),
                    Some(nonce.as_str()),
                    "restore failure must not clear remembered context via session/new"
                );
                let methods = read_fake_acp_methods(&dir);
                assert!(
                    !methods.iter().any(|method| method == "session/new"),
                    "stored-id restore must not fall back to session/new, saw: {methods:?}"
                );
            },
        );
    });

    let _ = std::fs::remove_dir_all(dir);
}

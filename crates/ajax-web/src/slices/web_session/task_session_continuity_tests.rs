use super::context_continuity::ContextState;
use super::test_support::{
    fake_acp_fixture, pump_until, recall_context, remember_context, scratch_dir,
    BlockingSessionDirectory,
};
use super::SessionServerEvent;
use crate::adapters::web_session_acp::{with_test_acp_extra_args, with_test_acp_program};
use crate::adapters::web_session_store;
use crate::adapters::web_session_store::prompt_ledger::{self, PromptPhase};
use ajax_core::models::AgentClient;
use std::time::Duration;

fn ledger_phase(state_dir: &std::path::Path, handle: &str, id: &str) -> Option<PromptPhase> {
    prompt_ledger::load(state_dir, handle)
        .ok()?
        .entry(id)
        .map(|entry| entry.phase)
}

#[test]
fn new_context_identity_persist_failure_leaves_previous_identity_and_no_client() {
    let dir = scratch_dir("new-context-meta-fail");
    let handle = "web/new-context-meta-fail";
    let script = fake_acp_fixture();
    let directory = BlockingSessionDirectory::new(dir.clone());
    let rt = directory.runtime_handle();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("first acquire");
            let before = web_session_store::load::<SessionServerEvent>(&dir, handle);
            assert!(
                before.acp_session_id.is_some(),
                "first acquire must persist identity"
            );
            let epoch_before = rt
                .block_on(
                    directory
                        .inner()
                        .attach_snapshot(handle, "auto".to_string(), None),
                )
                .snapshot
                .context_epoch;
            directory
                .submit_prompt_with_id(handle, "active-1".into(), "hold".into())
                .expect("active prompt");
            directory
                .submit_prompt_with_id(handle, "queued-1".into(), "queued".into())
                .expect("queue prompt");

            let _fail = web_session_store::ForceSaveMetaFailGuard::enable();
            let result = directory.acquire(handle, &dir, "composer-2.5", AgentClient::Cursor);
            drop(_fail);
            assert!(
                result.is_ok(),
                "model-change replace persist failure must remain attachable"
            );
            assert!(
                directory.child_id(handle).is_none(),
                "staged client must not remain installed"
            );
            let after = web_session_store::load::<SessionServerEvent>(&dir, handle);
            assert_eq!(
                after.acp_session_id, before.acp_session_id,
                "failed new-context install must leave stored identity unchanged"
            );
            assert_eq!(
                after.context_epoch, before.context_epoch,
                "replace persist failure must not advance context epoch on disk"
            );
            let attach = rt.block_on(directory.inner().attach_snapshot(
                handle,
                "composer-2.5".to_string(),
                None,
            ));
            assert_eq!(
                attach.snapshot.context_state,
                ContextState::Unavailable,
                "model-change replace persist failure must project unavailable"
            );
            assert_eq!(
                attach.snapshot.context_epoch, epoch_before,
                "replace persist failure must keep context epoch in snapshot"
            );
            assert!(
                attach.snapshot.context_error.is_some(),
                "replace persist failure must surface contextError"
            );
            assert_eq!(
                ledger_phase(&dir, handle, "queued-1"),
                Some(PromptPhase::Queued),
                "failed install must not dispatch queued prompts"
            );
            assert!(
                directory
                    .submit_prompt_with_id(handle, "blocked-1".into(), "nope".into())
                    .is_err(),
                "unavailable replace failure must reject prompts"
            );
        });
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn install_replaced_persist_failure_projects_unavailable_and_retains_stored_id() {
    let dir = scratch_dir("replace-meta-fail");
    let handle = "web/replace-meta-fail";
    let script = fake_acp_fixture();
    let directory = BlockingSessionDirectory::new(dir.clone());
    let rt = directory.runtime_handle();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("first acquire");
        let before = web_session_store::load::<SessionServerEvent>(&dir, handle);
        assert!(
            before.acp_session_id.is_some(),
            "first acquire must persist session id"
        );
        let epoch_before = rt
            .block_on(
                directory
                    .inner()
                    .attach_snapshot(handle, "auto".to_string(), None),
            )
            .snapshot
            .context_epoch;
        directory.kill_host_for_test(handle);

        let _fail = web_session_store::ForceSaveMetaFailGuard::enable();
        let result = directory.acquire(handle, &dir, "auto", AgentClient::Cursor);
        drop(_fail);
        assert!(
            result.is_ok(),
            "replace persist failure must remain attachable"
        );

        let after = web_session_store::load::<SessionServerEvent>(&dir, handle);
        assert_eq!(
            after.acp_session_id, before.acp_session_id,
            "replace persist failure must retain stored session id"
        );
        assert_eq!(
            after.context_epoch, before.context_epoch,
            "replace persist failure must not advance context epoch on disk"
        );

        let attach = rt.block_on(directory.inner().attach_snapshot(
            handle,
            "auto".to_string(),
            None,
        ));
        assert_eq!(
            attach.snapshot.context_state,
            ContextState::Unavailable,
            "replace persist failure must project unavailable"
        );
        assert_eq!(
            attach.snapshot.context_epoch, epoch_before,
            "replace persist failure must keep context epoch in snapshot"
        );
        assert!(
            attach.snapshot.context_error.is_some(),
            "replace persist failure must surface contextError"
        );
        assert!(
            directory.child_id(handle).is_none(),
            "replace persist failure must not install a live client"
        );
        assert!(
            directory
                .submit_prompt_with_id(handle, "blocked-1".into(), "nope".into())
                .is_err(),
            "unavailable replace failure must reject prompts"
        );

        rt.block_on(directory.inner().retry_restore(handle))
            .expect("retry restore must succeed after persist failure clears");
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
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn finish_first_acquire_persist_failure_remains_attachable_with_unavailable_snapshot() {
    let dir = scratch_dir("first-acquire-meta-fail");
    let handle = "web/first-acquire-meta-fail";
    let script = fake_acp_fixture();
    let directory = BlockingSessionDirectory::new(dir.clone());
    let rt = directory.runtime_handle();

    with_test_acp_program(&script, || {
        let _fail = web_session_store::ForceSaveMetaFailGuard::enable();
        let result = directory.acquire(handle, &dir, "auto", AgentClient::Cursor);
        drop(_fail);
        assert!(
            result.is_ok(),
            "first acquire persist failure must remain attachable"
        );

        let stored = web_session_store::load::<SessionServerEvent>(&dir, handle);
        assert!(
            stored.acp_session_id.is_none(),
            "persist failure must not write session id"
        );

        let attach = rt.block_on(directory.inner().attach_snapshot(
            handle,
            "auto".to_string(),
            None,
        ));
        assert_eq!(
            attach.snapshot.context_state,
            ContextState::Unavailable,
            "first acquire persist failure must project unavailable"
        );
        assert!(
            attach.snapshot.context_error.is_some(),
            "first acquire persist failure must surface contextError"
        );
        assert!(
            directory.child_id(handle).is_none(),
            "first acquire persist failure must not install a live client"
        );
        assert!(
            directory
                .submit_prompt_with_id(handle, "blocked-1".into(), "nope".into())
                .is_err(),
            "unavailable first acquire failure must reject prompts"
        );
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn start_new_context_advances_epoch_once_after_successful_persistence() {
    let dir = scratch_dir("start-new-context");
    let handle = "web/start-new-context";
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

        rt.block_on(directory.inner().start_new_context(handle))
            .expect("start new context must succeed");

        let after = web_session_store::load::<SessionServerEvent>(&dir, handle);
        assert_eq!(
            after.context_epoch,
            epoch_before + 1,
            "start new context must advance epoch once on disk"
        );
        let live = rt.block_on(
            directory
                .inner()
                .attach_snapshot(handle, "auto".to_string(), None),
        );
        assert_eq!(live.snapshot.context_state, ContextState::Live);
        assert_eq!(
            live.snapshot.context_epoch,
            epoch_before + 1,
            "start new context must advance epoch once in snapshot"
        );
        assert!(
            directory.child_id(handle).is_some(),
            "start new context must install a live client"
        );
        directory
            .submit_prompt_with_id(handle, "after-new".into(), "hello".into())
            .expect("new context must accept prompts");
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn start_new_context_persist_failure_retains_old_id_epoch_and_unavailable() {
    let dir = scratch_dir("start-new-context-fail");
    let handle = "web/start-new-context-fail";
    let script = fake_acp_fixture();
    let directory = BlockingSessionDirectory::new(dir.clone());
    let rt = directory.runtime_handle();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("first acquire");
        let before = web_session_store::load::<SessionServerEvent>(&dir, handle);
        assert!(before.acp_session_id.is_some());
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
        let epoch_before = unavailable.snapshot.context_epoch;

        let _fail = web_session_store::ForceSaveMetaFailGuard::enable();
        let result = rt.block_on(directory.inner().start_new_context(handle));
        drop(_fail);
        assert!(result.is_err(), "persist failure must fail closed");

        let after = web_session_store::load::<SessionServerEvent>(&dir, handle);
        assert_eq!(
            after.acp_session_id, before.acp_session_id,
            "failed start new context must retain stored session id"
        );
        assert_eq!(
            after.context_epoch, before.context_epoch,
            "failed start new context must retain context epoch on disk"
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
            "failed start new context must retain context epoch in snapshot"
        );
        assert!(
            directory.child_id(handle).is_none(),
            "failed start new context must not install a live client"
        );
    });

    let _ = std::fs::remove_dir_all(dir);
}
#[test]
fn harness_switch_advances_context_epoch_via_new_context_transaction() {
    let dir = scratch_dir("switch-epoch");
    let handle = "web/switch-epoch";
    let script = fake_acp_fixture();
    let directory = BlockingSessionDirectory::new(dir.clone());
    let rt = directory.runtime_handle();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("first acquire");
        let before = rt.block_on(directory.inner().attach_snapshot(
            handle,
            "auto".to_string(),
            None,
        ));
        let epoch_before = before.snapshot.context_epoch;

        rt.block_on(directory.inner().reset_harness_context(
            handle,
            &dir,
            AgentClient::Claude,
            "auto",
        ))
        .expect("harness switch");

        let after = web_session_store::load::<SessionServerEvent>(&dir, handle);
        assert_eq!(
            after.context_epoch,
            epoch_before + 1,
            "harness switch must advance context epoch once on disk"
        );
        let live = rt.block_on(
            directory
                .inner()
                .attach_snapshot(handle, "auto".to_string(), None),
        );
        assert_eq!(
            live.snapshot.context_epoch,
            epoch_before + 1,
            "harness switch must advance context epoch once in snapshot"
        );
        assert_eq!(live.snapshot.context_state, ContextState::Live);
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn directory_start_new_context_delegates_to_unavailable_session_actor() {
    let dir = scratch_dir("directory-start-new");
    let script = fake_acp_fixture();
    let handle = "web/directory-start-new";
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
        });
        let unavailable = rt.block_on(directory.inner().attach_snapshot(
            handle,
            "auto".to_string(),
            None,
        ));
        assert_eq!(
            unavailable.snapshot.context_state,
            super::context_continuity::ContextState::Unavailable
        );
        let epoch_before = unavailable.snapshot.context_epoch;

        let _fail = web_session_store::ForceSaveMetaFailGuard::enable();
        let failed = rt.block_on(directory.inner().start_new_context(handle));
        drop(_fail);
        assert!(
            failed.is_err(),
            "directory start new must surface persist failure"
        );

        rt.block_on(directory.inner().start_new_context(handle))
            .expect("directory start new must succeed when persist succeeds");
        let live = rt.block_on(
            directory
                .inner()
                .attach_snapshot(handle, "auto".to_string(), None),
        );
        assert_eq!(
            live.snapshot.context_state,
            super::context_continuity::ContextState::Live
        );
        assert_eq!(live.snapshot.context_epoch, epoch_before + 1);
    });

    let _ = std::fs::remove_dir_all(dir);
}
/// T19: prove fake ACP context survives intentional detach and re-acquire.
#[test]
fn remember_context_survives_detach_and_reacquire() {
    let dir = scratch_dir("t19-detach");
    let handle = "web/t19-detach";
    let script = fake_acp_fixture();
    let nonce = format!("detach-nonce-{}", std::process::id());

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--resume", "--remember-context"], || {
            let directory = BlockingSessionDirectory::new(dir.clone());
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            remember_context(&directory, handle, &dir, &nonce);
            directory.detach_session(handle);

            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("re-acquire after detach");
            recall_context(&directory, handle, &nonce);
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

/// T19: prove fake ACP context survives directory recreation (process restart).
#[test]
fn remember_context_survives_directory_recreation() {
    let dir = scratch_dir("t19-directory");
    let handle = "web/t19-directory";
    let script = fake_acp_fixture();
    let nonce = format!("directory-nonce-{}", std::process::id());

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--resume", "--remember-context"], || {
            {
                let directory = BlockingSessionDirectory::new(dir.clone());
                directory
                    .acquire(handle, &dir, "auto", AgentClient::Cursor)
                    .expect("acquire");
                remember_context(&directory, handle, &dir, &nonce);
                directory.detach_session(handle);
            }

            let directory = BlockingSessionDirectory::new(dir.clone());
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire after directory recreation");
            recall_context(&directory, handle, &nonce);
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

/// T19: prove fake ACP context survives ACP child replacement after exit.
#[test]
fn remember_context_survives_child_replacement() {
    let dir = scratch_dir("t19-replace");
    let handle = "web/t19-replace";
    let script = fake_acp_fixture();
    let nonce = format!("replace-nonce-{}", std::process::id());

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--resume", "--remember-context"], || {
            let directory = BlockingSessionDirectory::new(dir.clone());
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            remember_context(&directory, handle, &dir, &nonce);
            let child_before = directory.child_id(handle).expect("child before exit");
            directory.kill_host_for_test(handle);
            pump_until(&directory, handle, Duration::from_secs(5), |events| {
                events.iter().any(|event| matches!(
                    event,
                    SessionServerEvent::Error { message } if message.contains("ACP process exited")
                ))
            });

            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("replacement acquire");
            let child_after = directory.child_id(handle).expect("child after replacement");
            assert_ne!(
                child_before, child_after,
                "replacement must spawn a new ACP child"
            );
            recall_context(&directory, handle, &nonce);
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

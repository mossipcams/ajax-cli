//! Prompt admission must be atomic: a ledger commit and its transcript events land together.

use super::test_support::{fake_acp_fixture, scratch_dir, BlockingSessionDirectory};
use super::SessionServerEvent;
use crate::adapters::web_session_acp::with_test_acp_program;
use crate::adapters::web_session_store;
use ajax_core::models::AgentClient;

#[test]
fn forced_ledger_failure_leaves_no_user_bubble() {
    let dir = scratch_dir("commit-ledger-fail");
    let handle = "web/commit-ledger-fail";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        let (before, cursor) = directory.read_from(handle, 0);

        crate::adapters::web_session_store::prompt_ledger::set_force_persist_fail(true);
        let result = directory.submit_prompt_with_id(handle, "p1".to_string(), "hello".to_string());
        crate::adapters::web_session_store::prompt_ledger::set_force_persist_fail(false);

        assert!(
            result.is_err(),
            "a failed ledger commit must reject the prompt"
        );

        let (after, next) = directory.read_from(handle, 0);
        assert_eq!(
            (after, next),
            (before, cursor),
            "a forced persistence failure must leave the transcript untouched"
        );
    });
    drop(directory);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn admitted_prompt_and_user_bubble_survive_a_restart() {
    let dir = scratch_dir("commit-restart");
    let handle = "web/commit-restart";
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let directory = BlockingSessionDirectory::new(dir.clone());
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        directory
            .submit_prompt_with_id(handle, "p1".to_string(), "hello".to_string())
            .expect("prompt with a client id is admitted atomically");
        drop(directory);
    });

    // Restart: same directory, no live slot.
    let reloaded = BlockingSessionDirectory::new(dir.clone());
    with_test_acp_program(&script, || {
        let ledger = crate::adapters::web_session_store::prompt_ledger::load(&dir, handle)
            .expect("ledger survived the restart");
        assert!(
            ledger.entry("p1").is_some(),
            "the ledger must still own p1 after restart"
        );

        let transcript = web_session_store::load::<SessionServerEvent>(&dir, handle);
        assert!(
            transcript.events.iter().any(|event| matches!(
                event,
                SessionServerEvent::Message { role, item_id, .. } if role == "user" && item_id == "u:p1"
            )),
            "the user bubble for p1 must be in the durable transcript; events={:?}",
            transcript.events
        );
    });
    drop(reloaded);
    let _ = std::fs::remove_dir_all(dir);
}

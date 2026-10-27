use super::task_session_exit::begin_failure_events;
use super::test_support::{fake_acp_fixture, scratch_dir, BlockingSessionDirectory};
use super::SessionServerEvent;
use crate::adapters::web_session_acp::with_test_acp_program;
use crate::adapters::web_session_store::prompt_ledger::{self, PromptLedger, PromptPhase};
use ajax_core::models::AgentClient;

fn ack_count(events: &[SessionServerEvent], id: &str) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                SessionServerEvent::PromptAccepted { client_message_id } if client_message_id == id
            )
        })
        .count()
}

#[test]
fn interrupted_prompt_is_acknowledged_once_and_not_again_on_reconnect() {
    let dir = scratch_dir("prompt-ack-interrupted");
    let handle = "web/prompt-ack-interrupted";
    let script = fake_acp_fixture();

    let mut ledger = PromptLedger::default();
    ledger.upsert_queued(
        "interrupted-1".to_string(),
        "hold".to_string(),
        "hold".to_string(),
        Vec::new(),
    );
    assert!(ledger.mark_dispatching("interrupted-1"));
    prompt_ledger::persist(&dir, handle, &ledger).expect("seed ledger");

    with_test_acp_program(&script, || {
        let directory = BlockingSessionDirectory::new(dir.clone());
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        let (events, _) = directory.read_from(handle, 0);
        assert_eq!(ack_count(&events, "interrupted-1"), 1);
        assert_eq!(
            prompt_ledger::load(&dir, handle)
                .expect("load ledger")
                .entry("interrupted-1")
                .map(|entry| entry.phase),
            Some(PromptPhase::Interrupted)
        );
        drop(directory);

        let reconnected = BlockingSessionDirectory::new(dir.clone());
        reconnected
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("reacquire");
        let (replayed, _) = reconnected.read_from(handle, 0);
        assert_eq!(ack_count(&replayed, "interrupted-1"), 1);
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn begin_failure_acknowledges_unaccepted_prompt_exactly_once() {
    let events = begin_failure_events(Some("fail-1"), "boom".to_string(), false);
    assert_eq!(ack_count(&events, "fail-1"), 1);
    assert!(events
        .iter()
        .any(|event| matches!(event, SessionServerEvent::Error { message } if message == "boom")));
}

#[test]
fn begin_failure_of_already_accepted_queued_prompt_does_not_ack_again() {
    let events = begin_failure_events(Some("queued-1"), "queued prompt failed".to_string(), true);
    assert_eq!(ack_count(&events, "queued-1"), 0);
    assert_eq!(events.len(), 1);
}

#[test]
fn begin_failure_without_client_message_id_has_no_ack() {
    let events = begin_failure_events(None, "boom".to_string(), false);
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], SessionServerEvent::Error { .. }));
}

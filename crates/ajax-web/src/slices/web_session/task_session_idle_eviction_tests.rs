use super::test_support::{
    fake_acp_fixture, has_message, pump_until, scratch_dir, BlockingSessionDirectory,
};
use super::transcript::MAX_IDLE_SESSIONS;
use super::SessionServerEvent;
use crate::adapters::web_session_acp::{with_test_acp_extra_args, with_test_acp_program};
use ajax_core::models::AgentClient;
use std::time::Duration;

#[test]
fn idle_eviction_preserves_slots_with_in_flight_turn() {
    let dir = scratch_dir("evict-inflight");
    let handle_a = "web/evict-inflight-a";
    let handle_c = "web/evict-inflight-c";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle_a, &dir, "auto", AgentClient::Cursor)
                .expect("acquire a");
            directory
                .submit_prompt(handle_a, "first".to_string())
                .expect("first");
            directory.release(handle_a);

            for i in 0..MAX_IDLE_SESSIONS {
                let handle = format!("web/evict-inflight-idle-{i}");
                directory
                    .acquire(&handle, &dir, "auto", AgentClient::Cursor)
                    .expect("acquire idle");
                directory.release(&handle);
            }

            directory
                .acquire(handle_c, &dir, "auto", AgentClient::Cursor)
                .expect("acquire c");
            directory.release(handle_c);

            directory
                .acquire(handle_a, &dir, "auto", AgentClient::Cursor)
                .expect("re-acquire a");
            let (events, _) = directory.read_from(handle_a, 0);
            assert!(
                has_message(&events, "user", "first"),
                "in-flight slot must survive idle eviction"
            );
            directory.cancel(handle_a, true).expect("cancel in-flight");
            pump_until(&directory, handle_a, Duration::from_secs(5), |events| {
                events
                    .iter()
                    .any(|event| matches!(event, SessionServerEvent::TurnEnd { .. }))
            });
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn idle_eviction_preserves_slots_with_queued_prompts() {
    let dir = scratch_dir("evict-queue");
    let handle_a = "web/evict-a";
    let handle_c = "web/evict-c";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle_a, &dir, "auto", AgentClient::Cursor)
                .expect("acquire a");
            directory
                .submit_prompt(handle_a, "first".to_string())
                .expect("first");
            directory
                .submit_prompt(handle_a, "kept".to_string())
                .expect("kept");
            directory.release(handle_a);

            for i in 0..MAX_IDLE_SESSIONS {
                let handle = format!("web/evict-idle-{i}");
                directory
                    .acquire(&handle, &dir, "auto", AgentClient::Cursor)
                    .expect("acquire idle");
                directory.release(&handle);
            }

            directory
                .acquire(handle_c, &dir, "auto", AgentClient::Cursor)
                .expect("acquire c");
            directory.release(handle_c);

            directory
                .acquire(handle_a, &dir, "auto", AgentClient::Cursor)
                .expect("re-acquire a");
            directory.cancel(handle_a, true).expect("cancel keep queue");

            pump_until(&directory, handle_a, Duration::from_secs(5), |events| {
                events.iter().any(|event| {
                    matches!(
                        event,
                        SessionServerEvent::Message { text, .. } if text == "pong"
                    )
                }) && has_message(events, "user", "kept")
            });
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn idle_eviction_reclaims_finished_disconnected_sessions() {
    let dir = scratch_dir("evict-finished");
    let handle_a = "web/evict-finished-a";
    let handle_trigger = "web/evict-finished-trigger";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle_a, &dir, "auto", AgentClient::Cursor)
            .expect("acquire a");
        directory
            .submit_prompt(handle_a, "ping".to_string())
            .expect("prompt");
        directory.release(handle_a);

        pump_until(&directory, handle_a, Duration::from_secs(5), |events| {
            events.iter().any(|event| match event {
                SessionServerEvent::TurnEnd { .. } => true,
                SessionServerEvent::Message { text, .. } => text == "pong",
                _ => false,
            })
        });

        // The first pong/TurnEnd snapshot can still have `prompt_in_flight`
        // set on the agent client, so production reports `evictable == false`.
        // Wait until the session has drained to a true idle state before
        // asserting eligibility, instead of racing on the first event.
        pump_until(&directory, handle_a, Duration::from_secs(5), |_| {
            directory
                .eviction_snapshot(handle_a)
                .is_some_and(|snapshot| snapshot.evictable)
        });

        let snapshot = directory.eviction_snapshot(handle_a).expect("snapshot");
        assert!(
            snapshot.evictable,
            "finished disconnected session must become eviction-eligible"
        );

        let child_before = directory.child_id(handle_a).expect("child before");

        for i in 0..MAX_IDLE_SESSIONS {
            let handle = format!("web/evict-finished-idle-{i}");
            directory
                .acquire(&handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire idle");
            directory.release(&handle);
        }

        directory
            .acquire(handle_trigger, &dir, "auto", AgentClient::Cursor)
            .expect("acquire trigger");
        directory.release(handle_trigger);

        directory
            .acquire(handle_a, &dir, "auto", AgentClient::Cursor)
            .expect("re-acquire a");
        let child_after = directory.child_id(handle_a).expect("child after");
        assert_ne!(
            child_before, child_after,
            "idle cap must evict the finished disconnected session"
        );
        let (events, _) = directory.read_from(handle_a, 0);
        assert!(
            has_message(&events, "user", "ping"),
            "evicted session transcript reloads from disk on re-acquire"
        );
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn acquire_clears_directory_idle_release_marker() {
    let dir = scratch_dir("acquire-clear-released");
    let handle = "web/acquire-clear-released";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        directory.release(handle);
        assert_eq!(
            directory.is_marked_idle_release(handle),
            Some(true),
            "release must mark slot idle in directory"
        );

        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("re-acquire");
        assert_eq!(
            directory.is_marked_idle_release(handle),
            Some(false),
            "acquire must claim the slot in the same lock as sender lookup"
        );
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn reattached_session_survives_idle_cap_while_held() {
    let dir = scratch_dir("reattach-survives-evict");
    let handle_a = "web/reattach-survives-a";
    let handle_trigger = "web/reattach-survives-trigger";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle_a, &dir, "auto", AgentClient::Cursor)
            .expect("acquire a");
        directory.release(handle_a);
        directory
            .acquire(handle_a, &dir, "auto", AgentClient::Cursor)
            .expect("re-acquire a");
        assert_eq!(
            directory.is_marked_idle_release(handle_a),
            Some(false),
            "reattached session must not be in the idle-eviction pool"
        );

        let child_before = directory.child_id(handle_a).expect("child before");

        for i in 0..MAX_IDLE_SESSIONS {
            let handle = format!("web/reattach-survives-idle-{i}");
            directory
                .acquire(&handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire idle");
            directory.release(&handle);
        }

        directory
            .acquire(handle_trigger, &dir, "auto", AgentClient::Cursor)
            .expect("acquire trigger");
        directory.release(handle_trigger);

        assert_eq!(
            directory.child_id(handle_a),
            Some(child_before),
            "held reattached session must not be shut down during idle-cap eviction"
        );
        directory
            .submit_prompt(handle_a, "ping".to_string())
            .expect("prompt after eviction pressure");
    });

    let _ = std::fs::remove_dir_all(dir);
}

use super::task_session_commit::BacklogRetryBackoff;
use super::test_support::{
    fake_acp_fixture, note, pump_until, scratch_dir, BlockingSessionDirectory,
};
use super::SessionServerEvent;
use crate::adapters::web_session_acp::{with_test_acp_extra_args, with_test_acp_program};
use crate::adapters::web_session_store;
use ajax_core::models::AgentClient;
use std::time::{Duration, Instant};

fn message_index(events: &[SessionServerEvent], text: &str) -> Option<usize> {
    events
        .iter()
        .position(|event| matches!(event, SessionServerEvent::Message { text: t, .. } if t == text))
}

fn contains_backlog(events: &[SessionServerEvent]) -> bool {
    events.iter().any(|event| {
        matches!(
            event,
            SessionServerEvent::Message { text, .. }
                if text == "backlog-a" || text == "backlog-b"
        )
    })
}

#[cfg(unix)]
#[test]
fn unpersisted_backlog_commits_on_poll_tick_without_new_events() {
    use std::os::unix::fs::PermissionsExt;

    let dir = scratch_dir("backlog-poll-tick");
    let handle = "web/backlog-poll-tick";
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let directory = BlockingSessionDirectory::new(dir.clone());
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");

        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555))
            .expect("chmod state dir read-only");
        directory.record(handle, note("backlog-a"));
        directory.record(handle, note("backlog-b"));
        let (formed, _) = directory.read_from(handle, 0);
        assert!(
            !contains_backlog(&formed),
            "failed appends must leave the backlog unpersisted: {formed:?}"
        );
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755))
            .expect("chmod state dir writable");

        pump_until(&directory, handle, Duration::from_secs(5), |events| {
            message_index(events, "backlog-b").is_some()
        });

        let stored = web_session_store::load::<SessionServerEvent>(&dir, handle);
        let a = message_index(&stored.events, "backlog-a").expect("backlog-a persisted");
        let b = message_index(&stored.events, "backlog-b").expect("backlog-b persisted");
        assert!(
            a < b,
            "backlog must persist oldest-first: {:?}",
            stored.events
        );
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn backlog_formed_under_an_open_turn_commits_without_new_events() {
    use std::os::unix::fs::PermissionsExt;

    let dir = scratch_dir("backlog-open-turn");
    let handle = "web/backlog-open-turn";
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            let directory = BlockingSessionDirectory::new(dir.clone());
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt_with_id(handle, "open-1".into(), "first".into())
                .expect("active");

            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555))
                .expect("chmod state dir read-only");
            directory.record(handle, note("backlog-a"));
            directory.record(handle, note("backlog-b"));
            let (formed, _) = directory.read_from(handle, 0);
            assert!(
                !contains_backlog(&formed),
                "failed appends must leave the backlog unpersisted: {formed:?}"
            );
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755))
                .expect("chmod state dir writable");

            pump_until(&directory, handle, Duration::from_secs(5), |events| {
                message_index(events, "backlog-b").is_some()
            });

            let stored = web_session_store::load::<SessionServerEvent>(&dir, handle);
            let a = message_index(&stored.events, "backlog-a").expect("backlog-a persisted");
            let b = message_index(&stored.events, "backlog-b").expect("backlog-b persisted");
            assert!(
                a < b,
                "backlog must persist oldest-first: {:?}",
                stored.events
            );
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn unpersisted_backlog_commits_during_shutdown_once_writes_work() {
    use std::os::unix::fs::PermissionsExt;

    let dir = scratch_dir("backlog-shutdown");
    let handle = "web/backlog-shutdown";
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let directory = BlockingSessionDirectory::new(dir.clone());
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");

        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555))
            .expect("chmod state dir read-only");
        directory.record(handle, note("backlog-a"));
        directory.record(handle, note("backlog-b"));
        let (formed, _) = directory.read_from(handle, 0);
        assert!(
            !contains_backlog(&formed),
            "failed appends must leave the backlog unpersisted: {formed:?}"
        );
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755))
            .expect("chmod state dir writable");

        directory.drop_session(handle);

        let stored = web_session_store::load::<SessionServerEvent>(&dir, handle);
        let a = message_index(&stored.events, "backlog-a")
            .expect("backlog-a must be committed at shutdown");
        let b = message_index(&stored.events, "backlog-b")
            .expect("backlog-b must be committed at shutdown");
        assert!(
            a < b,
            "backlog must persist oldest-first: {:?}",
            stored.events
        );
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn backlog_retry_backs_off_from_two_seconds_to_a_thirty_second_cap() {
    let start = Instant::now();
    let mut backoff = BacklogRetryBackoff::default();
    assert!(backoff.is_due(start), "the first retry must not be delayed");

    let mut now = start;
    for seconds in [2, 4, 8, 16, 30, 30] {
        let delay = Duration::from_secs(seconds);
        backoff.record_failure(now);
        assert!(
            !backoff.is_due(now + delay - Duration::from_millis(1)),
            "retry must wait {delay:?} after a failure"
        );
        assert!(
            backoff.is_due(now + delay),
            "retry must be due once {delay:?} has passed"
        );
        now += delay;
    }
}

#[test]
fn backlog_formed_by_an_append_failure_waits_the_initial_delay_before_retrying() {
    let start = Instant::now();
    let mut backoff = BacklogRetryBackoff::default();
    backoff.record_failure_if_unscheduled(start);

    assert!(
        !backoff.is_due(start + Duration::from_millis(1999)),
        "the first tick after an append failure must not retry the backlog"
    );
    assert!(backoff.is_due(start + Duration::from_secs(2)));

    backoff.record_failure_if_unscheduled(start + Duration::from_secs(1));
    assert!(
        backoff.is_due(start + Duration::from_secs(2)),
        "a backlog that grows while a retry is scheduled keeps the schedule"
    );
}

#[test]
fn backlog_retry_backoff_resets_after_a_successful_commit() {
    let start = Instant::now();
    let mut backoff = BacklogRetryBackoff::default();
    backoff.record_failure(start);
    backoff.record_failure(start);
    backoff.record_success();

    assert!(
        backoff.is_due(start),
        "a successful commit clears the backoff"
    );
    backoff.record_failure(start);
    assert!(!backoff.is_due(start + Duration::from_millis(1999)));
    assert!(backoff.is_due(start + Duration::from_secs(2)));
}

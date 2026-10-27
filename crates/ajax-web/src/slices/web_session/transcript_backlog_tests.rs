use super::test_support::{
    fake_acp_fixture, note, pump_until, scratch_dir, BlockingSessionDirectory,
};
use super::SessionServerEvent;
use crate::adapters::web_session_acp::with_test_acp_program;
use crate::adapters::web_session_store;
use ajax_core::models::AgentClient;
use std::time::Duration;

fn message_index(events: &[SessionServerEvent], text: &str) -> Option<usize> {
    events
        .iter()
        .position(|event| matches!(event, SessionServerEvent::Message { text: t, .. } if t == text))
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

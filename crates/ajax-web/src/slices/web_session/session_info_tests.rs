//! Live ACP session title capture, snapshots, and transcript isolation.

use super::acp_drain::drain_acp_events;
use super::acp_map::map_acp_session_notification;
use super::acp_usage::UsageDeduper;
use super::protocol::{SessionChrome, SessionSnapshot};
use super::test_support::{fake_acp_fixture, scratch_dir, BlockingSessionDirectory};
use super::SessionServerEvent;
use crate::adapters::web_session_acp::{
    with_test_acp_extra_args, with_test_acp_program, AcpStdioClient,
};
use agent_client_protocol::schema::v1::{SessionInfoUpdate, SessionNotification, SessionUpdate};
use ajax_core::models::AgentClient;

#[test]
fn drain_stores_session_title_without_transcript_events() {
    let dir = scratch_dir("drain-session-title");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--session-info"], || {
            let (client, _report) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn");
            let mut deduper = UsageDeduper::default();
            let outcome = drain_acp_events(&client, &mut deduper);
            assert_eq!(
                outcome.session_title_update,
                Some(Some("Initial session title".to_string()))
            );
            assert!(outcome.events.is_empty());
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn typed_session_info_update_stays_out_of_transcript_map() {
    let update = SessionNotification::new(
        "sess",
        SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new().title("Fix auth flow")),
    );
    assert!(map_acp_session_notification(&update).is_empty());
}

#[test]
fn attach_snapshot_includes_advertised_title_before_first_poll() {
    let dir = scratch_dir("attach-session-title");
    let handle = "web/attach-session-title";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--session-info"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            let rt = tokio::runtime::Runtime::new().unwrap();
            let attach = rt.block_on(directory.inner().attach_snapshot(
                handle,
                "auto".to_string(),
                None,
            ));
            assert_eq!(
                attach.snapshot.session_title.as_deref(),
                Some("Initial session title")
            );
            let (events, _) = directory.read_from(handle, 0);
            assert!(!events.iter().any(|event| matches!(
                event,
                SessionServerEvent::Artifact { kind, .. } if kind == "session_info_update"
            )));
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn later_title_update_republishes_snapshot_without_model_change() {
    let dir = scratch_dir("session-title-republish");
    let handle = "web/session-title-republish";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--session-info", "--session-info-replace"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            let rt = tokio::runtime::Runtime::new().unwrap();
            let generation = directory.generation(handle);
            directory
                .submit_prompt(handle, "trigger-rename".to_string())
                .expect("prompt");
            directory.pump(handle);

            let mut cursor = 0;
            let mut saw_replacement = false;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while std::time::Instant::now() < deadline {
                let batch = rt.block_on(
                    directory
                        .inner()
                        .collect_outbound(handle, cursor, generation),
                );
                cursor = batch.cursor;
                if let Some(snapshot) = batch.snapshot {
                    if snapshot.session_title.as_deref() == Some("Renamed session") {
                        assert!(!snapshot.reset);
                        saw_replacement = true;
                        break;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert!(saw_replacement, "expected replacement title snapshot");
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn null_title_clears_live_chrome() {
    let update = SessionNotification::new(
        "sess",
        SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new().title(None)),
    );
    assert!(map_acp_session_notification(&update).is_empty());
}

#[test]
fn snapshot_serializes_session_title_field() {
    let snapshot = SessionSnapshot::new(
        0,
        "auto".to_string(),
        false,
        false,
        None,
        None,
        SessionChrome {
            session_config_options: None,
            available_commands: None,
            prompt_capabilities: None,
            session_title: Some("Fix auth flow".to_string()),
        },
    );
    let json = serde_json::to_value(&snapshot).unwrap();
    assert_eq!(json["sessionTitle"], "Fix auth flow");
}

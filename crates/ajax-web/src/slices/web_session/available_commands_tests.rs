//! Live ACP slash-command capture, snapshots, and transcript isolation.

use super::acp_drain::drain_acp_events;
use super::acp_map::map_acp_session_notification;
use super::acp_usage::UsageDeduper;
use super::protocol::{SessionChrome, SessionSnapshot};
use super::test_support::{fake_acp_fixture, scratch_dir, BlockingSessionDirectory};
use super::SessionServerEvent;
use crate::adapters::web_session_acp::{
    with_test_acp_extra_args, with_test_acp_program, AcpStdioClient, AvailableCommandDescriptor,
};
use agent_client_protocol::schema::v1::{
    AvailableCommand, AvailableCommandsUpdate, SessionNotification, SessionUpdate,
};
use ajax_core::models::AgentClient;

#[test]
fn drain_stores_available_commands_without_transcript_events() {
    let dir = scratch_dir("drain-commands");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--slash-commands"], || {
            let (client, _report) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn");
            let mut deduper = UsageDeduper::default();
            let outcome = drain_acp_events(&client, &mut deduper);
            assert_eq!(
                outcome.session_available_commands,
                Some(vec![
                    AvailableCommandDescriptor {
                        name: "web".to_string(),
                        description: "Query the web".to_string(),
                        input_hint: Some("query".to_string()),
                    },
                    AvailableCommandDescriptor {
                        name: "help".to_string(),
                        description: "Show help".to_string(),
                        input_hint: None,
                    },
                ])
            );
            assert!(outcome.events.is_empty());
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn typed_available_commands_update_stays_out_of_transcript_map() {
    let update = SessionNotification::new(
        "sess",
        SessionUpdate::AvailableCommandsUpdate(AvailableCommandsUpdate::new(vec![
            AvailableCommand::new("web", "Query the web"),
        ])),
    );
    assert!(map_acp_session_notification(&update).is_empty());
}

#[test]
fn attach_snapshot_includes_advertised_commands_before_first_poll() {
    let dir = scratch_dir("attach-commands");
    let handle = "web/attach-commands";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--slash-commands"], || {
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
                attach.snapshot.available_commands,
                Some(vec![
                    AvailableCommandDescriptor {
                        name: "web".to_string(),
                        description: "Query the web".to_string(),
                        input_hint: Some("query".to_string()),
                    },
                    AvailableCommandDescriptor {
                        name: "help".to_string(),
                        description: "Show help".to_string(),
                        input_hint: None,
                    },
                ])
            );
            let (events, _) = directory.read_from(handle, 0);
            assert!(!events.iter().any(|event| matches!(
                event,
                SessionServerEvent::Artifact { kind, .. }
                    if kind == "available_commands_update"
            )));
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn later_command_replacement_republishes_snapshot_without_model_change() {
    let dir = scratch_dir("commands-republish");
    let handle = "web/commands-republish";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--slash-commands", "--slash-commands-replace"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            let rt = tokio::runtime::Runtime::new().unwrap();
            let generation = directory.generation(handle);
            directory
                .submit_prompt(handle, "trigger-replace".to_string())
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
                    if snapshot.available_commands.as_deref()
                        == Some(&[AvailableCommandDescriptor {
                            name: "plan".to_string(),
                            description: "Create a plan".to_string(),
                            input_hint: None,
                        }])
                    {
                        assert!(!snapshot.reset);
                        saw_replacement = true;
                        break;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert!(saw_replacement, "expected replacement command snapshot");
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn reconnect_without_live_advertisement_omits_available_commands() {
    let dir = scratch_dir("commands-reconnect");
    let handle = "web/commands-reconnect";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let attach = rt.block_on(directory.inner().attach_snapshot(
            handle,
            "auto".to_string(),
            None,
        ));
        assert!(attach.snapshot.available_commands.is_none());
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn snapshot_serializes_available_commands_field() {
    let snapshot = SessionSnapshot::new(
        0,
        "auto".to_string(),
        false,
        false,
        None,
        None,
        SessionChrome {
            session_config_options: None,
            available_commands: Some(vec![AvailableCommandDescriptor {
                name: "web".to_string(),
                description: "Query the web".to_string(),
                input_hint: Some("query".to_string()),
            }]),
            prompt_capabilities: None,
            session_title: None,
        },
    );
    let json = serde_json::to_value(&snapshot).unwrap();
    assert_eq!(json["availableCommands"][0]["name"], "web");
    assert_eq!(json["availableCommands"][0]["inputHint"], "query");
}

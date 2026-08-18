use super::test_support::BlockingSessionDirectory;
use super::ws_bridge::{should_send_keepalive, MAX_SESSION_FRAME_BYTES, SESSION_PING_INTERVAL};
use super::{apply_client_message, SessionClientMessage, SessionServerEvent, TaskSessionDirectory};
use crate::adapters::web_session_acp::with_test_acp_program;
use ajax_core::models::AgentClient;
use std::{path::PathBuf, time::Duration};

fn scratch_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ajax-web-bridge-{label}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn fake_acp_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_acp.js")
}

#[test]
fn max_session_frame_bytes_is_256_kib() {
    assert_eq!(MAX_SESSION_FRAME_BYTES, 256 * 1024);
}

#[test]
fn keepalive_waits_for_silence_then_pings() {
    assert!(!should_send_keepalive(Duration::ZERO));
    assert!(!should_send_keepalive(
        SESSION_PING_INTERVAL - Duration::from_millis(1)
    ));
    assert!(should_send_keepalive(SESSION_PING_INTERVAL));
    assert!(should_send_keepalive(SESSION_PING_INTERVAL * 3));
}

#[tokio::test]
async fn apply_client_message_rejects_invalid_model() {
    let directory = TaskSessionDirectory::new(std::env::temp_dir());
    let mut generation = 0;
    let error = apply_client_message(
        &directory,
        "web/fix-login",
        std::path::Path::new("/tmp"),
        SessionClientMessage::SetModel {
            model: "bad model".to_string(),
        },
        &mut generation,
        None,
    )
    .await
    .unwrap_err();
    assert!(error.contains("whitespace"));
}

#[test]
fn apply_client_message_prompt_records_user_message_immediately() {
    let dir = scratch_dir("prompt-flush");
    let handle = "web/prompt-flush";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        let mut generation = directory.generation(handle);
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(apply_client_message(
            directory.inner(),
            handle,
            &dir,
            SessionClientMessage::Prompt {
                text: "hello".to_string(),
                client_message_id: "prompt-1".to_string(),
            },
            &mut generation,
            None,
        ))
        .expect("prompt");

        let (events, _) = directory.read_from(handle, 0);
        assert!(events.iter().any(|event| {
            matches!(
                event,
                SessionServerEvent::Message { role, text, .. }
                    if role == "user" && text == "hello"
            )
        }));
    });

    let _ = std::fs::remove_dir_all(dir);
}

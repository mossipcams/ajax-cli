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

// Regression for issue #931: in-session set_model must persist on the task
// before the host replaces its ACP child; persistence failure leaves the slot
// unchanged and returns a typed error.
#[test]
fn apply_client_message_set_model_persists_before_respawn() {
    let dir = scratch_dir("set-model-persist");
    let handle = "web/set-model";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();
    let persisted = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));
    let persisted_for_closure = std::sync::Arc::clone(&persisted);

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        let before = directory.child_id(handle).expect("child");
        let mut generation = directory.generation(handle);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let persist: super::PersistSessionModel = std::sync::Arc::new(move |model: &str| {
            *persisted_for_closure.lock().unwrap() = Some(model.to_string());
            Ok(())
        });
        rt.block_on(apply_client_message(
            directory.inner(),
            handle,
            &dir,
            SessionClientMessage::SetModel {
                model: "composer-2.5".to_string(),
            },
            &mut generation,
            Some(persist),
        ))
        .expect("set model");

        assert_eq!(persisted.lock().unwrap().as_deref(), Some("composer-2.5"));
        assert_ne!(directory.child_id(handle), Some(before));
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn apply_client_message_set_model_leaves_child_unchanged_when_persist_fails() {
    let dir = scratch_dir("set-model-persist-fail");
    let handle = "web/set-model-fail";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        let before = directory.child_id(handle).expect("child");
        let mut generation = directory.generation(handle);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let persist: super::PersistSessionModel =
            std::sync::Arc::new(|_model: &str| Err("registry write failed".to_string()));
        let error = rt
            .block_on(apply_client_message(
                directory.inner(),
                handle,
                &dir,
                SessionClientMessage::SetModel {
                    model: "composer-2.5".to_string(),
                },
                &mut generation,
                Some(persist),
            ))
            .unwrap_err();
        assert!(error.contains("registry write failed"));
        assert_eq!(directory.child_id(handle), Some(before));
    });

    let _ = std::fs::remove_dir_all(dir);
}

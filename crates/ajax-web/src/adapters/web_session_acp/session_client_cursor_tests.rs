//! Tests for the `SessionClient::Cursor` variant of [`super::session_client::SessionClient`]:
//! the Cursor SDK transport reachable through the unified enum, driven by the
//! fake Cursor SDK fixture so every assertion goes through the enum's own
//! match arms.

use std::path::PathBuf;
use std::time::Duration;

use agent_client_protocol::schema::v1::{ContentBlock, ImageContent, TextContent};

use super::agent_event::{AgentEvent, NativeEvent};
use super::cursor_sdk_spawn::{spawn_cursor_sdk, with_test_cursor_sdk_module};
use super::session_client::SessionClient;
use super::SessionConfigValue;

/// Generous per-step bound: node startup plus one scripted record burst.
const TIMEOUT: Duration = Duration::from_secs(10);

fn fake_sdk() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_cursor_sdk.mjs")
}

/// Build the Cursor session without going through the launch table.
fn session() -> SessionClient {
    let fake = fake_sdk();
    let cwd = std::env::temp_dir().join(format!(
        "ajax-web-cursor-sdk-test-{}-{}",
        std::process::id(),
        std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .replace(' ', "_")
    ));
    let _ = std::fs::remove_dir_all(&cwd);
    std::fs::create_dir_all(&cwd).expect("temp cwd must be creatable");

    with_test_cursor_sdk_module(&fake, || match spawn_cursor_sdk(&cwd, "", None) {
        Ok((client, _report)) => SessionClient::from_cursor(client),
        Err(_) => panic!("cursor sdk session must spawn against the fake"),
    })
}

fn text_blocks(text: &str) -> Vec<ContentBlock> {
    vec![ContentBlock::Text(TextContent::new(text))]
}

#[test]
fn cursor_session_reports_id_and_pid() {
    let mut client = session();
    assert_eq!(client.session_id(), "agent-fake-1");
    assert!(client.child_pid().is_some(), "child pid must be live");
    let result = client.session_new_result();
    assert_eq!(result["sessionId"], "agent-fake-1");
    let _ = client.shutdown();
}

#[test]
fn cursor_session_runs_a_text_prompt() {
    let mut client = session();

    let id = client
        .begin_prompt(&text_blocks("hi"))
        .expect("begin_prompt");

    let mut echo_seen = false;
    loop {
        let event = client
            .wait_event(TIMEOUT)
            .expect("event before turn finish");
        match &event {
            AgentEvent::Native(NativeEvent::AgentText(text)) => {
                if text.contains("echo[default]: hi") {
                    echo_seen = true;
                }
            }
            AgentEvent::Native(NativeEvent::TurnFinished { request_id, .. }) => {
                assert_eq!(*request_id, id);
                break;
            }
            _ => {}
        }
    }

    assert!(
        echo_seen,
        "the fake echo must surface: \"echo[default]: hi\""
    );
    assert!(
        !client.prompt_in_flight(),
        "prompt must no longer be in flight"
    );
    let _ = client.shutdown();
}

#[test]
fn cursor_session_applies_model_and_option_config() {
    let mut client = session();

    let outcome = client
        .apply_config_option("model", SessionConfigValue::Select("grok-4.6".to_string()))
        .expect("apply model");
    assert!(outcome.error.is_none(), "error: {:?}", outcome.error);
    assert_eq!(outcome.applied_model, "grok-4.6|effort=high");

    let outcome = client
        .apply_config_option("effort", SessionConfigValue::Select("low".to_string()))
        .expect("apply effort");
    assert!(outcome.error.is_none(), "error: {:?}", outcome.error);
    assert_eq!(outcome.applied_model, "grok-4.6|effort=low");

    let outcome = client
        .apply_config_option("effort", SessionConfigValue::Select("bogus".to_string()))
        .expect("apply bogus effort must be an outcome error, not a failure");
    assert!(
        outcome.error.is_some(),
        "invalid option value must yield an error"
    );

    let outcome = client
        .apply_config_option("mode", SessionConfigValue::Select("plan".to_string()))
        .expect("apply mode");
    assert!(outcome.error.is_none(), "error: {:?}", outcome.error);

    let _ = client.shutdown();
}

#[test]
fn cursor_session_applies_a_legacy_pin() {
    let mut client = session();

    let outcome = client
        .apply_model_pin("cursor-grok-4.6-low")
        .expect("apply_model_pin");
    assert!(outcome.error.is_none(), "error: {:?}", outcome.error);
    assert_eq!(outcome.applied_model, "grok-4.6|effort=low");

    let _ = client.shutdown();
}

#[test]
fn cursor_session_rejects_permission_and_elicitation_responses() {
    let mut client = session();

    let err = client
        .respond_client_request(&serde_json::json!(1), serde_json::json!({}))
        .expect_err("respond_client_request must be rejected");
    assert!(
        err.contains("Cursor SDK mode"),
        "rejection must name the Cursor SDK mode: {err:?}"
    );

    let _ = client.shutdown();
}

#[test]
fn cursor_session_shutdown_reports_no_error() {
    let mut client = session();
    assert_eq!(
        client.shutdown(),
        None,
        "successful teardown must not report an error"
    );
    assert!(
        client.child_pid().is_none(),
        "pid must be gone after shutdown"
    );
}

fn sample_image_block() -> ContentBlock {
    ContentBlock::Image(ImageContent::new(
        "aGk=".to_string(),
        "image/png".to_string(),
    ))
}

#[test]
fn cursor_session_forwards_images() {
    let mut client = session();
    let blocks = vec![
        ContentBlock::Text(TextContent::new("look")),
        sample_image_block(),
    ];

    let id = client
        .begin_prompt(&blocks)
        .expect("begin_prompt with text and an image");

    let mut echo_seen = false;
    loop {
        let event = client
            .wait_event(TIMEOUT)
            .expect("event before turn finish");
        match &event {
            AgentEvent::Native(NativeEvent::AgentText(text)) => {
                if text.contains("echo[default]: look") && text.contains("images=1") {
                    echo_seen = true;
                }
            }
            AgentEvent::Native(NativeEvent::TurnFinished { request_id, .. }) => {
                assert_eq!(*request_id, id);
                break;
            }
            _ => {}
        }
    }

    assert!(
        echo_seen,
        "the fake echo must surface: \"echo[default]: look images=1\""
    );
    let _ = client.shutdown();
}

#[test]
fn cursor_session_accepts_an_image_only_prompt() {
    let mut client = session();
    let blocks = vec![sample_image_block()];

    let id = client
        .begin_prompt(&blocks)
        .expect("begin_prompt with only an image");

    let mut echo_seen = false;
    loop {
        let event = client
            .wait_event(TIMEOUT)
            .expect("event before turn finish");
        match &event {
            AgentEvent::Native(NativeEvent::AgentText(text)) => {
                if text.contains("images=1") {
                    echo_seen = true;
                }
            }
            AgentEvent::Native(NativeEvent::TurnFinished { request_id, .. }) => {
                assert_eq!(*request_id, id);
                break;
            }
            _ => {}
        }
    }

    assert!(echo_seen, "the fake echo must surface: \"images=1\"");
    let _ = client.shutdown();
}

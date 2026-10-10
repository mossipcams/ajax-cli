//! Tests for the `SessionClient::Claude` variant of [`super::session_client::SessionClient`]:
//! the Claude Agent SDK transport reachable through the unified enum, driven by
//! the fake Claude Agent SDK fixture so every assertion goes through the enum's
//! own match arms.

use std::path::{Path, PathBuf};
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    ContentBlock, ElicitationAction, SessionConfigOptionValue, SessionUpdate, TextContent,
};
use ajax_core::models::AgentClient;

use super::claude_sdk_spawn::with_test_claude_sdk_module;
use super::client::{AcpClientEvent, AcpSpawnError, RestoreFailure};
use super::session_client::SessionClient;

/// Generous per-step bound: node startup plus one scripted record burst.
const TIMEOUT: Duration = Duration::from_secs(10);

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// The fake Claude Agent SDK fixture path under the crate.
fn fake_sdk() -> PathBuf {
    manifest_dir().join("tests/fixtures/fake_claude_sdk.mjs")
}

/// Run `f` with the fake SDK module override installed.
fn with_fake<R>(f: impl FnOnce() -> R) -> R {
    with_test_claude_sdk_module(&fake_sdk(), f)
}

fn text_blocks(text: &str) -> Vec<ContentBlock> {
    vec![ContentBlock::Text(TextContent::new(text))]
}

/// Spawn the Claude transport through the enum and return the variant's state.
fn spawn_claude_variant() -> (SessionClient, super::client::SpawnReport) {
    SessionClient::spawn_with_operator_pin(AgentClient::Claude, manifest_dir(), "default", None)
        .expect("fake claude sdk spawn must succeed")
}

#[test]
fn spawn_returns_claude_variant_with_report() {
    with_fake(|| {
        let (client, report) = spawn_claude_variant();

        assert!(
            !client.session_id().is_empty(),
            "session id must be non-empty"
        );

        let ids: Vec<&str> = report
            .config_options
            .as_ref()
            .expect("config options must be advertised")
            .iter()
            .map(|option| option.id.0.as_ref())
            .collect();
        assert!(ids.contains(&"model"), "model option id: {ids:?}");
        assert!(ids.contains(&"effort"), "effort option id: {ids:?}");
    });
}

#[test]
fn prompt_flow_streams_through_the_enum() {
    with_fake(|| {
        let (mut client, _report) = spawn_claude_variant();

        let id = client
            .begin_prompt(&text_blocks("hello"))
            .expect("begin_prompt");

        let mut chunk_seen = false;
        let mut usage_seen = false;
        loop {
            let event = client
                .wait_event(TIMEOUT)
                .expect("event before RequestFinished");
            match &event {
                AcpClientEvent::SessionUpdate(notification) => match &notification.update {
                    SessionUpdate::AgentMessageChunk(chunk) => match &chunk.content {
                        ContentBlock::Text(text) => {
                            assert_eq!(text.text, "ok");
                            chunk_seen = true;
                        }
                        _ => panic!("expected a text chunk, got {:?}", chunk.content),
                    },
                    SessionUpdate::UsageUpdate(_) => usage_seen = true,
                    _ => {}
                },
                AcpClientEvent::RequestFinished {
                    id: finished_id,
                    result,
                    ..
                } => {
                    assert_eq!(*finished_id, id);
                    let stop_reason = result
                        .as_ref()
                        .expect("ok result")
                        .get("stopReason")
                        .and_then(|s| s.as_str());
                    assert_eq!(stop_reason, Some("end_turn"));
                    break;
                }
                _ => {}
            }
        }

        assert!(chunk_seen, "a message chunk must arrive");
        assert!(usage_seen, "a UsageUpdate must arrive");
    });
}

#[test]
fn cancel_of_slow_run_and_idle_cancel_through_the_enum() {
    with_fake(|| {
        let (mut client, _report) = spawn_claude_variant();

        let id = client
            .begin_prompt(&text_blocks("please SLOW down"))
            .expect("begin_prompt");

        // Wait for the first streamed chunk so the prompt is genuinely in flight.
        loop {
            let event = client
                .wait_event(TIMEOUT)
                .expect("event during slow prompt");
            if matches!(
                &event,
                AcpClientEvent::SessionUpdate(notification)
                    if matches!(&notification.update, SessionUpdate::AgentMessageChunk(_))
            ) {
                break;
            }
        }

        let outcome = client.cancel().expect("cancel of an in-flight prompt");
        assert!(outcome.permissions.is_empty());
        assert!(outcome.elicitations.is_empty());

        // Drain until the run ends with stopReason cancelled.
        loop {
            let event = client.wait_event(TIMEOUT).expect("event after cancel");
            if let AcpClientEvent::RequestFinished {
                id: finished_id,
                result,
                ..
            } = &event
            {
                assert_eq!(*finished_id, id);
                let stop_reason = result
                    .as_ref()
                    .expect("ok result")
                    .get("stopReason")
                    .and_then(|s| s.as_str());
                assert_eq!(stop_reason, Some("cancelled"));
                break;
            }
        }

        // Cancel with no run in flight is an error.
        assert!(client.cancel().is_err(), "cancel with no run must fail");
    });
}

#[test]
fn request_responses_are_rejected_in_claude_mode() {
    with_fake(|| {
        let (mut client, _report) = spawn_claude_variant();

        client
            .respond_client_request(&serde_json::json!(1), serde_json::json!({}))
            .expect_err("respond_client_request must be rejected");
        client
            .respond_elicitation("r-1", ElicitationAction::Decline)
            .expect_err("respond_elicitation must be rejected");
    });
}

#[test]
fn model_pin_and_config_options_flow_through_the_enum() {
    with_fake(|| {
        let (client, _report) = spawn_claude_variant();

        let outcome = client
            .apply_model_pin("haiku|effort=high")
            .expect("apply_model_pin");
        assert!(outcome.error.is_none(), "error: {:?}", outcome.error);
        assert_eq!(outcome.applied_model, "haiku");

        // Effort option updates through apply_config_option.
        let effort = SessionConfigOptionValue::ValueId {
            value: agent_client_protocol::schema::v1::SessionConfigValueId::new("low"),
        };
        let outcome = client
            .apply_config_option("effort", effort)
            .expect("apply effort");
        assert!(outcome.error.is_none(), "error: {:?}", outcome.error);
        let options = outcome
            .config_options
            .as_ref()
            .expect("config options must be advertised");
        let option = options
            .iter()
            .find(|option| option.id.0.as_ref() == "effort")
            .expect("effort option must be advertised");
        let agent_client_protocol::schema::v1::SessionConfigKind::Select(select) = &option.kind
        else {
            panic!("effort option must be a select");
        };
        assert_eq!(select.current_value.0.as_ref(), "low");

        // An unknown model id surfaces as an outcome error, not a hard failure.
        let bad_model = SessionConfigOptionValue::ValueId {
            value: agent_client_protocol::schema::v1::SessionConfigValueId::new("not-a-model"),
        };
        let outcome = client
            .apply_config_option("model", bad_model)
            .expect("apply model");
        assert!(
            outcome
                .error
                .as_ref()
                .is_some_and(|err| err.contains("unknown model")),
            "error: {:?}",
            outcome.error
        );

        // An unsupported option id is likewise an outcome error.
        let nope = SessionConfigOptionValue::ValueId {
            value: agent_client_protocol::schema::v1::SessionConfigValueId::new("x"),
        };
        let outcome = client
            .apply_config_option("nope", nope)
            .expect("apply nope");
        assert!(
            outcome.error.is_some(),
            "unsupported option must yield an error"
        );
    });
}

#[test]
fn resume_missing_session_fails_through_the_enum() {
    with_fake(|| {
        let result = SessionClient::spawn_with_operator_pin(
            AgentClient::Claude,
            manifest_dir(),
            "default",
            Some("missing-1"),
        );
        match result {
            Ok(_) => panic!("resuming a nonexistent session must fail"),
            Err(AcpSpawnError::Restore(RestoreFailure::Rejected { session_id, .. })) => {
                assert_eq!(session_id, "missing-1");
            }
            Err(other) => panic!("expected a restore rejection, got {other:?}"),
        }
    });
}

#[test]
fn shutdown_reports_the_session_id_once() {
    with_fake(|| {
        let (mut client, _report) = spawn_claude_variant();
        let session_id = client.session_id().to_string();

        let first = client.shutdown();
        assert_eq!(first.as_deref(), Some(session_id.as_str()));
        assert!(client.shutdown().is_none(), "second shutdown must be None");
    });
}

#[test]
fn other_agents_do_not_take_the_claude_path() {
    with_fake(|| {
        // With the module override installed, only AgentClient::Claude routes to
        // the SDK spawn; the others keep their existing dispatch.
        assert!(!super::claude_sdk_spawn::uses_claude_sdk(
            AgentClient::Cursor
        ));
    });
}

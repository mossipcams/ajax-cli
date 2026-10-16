//! Tests for the Pi RPC spawn path of [`super::session_client`], running the
//! fake Pi RPC fixture as a directly-executable child (node shebang) through
//! [`SessionClient::spawn_with_operator_pin`] and asserting: the spawn report
//! shape, session resume, fail-closed restore when Pi starts the wrong
//! session, and the test-only Pi gating.

use std::path::{Path, PathBuf};
use std::time::Duration;

use agent_client_protocol::schema::v1::{ContentBlock, ImageContent, TextContent};

use super::agent_event::{AgentEvent, NativeEvent};
use super::client::AcpSpawnError;
use super::session_client::{transport_for, with_test_pi_program, SessionClient};
use ajax_core::adapters::HarnessTransport;
use ajax_core::models::AgentClient;

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_pi_rpc.js")
}

fn worktree_path() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn pi_spawn_returns_client_and_report() {
    with_test_pi_program(&fixture_path(), &[], || {
        let (client, report) = match SessionClient::spawn_with_operator_pin(
            AgentClient::Pi,
            worktree_path(),
            "default",
            None,
        ) {
            Ok(pair) => pair,
            Err(_) => panic!("fake pi rpc spawn must succeed"),
        };

        assert!(matches!(client, SessionClient::Pi(_)));
        assert!(report.load_session_advertised);
        assert!(report.restore_advertised);
        assert!(!report.close_advertised);
        assert!(!report.resumed);
        assert_eq!(report.applied_model, "fake-provider/fake-model");
        assert!(report.model_apply_error.is_none());
        let options = report
            .config_options
            .expect("pi rpc advertises config options");
        assert!(options.iter().any(|option| option.id == "model"));
        assert!(options.iter().any(|option| option.id == "thought_level"));
    });
}

#[test]
fn pi_spawn_resumes_requested_session() {
    with_test_pi_program(&fixture_path(), &[], || {
        let (client, report) = match SessionClient::spawn_with_operator_pin(
            AgentClient::Pi,
            worktree_path(),
            "default",
            Some("resumed-1"),
        ) {
            Ok(pair) => pair,
            Err(_) => panic!("fake pi rpc resume must succeed"),
        };

        assert!(report.resumed);
        assert_eq!(client.session_id(), "resumed-1");
    });
}

#[test]
fn pi_spawn_fails_closed_when_session_id_differs() {
    with_test_pi_program(&fixture_path(), &["--ignore-session"], || {
        let error = match SessionClient::spawn_with_operator_pin(
            AgentClient::Pi,
            worktree_path(),
            "default",
            Some("resumed-1"),
        ) {
            Err(error) => error,
            Ok(_) => panic!("a mismatched session id must fail closed, never fall back"),
        };

        match error {
            AcpSpawnError::Restore(super::client::RestoreFailure::Rejected {
                session_id, ..
            }) => {
                assert_eq!(session_id, "resumed-1");
            }
            _ => panic!("expected a Rejected restore failure"),
        }
    });
}

#[test]
fn uses_pi_rpc_requires_pi_agent_and_override() {
    assert!(
        transport_for(AgentClient::Pi) != HarnessTransport::PiRpc,
        "production gating: pi rpc stays off without the test override"
    );
    with_test_pi_program(&fixture_path(), &[], || {
        assert!(transport_for(AgentClient::Pi) == HarnessTransport::PiRpc);
        assert!(
            transport_for(AgentClient::Cursor) != HarnessTransport::PiRpc,
            "the override must not switch other agents to the pi path"
        );
    });
    assert!(
        transport_for(AgentClient::Pi) != HarnessTransport::PiRpc,
        "the override must be cleared after with_test_pi_program returns"
    );
}

const TIMEOUT: Duration = Duration::from_secs(10);

#[test]
fn pi_prompt_forwards_images() {
    with_test_pi_program(&fixture_path(), &[], || {
        let (mut client, report) = match SessionClient::spawn_with_operator_pin(
            AgentClient::Pi,
            worktree_path(),
            "default",
            None,
        ) {
            Ok(pair) => pair,
            Err(_) => panic!("fake pi rpc spawn must succeed"),
        };

        assert!(report.prompt_capabilities.image);

        let blocks = vec![
            ContentBlock::Text(TextContent::new("look")),
            ContentBlock::Image(ImageContent::new("AAAA", "image/png")),
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
                    if text.contains("images=1:image/image/png") {
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
            "the fake echo must surface: \"images=1:image/image/png\""
        );
    });
}

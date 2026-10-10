//! Tests for [`super::session_client::SessionClient`] using the Pi RPC variant
//! backed by the `fake_pi_rpc.js` fixture.

use std::path::Path;
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    ContentBlock, SessionConfigKind, SessionConfigOptionValue, SessionUpdate,
};

use super::pi_rpc_client::PiRpcClient;
use super::session_client::SessionClient;
use super::{AcpClientEvent, CancelOutcome};

const WAIT: Duration = Duration::from_secs(10);

fn fixture_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_pi_rpc.js")
}

fn manifest_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn spawn_pi(extra_args: &[String]) -> SessionClient {
    let client = PiRpcClient::spawn(
        &fixture_path(),
        extra_args,
        &manifest_dir(),
        None,
        Duration::from_secs(10),
    )
    .expect("fake_pi_rpc.js fixture should spawn");
    SessionClient::from_pi(client)
}

fn text_block(text: &str) -> ContentBlock {
    ContentBlock::Text(agent_client_protocol::schema::v1::TextContent::new(text))
}

fn is_agent_message_chunk(event: &AcpClientEvent) -> bool {
    matches!(
        event,
        AcpClientEvent::SessionUpdate(notification)
            if matches!(notification.update, SessionUpdate::AgentMessageChunk(_))
    )
}

/// True when `event` is a `SessionUpdate` carrying a `UsageUpdate`.
fn is_usage_update(event: &AcpClientEvent) -> bool {
    matches!(
        event,
        AcpClientEvent::SessionUpdate(notification)
            if matches!(notification.update, SessionUpdate::UsageUpdate(_))
    )
}

#[test]
fn pi_variant_carries_session_id_and_new_result() {
    let mut session = spawn_pi(&[]);

    assert_eq!(session.session_id(), "fake-pi-rpc-session-1");
    assert_eq!(
        session.session_new_result()["sessionId"],
        serde_json::json!("fake-pi-rpc-session-1")
    );

    let _ = session.shutdown();
}

#[test]
fn pi_variant_prompt_flow_streams_chunk_then_request_finished() {
    let mut session = spawn_pi(&[]);

    let id = session
        .begin_prompt(&[text_block("hello fake pi")])
        .expect("begin_prompt should succeed");

    let first = session.wait_event(WAIT).expect("first event should arrive");
    assert!(
        is_agent_message_chunk(&first),
        "expected an AgentMessageChunk, got {first:?}"
    );

    let second = loop {
        let event = session
            .wait_event(WAIT)
            .expect("RequestFinished should follow the chunk");
        match event {
            AcpClientEvent::SessionUpdate(notification)
                if matches!(notification.update, SessionUpdate::UsageUpdate(_)) =>
            {
                continue
            }
            AcpClientEvent::RequestFinished { .. } => break event,
            other => panic!("expected RequestFinished, got {other:?}"),
        }
    };
    match &second {
        AcpClientEvent::RequestFinished {
            id: finished_id, ..
        } => {
            assert_eq!(*finished_id, id);
        }
        other => panic!("expected RequestFinished, got {other:?}"),
    }

    let _ = session.shutdown();
}

#[test]
fn pi_variant_cancel_active_run_yields_empty_outcome_and_cancelled_stop() {
    let mut session = spawn_pi(&["--hold-run".to_string()]);

    let id = session
        .begin_prompt(&[text_block("please hold")])
        .expect("begin_prompt should succeed");

    assert!(session.prompt_in_flight());

    let outcome: CancelOutcome = session.cancel().expect("cancel should succeed");
    assert!(outcome.permissions.is_empty());
    assert!(outcome.elicitations.is_empty());

    // The fake may emit a chunk before the run finishes; drain until RequestFinished.
    let finished = loop {
        match session.wait_event(WAIT) {
            None => panic!("run should finish after cancel"),
            Some(event) if matches!(event, AcpClientEvent::RequestFinished { .. }) => break event,
            Some(event) if is_agent_message_chunk(&event) => {}
            Some(event) if is_usage_update(&event) => {}
            Some(other) => panic!("expected chunk or RequestFinished after cancel, got {other:?}"),
        }
    };
    match &finished {
        AcpClientEvent::RequestFinished {
            id: finished_id,
            result,
            ..
        } => {
            assert_eq!(*finished_id, id);
            let value = result
                .as_ref()
                .expect("held run should finish with a result");
            assert!(
                value.to_string().contains("cancelled"),
                "stopReason should be cancelled, got {value:?}"
            );
        }
        other => panic!("expected RequestFinished after cancel, got {other:?}"),
    }

    let _ = session.shutdown();
}

#[test]
fn pi_variant_cancel_without_run_is_err() {
    let mut session = spawn_pi(&[]);

    assert!(!session.prompt_in_flight());
    assert!(session.cancel().is_err(), "cancel with no run should fail");

    let _ = session.shutdown();
}

#[test]
fn pi_variant_rejects_request_and_elicitation_responses() {
    let mut session = spawn_pi(&[]);

    let err = session
        .respond_client_request(&serde_json::json!("req-1"), serde_json::json!({}))
        .expect_err("respond_client_request should be unsupported");
    assert_eq!(err, "Pi RPC mode has no permission or elicitation requests");

    let err = session
        .respond_elicitation(
            "elicit-1",
            agent_client_protocol::schema::v1::ElicitationAction::Decline,
        )
        .expect_err("respond_elicitation should be unsupported");
    assert_eq!(err, "Pi RPC mode has no permission or elicitation requests");

    let _ = session.shutdown();
}

#[test]
fn pi_variant_applies_model_and_option_changes() {
    let mut session = spawn_pi(&[]);

    let outcome = session
        .apply_model_pin("fake-provider/fake-model|thought_level=high")
        .expect("apply_model_pin should apply the pin");
    assert!(outcome.error.is_none());
    let current = outcome
        .config_options
        .as_deref()
        .and_then(|options| {
            options
                .iter()
                .find(|option| &*option.id.0 == "thought_level")
                .map(|option| &option.kind)
        })
        .expect("thought_level option must be advertised");
    match current {
        SessionConfigKind::Select(select) => {
            assert_eq!(select.current_value.0.to_string(), "high");
        }
        other => panic!("expected a select thought_level option, got {other:?}"),
    }

    let outcome = session
        .apply_config_option(
            "model",
            SessionConfigOptionValue::value_id("fake-provider/does-not-exist"),
        )
        .expect("apply_config_option should report rather than fail");
    let error = outcome
        .error
        .as_deref()
        .expect("the model failure must be reported");
    assert!(
        error.contains("Model not found"),
        "expected a Pi model error, got {error}"
    );

    let _ = session.shutdown();
}

#[test]
fn pi_variant_shutdown_returns_session_id_once_then_none() {
    let mut session = spawn_pi(&[]);

    let first = session
        .shutdown()
        .expect("first shutdown returns the session id");
    assert_eq!(first, "fake-pi-rpc-session-1");

    assert!(session.shutdown().is_none(), "second shutdown returns None");
}

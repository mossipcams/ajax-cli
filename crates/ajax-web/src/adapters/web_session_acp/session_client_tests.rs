//! Tests for [`super::session_client::SessionClient`] using the Pi RPC variant
//! backed by the `fake_pi_rpc.js` fixture.

use std::path::Path;
use std::time::Duration;

use agent_client_protocol::schema::v1::{ContentBlock, ImageContent, ResourceLink, TextContent};

use super::agent_event::NativeEvent;
use super::pi_rpc_client::PiRpcClient;
use super::session_client::{prompt_text, SessionClient};
use super::{AgentEvent, CancelOutcome, SessionConfigValue};

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

fn is_agent_message_chunk(event: &AgentEvent) -> bool {
    matches!(event, AgentEvent::Native(NativeEvent::AgentText(_)))
}

/// True when `event` is a native usage event.
fn is_usage_update(event: &AgentEvent) -> bool {
    matches!(event, AgentEvent::Native(NativeEvent::Usage { .. }))
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
            AgentEvent::Native(NativeEvent::Usage { .. }) => continue,
            AgentEvent::Native(NativeEvent::TurnFinished { .. }) => break event,
            other => panic!("expected RequestFinished, got {other:?}"),
        }
    };
    match &second {
        AgentEvent::Native(NativeEvent::TurnFinished {
            request_id: finished_id,
            ..
        }) => {
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
            Some(event)
                if matches!(event, AgentEvent::Native(NativeEvent::TurnFinished { .. })) =>
            {
                break event
            }
            Some(event) if is_agent_message_chunk(&event) => {}
            Some(event) if is_usage_update(&event) => {}
            Some(other) => panic!("expected chunk or RequestFinished after cancel, got {other:?}"),
        }
    };
    match &finished {
        AgentEvent::Native(NativeEvent::TurnFinished {
            request_id: finished_id,
            result,
            ..
        }) => {
            assert_eq!(*finished_id, id);
            let value = result
                .as_ref()
                .expect("held run should finish with a result");
            assert!(
                value.contains("cancelled"),
                "stop reason should be cancelled, got {value:?}"
            );
        }
        other => panic!("expected RequestFinished after cancel, got {other:?}"),
    }

    let _ = session.shutdown();
}

#[test]
fn issue_1272_pi_idle_cancel_yields_empty_success() {
    let mut session = spawn_pi(&[]);

    assert!(!session.prompt_in_flight());
    let outcome = session.cancel().expect("idle cancel must succeed");
    assert!(outcome.permissions.is_empty());
    assert!(outcome.elicitations.is_empty());
    assert!(!session.prompt_in_flight());
    assert!(!session.host_exited());

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
        .respond_elicitation("elicit-1", "decline", None)
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
                .find(|option| option.id == "thought_level")
                .map(|option| &option.kind)
        })
        .expect("thought_level option must be advertised");
    match current {
        super::NeutralConfigKind::Select { current_value, .. } => {
            assert_eq!(current_value.as_str(), "high");
        }
        other => panic!("expected a select thought_level option, got {other:?}"),
    }

    let outcome = session
        .apply_config_option(
            "model",
            SessionConfigValue::Select("fake-provider/does-not-exist".to_string()),
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
fn issue_1273_pi_shutdown_and_detach_return_no_error() {
    for detach in [false, true] {
        let mut session = spawn_pi(&[]);
        let session_id = session.session_id().to_string();
        let error = if detach {
            session.detach()
        } else {
            session.shutdown()
        };
        assert_eq!(error, None, "successful teardown must not report an error");
        assert!(session.host_exited());
        assert_eq!(session.session_id(), session_id);
        assert_eq!(session.shutdown(), None);
        assert_eq!(session.detach(), None);
        assert!(
            session.cancel().is_err(),
            "a shut-down transport is unavailable"
        );
    }
}

#[test]
fn prompt_text_flattens_text_links_and_images() {
    let blocks = vec![
        ContentBlock::Text(TextContent::new("one")),
        ContentBlock::ResourceLink(ResourceLink::new("f", "file:///a.rs")),
        ContentBlock::Image(ImageContent::new("aGVsbG8=", "image/png")),
        ContentBlock::Text(TextContent::new("two")),
    ];

    assert_eq!(
        prompt_text(&blocks),
        "one\n\nfile:///a.rs\n\n[image omitted]\n\ntwo"
    );
    assert_eq!(prompt_text(&[]), "");
}

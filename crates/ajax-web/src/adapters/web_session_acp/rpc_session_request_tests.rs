//! Integration tests for [`RpcSession::request`], running the real `node`
//! child behind the fake Pi RPC fixture (LF-delimited JSONL over stdin/stdout)
//! and asserting that a one-shot request correlates its response by id, keeps
//! every unrelated record queued for the next [`RpcStep`] call, and surfaces
//! failures (rejected model, silent command, child exit).

use std::path::Path;
use std::time::Duration;

use agent_client_protocol::schema::v1::{ContentBlock, SessionUpdate};
use serde_json::json;

use super::client::AcpClientEvent;
use super::jsonl_process::JsonlProcess;
use super::pi_rpc_handshake::handshake;
use super::pi_rpc_map::map_record;
use super::rpc_session::{RpcSession, RpcStep};

/// Generous bound: node startup plus one scripted record burst.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// Bound for "this must not hang" negative tests.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Per-step bound after a request has queued records.
const STEP_TIMEOUT: Duration = Duration::from_secs(10);
/// Short bound that must elapse with no response on the wire (timeout checks).
const SILENT_TIMEOUT: Duration = Duration::from_millis(500);

/// Spawn the fake pi RPC child under `node`, passing `extra_flags` to the
/// fixture script. Fails loudly (panics) when `node` is missing.
fn spawn_fake_pi(extra_flags: &[&str]) -> JsonlProcess {
    let mut args: Vec<String> = vec![Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fake_pi_rpc.js")
        .to_string_lossy()
        .into_owned()];
    args.extend(extra_flags.iter().copied().map(str::to_owned));
    let cwd = Path::new(env!("CARGO_MANIFEST_DIR"));
    JsonlProcess::spawn(Path::new("node"), &args, cwd)
        .expect("fake pi rpc fixture must spawn (is node installed?)")
}

/// Spawn the fake, run the real four-command handshake, and build a session.
fn handshaken_session(extra_flags: &[&str]) -> RpcSession {
    let mut process = spawn_fake_pi(extra_flags);
    let handshake = handshake(&mut process, HANDSHAKE_TIMEOUT).expect("handshake");
    RpcSession::new(process, handshake, map_record)
}

#[test]
fn request_stats_returns_data() {
    let mut session = handshaken_session(&[]);

    let data = session
        .request("get_session_stats", json!({}), REQUEST_TIMEOUT)
        .expect("get_session_stats must succeed");

    assert_eq!(
        data.pointer("/contextUsage/contextWindow")
            .and_then(serde_json::Value::as_u64),
        Some(1_000_000),
        "context window must be 1000000: {data:?}"
    );
    assert_eq!(
        data.get("sessionId").and_then(|v| v.as_str()),
        Some("fake-pi-rpc-session-1"),
    );
}

#[test]
fn request_set_model_returns_model() {
    let mut session = handshaken_session(&[]);

    let data = session
        .request(
            "set_model",
            json!({ "provider": "opencode-go", "modelId": "glm-5.2" }),
            REQUEST_TIMEOUT,
        )
        .expect("set_model must succeed");

    assert_eq!(data.get("id").and_then(|v| v.as_str()), Some("glm-5.2"));
}

#[test]
fn request_set_model_missing_is_error() {
    let mut session = handshaken_session(&[]);

    let err = session
        .request(
            "set_model",
            json!({ "provider": "opencode-go", "modelId": "does-not-exist" }),
            REQUEST_TIMEOUT,
        )
        .expect_err("unknown model must fail");

    assert!(err.contains("Model not found"), "error text: {err}");
}

#[test]
fn request_set_thinking_level_returns_null() {
    let mut session = handshaken_session(&[]);

    let data = session
        .request(
            "set_thinking_level",
            json!({ "level": "high" }),
            REQUEST_TIMEOUT,
        )
        .expect("set_thinking_level must succeed");

    assert!(data.is_null(), "no-data response maps to Null: {data:?}");
}

#[test]
fn request_preserves_interleaved_events_for_next_step() {
    let mut session = handshaken_session(&["--hold-run"]);
    session.begin_prompt("hi").expect("begin_prompt");

    let data = session
        .request("get_session_stats", json!({}), REQUEST_TIMEOUT)
        .expect("stats request must succeed");
    assert_eq!(data.get("totalMessages").and_then(|v| v.as_u64()), Some(3));

    // The prompt's agent_start + text_delta arrived while the request waited;
    // nothing may be dropped, and they surface in order.
    let step = session.next_step(STEP_TIMEOUT);
    assert!(
        matches!(step, RpcStep::Events(_)),
        "events must not be dropped: {step:?}"
    );
    if let RpcStep::Events(events) = &step {
        assert_eq!(
            events.len(),
            1,
            "one agent message chunk expected: {events:?}"
        );
        match &events[0] {
            AcpClientEvent::SessionUpdate(update) => match &update.update {
                SessionUpdate::AgentMessageChunk(chunk) => {
                    assert!(matches!(chunk.content, ContentBlock::Text(_)))
                }
                other => panic!("expected AgentMessageChunk, got {other:?}"),
            },
            other => panic!("expected SessionUpdate, got {other:?}"),
        }
    }

    // The held run still needs an abort to settle: request queued nothing of
    // its own, and the next step is idle.
    session.abort().expect("abort");
}

#[test]
fn request_times_out_for_silent_command() {
    let mut session = handshaken_session(&["--silent-stats"]);

    let err = session
        .request("get_session_stats", json!({}), SILENT_TIMEOUT)
        .expect_err("silence must time out");

    assert!(
        err.contains("get_session_stats"),
        "error names the command: {err}"
    );
}

#[test]
fn request_after_child_exit_errors_and_next_step_exits_once() {
    let mut process = spawn_fake_pi(&[]);
    let handshake_result = handshake(&mut process, HANDSHAKE_TIMEOUT).expect("handshake");

    // Closing stdin is pi's documented orderly shutdown.
    process.close_stdin();
    let mut session = RpcSession::new(process, handshake_result, map_record);

    let err = session
        .request("get_session_stats", json!({}), REQUEST_TIMEOUT)
        .expect_err("a dead child must not answer");
    // A closed stdin can no longer be written to, so the request fails at
    // send; the child's exit must still surface exactly once via `next_step`.
    assert!(
        err.to_lowercase().contains("closed"),
        "a closed child cannot be sent to: {err}"
    );

    let step = session.next_step(STEP_TIMEOUT);
    assert!(
        matches!(step, RpcStep::Exited),
        "first step after exit: {step:?}"
    );
    let step2 = session.next_step(SILENT_TIMEOUT);
    assert!(
        matches!(step2, RpcStep::Idle),
        "exit must be reported exactly once: {step2:?}"
    );
}

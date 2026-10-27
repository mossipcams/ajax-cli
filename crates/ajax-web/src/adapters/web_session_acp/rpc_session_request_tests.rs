use std::path::Path;
use std::time::Duration;

use serde_json::json;

use super::agent_event::{AgentEvent, NativeEvent};
use super::jsonl_process::JsonlProcess;
use super::pi_rpc_handshake::handshake;
use super::pi_rpc_map::map_record;
use super::rpc_session::{RpcSession, RpcStep};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const STEP_TIMEOUT: Duration = Duration::from_secs(10);
const SILENT_TIMEOUT: Duration = Duration::from_millis(500);

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
            AgentEvent::Native(NativeEvent::AgentText(_)) => {}
            other => panic!("expected an AgentText, got {other:?}"),
        }
    }

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

    process.close_stdin();
    let mut session = RpcSession::new(process, handshake_result, map_record);

    let err = session
        .request("get_session_stats", json!({}), REQUEST_TIMEOUT)
        .expect_err("a dead child must not answer");
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

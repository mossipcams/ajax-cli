//! Tests for [`super::pi_rpc_client`], running the fake Pi RPC fixture as a
//! directly-executable child (the file has a node shebang) and asserting that
//! [`PiRpcClient`] turns the JSONL record stream into ACP client events: spawn
//! and session id, prompt run, abort run, guard errors, rejection, resume id,
//! shutdown, and request-id sequencing.

use std::path::{Path, PathBuf};
use std::time::Duration;

use agent_client_protocol::schema::v1::{ContentBlock, SessionUpdate, TextContent};

use super::client::AcpClientEvent;
use super::pi_rpc_client::PiRpcClient;

/// Generous per-step bound: node startup plus one scripted record burst.
const TIMEOUT: Duration = Duration::from_secs(10);

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_pi_rpc.js")
}

/// Spawn the fake pi RPC child directly (executable, node shebang), passing
/// `extra_args` as the fixture switches. Fails loudly when spawn fails.
fn spawn_client(extra_args: &[&str]) -> PiRpcClient {
    let args: Vec<String> = extra_args.iter().map(|arg| (*arg).to_string()).collect();
    let cwd = Path::new(env!("CARGO_MANIFEST_DIR"));
    PiRpcClient::spawn(&fixture_path(), &args, cwd, None, TIMEOUT)
        .expect("fake pi rpc fixture must spawn (is node installed?)")
}

fn text_blocks() -> Vec<ContentBlock> {
    vec![ContentBlock::Text(TextContent::new("hello"))]
}

/// True when `event` is a `SessionUpdate` carrying an `AgentMessageChunk`.
fn is_agent_message_chunk(event: &AcpClientEvent) -> bool {
    matches!(
        event,
        AcpClientEvent::SessionUpdate(notification)
            if matches!(notification.update, SessionUpdate::AgentMessageChunk(_))
    )
}

/// Collect events until a `RequestFinished` for `expected_id` arrives. The
/// finished request's result is returned; earlier events are kept. Panics on
/// any other terminal shape (bounded loop so a hang is a failure, not a stall).
fn drain_until_finished(
    client: &PiRpcClient,
    expected_id: u64,
) -> (Vec<AcpClientEvent>, AcpClientEvent) {
    let mut events = Vec::new();
    for _ in 0..64 {
        let event = client
            .wait_event(TIMEOUT)
            .unwrap_or_else(|| panic!("timed out waiting for request {expected_id} to finish"));
        if matches!(
            &event,
            AcpClientEvent::RequestFinished { id, .. } if *id == expected_id
        ) {
            events.push(event.clone());
            return (events, event);
        }
        events.push(event);
    }
    panic!("no RequestFinished for id {expected_id} within the bounded drain");
}

/// The `stopReason` string of a successful prompt result, if present.
fn stop_reason(result: &serde_json::Value) -> Option<&str> {
    result.get("stopReason")?.as_str()
}

#[test]
fn spawn_reports_fixture_session_id() {
    let client = spawn_client(&[]);
    assert_eq!(client.session_id(), "fake-pi-rpc-session-1");
}

#[test]
fn prompt_flow_streams_chunk_then_finishes_end_turn() {
    let client = spawn_client(&[]);
    let id = client.begin_prompt(&text_blocks()).expect("begin_prompt");
    assert_eq!(id, 1);
    assert!(client.prompt_in_flight());

    let (events, finished) = drain_until_finished(&client, 1);
    assert!(
        events.iter().any(is_agent_message_chunk),
        "expected an AgentMessageChunk before the run finished: {events:?}"
    );
    match finished {
        AcpClientEvent::RequestFinished { id, method, result } => {
            assert_eq!(id, 1);
            assert_eq!(method, "session/prompt");
            let value = result.expect("prompt should succeed");
            assert_eq!(stop_reason(&value), Some("end_turn"));
        }
        other => panic!("drain returned a non-finished event: {other:?}"),
    }
    assert!(!client.prompt_in_flight());
}

#[test]
fn abort_flow_finishes_cancelled() {
    let client = spawn_client(&["--hold-run"]);
    let id = client.begin_prompt(&text_blocks()).expect("begin_prompt");

    // The fake emits the text delta before holding; the first mappable event
    // must surface as an AgentMessageChunk.
    let first = client.wait_event(TIMEOUT).expect("first prompt event");
    assert!(
        is_agent_message_chunk(&first),
        "expected the first event to be an AgentMessageChunk: {first:?}"
    );

    client.cancel().expect("cancel while a run is in flight");

    let (_, finished) = drain_until_finished(&client, id);
    match finished {
        AcpClientEvent::RequestFinished {
            id: finished_id,
            method,
            result,
        } => {
            assert_eq!(finished_id, 1);
            assert_eq!(method, "session/prompt");
            let value = result.expect("aborted run should settle as Ok");
            assert_eq!(stop_reason(&value), Some("cancelled"));
        }
        other => panic!("drain returned a non-finished event: {other:?}"),
    }
    assert!(!client.prompt_in_flight());
}

#[test]
fn second_begin_prompt_while_run_active_is_err() {
    let client = spawn_client(&["--hold-run"]);
    client
        .begin_prompt(&text_blocks())
        .expect("first begin_prompt");
    let error = client
        .begin_prompt(&text_blocks())
        .expect_err("a second prompt while one is in flight must be rejected");
    assert!(!error.is_empty());
}

#[test]
fn cancel_without_run_is_err() {
    let client = spawn_client(&[]);
    assert!(!client.prompt_in_flight());
    let error = client
        .cancel()
        .expect_err("cancel with no run must be rejected");
    assert!(!error.is_empty());
}

#[test]
fn rejected_prompt_reports_error_result() {
    let client = spawn_client(&["--reject-prompt"]);
    let id = client.begin_prompt(&text_blocks()).expect("begin_prompt");

    let (_, finished) = drain_until_finished(&client, id);
    match finished {
        AcpClientEvent::RequestFinished {
            id: finished_id,
            method,
            result,
        } => {
            assert_eq!(finished_id, 1);
            assert_eq!(method, "session/prompt");
            let message = result.expect_err("rejected prompt must finish as Err");
            assert_eq!(message, "rejected for test");
        }
        other => panic!("drain returned a non-finished event: {other:?}"),
    }
}

#[test]
fn resume_reports_passed_session_id() {
    // The client injects `--session <id>` into the child argv itself.
    let cwd = Path::new(env!("CARGO_MANIFEST_DIR"));
    let client = PiRpcClient::spawn(
        &fixture_path(),
        &[],
        cwd,
        Some("resumed-session-9"),
        TIMEOUT,
    )
    .expect("fake pi rpc fixture must spawn");
    assert_eq!(client.session_id(), "resumed-session-9");
}

#[test]
fn shutdown_returns_session_id_once_then_host_exited() {
    let client = spawn_client(&[]);
    let id = client.session_id();

    let first = client
        .shutdown()
        .expect("first shutdown returns the session id");
    assert_eq!(first, id);
    assert!(client.host_exited());

    // Idempotent: a second call has nothing more to report.
    assert_eq!(client.shutdown(), None);
}

#[test]
fn empty_block_list_is_err() {
    let client = spawn_client(&[]);
    let error = client
        .begin_prompt(&[])
        .expect_err("an empty prompt must be rejected");
    assert!(!error.is_empty());
}

#[test]
fn second_prompt_after_finish_gets_request_id_two() {
    let client = spawn_client(&[]);
    let first_id = client
        .begin_prompt(&text_blocks())
        .expect("first begin_prompt");
    assert_eq!(first_id, 1);
    drain_until_finished(&client, 1);

    let second_id = client
        .begin_prompt(&text_blocks())
        .expect("second begin_prompt");
    assert_eq!(second_id, 2);
    drain_until_finished(&client, 2);
}

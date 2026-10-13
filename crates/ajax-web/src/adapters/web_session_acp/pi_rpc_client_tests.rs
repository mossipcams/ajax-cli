//! Tests for [`super::pi_rpc_client`], running the fake Pi RPC fixture as a
//! directly-executable child (the file has a node shebang) and asserting that
//! [`PiRpcClient`] turns the JSONL record stream into ACP client events: spawn
//! and session id, prompt run, abort run, guard errors, rejection, resume id,
//! shutdown, and request-id sequencing.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::agent_event::{AgentEvent, NativeCommand, NativeEvent};
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

fn text_blocks() -> String {
    "hello".to_string()
}

/// True when `event` is a native `AgentText` (the pi message chunk).
fn is_agent_text(event: &AgentEvent) -> bool {
    matches!(event, AgentEvent::Native(NativeEvent::AgentText(_)))
}

/// Collect events until a native `TurnFinished` for `expected_id` arrives. The
/// finished turn's result is returned; earlier events are kept. Panics on
/// any other terminal shape (bounded loop so a hang is a failure, not a stall).
fn drain_until_finished(client: &PiRpcClient, expected_id: u64) -> (Vec<AgentEvent>, NativeEvent) {
    let mut events = Vec::new();
    for _ in 0..64 {
        let event = client
            .wait_event(TIMEOUT)
            .unwrap_or_else(|| panic!("timed out waiting for request {expected_id} to finish"));
        if let AgentEvent::Native(NativeEvent::TurnFinished { request_id, .. }) = &event {
            if *request_id == expected_id {
                let AgentEvent::Native(finished) = event else {
                    unreachable!()
                };
                events.push(AgentEvent::Native(finished.clone()));
                return (events, finished);
            }
        }
        events.push(event);
    }
    panic!("no TurnFinished for request_id {expected_id} within the bounded drain");
}

/// The stop-reason string of a successful turn result, if present.
fn stop_reason(result: &Result<String, String>) -> Option<&str> {
    result.as_ref().ok().map(|s| s.as_str())
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
        events.iter().any(is_agent_text),
        "expected an AgentText before the run finished: {events:?}"
    );
    match finished {
        NativeEvent::TurnFinished { request_id, result } => {
            assert_eq!(request_id, 1);
            assert_eq!(stop_reason(&result), Some("end_turn"));
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
    // must surface as a native AgentText.
    let first = client.wait_event(TIMEOUT).expect("first prompt event");
    assert!(
        is_agent_text(&first),
        "expected the first event to be an AgentText: {first:?}"
    );

    client.cancel().expect("cancel while a run is in flight");

    let (_, finished) = drain_until_finished(&client, id);
    match finished {
        NativeEvent::TurnFinished {
            request_id: finished_id,
            result,
        } => {
            assert_eq!(finished_id, 1);
            assert_eq!(stop_reason(&result), Some("cancelled"));
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
fn issue_1272_idle_cancel_is_adapted_at_session_boundary() {
    let client = spawn_client(&[]);
    assert!(!client.prompt_in_flight());
    let error = client
        .cancel()
        .expect_err("cancel with no run must be rejected");
    assert!(!error.is_empty());
    let mut session = super::session_client::SessionClient::from_pi(client);
    let outcome = session.cancel().expect("idle boundary cancel must succeed");
    assert!(outcome.permissions.is_empty());
    assert!(outcome.elicitations.is_empty());
}

#[test]
fn rejected_prompt_reports_error_result() {
    let client = spawn_client(&["--reject-prompt"]);
    let id = client.begin_prompt(&text_blocks()).expect("begin_prompt");

    let (_, finished) = drain_until_finished(&client, id);
    match finished {
        NativeEvent::TurnFinished {
            request_id: finished_id,
            result,
        } => {
            assert_eq!(finished_id, 1);
            let message = result.expect_err("rejected prompt must finish as Err");
            assert_eq!(message, "rejected for test");
        }
        other => panic!("drain returned a non-finished event: {other:?}"),
    }
}

#[test]
fn resume_reports_passed_session_id() {
    // The client injects `--session-id <id>` into the child argv itself.
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
        .begin_prompt("")
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

#[test]
fn issue_1283_resume_of_a_session_pi_never_persisted_keeps_that_id() {
    // Real Pi writes no session file until the first message; resuming such a
    // session must use `--session-id` (which creates it), not `--session`
    // (the fixture, like real Pi, exits 1 with 'No session found matching
    // ...' for that flag - issue #1283).
    let cwd = Path::new(env!("CARGO_MANIFEST_DIR"));
    let client = PiRpcClient::spawn(
        &fixture_path(),
        &[],
        cwd,
        Some("never-persisted-1283"),
        TIMEOUT,
    )
    .expect("fake pi rpc fixture must spawn with --session-id for an unpersisted session");
    assert_eq!(client.session_id(), "never-persisted-1283");
    client
        .shutdown()
        .expect("shutdown after resume returns the session id");
}

#[test]
fn spawn_exposes_the_advertised_command_list_once() {
    let client = spawn_client(&[]);

    assert_eq!(
        client.take_available_commands(),
        [
            NativeCommand {
                name: "compact".to_string(),
                description: "Compact the conversation".to_string(),
                input_hint: None,
            },
            // The fixture's last entry has no description; it becomes "".
            NativeCommand {
                name: "extensions".to_string(),
                description: String::new(),
                input_hint: None,
            },
        ],
        "the fake pi rpc fixture advertises exactly two commands"
    );
    assert!(
        client.take_available_commands().is_empty(),
        "the second take must not re-deliver the handshake commands"
    );
}

#[test]
fn degraded_handshake_exposes_no_commands() {
    let client = spawn_client(&["--degrade-optionals"]);
    assert!(
        client.take_available_commands().is_empty(),
        "an empty handshake command list must expose no commands on the first take"
    );
    client
        .shutdown()
        .expect("shutdown after degraded handshake");
}

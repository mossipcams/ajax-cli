//! Integration tests for [`super::pi_rpc_session`], running the real `node`
//! child behind the fake Pi RPC fixture (LF-delimited JSONL over stdin/stdout)
//! and asserting that a [`PiRpcSession`] on top of the real handshake turns the
//! record stream into [`PiStep`]s: prompt and abort runs, guard errors, prompt
//! rejection, handshake pending records first, and child exit.

use std::path::Path;
use std::time::Duration;

use agent_client_protocol::schema::v1::{ContentBlock, SessionUpdate};

use super::client::AcpClientEvent;
use super::pi_rpc_handshake::{handshake, PiHandshake};
use super::pi_rpc_process::PiRpcProcess;
use super::pi_rpc_session::{PiRpcSession, PiStep};

/// Generous per-step bound: node startup plus one scripted record burst.
const STEP_TIMEOUT: Duration = Duration::from_secs(10);
/// Bound for "this must not hang" negative tests.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Short bound that must elapse with no records on the wire (Idle checks).
const IDLE_TIMEOUT: Duration = Duration::from_millis(500);

/// Spawn the fake pi RPC child under `node`, passing `extra_flags` to the
/// fixture script. Fails loudly (panics) when `node` is missing.
fn spawn_fake_pi(extra_flags: &[&str]) -> PiRpcProcess {
    let mut args: Vec<String> = vec![Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fake_pi_rpc.js")
        .to_string_lossy()
        .into_owned()];
    args.extend(extra_flags.iter().copied().map(str::to_owned));
    let cwd = Path::new(env!("CARGO_MANIFEST_DIR"));
    PiRpcProcess::spawn(Path::new("node"), &args, cwd)
        .expect("fake pi rpc fixture must spawn (is node installed?)")
}

/// Spawn the fake and run the real four-command handshake.
fn handshaken(extra_flags: &[&str]) -> (PiRpcProcess, PiHandshake) {
    let mut process = spawn_fake_pi(extra_flags);
    let handshake = handshake(&mut process, HANDSHAKE_TIMEOUT).expect("handshake");
    (process, handshake)
}

fn contains_agent_message_chunk(events: &[AcpClientEvent]) -> bool {
    events.iter().any(|event| {
        matches!(
            event,
            AcpClientEvent::SessionUpdate(notification)
                if matches!(notification.update, SessionUpdate::AgentMessageChunk(_))
        )
    })
}

/// The text of the first `AgentMessageChunk` in `events`, if one is present.
fn agent_message_chunk_text(events: &[AcpClientEvent]) -> Option<&str> {
    events.iter().find_map(|event| {
        let AcpClientEvent::SessionUpdate(notification) = event else {
            return None;
        };
        match &notification.update {
            SessionUpdate::AgentMessageChunk(chunk) => match &chunk.content {
                ContentBlock::Text(text) => Some(text.text.as_str()),
                _ => None,
            },
            _ => None,
        }
    })
}
#[test]
fn prompt_flow_surfaces_events_then_finishes() {
    let (process, handshake) = handshaken(&[]);
    let mut session = PiRpcSession::new(process, handshake);

    assert_eq!(session.session_id(), "fake-pi-rpc-session-1");
    assert!(!session.run_active());

    session
        .begin_prompt("hello fake pi")
        .expect("prompt must be sent");
    assert!(
        session.run_active(),
        "run is active right after begin_prompt"
    );

    // The prompt response (`disposition: "started"`), `agent_start`, and the
    // first `message_update` all resolve within one call.
    let step = session.next_step(STEP_TIMEOUT);
    assert!(
        matches!(&step, PiStep::Events(events) if contains_agent_message_chunk(events)),
        "first step must carry an AgentMessageChunk: {step:?}"
    );
    assert!(session.run_active(), "run stays active until agent_settled");

    let step = session.next_step(STEP_TIMEOUT);
    assert!(
        matches!(step, PiStep::RunFinished { aborted: false }),
        "agent_settled must end the run without abort"
    );
    assert!(!session.run_active(), "run is inactive after RunFinished");
}

#[test]
fn abort_flow_finishes_with_aborted() {
    let (process, handshake) = handshaken(&["--hold-run"]);
    let mut session = PiRpcSession::new(process, handshake);

    session
        .begin_prompt("hello fake pi")
        .expect("prompt must be sent");

    let step = session.next_step(STEP_TIMEOUT);
    assert!(
        matches!(&step, PiStep::Events(events) if contains_agent_message_chunk(events)),
        "first step must carry an AgentMessageChunk: {step:?}"
    );

    session
        .abort()
        .expect("abort of the active run must be sent");

    // The abort response (not the prompt's id), the aborted `message_end`, and
    // `agent_settled` all resolve within one call.
    let step = session.next_step(STEP_TIMEOUT);
    assert!(
        matches!(step, PiStep::RunFinished { aborted: true }),
        "abort must end the run flagged aborted: {step:?}"
    );
    assert!(!session.run_active());
}

#[test]
fn begin_prompt_twice_while_active_is_an_error() {
    let (process, handshake) = handshaken(&[]);
    let mut session = PiRpcSession::new(process, handshake);

    session
        .begin_prompt("first")
        .expect("first prompt must be sent");

    let error = session
        .begin_prompt("second")
        .expect_err("second prompt must fail");
    assert!(
        error.to_lowercase().contains("active"),
        "error must say a run is active: {error}"
    );
    assert!(session.run_active());
}

#[test]
fn abort_while_idle_is_an_error() {
    let (process, handshake) = handshaken(&[]);
    let mut session = PiRpcSession::new(process, handshake);

    let error = session.abort().expect_err("abort with no run must fail");
    assert!(
        error.to_lowercase().contains("active"),
        "error must say there is no active run: {error}"
    );
}

#[test]
fn rejected_prompt_surfaces_pi_error_text() {
    let (process, handshake) = handshaken(&["--reject-prompt"]);
    let mut session = PiRpcSession::new(process, handshake);

    session.begin_prompt("nope").expect("prompt must be sent");

    let step = session.next_step(STEP_TIMEOUT);
    assert!(
        matches!(step, PiStep::PromptRejected(ref detail) if detail == "rejected for test"),
        "prompt response success:false must reject with pi's text: {step:?}"
    );
    assert!(
        !session.run_active(),
        "rejected prompt leaves no active run"
    );
}

#[test]
fn handshake_pending_records_are_processed_first() {
    let (process, handshake) = handshaken(&["--interleave-text"]);
    let mut session = PiRpcSession::new(process, handshake);

    // No command has been sent: the only record that can arrive is the pending
    // `message_update` captured during the handshake.
    let step = session.next_step(STEP_TIMEOUT);
    assert!(
        matches!(&step, PiStep::Events(events) if agent_message_chunk_text(events) == Some("pending-text")),
        "pending text record must come out first: {step:?}"
    );

    // The queue is drained; nothing else is on the wire.
    let step = session.next_step(IDLE_TIMEOUT);
    assert!(
        matches!(step, PiStep::Idle),
        "nothing left to report: {step:?}"
    );
}

#[test]
fn closed_child_surfaces_exited() {
    let (mut process, handshake) = handshaken(&[]);

    // Closing stdin is pi's documented orderly shutdown.
    process.close_stdin();
    let mut session = PiRpcSession::new(process, handshake);

    let step = session.next_step(STEP_TIMEOUT);
    assert!(
        matches!(step, PiStep::Exited),
        "child exit must surface: {step:?}"
    );
    assert!(!session.run_active());
}

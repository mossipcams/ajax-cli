//! Integration tests for [`super::rpc_session`], running the real `node`
//! child behind the fake Pi RPC fixture (LF-delimited JSONL over stdin/stdout)
//! and asserting that a [`RpcSession`] on top of the real handshake turns the
//! record stream into [`RpcStep`]s: prompt and abort runs, guard errors, prompt
//! rejection, handshake pending records first, and child exit.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use super::agent_event::{AgentEvent, NativeEvent};
use super::jsonl_process::JsonlProcess;
use super::pi_rpc_handshake::handshake;
use super::pi_rpc_map::map_record;
use super::rpc_handshake::RpcHandshake;
use serde_json::{json, Value};

use super::rpc_session::{carry_tool_inputs, fold_pending_error, RpcSession, RpcStep};

/// Generous per-step bound: node startup plus one scripted record burst.
const STEP_TIMEOUT: Duration = Duration::from_secs(10);
/// Bound for "this must not hang" negative tests.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Short bound that must elapse with no records on the wire (Idle checks).
const IDLE_TIMEOUT: Duration = Duration::from_millis(500);

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

/// Spawn the fake and run the real four-command handshake.
fn handshaken(extra_flags: &[&str]) -> (JsonlProcess, RpcHandshake) {
    let mut process = spawn_fake_pi(extra_flags);
    let handshake = handshake(&mut process, HANDSHAKE_TIMEOUT).expect("handshake");
    (process, handshake)
}

fn contains_agent_text(events: &[AgentEvent]) -> bool {
    events
        .iter()
        .any(|event| matches!(event, AgentEvent::Native(NativeEvent::AgentText(_))))
}

/// The text of the first native `AgentText` in `events`, if one is present.
fn agent_text(events: &[AgentEvent]) -> Option<&str> {
    events.iter().find_map(|event| match event {
        AgentEvent::Native(NativeEvent::AgentText(text)) => Some(text.as_str()),
        _ => None,
    })
}
#[test]
fn prompt_flow_surfaces_events_then_finishes() {
    let (process, handshake) = handshaken(&[]);
    let mut session = RpcSession::new(process, handshake, map_record);

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
        matches!(&step, RpcStep::Events(events) if contains_agent_text(events)),
        "first step must carry an AgentText: {step:?}"
    );
    assert!(session.run_active(), "run stays active until agent_settled");

    let step = session.next_step(STEP_TIMEOUT);
    assert!(
        matches!(step, RpcStep::RunFinished { aborted: false }),
        "agent_settled must end the run without abort"
    );
    assert!(!session.run_active(), "run is inactive after RunFinished");
}

#[test]
fn abort_flow_finishes_with_aborted() {
    let (process, handshake) = handshaken(&["--hold-run"]);
    let mut session = RpcSession::new(process, handshake, map_record);

    session
        .begin_prompt("hello fake pi")
        .expect("prompt must be sent");

    let step = session.next_step(STEP_TIMEOUT);
    assert!(
        matches!(&step, RpcStep::Events(events) if contains_agent_text(events)),
        "first step must carry an AgentText: {step:?}"
    );

    session
        .abort()
        .expect("abort of the active run must be sent");

    // The abort response (not the prompt's id), the aborted `message_end`, and
    // `agent_settled` all resolve within one call.
    let step = session.next_step(STEP_TIMEOUT);
    assert!(
        matches!(step, RpcStep::RunFinished { aborted: true }),
        "abort must end the run flagged aborted: {step:?}"
    );
    assert!(!session.run_active());
}

#[test]
fn begin_prompt_twice_while_active_is_an_error() {
    let (process, handshake) = handshaken(&[]);
    let mut session = RpcSession::new(process, handshake, map_record);

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
fn issue_1272_low_level_abort_remains_strict_while_idle() {
    let (process, handshake) = handshaken(&[]);
    let mut session = RpcSession::new(process, handshake, map_record);

    let error = session.abort().expect_err("abort with no run must fail");
    assert!(
        error.to_lowercase().contains("active"),
        "error must say there is no active run: {error}"
    );
    // SessionClient adapts idle cancellation; the transport still requires a run.
    assert!(!session.run_active());
    session.begin_prompt("hello fake pi").expect("begin prompt");
    session.abort().expect("active abort must still succeed");
}

#[test]
fn rejected_prompt_surfaces_pi_error_text() {
    let (process, handshake) = handshaken(&["--reject-prompt"]);
    let mut session = RpcSession::new(process, handshake, map_record);

    session.begin_prompt("nope").expect("prompt must be sent");

    let step = session.next_step(STEP_TIMEOUT);
    assert!(
        matches!(step, RpcStep::PromptRejected(ref detail) if detail == "rejected for test"),
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
    let mut session = RpcSession::new(process, handshake, map_record);

    // No command has been sent: the only record that can arrive is the pending
    // `message_update` captured during the handshake.
    let step = session.next_step(STEP_TIMEOUT);
    assert!(
        matches!(&step, RpcStep::Events(events) if agent_text(events) == Some("pending-text")),
        "pending text record must come out first: {step:?}"
    );

    // The queue is drained; nothing else is on the wire.
    let step = session.next_step(IDLE_TIMEOUT);
    assert!(
        matches!(step, RpcStep::Idle),
        "nothing left to report: {step:?}"
    );
}

#[test]
fn closed_child_surfaces_exited() {
    let (mut process, handshake) = handshaken(&[]);

    // Closing stdin is pi's documented orderly shutdown.
    process.close_stdin();
    let mut session = RpcSession::new(process, handshake, map_record);

    let step = session.next_step(STEP_TIMEOUT);
    assert!(
        matches!(step, RpcStep::Exited),
        "child exit must surface: {step:?}"
    );
    assert!(!session.run_active());
}

/// Build a `ToolCall` event with the given id and optional raw input.
fn tool_call(id: &str, raw_input: Option<Value>) -> AgentEvent {
    AgentEvent::Native(NativeEvent::ToolCall {
        id: id.to_owned(),
        title: "Edit file".to_owned(),
        kind: "edit".to_owned(),
        status: "in_progress".to_owned(),
        raw_input,
    })
}

/// Build a `ToolCallUpdate` event with the given id, status, and optional raw input.
fn tool_update(id: &str, status: &str, raw_input: Option<Value>) -> AgentEvent {
    AgentEvent::Native(NativeEvent::ToolCallUpdate {
        id: id.to_owned(),
        status: status.to_owned(),
        content: Some("tool output".to_owned()),
        raw_input,
    })
}

/// Extract `raw_input` from a `ToolCallUpdate`, panicking on any other shape.
fn update_raw_input(event: &AgentEvent) -> Option<Value> {
    match event {
        AgentEvent::Native(NativeEvent::ToolCallUpdate { raw_input, .. }) => raw_input.clone(),
        other => panic!("expected ToolCallUpdate, got: {other:?}"),
    }
}

#[test]
fn carry_tool_inputs_attaches_call_input_to_later_updates() {
    let mut memo = HashMap::new();
    let input = json!({ "path": "src/a.rs", "old_text": "x" });
    let mut events = vec![
        tool_call("c1", Some(input.clone())),
        tool_update("c1", "in_progress", None),
        tool_update("c1", "completed", None),
    ];

    carry_tool_inputs(&mut memo, &mut events);

    assert_eq!(update_raw_input(&events[1]), Some(input.clone()));
    // A terminal update consumes the memo entry.
    assert_eq!(update_raw_input(&events[2]), Some(input));
    assert!(memo.is_empty());
}

#[test]
fn carry_tool_inputs_drops_entry_after_terminal_update() {
    let mut memo = HashMap::new();
    let input = json!({ "path": "src/a.rs" });
    let mut events = vec![
        tool_call("c1", Some(input)),
        tool_update("c1", "completed", None),
        tool_update("c1", "completed", None),
    ];

    carry_tool_inputs(&mut memo, &mut events);

    // The second update arrives after the entry was removed: no input leaks.
    assert_eq!(update_raw_input(&events[2]), None);
    assert!(memo.is_empty());
}

#[test]
fn carry_tool_inputs_update_with_own_input_is_kept_and_refreshes_memo() {
    let mut memo = HashMap::new();
    let first = json!({ "path": "src/a.rs" });
    let second = json!({ "path": "src/b.rs" });
    let mut events = vec![
        tool_call("c1", Some(first)),
        tool_update("c1", "in_progress", Some(second.clone())),
        tool_update("c1", "completed", None),
    ];

    carry_tool_inputs(&mut memo, &mut events);

    // The update's own input is untouched and the later update gets it.
    assert_eq!(update_raw_input(&events[1]), Some(second.clone()));
    assert_eq!(update_raw_input(&events[2]), Some(second));
}

#[test]
fn carry_tool_inputs_ignores_non_object_inputs_and_foreign_ids() {
    let mut memo = HashMap::new();
    let object_input = json!({ "path": "src/a.rs" });
    let mut events = vec![
        // Non-object inputs are never remembered.
        tool_call("c1", Some(json!(42))),
        tool_update("c1", "completed", None),
        // Unrelated ids do not leak inputs across calls.
        tool_call("c2", Some(object_input.clone())),
        tool_update("c9", "completed", None),
    ];

    carry_tool_inputs(&mut memo, &mut events);

    assert_eq!(update_raw_input(&events[1]), None);
    assert_eq!(update_raw_input(&events[3]), None);
    // c2's entry is still pending: no terminal update arrived for it.
    assert_eq!(memo.len(), 1);
    assert_eq!(memo.get("c2"), Some(&object_input));
}

/// Fold a whole Pi record sequence through the mapper and the pending-error
/// memo, returning every event in the order a session would surface them.
fn fold_pi_records(records: &[Value]) -> Vec<AgentEvent> {
    let mut pending = None;
    let mut surfaced = Vec::new();
    for record in records {
        let mut mapping = map_record(record, "acp-session-1");
        fold_pending_error(&mut pending, record, &mut mapping);
        surfaced.extend(mapping.events);
    }
    surfaced
}

fn assistant_error(message: &str) -> Value {
    json!({
        "type": "message_end",
        "message": { "role": "assistant", "stopReason": "error", "errorMessage": message },
    })
}

fn assistant_stop(text: &str) -> Value {
    json!({
        "type": "message_end",
        "message": { "role": "assistant", "stopReason": "stop", "content": [{ "type": "text", "text": text }] },
    })
}

fn error_texts(events: &[AgentEvent]) -> Vec<&str> {
    events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::Native(NativeEvent::Error(text)) => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn unretried_assistant_error_flushes_once_before_settle_1335() {
    let events = fold_pi_records(&[
        assistant_error("401 invalid api key"),
        json!({ "type": "agent_settled", "aborted": false }),
    ]);

    assert_eq!(error_texts(&events), vec!["401 invalid api key"]);
    // The flushed error is the last event of the run, ahead of the finish.
    assert!(matches!(
        events.last(),
        Some(AgentEvent::Native(NativeEvent::Error(_)))
    ));
}

#[test]
fn retried_assistant_error_that_succeeds_is_silent_1335() {
    let events = fold_pi_records(&[
        assistant_error("529 overloaded"),
        json!({ "type": "auto_retry_start", "attempt": 1, "maxAttempts": 3, "delayMs": 2000, "errorMessage": "529 overloaded" }),
        assistant_stop("ok"),
        json!({ "type": "auto_retry_end", "success": true, "attempt": 1 }),
        json!({ "type": "agent_settled", "aborted": false }),
    ]);

    assert!(error_texts(&events).is_empty(), "{events:?}");
}

#[test]
fn overflow_compaction_retry_that_succeeds_is_silent_1335() {
    let events = fold_pi_records(&[
        assistant_error("prompt is too long"),
        json!({ "type": "compaction_start", "reason": "overflow" }),
        json!({ "type": "compaction_end", "reason": "overflow", "aborted": false, "willRetry": true }),
        assistant_stop("ok"),
        json!({ "type": "agent_settled", "aborted": false }),
    ]);

    assert!(error_texts(&events).is_empty(), "{events:?}");
}

#[test]
fn exhausted_auto_retry_emits_only_final_error_1335() {
    let events = fold_pi_records(&[
        assistant_error("529 overloaded"),
        json!({ "type": "auto_retry_start", "attempt": 1, "maxAttempts": 3, "delayMs": 2000, "errorMessage": "529 overloaded" }),
        assistant_error("529 overloaded"),
        json!({ "type": "auto_retry_start", "attempt": 2, "maxAttempts": 3, "delayMs": 4000, "errorMessage": "529 overloaded" }),
        assistant_error("529 overloaded"),
        json!({ "type": "auto_retry_end", "success": false, "attempt": 3, "finalError": "529 overloaded after 3 attempts" }),
        json!({ "type": "agent_settled", "aborted": false }),
    ]);

    assert_eq!(
        error_texts(&events),
        vec!["529 overloaded after 3 attempts"]
    );
}

#[test]
fn aborted_run_with_no_error_emits_no_error_1335() {
    let events = fold_pi_records(&[
        json!({
            "type": "message_end",
            "message": { "role": "assistant", "stopReason": "aborted", "content": [] },
        }),
        json!({ "type": "agent_settled", "aborted": true }),
    ]);

    assert!(error_texts(&events).is_empty(), "{events:?}");
}

#[test]
fn extension_error_does_not_cancel_pending_assistant_error_1335() {
    let events = fold_pi_records(&[
        assistant_error("500 internal"),
        json!({ "type": "extension_error", "extensionPath": "/p/ext.ts", "event": "tool_call", "error": "boom" }),
        json!({ "type": "agent_settled", "aborted": false }),
    ]);
    let texts = error_texts(&events);

    assert_eq!(texts.len(), 2, "{texts:?}");
    assert!(texts[0].contains("boom") && texts[0].contains("/p/ext.ts"));
    assert_eq!(texts[1], "500 internal");
}

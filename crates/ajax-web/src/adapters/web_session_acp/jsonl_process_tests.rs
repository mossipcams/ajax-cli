//! Integration tests for [`super::jsonl_process`], driving the fake
//! `pi --mode rpc` child in `tests/fixtures/fake_pi_rpc.js` through a real
//! spawned `node` process (std threads + std::sync::mpsc, no tokio).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::jsonl_process::{JsonlProcess, JsonlRecord};

/// How long any single wait for a record (or stderr output) may take before
/// the test panics.
const RECV_TIMEOUT: Duration = Duration::from_secs(10);

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_pi_rpc.js")
}

/// Spawn the fake pi RPC child under `node`, passing `extra_args` to the
/// fixture script. Fails loudly (panics) when `node` is missing.
fn spawn_fake(extra_args: &[&str]) -> JsonlProcess {
    let mut args: Vec<String> = vec![fixture_path().to_string_lossy().into_owned()];
    args.extend(extra_args.iter().map(|arg| (*arg).to_owned()));
    let cwd = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    JsonlProcess::spawn(Path::new("node"), &args, &cwd)
        .expect("spawn `node` with the fake pi rpc fixture (is node installed?)")
}

/// Receive the next record, waiting up to [`RECV_TIMEOUT`] for it.
fn recv_within(process: &JsonlProcess) -> JsonlRecord {
    let deadline = Instant::now() + RECV_TIMEOUT;
    loop {
        match process.try_recv() {
            Ok(record) => return record,
            Err(mpsc::TryRecvError::Empty) => {
                if Instant::now() >= deadline {
                    panic!("timed out after {RECV_TIMEOUT:?} waiting for a pi-rpc record");
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                panic!("pi-rpc record channel closed before the expected record arrived");
            }
        }
    }
}

/// Unwrap a received record as JSON, panicking with context otherwise. Takes
/// ownership so the caller never borrows a temporary record.
fn as_json(record: JsonlRecord) -> Value {
    match record {
        JsonlRecord::Record(value) => value,
        other => panic!("expected a JSON record, got {other:?}"),
    }
}

/// Wait until the stderr tail contains `needle`; the stderr reader thread may
/// still be draining when the stdout EOF record arrives.
fn stderr_tail_containing(process: &JsonlProcess, needle: &str) -> String {
    let deadline = Instant::now() + RECV_TIMEOUT;
    loop {
        let tail = process.stderr_tail();
        if tail.contains(needle) {
            return tail;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for {needle:?} in the stderr tail: {tail:?}");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn get_state_response_carries_the_command_id() {
    let mut process = spawn_fake(&[]);
    let id = process
        .send("get_state", json!({}))
        .expect("send get_state");

    let value = as_json(recv_within(&process));
    assert_eq!(value.get("id").and_then(Value::as_str), Some(id.as_str()));
    assert_eq!(value.get("type").and_then(Value::as_str), Some("response"));
    assert_eq!(value.get("success"), Some(&Value::Bool(true)));
    let data = value.get("data").expect("response data");
    assert_eq!(
        data.get("sessionId").and_then(Value::as_str),
        Some("fake-pi-rpc-session-1")
    );

    // Every send gets a fresh unique id, and the next response echoes it.
    let second = process
        .send("get_state", json!({}))
        .expect("send get_state");
    assert_ne!(id, second, "each send must allocate a fresh id");
    let value = as_json(recv_within(&process));
    assert_eq!(
        value.get("id").and_then(Value::as_str),
        Some(second.as_str())
    );
    assert_eq!(value.get("type").and_then(Value::as_str), Some("response"));
}

#[test]
fn prompt_streams_events_through_agent_settled() {
    let mut process = spawn_fake(&[]);
    let id = process
        .send("prompt", json!({ "message": "say ok" }))
        .expect("send prompt");

    // The response echoes the prompt id, then the scripted events stream in
    // order through agent_settled.
    let value = as_json(recv_within(&process));
    assert_eq!(value.get("type").and_then(Value::as_str), Some("response"));
    assert_eq!(value.get("id").and_then(Value::as_str), Some(id.as_str()));

    for expected_type in [
        "agent_start",
        "message_update",
        "agent_end",
        "agent_settled",
    ] {
        let value = as_json(recv_within(&process));
        let record_type = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        assert_eq!(
            record_type, expected_type,
            "prompt events arrived out of order"
        );
    }
}

#[test]
fn prompt_response_id_and_event_details() {
    let mut process = spawn_fake(&[]);
    let id = process
        .send("prompt", json!({ "message": "say ok" }))
        .expect("send prompt");

    // response{command:"prompt", success:true, data:{disposition:"started"}}
    let value = as_json(recv_within(&process));
    assert_eq!(value.get("type").and_then(Value::as_str), Some("response"));
    assert_eq!(value.get("id").and_then(Value::as_str), Some(id.as_str()));
    assert_eq!(value.get("command").and_then(Value::as_str), Some("prompt"));
    assert_eq!(value.get("success"), Some(&Value::Bool(true)));
    assert_eq!(
        value
            .get("data")
            .and_then(|data| data.get("disposition"))
            .and_then(Value::as_str),
        Some("started")
    );

    let _ = as_json(recv_within(&process)); // agent_start

    // message_update{assistantMessageEvent{type:"text_delta"}}
    let value = as_json(recv_within(&process));
    assert_eq!(
        value.get("type").and_then(Value::as_str),
        Some("message_update")
    );
    assert_eq!(
        value
            .get("assistantMessageEvent")
            .and_then(|event| event.get("type"))
            .and_then(Value::as_str),
        Some("text_delta")
    );

    let _ = as_json(recv_within(&process)); // agent_end
    let value = as_json(recv_within(&process));
    assert_eq!(
        value.get("type").and_then(Value::as_str),
        Some("agent_settled")
    );
}

#[test]
fn abort_replies_then_message_end_aborted_then_agent_settled() {
    let mut process = spawn_fake(&[]);
    let id = process.send("abort", json!({})).expect("send abort");

    let value = as_json(recv_within(&process));
    assert_eq!(value.get("type").and_then(Value::as_str), Some("response"));
    assert_eq!(value.get("id").and_then(Value::as_str), Some(id.as_str()));
    assert_eq!(value.get("command").and_then(Value::as_str), Some("abort"));
    assert_eq!(value.get("success"), Some(&Value::Bool(true)));

    let value = as_json(recv_within(&process));
    assert_eq!(
        value.get("type").and_then(Value::as_str),
        Some("message_end")
    );
    assert_eq!(
        value.get("stopReason").and_then(Value::as_str),
        Some("aborted")
    );

    let value = as_json(recv_within(&process));
    assert_eq!(
        value.get("type").and_then(Value::as_str),
        Some("agent_settled")
    );
}

#[test]
fn line_separator_u2028_inside_a_string_stays_one_record() {
    let mut process = spawn_fake(&["--emit-u2028"]);

    let value = as_json(recv_within(&process));
    assert_eq!(value.get("type").and_then(Value::as_str), Some("note"));
    assert_eq!(
        value.get("text").and_then(Value::as_str),
        Some("one\u{2028}two"),
        "U+2028 inside a JSON string must arrive intact within ONE record"
    );

    // Nothing more was emitted, so the next record is EOF, not a second
    // fragment of the U+2028 line.
    process.close_stdin();
    assert!(matches!(recv_within(&process), JsonlRecord::Exited));
}

#[test]
fn non_json_stdout_line_is_delivered_as_error_record() {
    let mut process = spawn_fake(&["--emit-garbage"]);

    match recv_within(&process) {
        JsonlRecord::Error(text) => assert_eq!(text, "this line is not json"),
        other => panic!("expected an Error record, got {other:?}"),
    }

    // The reader survives the bad line: closing stdin still yields EOF.
    process.close_stdin();
    assert!(matches!(recv_within(&process), JsonlRecord::Exited));
}

#[test]
fn close_stdin_exits_child_and_delivers_exited_exactly_once() {
    let mut process = spawn_fake(&[]);
    process.close_stdin();

    let deadline = Instant::now() + RECV_TIMEOUT;
    let mut exited = 0_usize;
    let mut other = 0_usize;
    loop {
        match process.try_recv() {
            Ok(JsonlRecord::Exited) => exited += 1,
            Ok(_) => other += 1,
            Err(mpsc::TryRecvError::Disconnected) => break,
            Err(mpsc::TryRecvError::Empty) => {
                assert!(
                    Instant::now() < deadline,
                    "timed out waiting for the record channel to disconnect"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
    assert_eq!(
        other, 0,
        "no commands were sent, so only Exited may be delivered"
    );
    assert_eq!(
        exited, 1,
        "Exited must be delivered exactly once before the channel disconnects"
    );
    // And the channel stays disconnected afterwards.
    assert!(process.recv().is_err());
}

#[test]
fn stderr_never_enters_the_record_channel() {
    let mut process = spawn_fake(&["--emit-stderr"]);
    process.close_stdin();

    // Only the stdout EOF record may arrive; stderr must not leak into it.
    assert!(matches!(recv_within(&process), JsonlRecord::Exited));
    assert!(process.recv().is_err());

    let tail = stderr_tail_containing(&process, "fake pi rpc stderr noise");
    assert_eq!(tail.trim_end(), "fake pi rpc stderr noise");
}

#[test]
fn stderr_tail_is_bounded_to_the_last_4k() {
    let mut process = spawn_fake(&["--emit-stderr-noise"]);
    process.close_stdin();
    assert!(matches!(recv_within(&process), JsonlRecord::Exited));

    let tail = stderr_tail_containing(&process, "STDERR-TAIL-END");
    assert!(tail.contains("STDERR-TAIL-END"), "tail was: {tail:?}");
    assert!(
        !tail.contains("STDERR-HEAD-BEGIN"),
        "more than 4 KiB was written, so the head must have been dropped; tail: {tail:?}"
    );
    assert!(
        tail.len() <= 4 * 1024,
        "tail must stay bounded to 4 KiB, got {}",
        tail.len()
    );
}

#[test]
fn dropping_the_process_reaps_the_child() {
    let mut process = spawn_fake(&[]);
    let id = process
        .send("get_state", json!({}))
        .expect("send get_state");
    let value = as_json(recv_within(&process));
    assert_eq!(value.get("id").and_then(Value::as_str), Some(id.as_str()));

    process.close_stdin();
    // EOF means the child exited; until Drop waits it is unreaped.
    assert!(matches!(recv_within(&process), JsonlRecord::Exited));

    let pid = process.child_id();
    drop(process);

    // A reaped pid no longer answers `kill -0` (a zombie still would, so this
    // failure proves Drop waited for the child instead of abandoning it).
    let status = Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .expect("run `kill -0` to probe for the reaped child");
    assert!(
        !status.success(),
        "child pid {pid} still exists after Drop; expected it to be reaped"
    );
}

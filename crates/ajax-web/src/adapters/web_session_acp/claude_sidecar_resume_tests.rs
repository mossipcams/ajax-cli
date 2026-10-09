//! Tests for the sidecar's fail-closed resume check: a `resume` id that the
//! SDK reports as missing fails init with "session not found" (leaving the
//! sidecar uninitialized), while a lookup that throws, or an SDK without
//! `getSessionInfo`, degrades to resuming as before.

use std::path::{Path, PathBuf};
use std::sync::mpsc::TryRecvError;
use std::thread::sleep;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::claude_sdk_client::ClaudeSdkClient;
use super::jsonl_process::{JsonlProcess, JsonlRecord};

/// Generous per-step bound: node startup plus one scripted turn.
const STEP_TIMEOUT: Duration = Duration::from_secs(10);
/// Idle poll interval while waiting for records.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn sidecar_path() -> PathBuf {
    manifest_dir().join("sidecar/claude_sdk_sidecar.mjs")
}

fn fixture(name: &str) -> PathBuf {
    manifest_dir().join("tests/fixtures").join(name)
}

/// Spawn the sidecar under `node` against a named SDK fixture.
fn spawn_sidecar_with(sdk_fixture: &str) -> JsonlProcess {
    let args = vec![
        sidecar_path().to_string_lossy().into_owned(),
        "--sdk-module".to_owned(),
        fixture(sdk_fixture).to_string_lossy().into_owned(),
    ];
    JsonlProcess::spawn(Path::new("node"), &args, manifest_dir()).unwrap_or_else(|error| {
        panic!("claude sdk sidecar must spawn against {sdk_fixture}: {error}")
    })
}

fn cwd() -> String {
    manifest_dir().to_string_lossy().into_owned()
}

/// Poll until `predicate` holds over the records drained so far; return them.
fn drain_until(
    process: &mut JsonlProcess,
    label: &str,
    predicate: impl Fn(&[Value]) -> bool,
) -> Vec<Value> {
    let deadline = Instant::now() + STEP_TIMEOUT;
    let mut records = Vec::new();
    while !predicate(&records) {
        match process.try_recv() {
            Ok(JsonlRecord::Record(value)) => records.push(value),
            Ok(JsonlRecord::Error(text)) => {
                panic!("sidecar protocol error while waiting for {label}: {text}")
            }
            Ok(JsonlRecord::Exited) => panic!(
                "sidecar exited while waiting for {label}; stderr: {}",
                process.stderr_tail()
            ),
            Err(TryRecvError::Empty) if Instant::now() < deadline => sleep(POLL_INTERVAL),
            Err(TryRecvError::Empty) => panic!(
                "timed out waiting for {label}; records so far: {}",
                records
                    .iter()
                    .map(Value::to_string)
                    .collect::<Vec<_>>()
                    .join(" | ")
            ),
            Err(TryRecvError::Disconnected) => panic!(
                "sidecar record stream ended while waiting for {label}; stderr: {}",
                process.stderr_tail()
            ),
        }
    }
    records
}

/// Send an init command and return its response.
fn init(process: &mut JsonlProcess, fields: Value) -> Value {
    process
        .send("init", fields)
        .unwrap_or_else(|error| panic!("send init: {error}"));
    let records = drain_until(process, "init response", |records| {
        records
            .iter()
            .any(|record| record.get("command").and_then(Value::as_str) == Some("init"))
    });
    records
        .into_iter()
        .find(|record| record.get("command").and_then(Value::as_str) == Some("init"))
        .expect("init response was drained")
}

#[test]
fn resume_known_session_id_is_echoed() {
    let mut process = spawn_sidecar_with("fake_claude_sdk.mjs");
    let response = init(&mut process, json!({ "cwd": cwd(), "resume": "r-1" }));
    assert_eq!(
        response.get("success").and_then(Value::as_bool),
        Some(true),
        "{response}"
    );
    assert_eq!(
        response
            .get("data")
            .and_then(|data| data.get("sessionId"))
            .and_then(Value::as_str),
        Some("r-1"),
        "{response}"
    );
}

#[test]
fn resume_missing_session_fails_closed_and_sidecar_stays_usable() {
    let mut process = spawn_sidecar_with("fake_claude_sdk.mjs");
    let response = init(&mut process, json!({ "cwd": cwd(), "resume": "missing-1" }));
    assert_eq!(
        response.get("success").and_then(Value::as_bool),
        Some(false),
        "{response}"
    );
    let error = response
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(error.contains("session not found"), "error: {error}");

    // The sidecar must remain uninitialized: a plain init still succeeds.
    let followup = init(&mut process, json!({ "cwd": cwd() }));
    assert_eq!(
        followup.get("success").and_then(Value::as_bool),
        Some(true),
        "{followup}"
    );
    assert!(
        followup
            .get("data")
            .and_then(|data| data.get("sessionId"))
            .and_then(Value::as_str)
            .is_some(),
        "expected a fresh sessionId in {followup}"
    );
}

#[test]
fn resume_lookup_failure_degrades_to_resuming() {
    let mut process = spawn_sidecar_with("fake_claude_sdk.mjs");
    let response = init(&mut process, json!({ "cwd": cwd(), "resume": "boom-1" }));
    assert_eq!(
        response.get("success").and_then(Value::as_bool),
        Some(true),
        "{response}"
    );
    assert_eq!(
        response
            .get("data")
            .and_then(|data| data.get("sessionId"))
            .and_then(Value::as_str),
        Some("boom-1"),
        "{response}"
    );
}

#[test]
fn legacy_sdk_without_get_session_info_skips_the_check() {
    let mut process = spawn_sidecar_with("fake_claude_sdk_legacy.mjs");
    let response = init(&mut process, json!({ "cwd": cwd(), "resume": "missing-1" }));
    assert_eq!(
        response.get("success").and_then(Value::as_bool),
        Some(true),
        "{response}"
    );
}

#[test]
fn client_resume_missing_session_fails_and_known_session_succeeds() {
    let args = vec![
        sidecar_path().to_string_lossy().into_owned(),
        "--sdk-module".to_owned(),
        fixture("fake_claude_sdk.mjs")
            .to_string_lossy()
            .into_owned(),
    ];

    let missing = ClaudeSdkClient::spawn(
        Path::new("node"),
        &args,
        manifest_dir(),
        None,
        Some("missing-1"),
        STEP_TIMEOUT,
    );
    let error = match missing {
        Err(error) => error,
        Ok(_) => panic!("expected an init failure for a missing session"),
    };
    assert!(error.contains("session not found"), "error: {error}");

    let known = match ClaudeSdkClient::spawn(
        Path::new("node"),
        &args,
        manifest_dir(),
        None,
        Some("r-1"),
        STEP_TIMEOUT,
    ) {
        Ok(client) => client,
        Err(error) => panic!("claude sdk client must spawn against the fake fixture: {error}"),
    };
    assert_eq!(known.session_id(), "r-1");
}

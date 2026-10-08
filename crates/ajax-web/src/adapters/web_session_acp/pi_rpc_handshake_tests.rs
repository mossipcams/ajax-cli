//! Integration tests for [`super::pi_rpc_handshake`], running the real `node`
//! child behind the fake Pi RPC fixture (LF-delimited JSONL over stdin/stdout)
//! and asserting that the four discovery commands are id-correlated into a
//! typed [`PiHandshake`] — including pending-record collection, failure and
//! degradation paths, and the pre-get_state timeout path with stderr tail.

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::pi_rpc_handshake::{handshake, PiHandshake};
use super::pi_rpc_process::PiRpcProcess;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Generous bound for "this must not hang" negative tests.
/// Tight timeout for the deliberate deadline test; node startup on a loaded
/// machine can exceed 300ms, so keep it well below HANDSHAKE_TIMEOUT.
const DEADLINE_TEST_TIMEOUT: Duration = Duration::from_millis(1_500);

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

#[test]
fn handshake_full_success_types_all_four_responses() {
    let mut process = spawn_fake_pi(&[]);
    let started = Instant::now();

    let handshake: PiHandshake = handshake(&mut process, HANDSHAKE_TIMEOUT).expect("handshake");

    assert!(started.elapsed() < Duration::from_secs(5), "handshake hung");
    assert_eq!(handshake.session_id, "fake-pi-rpc-session-1");
    assert!(
        handshake
            .session_file
            .as_deref()
            .is_some_and(|f| f.contains("2026-01-01T00-00-00Z")),
        "session_file: {:?}",
        handshake.session_file
    );
    assert_eq!(handshake.model_id.as_deref(), Some("fake-model"));
    assert_eq!(handshake.thinking_level.as_deref(), Some("medium"));
    assert_eq!(handshake.context_window, Some(131_072));

    let models = &handshake.models;
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "fake-model");
    assert_eq!(models[0].name, "Fake Model");
    assert_eq!(models[0].provider, "fake-provider");
    assert!(models[0].reasoning);
    assert!(!models[1].reasoning);

    assert_eq!(
        handshake.thinking_levels,
        vec!["off", "low", "medium", "high"]
    );

    let commands = &handshake.commands;
    assert_eq!(commands.len(), 2);
    assert_eq!(commands[0].name, "compact");
    assert_eq!(
        commands[0].description.as_deref(),
        Some("Compact the conversation")
    );
    assert_eq!(commands[1].name, "extensions");
    assert!(commands[1].description.is_none());

    // No extra records are pending when the fake only answers the four.
    assert!(
        handshake.pending.is_empty(),
        "unexpected pending: {:?}",
        handshake.pending
    );
}

#[test]
fn handshake_collects_interleaved_event_into_pending() {
    let mut process = spawn_fake_pi(&["--interleave-event"]);

    let handshake = handshake(&mut process, HANDSHAKE_TIMEOUT).expect("handshake");

    assert_eq!(
        handshake.session_id, "fake-pi-rpc-session-1",
        "state still parsed"
    );
    assert_eq!(
        handshake.pending.len(),
        1,
        "exactly one event pending: {:?}",
        handshake.pending
    );
    let event = &handshake.pending[0];
    assert_eq!(
        event.get("type").and_then(Value::as_str),
        Some("extension_ui_request")
    );
    assert_eq!(
        event.pointer("/payload/items/0").and_then(Value::as_str),
        Some("a"),
        "pending record must be the full unmodified value"
    );
}

#[test]
fn handshake_fails_on_get_state_error() {
    let mut process = spawn_fake_pi(&["--state-fail"]);

    let error = handshake(&mut process, HANDSHAKE_TIMEOUT).expect_err("must fail");

    assert!(
        error.contains("fake pi state failure"),
        "error must carry pi's text: {error}"
    );
}

#[test]
fn handshake_fails_when_session_id_missing() {
    let mut process = spawn_fake_pi(&["--no-session-id"]);

    let error = handshake(&mut process, HANDSHAKE_TIMEOUT).expect_err("must fail");

    assert!(
        error.to_lowercase().contains("sessionid"),
        "error must mention the missing field: {error}"
    );
}

#[test]
fn handshake_times_out_with_silent_state_and_includes_stderr_tail() {
    let mut process = spawn_fake_pi(&["--silent-state", "--emit-stderr"]);

    let error = handshake(&mut process, DEADLINE_TEST_TIMEOUT).expect_err("must time out");

    assert!(error.contains("timed out"), "timeout message: {error}");
    // The pre-get_state failure path must surface the process stderr tail.
    assert!(
        error.contains("fake pi rpc stderr noise"),
        "stderr tail missing from: {error}"
    );
}

#[test]
fn handshake_degrades_failed_optional_commands_to_empty_lists() {
    let mut process = spawn_fake_pi(&["--degrade-optionals"]);

    let handshake = handshake(&mut process, HANDSHAKE_TIMEOUT).expect("handshake");

    // get_state itself is unaffected: the handshake must still succeed.
    assert_eq!(handshake.session_id, "fake-pi-rpc-session-1");
    assert!(
        handshake.models.is_empty(),
        "models degraded: {:?}",
        handshake.models
    );
    assert!(
        handshake.thinking_levels.is_empty(),
        "levels degraded: {:?}",
        handshake.thinking_levels
    );
    assert!(
        handshake.commands.is_empty(),
        "commands degraded: {:?}",
        handshake.commands
    );
}

//! Tests for an explicit `--sdk-module` override in the Claude Agent SDK
//! sidecar: when a caller supplies a module that cannot load, init must fail
//! instead of silently falling back to the installed SDK. Spawns the real
//! `node` child behind [`JsonlProcess`] JSONL framing, mirroring the style of
//! [`claude_sidecar_tests`].

use std::path::Path;
use std::sync::mpsc::TryRecvError;
use std::thread::sleep;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::jsonl_process::{JsonlProcess, JsonlRecord};

/// Generous per-step bound: node startup plus one scripted turn.
const STEP_TIMEOUT: Duration = Duration::from_secs(10);
/// Idle poll interval while waiting for records.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Spawn the sidecar under `node`, pointing `--sdk-module` at an explicit path.
fn spawn_with_module(module: &str) -> JsonlProcess {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let args: Vec<String> = vec![
        manifest
            .join("sidecar/claude_sdk_sidecar.mjs")
            .to_string_lossy()
            .into_owned(),
        "--sdk-module".to_owned(),
        module.to_owned(),
    ];
    JsonlProcess::spawn(Path::new("node"), &args, manifest)
        .expect("claude sdk sidecar must spawn (is node installed?)")
}

/// A spawned override-sidecar plus every record drained from it so later
/// assertions can search records that earlier waits already consumed.
struct OverrideSidecar {
    process: JsonlProcess,
    records: Vec<Value>,
}

impl OverrideSidecar {
    fn spawn(module: &str) -> Self {
        Self {
            process: spawn_with_module(module),
            records: Vec::new(),
        }
    }

    /// Send one command and return its id.
    fn command(&mut self, command_type: &str, fields: Value) -> String {
        self.process
            .send(command_type, fields)
            .unwrap_or_else(|error| panic!("send {command_type}: {error}"))
    }

    /// Poll until `predicate` holds over all records drained so far.
    fn wait_until(&mut self, label: &str, predicate: impl Fn(&[Value]) -> bool) {
        let deadline = Instant::now() + STEP_TIMEOUT;
        while !predicate(&self.records) {
            match self.process.try_recv() {
                Ok(JsonlRecord::Record(value)) => self.records.push(value),
                Ok(JsonlRecord::Error(text)) => {
                    panic!("sidecar protocol error while waiting for {label}: {text}")
                }
                Ok(JsonlRecord::Exited) => panic!(
                    "sidecar exited while waiting for {label}; stderr: {}",
                    self.process.stderr_tail()
                ),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => panic!(
                    "sidecar record stream ended while waiting for {label}; stderr: {}",
                    self.process.stderr_tail()
                ),
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {label}; records so far: {:?}; stderr: {}",
                self.records,
                self.process.stderr_tail()
            );
            sleep(POLL_INTERVAL);
        }
    }

    /// Wait for and return the response carrying `id`.
    fn response(&mut self, id: &str) -> Value {
        self.wait_until(&format!("response {id}"), |records| {
            records.iter().any(|record| is_response_for(record, id))
        });
        self.records
            .iter()
            .find(|record| is_response_for(record, id))
            .cloned()
            .expect("response collected")
    }

    /// Send `init` with the default cwd and return its response.
    fn init(&mut self) -> Value {
        let id = self.command("init", json!({ "cwd": env!("CARGO_MANIFEST_DIR") }));
        self.response(&id)
    }

    /// Assert an init failure that names the override path and never mentions
    /// the installed-SDK fallback locations.
    fn assert_override_failure(&self, response: &Value) {
        assert_eq!(
            response.get("success"),
            Some(&Value::Bool(false)),
            "init with an unloadable --sdk-module must not succeed"
        );
        let error = response
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("expected an init failure error; got {response}"));
        assert!(
            error.contains("does-not-exist.mjs"),
            "failure must name the explicit override path: {error}"
        );
        assert!(
            !error.contains("package import"),
            "an explicit override must not fall back to the package import: {error}"
        );
        assert!(
            !error.contains("npm root"),
            "an explicit override must not fall back to npm root locations: {error}"
        );
    }
}

fn is_response_for(record: &Value, id: &str) -> bool {
    record.get("type").and_then(Value::as_str) == Some("response")
        && record.get("id").and_then(Value::as_str) == Some(id)
}

#[test]
fn init_with_unloadable_override_fails_instead_of_falling_back() {
    let mut sidecar = OverrideSidecar::spawn("does-not-exist.mjs");
    // The manifest-relative fixture path that is guaranteed to be missing.
    let _ = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/does-not-exist.mjs");
    let response = sidecar.init();
    sidecar.assert_override_failure(&response);
}

#[test]
fn second_init_on_uninitialised_override_child_fails_again() {
    let mut sidecar = OverrideSidecar::spawn(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/does-not-exist.mjs")
            .to_string_lossy()
            .as_ref(),
    );
    let first = sidecar.init();
    sidecar.assert_override_failure(&first);
    // The sidecar must stay uninitialised and keep refusing, not fall back.
    let second = sidecar.init();
    sidecar.assert_override_failure(&second);
}

#[test]
fn init_with_loadable_override_still_succeeds() {
    let module = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fake_claude_sdk.mjs")
        .to_string_lossy()
        .into_owned();
    let mut sidecar = OverrideSidecar::spawn(&module);
    let response = sidecar.init();
    assert_eq!(response.get("success"), Some(&Value::Bool(true)));
}

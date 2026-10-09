//! Integration tests for the Claude Agent SDK sidecar
//! (`crates/ajax-web/sidecar/claude_sdk_sidecar.mjs`), running the real `node`
//! child behind the fake SDK fixture (`crates/ajax-web/tests/fixtures/
//! fake_claude_sdk.mjs`) through the existing [`PiRpcProcess`] JSONL framing:
//! init/session ids, prompt/abort/tool/model/effort/context flows, guard
//! errors, U+2028 line framing, and shutdown/exit.

use std::path::Path;
use std::sync::mpsc::TryRecvError;
use std::thread::sleep;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::pi_rpc_process::{PiRpcProcess, PiRpcRecord};

/// Generous per-step bound: node startup plus one scripted turn.
const STEP_TIMEOUT: Duration = Duration::from_secs(10);
/// Idle poll interval while waiting for records.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Spawn the sidecar under `node`, pointing it at the fake SDK fixture.
fn spawn_sidecar() -> PiRpcProcess {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let args: Vec<String> = vec![
        manifest
            .join("sidecar/claude_sdk_sidecar.mjs")
            .to_string_lossy()
            .into_owned(),
        "--sdk-module".to_owned(),
        manifest
            .join("tests/fixtures/fake_claude_sdk.mjs")
            .to_string_lossy()
            .into_owned(),
    ];
    PiRpcProcess::spawn(Path::new("node"), &args, manifest)
        .expect("claude sdk sidecar must spawn (is node installed?)")
}

/// A spawned sidecar plus every record drained from it so far, so later
/// assertions can search records that earlier waits already consumed.
struct Sidecar {
    process: PiRpcProcess,
    records: Vec<Value>,
}

impl Sidecar {
    fn spawn() -> Self {
        Self {
            process: spawn_sidecar(),
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
                Ok(PiRpcRecord::Record(value)) => self.records.push(value),
                Ok(PiRpcRecord::Error(text)) => {
                    panic!("sidecar protocol error while waiting for {label}: {text}");
                }
                Ok(PiRpcRecord::Exited) => panic!(
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
}

fn is_response_for(record: &Value, id: &str) -> bool {
    record.get("type").and_then(Value::as_str) == Some("response")
        && record.get("id").and_then(Value::as_str) == Some(id)
}

/// Records of the given SDK event `type`, in arrival order.
fn events<'a>(records: &'a [Value], kind: &'a str) -> impl Iterator<Item = &'a Value> + 'a {
    records
        .iter()
        .filter(move |record| record.get("type").and_then(Value::as_str) == Some(kind))
}

fn event_kind(record: &Value) -> Option<&str> {
    record.get("type").and_then(Value::as_str)
}

fn subtype(record: &Value) -> &str {
    record
        .get("subtype")
        .and_then(Value::as_str)
        .expect("subtype")
}

/// The `fake_call` record for `method`, panicking when absent.
fn fake_call<'a>(records: &'a [Value], method: &str) -> &'a Value {
    events(records, "fake_call")
        .find(|record| record.get("method").and_then(Value::as_str) == Some(method))
        .unwrap_or_else(|| panic!("fake_call {method} present"))
}

fn assert_uuid_shaped(text: &str) {
    let parts: Vec<&str> = text.split('-').collect();
    let lengths: Vec<usize> = parts.iter().map(|part| part.len()).collect();
    assert_eq!(
        lengths,
        vec![8, 4, 4, 4, 12],
        "uuid group lengths in {text}"
    );
    assert!(
        text.chars().all(|c| c.is_ascii_hexdigit() || c == '-'),
        "uuid-shaped id is hex-and-dash: {text}"
    );
}

#[test]
fn init_returns_uuid_session_models_and_commands() {
    let mut sidecar = Sidecar::spawn();
    let response = sidecar.init();
    assert_eq!(
        response.get("command").and_then(Value::as_str),
        Some("init")
    );
    assert_eq!(response.get("success"), Some(&Value::Bool(true)));
    let data = response.get("data").expect("init data");
    let session_id = data
        .get("sessionId")
        .and_then(Value::as_str)
        .expect("sessionId");
    assert_uuid_shaped(session_id);
    let models = data
        .get("models")
        .and_then(Value::as_array)
        .expect("models");
    assert_eq!(models.len(), 2);
    assert_eq!(
        models[0].get("value").and_then(Value::as_str),
        Some("default")
    );
    assert_eq!(
        models[1].get("displayName").and_then(Value::as_str),
        Some("Haiku")
    );
    let commands = data
        .get("commands")
        .and_then(Value::as_array)
        .expect("commands");
    assert_eq!(commands.len(), 1);
    assert_eq!(
        commands[0].get("name").and_then(Value::as_str),
        Some("compact")
    );
    let initialization = data.get("initialization").expect("initialization");
    assert_eq!(
        initialization.get("models").and_then(Value::as_array),
        Some(models)
    );
}

#[test]
fn init_with_resume_preserves_session_id() {
    let mut sidecar = Sidecar::spawn();
    let id = sidecar.command(
        "init",
        json!({ "cwd": env!("CARGO_MANIFEST_DIR"), "resume": "r-1" }),
    );
    let response = sidecar.response(&id);
    assert_eq!(response.get("success"), Some(&Value::Bool(true)));
    assert_eq!(
        response
            .get("data")
            .and_then(|data| data.get("sessionId"))
            .and_then(Value::as_str),
        Some("r-1")
    );
    // The forwarded system/init message carries the resumed session id.
    let prompt_id = sidecar.command("prompt", json!({ "message": "hello" }));
    let prompt_response = sidecar.response(&prompt_id);
    assert_eq!(prompt_response.get("success"), Some(&Value::Bool(true)));
    sidecar.wait_until("system init", |records| {
        events(records, "system").any(|record| subtype(record) == "init")
    });
    let system = events(&sidecar.records, "system")
        .find(|record| subtype(record) == "init")
        .expect("system init");
    assert_eq!(
        system.get("session_id").and_then(Value::as_str),
        Some("r-1")
    );
}

#[test]
fn prompt_yields_started_disposition_and_full_turn() {
    let mut sidecar = Sidecar::spawn();
    sidecar.init();
    let prompt_id = sidecar.command("prompt", json!({ "message": "hello" }));
    let response = sidecar.response(&prompt_id);
    assert_eq!(response.get("success"), Some(&Value::Bool(true)));
    assert_eq!(
        response
            .get("data")
            .and_then(|data| data.get("disposition"))
            .and_then(Value::as_str),
        Some("started")
    );
    sidecar.wait_until("result success", |records| {
        events(records, "result").any(|record| subtype(record) == "success")
    });
    let turn: Vec<&str> = sidecar
        .records
        .iter()
        .filter(|record| {
            matches!(
                event_kind(record),
                Some("system") | Some("stream_event") | Some("assistant") | Some("result")
            )
        })
        .filter_map(event_kind)
        .collect();
    assert_eq!(
        turn,
        vec!["system", "stream_event", "assistant", "result"],
        "turn messages arrive in order"
    );
    let system = events(&sidecar.records, "system").next().expect("system");
    assert_eq!(subtype(system), "init");
    let result = events(&sidecar.records, "result").next().expect("result");
    assert_eq!(subtype(result), "success");
}

#[test]
fn abort_interrupts_slow_turn_and_process_stays_usable() {
    let mut sidecar = Sidecar::spawn();
    sidecar.init();
    let slow_id = sidecar.command("prompt", json!({ "message": "SLOW please" }));
    let slow_response = sidecar.response(&slow_id);
    assert_eq!(slow_response.get("success"), Some(&Value::Bool(true)));
    sidecar.wait_until("first stream_event", |records| {
        events(records, "stream_event").next().is_some()
    });
    let abort_id = sidecar.command("abort", json!({}));
    let abort_response = sidecar.response(&abort_id);
    assert_eq!(
        abort_response.get("command").and_then(Value::as_str),
        Some("abort")
    );
    assert_eq!(abort_response.get("success"), Some(&Value::Bool(true)));
    sidecar.wait_until("interrupted result", |records| {
        events(records, "result").any(|record| subtype(record) == "error_during_execution")
    });
    let interrupt_call = fake_call(&sidecar.records, "interrupt");
    assert_eq!(
        interrupt_call
            .get("args")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(0)
    );
    let result = events(&sidecar.records, "result")
        .find(|record| subtype(record) == "error_during_execution")
        .expect("interrupted result");
    assert_eq!(
        result.get("terminal_reason").and_then(Value::as_str),
        Some("aborted_streaming")
    );
    assert_eq!(result.get("is_error"), Some(&Value::Bool(true)));
    // The same child stays usable: a second plain prompt completes normally.
    let again_id = sidecar.command("prompt", json!({ "message": "again" }));
    let again_response = sidecar.response(&again_id);
    assert_eq!(again_response.get("success"), Some(&Value::Bool(true)));
    sidecar.wait_until("second result success", |records| {
        events(records, "result").any(|record| subtype(record) == "success")
    });
}

#[test]
fn tool_prompt_observes_can_use_tool_allow() {
    let mut sidecar = Sidecar::spawn();
    sidecar.init();
    let prompt_id = sidecar.command("prompt", json!({ "message": "TOOL please" }));
    let prompt_response = sidecar.response(&prompt_id);
    assert_eq!(prompt_response.get("success"), Some(&Value::Bool(true)));
    sidecar.wait_until("tool turn result", |records| {
        events(records, "result").any(|record| subtype(record) == "success")
    });
    let call = fake_call(&sidecar.records, "canUseTool");
    let decision = call.get("result").expect("canUseTool decision");
    assert_eq!(
        decision.get("behavior").and_then(Value::as_str),
        Some("allow")
    );
}

#[test]
fn set_model_and_set_effort_reach_the_query() {
    let mut sidecar = Sidecar::spawn();
    sidecar.init();
    let model_id = sidecar.command("set_model", json!({ "model": "haiku" }));
    let model_response = sidecar.response(&model_id);
    assert_eq!(model_response.get("success"), Some(&Value::Bool(true)));
    sidecar.wait_until("setModel fake_call", |records| {
        events(records, "fake_call")
            .any(|record| record.get("method").and_then(Value::as_str) == Some("setModel"))
    });
    assert_eq!(
        fake_call(&sidecar.records, "setModel").get("args"),
        Some(&json!(["haiku"]))
    );
    let effort_id = sidecar.command("set_effort", json!({ "level": "high" }));
    let effort_response = sidecar.response(&effort_id);
    assert_eq!(effort_response.get("success"), Some(&Value::Bool(true)));
    sidecar.wait_until("applyFlagSettings fake_call", |records| {
        events(records, "fake_call")
            .any(|record| record.get("method").and_then(Value::as_str) == Some("applyFlagSettings"))
    });
    let settings = fake_call(&sidecar.records, "applyFlagSettings")
        .get("args")
        .and_then(Value::as_array)
        .and_then(|args| args.first())
        .expect("applyFlagSettings settings");
    assert_eq!(
        settings.get("effortLevel").and_then(Value::as_str),
        Some("high")
    );
}

#[test]
fn get_context_usage_returns_fake_usage() {
    let mut sidecar = Sidecar::spawn();
    sidecar.init();
    let id = sidecar.command("get_context_usage", json!({}));
    let response = sidecar.response(&id);
    assert_eq!(response.get("success"), Some(&Value::Bool(true)));
    let data = response.get("data").expect("usage data");
    assert_eq!(data.get("totalTokens").and_then(Value::as_i64), Some(25730));
    assert_eq!(data.get("maxTokens").and_then(Value::as_i64), Some(200000));
    assert_eq!(data.get("percentage").and_then(Value::as_i64), Some(13));
}

#[test]
fn guard_rejects_prompt_before_init_and_second_init() {
    let mut sidecar = Sidecar::spawn();
    let early_id = sidecar.command("prompt", json!({ "message": "too early" }));
    let early = sidecar.response(&early_id);
    assert_eq!(
        early.get("success"),
        Some(&Value::Bool(false)),
        "prompt before init fails"
    );
    assert!(early.get("error").and_then(Value::as_str).is_some());
    let init_response = sidecar.init();
    assert_eq!(init_response.get("success"), Some(&Value::Bool(true)));
    let again_id = sidecar.command("init", json!({ "cwd": env!("CARGO_MANIFEST_DIR") }));
    let again = sidecar.response(&again_id);
    assert_eq!(
        again.get("success"),
        Some(&Value::Bool(false)),
        "second init fails"
    );
    assert!(again.get("error").and_then(Value::as_str).is_some());
}

#[test]
fn prompt_containing_u2028_is_one_command() {
    let mut sidecar = Sidecar::spawn();
    sidecar.init();
    let message = format!("hello{}world", '\u{2028}');
    let prompt_id = sidecar.command("prompt", json!({ "message": message }));
    let response = sidecar.response(&prompt_id);
    assert_eq!(response.get("success"), Some(&Value::Bool(true)));
    assert_eq!(
        response
            .get("data")
            .and_then(|data| data.get("disposition"))
            .and_then(Value::as_str),
        Some("started")
    );
    // The single parsed command runs exactly one turn.
    sidecar.wait_until("turn result", |records| {
        events(records, "result").any(|record| subtype(record) == "success")
    });
    let result_count = events(&sidecar.records, "result").count();
    assert_eq!(result_count, 1, "one turn for the U+2028 prompt");
    assert!(
        !sidecar
            .records
            .iter()
            .any(|record| record.get("command").and_then(Value::as_str) == Some("parse")),
        "no parse failure was answered"
    );
}

#[test]
fn shutdown_responds_then_child_exits_once() {
    let mut sidecar = Sidecar::spawn();
    let init_response = sidecar.init();
    assert_eq!(init_response.get("success"), Some(&Value::Bool(true)));
    let shutdown_id = sidecar.command("shutdown", json!({}));
    let shutdown_response = sidecar.response(&shutdown_id);
    assert_eq!(
        shutdown_response.get("command").and_then(Value::as_str),
        Some("shutdown")
    );
    assert_eq!(shutdown_response.get("success"), Some(&Value::Bool(true)));
    // The child exits and Exited arrives exactly once before the stream ends.
    let deadline = Instant::now() + STEP_TIMEOUT;
    let mut exited = 0;
    loop {
        match sidecar.process.try_recv() {
            Ok(PiRpcRecord::Record(_)) => {}
            Ok(PiRpcRecord::Error(text)) => panic!("protocol error after shutdown: {text}"),
            Ok(PiRpcRecord::Exited) => exited += 1,
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => break,
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for exit; stderr: {}",
            sidecar.process.stderr_tail()
        );
        sleep(POLL_INTERVAL);
    }
    assert_eq!(exited, 1, "Exited arrives exactly once");
}

//! Integration tests for the Claude Agent SDK sidecar
//! (`crates/ajax-web/sidecar/claude_sdk_sidecar.mjs`), running the real `node`
//! child behind the fake SDK fixture (`crates/ajax-web/tests/fixtures/
//! fake_claude_sdk.mjs`) through the existing [`JsonlProcess`] JSONL framing:
//! init/session ids, prompt/abort/tool/model/effort/context flows, guard
//! errors, U+2028 line framing, and shutdown/exit.

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

/// Spawn the sidecar under `node`, pointing it at the fake SDK fixture.
fn spawn_sidecar() -> JsonlProcess {
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
    JsonlProcess::spawn(Path::new("node"), &args, manifest)
        .expect("claude sdk sidecar must spawn (is node installed?)")
}

/// A spawned sidecar plus every record drained from it so far, so later
/// assertions can search records that earlier waits already consumed.
struct Sidecar {
    process: JsonlProcess,
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
                Ok(JsonlRecord::Record(value)) => self.records.push(value),
                Ok(JsonlRecord::Error(text)) => {
                    panic!("sidecar protocol error while waiting for {label}: {text}");
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
    assert_eq!(lengths, vec![8, 4, 4, 4, 12], "uuid group lengths");
    assert!(
        text.chars().all(|c| c.is_ascii_hexdigit() || c == '-'),
        "uuid-shaped id is hex-and-dash"
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

/// Issue #1274: when the Claude SDK message pump throws, or the query iterator
/// ends/exhausts while a prompt is in flight, the session must not stay busy
/// and healthy-looking. The in-flight turn has to finish with an error `result`
/// that the Rust client can surface; a silent dead sidecar is the defect.
#[test]
fn claude_pump_failure_ends_the_turn_and_reports_error_1274() {
    fn assert_lost_turn(sidecar: &mut Sidecar, label: &str, message: &str) {
        let prompt_id = sidecar.command("prompt", json!({ "message": message }));
        assert_eq!(
            sidecar.response(&prompt_id).get("success"),
            Some(&Value::Bool(true)),
            "prompt accepted: {label}"
        );
        // The in-flight turn must finish with an error result; if the pump
        // silently died the sidecar would stay busy and this times out.
        sidecar.wait_until(label, |records| {
            events(records, "result")
                .any(|record| record.get("is_error").and_then(Value::as_bool) == Some(true))
        });
        let result = events(&sidecar.records, "result")
            .find(|record| record.get("is_error").and_then(Value::as_bool) == Some(true))
            .expect("error result recorded");
        assert_eq!(subtype(result), "error_during_execution");
        let text = result
            .get("result")
            .and_then(Value::as_str)
            .unwrap_or_default();
        assert!(!text.is_empty(), "error result carries a message: {label}");
    }

    // Scenario 1: the SDK query iterator throws mid-turn (pump failure).
    {
        let mut sidecar = Sidecar::spawn();
        sidecar.init();
        assert_lost_turn(&mut sidecar, "pump failure result", "PUMP_FAIL now");
    }

    // Scenario 2: the iterator ends/exhausts mid-turn without a result.
    {
        let mut sidecar = Sidecar::spawn();
        sidecar.init();
        assert_lost_turn(&mut sidecar, "exhausted stream result", "EXHAUST now");
    }
}
#[test]
fn prompt_with_images_completes_turn_with_image_reply() {
    let mut sidecar = Sidecar::spawn();
    sidecar.init();
    let prompt_id = sidecar.command(
        "prompt",
        json!({
            "message": "look",
            "images": [{ "data": "AAAA", "mimeType": "image/png" }]
        }),
    );
    let response = sidecar.response(&prompt_id);
    assert_eq!(response.get("success"), Some(&Value::Bool(true)));
    sidecar.wait_until("result success", |records| {
        events(records, "result").any(|record| subtype(record) == "success")
    });
    let assistant = events(&sidecar.records, "assistant")
        .next()
        .expect("assistant event");
    let text: String = assistant
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_array)
        .expect("assistant content array")
        .iter()
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect();
    assert!(
        text.contains("images=1:image/png"),
        "assistant text: {text}"
    );
}

#[test]
fn prompt_with_malformed_images_is_rejected() {
    let mut sidecar = Sidecar::spawn();
    sidecar.init();
    let prompt_id = sidecar.command(
        "prompt",
        json!({ "message": "look", "images": [{ "data": 1 }] }),
    );
    let response = sidecar.response(&prompt_id);
    assert_eq!(
        response.get("command").and_then(Value::as_str),
        Some("prompt")
    );
    assert_eq!(response.get("success"), Some(&Value::Bool(false)));
    let error = response
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(error.contains("prompt images must be"), "error: {error}");
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
            Ok(JsonlRecord::Record(_)) => {}
            Ok(JsonlRecord::Error(text)) => panic!("protocol error after shutdown: {text}"),
            Ok(JsonlRecord::Exited) => exited += 1,
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

/// The assistant text of the first `assistant` event, panicking when absent.
fn first_assistant_text(records: &[Value]) -> String {
    let assistant = events(records, "assistant")
        .next()
        .expect("assistant event");
    assistant
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_array)
        .expect("assistant content array")
        .iter()
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect()
}

#[test]
fn form_elicitation_is_forwarded_and_answered() {
    let mut sidecar = Sidecar::spawn();
    sidecar.init();
    let prompt_id = sidecar.command("prompt", json!({ "message": "ELICIT go" }));
    assert_eq!(
        sidecar.response(&prompt_id).get("success"),
        Some(&Value::Bool(true))
    );
    // The form elicitation is forwarded to the host before any turn output.
    sidecar.wait_until("elicitation_request", |records| {
        events(records, "elicitation_request").next().is_some()
    });
    let request = events(&sidecar.records, "elicitation_request")
        .next()
        .expect("elicitation request record");
    assert_eq!(
        request.get("message").and_then(Value::as_str),
        Some("Pick a name")
    );
    let properties = request
        .get("schema")
        .and_then(|schema| schema.get("properties"))
        .and_then(Value::as_object)
        .expect("schema properties object");
    assert!(
        properties.contains_key("name"),
        "schema has a `name` property: {properties:?}"
    );
    let request_id = request
        .get("id")
        .and_then(Value::as_str)
        .expect("request id")
        .to_owned();
    // The response command's `id` field is the REQUEST id, not a fresh one.
    sidecar
        .process
        .send_with_id(
            "elicitation_response",
            &request_id,
            json!({ "action": "accept", "content": { "name": "Ada" } }),
        )
        .expect("send elicitation response");
    let answer = sidecar.response(&request_id);
    assert_eq!(answer.get("success"), Some(&Value::Bool(true)));
    sidecar.wait_until("elicit result", |records| {
        events(records, "result").any(|record| subtype(record) == "success")
    });
    let text = first_assistant_text(&sidecar.records);
    assert_eq!(text, r#"elicit:accept:{"name":"Ada"}"#);
}

#[test]
fn abort_cancels_pending_elicitation() {
    let mut sidecar = Sidecar::spawn();
    sidecar.init();
    let prompt_id = sidecar.command("prompt", json!({ "message": "ELICIT go" }));
    assert_eq!(
        sidecar.response(&prompt_id).get("success"),
        Some(&Value::Bool(true))
    );
    sidecar.wait_until("elicitation_request", |records| {
        events(records, "elicitation_request").next().is_some()
    });
    let abort_id = sidecar.command("abort", json!({}));
    assert_eq!(
        sidecar.response(&abort_id).get("success"),
        Some(&Value::Bool(true))
    );
    // The still-pending elicitation resolves as cancelled, completing the turn.
    sidecar.wait_until("elicit result", |records| {
        events(records, "result").any(|record| subtype(record) == "success")
    });
    let text = first_assistant_text(&sidecar.records);
    assert!(text.contains("elicit:cancel"), "assistant text: {text}");
}

#[test]
fn secret_elicitation_is_declined_without_forwarding() {
    let mut sidecar = Sidecar::spawn();
    sidecar.init();
    let prompt_id = sidecar.command("prompt", json!({ "message": "ELICIT_SECRET go" }));
    assert_eq!(
        sidecar.response(&prompt_id).get("success"),
        Some(&Value::Bool(true))
    );
    sidecar.wait_until("elicit result", |records| {
        events(records, "result").any(|record| subtype(record) == "success")
    });
    assert!(
        !sidecar.records.iter().any(|record| {
            record.get("type").and_then(Value::as_str) == Some("elicitation_request")
        }),
        "no elicitation_request was forwarded for a secret schema"
    );
    let text = first_assistant_text(&sidecar.records);
    assert_eq!(text, "elicit:decline:null");
}

#[test]
fn elicitation_response_with_unknown_id_fails() {
    let mut sidecar = Sidecar::spawn();
    sidecar.init();
    let id = "not-a-real-elicitation-id";
    sidecar
        .process
        .send_with_id("elicitation_response", id, json!({ "action": "accept" }))
        .expect("send elicitation response with unknown id");
    let answer = sidecar.response(id);
    assert_eq!(answer.get("success"), Some(&Value::Bool(false)));
    let error = answer
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(
        error.contains("unknown or invalid elicitation answer"),
        "error: {error}"
    );
}

#[test]
fn init_uses_claude_code_system_prompt_preset_1333() {
    let mut sidecar = Sidecar::spawn();
    sidecar.init();
    sidecar.wait_until("queryOptions fake_call", |records| {
        fake_call_present(records, "queryOptions")
    });
    let call = fake_call(&sidecar.records, "queryOptions");
    let options = call
        .get("args")
        .and_then(Value::as_array)
        .expect("queryOptions args")
        .first()
        .expect("options object");
    assert_eq!(
        options.get("systemPrompt"),
        Some(&json!({ "type": "preset", "preset": "claude_code" })),
        "init must pass the claude_code system prompt preset (issue #1333)"
    );
}

fn fake_call_present(records: &[Value], method: &str) -> bool {
    events(records, "fake_call")
        .any(|record| record.get("method").and_then(Value::as_str) == Some(method))
}

/// Issue #1357 (slice A): with CLAUDE_CODE_ENABLE_TODO_TOOLS=1 the Claude
/// Agent SDK exposes TaskCreate/TaskUpdate, and task ids only appear in the
/// TaskCreate tool result. The sidecar must mirror every change of the main-
/// loop ordered task list as an `ajax_plan` line, emitted AFTER forwarding
/// the SDK message that caused it.
#[test]
fn claude_task_tools_track_plan_as_ajax_plan_1357() {
    let mut sidecar = Sidecar::spawn();
    sidecar.init();
    // The init query must enable the Claude Agent SDK's todo tools.
    sidecar.wait_until("queryOptions fake_call", |records| {
        fake_call_present(records, "queryOptions")
    });
    let options = fake_call(&sidecar.records, "queryOptions")
        .get("args")
        .and_then(Value::as_array)
        .expect("queryOptions args")
        .first()
        .expect("options object");
    assert_eq!(
        options
            .get("env")
            .and_then(|env| env.get("CLAUDE_CODE_ENABLE_TODO_TOOLS")),
        Some(&json!("1")),
        "init must pass CLAUDE_CODE_ENABLE_TODO_TOOLS=1 (issue #1357)"
    );

    let prompt_id = sidecar.command("prompt", json!({ "message": "TASKS now" }));
    assert_eq!(
        sidecar.response(&prompt_id).get("success"),
        Some(&Value::Bool(true)),
        "prompt accepted"
    );
    sidecar.wait_until("task turn result", |records| {
        events(records, "result").any(|record| subtype(record) == "success")
    });

    let plans: Vec<Value> = events(&sidecar.records, "ajax_plan").cloned().collect();
    assert_eq!(
        plans.len(),
        4,
        "one ajax_plan per task-list change: {plans:?}"
    );
    assert_eq!(
        plans[0].get("entries"),
        Some(&json!([{ "content": "alpha", "status": "pending" }])),
        "create alpha"
    );
    assert_eq!(
        plans[1].get("entries"),
        Some(&json!([
            { "content": "alpha", "status": "pending" },
            { "content": "beta", "status": "pending" }
        ])),
        "create beta"
    );
    assert_eq!(
        plans[2].get("entries"),
        Some(&json!([
            { "content": "alpha", "status": "in_progress" },
            { "content": "beta", "status": "pending" }
        ])),
        "update alpha in_progress"
    );
    assert_eq!(
        plans[3].get("entries"),
        Some(&json!([
            { "content": "alpha", "status": "completed" },
            { "content": "beta", "status": "pending" }
        ])),
        "update alpha completed"
    );

    // Each ajax_plan line is emitted AFTER the SDK message that caused it, and
    // all of them arrive inside the forwarded turn (before its result).
    let plan_positions: Vec<usize> = sidecar
        .records
        .iter()
        .enumerate()
        .filter(|(_, record)| event_kind(record) == Some("ajax_plan"))
        .map(|(index, _)| index)
        .collect();
    let task_result_index = |needle: &str| {
        sidecar
            .records
            .iter()
            .position(|record| {
                event_kind(record) == Some("user") && record.to_string().contains(needle)
            })
            .unwrap_or_else(|| panic!("task result for {needle} forwarded"))
    };
    assert!(
        plan_positions[0] > task_result_index("Task #1"),
        "plan 1 follows the alpha create result"
    );
    assert!(
        plan_positions[1] > task_result_index("Task #2"),
        "plan 2 follows the beta create result"
    );
    let result_index = sidecar
        .records
        .iter()
        .position(|record| event_kind(record) == Some("result"))
        .expect("turn result forwarded");
    assert!(
        plan_positions[3] < result_index,
        "plans arrive before the turn result"
    );
}

/// Issue #1358: under permissionMode "bypassPermissions" the SDK never
/// consults canUseTool, so AskUserQuestion gets auto-answered. A PreToolUse
/// hook must forward the questions to the operator as an elicitation form and
/// inject the answers back into the tool input (deny when declined).
#[test]
fn ask_user_question_hook_forwards_elicitation_1358() {
    fn run_and_decide(sidecar: &mut Sidecar, action: &str) -> Value {
        let prompt_id = sidecar.command("prompt", json!({ "message": "ASK please" }));
        assert_eq!(
            sidecar.response(&prompt_id).get("success"),
            Some(&Value::Bool(true)),
            "prompt accepted"
        );
        // The hook forwards the question as an elicitation form before any
        // turn output.
        sidecar.wait_until("elicitation_request", |records| {
            events(records, "elicitation_request").next().is_some()
        });
        let request = events(&sidecar.records, "elicitation_request")
            .next()
            .expect("elicitation request record");
        assert_eq!(
            request.get("message").and_then(Value::as_str),
            Some("Red or blue?")
        );
        let properties = request
            .get("schema")
            .and_then(|schema| schema.get("properties"))
            .and_then(Value::as_object)
            .expect("schema properties object");
        let question_0 = properties.get("question_0").expect("schema has question_0");
        assert_eq!(
            question_0.get("title").and_then(Value::as_str),
            Some("Color")
        );
        let one_of = question_0
            .get("oneOf")
            .and_then(Value::as_array)
            .expect("question_0 oneOf");
        let labels: Vec<&str> = one_of
            .iter()
            .filter_map(|option| option.get("const").and_then(Value::as_str))
            .collect();
        assert_eq!(
            labels,
            vec!["Red", "Blue"],
            "oneOf consts are the option labels"
        );
        assert!(
            properties.contains_key("question_0_other"),
            "schema has question_0_other"
        );

        let request_id = request
            .get("id")
            .and_then(Value::as_str)
            .expect("request id")
            .to_owned();
        sidecar
            .process
            .send_with_id(
                "elicitation_response",
                &request_id,
                json!({ "action": action, "content": { "question_0": "Blue" } }),
            )
            .expect("send elicitation response");
        assert_eq!(
            sidecar.response(&request_id).get("success"),
            Some(&Value::Bool(true))
        );

        sidecar.wait_until("ask turn result", |records| {
            events(records, "result").any(|record| subtype(record) == "success")
        });
        let text = first_assistant_text(&sidecar.records);
        let decision: Value = serde_json::from_str(text.trim_start_matches("ask:"))
            .unwrap_or_else(|error| panic!("parse hook decision from {text:?}: {error}"));
        decision
    }

    // Accept: the hook allows the tool with the answers injected.
    {
        let mut sidecar = Sidecar::spawn();
        sidecar.init();
        let decision = run_and_decide(&mut sidecar, "accept");
        let output = decision
            .get("hookSpecificOutput")
            .expect("hookSpecificOutput");
        assert_eq!(
            output.get("permissionDecision").and_then(Value::as_str),
            Some("allow")
        );
        assert_eq!(
            output
                .get("updatedInput")
                .and_then(|input| input.get("answers")),
            Some(&json!({ "Red or blue?": "Blue" })),
            "hook injects the operator's answers into the tool input"
        );
    }

    // Decline: the hook denies the tool so no auto-answer is invented.
    {
        let mut sidecar = Sidecar::spawn();
        sidecar.init();
        let decision = run_and_decide(&mut sidecar, "decline");
        let output = decision
            .get("hookSpecificOutput")
            .expect("hookSpecificOutput");
        assert_eq!(
            output.get("permissionDecision").and_then(Value::as_str),
            Some("deny")
        );
    }
}

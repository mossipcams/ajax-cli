use super::map_acp_session_update;
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
};

struct FakeAcp {
    child: Child,
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
}

impl Drop for FakeAcp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl FakeAcp {
    fn spawn(extra_args: &[&str]) -> Option<Self> {
        if Command::new("node")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_err()
        {
            eprintln!("skip fake_acp tests: node not found on PATH");
            return None;
        }

        let script = fake_acp_fixture_path();
        let mut command = Command::new("node");
        command.arg(&script);
        command.args(extra_args);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn().expect("spawn fake acp");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        Some(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    fn call(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.write_request(id, method, params);
        self.read_line().expect("response line")
    }

    fn notify(&mut self, method: &str, params: Value) {
        let payload = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        write_line(&mut self.stdin, &payload);
    }

    fn write_request(&mut self, id: u64, method: &str, params: Value) {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        write_line(&mut self.stdin, &payload);
    }

    fn read_line(&mut self) -> Option<Value> {
        let mut line = String::new();
        loop {
            line.clear();
            match self.stdout.read_line(&mut line) {
                Ok(0) => return None,
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    return Some(serde_json::from_str(trimmed).expect("json line"));
                }
                Err(error) => panic!("read fake acp stdout: {error}"),
            }
        }
    }

    fn read_until_result(&mut self, id: u64) -> (Vec<Value>, Value) {
        let mut notifications = Vec::new();
        loop {
            let value = self.read_line().expect("line before result");
            if value.get("id").and_then(Value::as_u64) == Some(id) {
                return (notifications, value);
            }
            notifications.push(value);
        }
    }
}

fn fake_acp_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_acp.js")
}

fn write_line(stdin: &mut std::process::ChildStdin, payload: &Value) {
    let mut line = serde_json::to_string(payload).expect("serialize");
    line.push('\n');
    stdin.write_all(line.as_bytes()).expect("write stdin");
    stdin.flush().expect("flush stdin");
}

fn chunk_text_from_update(value: &Value) -> Option<String> {
    let params = value.get("params")?;
    let events = map_acp_session_update(params);
    events.into_iter().find_map(|event| match event {
        super::SessionServerEvent::Message { text, .. } => Some(text),
        _ => None,
    })
}

#[test]
fn initialize_advertises_load_session() {
    let Some(mut acp) = FakeAcp::spawn(&[]) else {
        return;
    };
    let response = acp.call(1, "initialize", json!({}));
    assert_eq!(
        response
            .pointer("/result/agentCapabilities/loadSession")
            .and_then(Value::as_bool),
        Some(true)
    );
}

#[test]
fn session_new_returns_session_id() {
    let Some(mut acp) = FakeAcp::spawn(&[]) else {
        return;
    };
    acp.call(1, "initialize", json!({}));
    let response = acp.call(2, "session/new", json!({ "cwd": "/tmp", "mcpServers": [] }));
    assert_eq!(
        response
            .pointer("/result/sessionId")
            .and_then(Value::as_str),
        Some("fake-sess-1")
    );
}

#[test]
fn session_prompt_emits_pong_then_end_turn() {
    let Some(mut acp) = FakeAcp::spawn(&[]) else {
        return;
    };
    acp.call(1, "initialize", json!({}));
    acp.call(2, "session/new", json!({ "cwd": "/tmp", "mcpServers": [] }));
    acp.write_request(
        3,
        "session/prompt",
        json!({
            "sessionId": "fake-sess-1",
            "prompt": [{ "type": "text", "text": "ping" }],
        }),
    );
    let (notifications, response) = acp.read_until_result(3);
    assert_eq!(notifications.len(), 1);
    assert_eq!(
        chunk_text_from_update(&notifications[0]).as_deref(),
        Some("pong")
    );
    assert_eq!(
        response
            .pointer("/result/stopReason")
            .and_then(Value::as_str),
        Some("end_turn")
    );
}

#[test]
fn session_load_replays_update_then_result() {
    let Some(mut acp) = FakeAcp::spawn(&[]) else {
        return;
    };
    acp.call(1, "initialize", json!({}));
    acp.write_request(
        2,
        "session/load",
        json!({
            "cwd": "/tmp",
            "mcpServers": [],
            "sessionId": "fake-sess-1",
        }),
    );
    let (notifications, response) = acp.read_until_result(2);
    assert_eq!(notifications.len(), 1);
    assert_eq!(
        chunk_text_from_update(&notifications[0]).as_deref(),
        Some("replayed")
    );
    assert!(response.get("result").is_some());
    assert!(response.get("error").is_none());
}

#[test]
fn session_load_with_load_fail_returns_error() {
    let Some(mut acp) = FakeAcp::spawn(&["--load-fail"]) else {
        return;
    };
    acp.call(1, "initialize", json!({}));
    let response = acp.call(
        2,
        "session/load",
        json!({
            "cwd": "/tmp",
            "mcpServers": [],
            "sessionId": "fake-sess-1",
        }),
    );
    assert!(response.get("error").is_some());
    assert!(response.get("result").is_none());
}

// Checked against every installed harness: cancel is a notification. Sent as a
// request, Cursor, Codex, Claude, and Pi all answer "Method not found" and keep
// working — which is how Stop stayed a no-op.
#[test]
fn session_cancel_as_a_request_is_rejected() {
    let Some(mut acp) = FakeAcp::spawn(&[]) else {
        return;
    };
    acp.call(1, "initialize", json!({}));
    acp.call(2, "session/new", json!({ "cwd": "/tmp", "mcpServers": [] }));

    let response = acp.call(3, "session/cancel", json!({ "sessionId": "fake-sess-1" }));

    assert!(response.get("result").is_none());
    assert_eq!(
        response.pointer("/error/code").and_then(Value::as_i64),
        Some(-32601)
    );
}

#[test]
fn session_cancel_as_a_notification_ends_the_turn() {
    let Some(mut acp) = FakeAcp::spawn(&["--hold-prompt"]) else {
        return;
    };
    acp.call(1, "initialize", json!({}));
    acp.call(2, "session/new", json!({ "cwd": "/tmp", "mcpServers": [] }));
    acp.write_request(
        3,
        "session/prompt",
        json!({ "sessionId": "fake-sess-1", "prompt": [{ "type": "text", "text": "hold" }] }),
    );

    acp.notify("session/cancel", json!({ "sessionId": "fake-sess-1" }));

    let response = acp.read_line().expect("held prompt settles");
    assert_eq!(response.get("id").and_then(Value::as_u64), Some(3));
    assert_eq!(
        response
            .pointer("/result/stopReason")
            .and_then(Value::as_str),
        Some("cancelled")
    );
}

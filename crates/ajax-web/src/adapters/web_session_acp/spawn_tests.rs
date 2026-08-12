//! Spawn-level ACP reliability: fake stdio agent (G1) and optional live Cursor smoke.

use super::client::{with_test_acp_extra_args, with_test_acp_program, AcpStdioClient};
use super::hub::WebSessionHub;
use crate::slices::web_session::SessionServerEvent;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const CONTEXT_RESET_NOTE: &str =
    "Model context reset after restart. Prior turns are still visible here.";

const FAKE_ACP_SCRIPT: &str = r#"#!/usr/bin/env node
'use strict';
const readline = require('readline');
const loadFail = process.argv.includes('--load-fail');
const sessionId = 'fake-sess-1';

function send(obj) {
  process.stdout.write(JSON.stringify(obj) + '\n');
}

function replayUpdate(text) {
  send({
    jsonrpc: '2.0',
    method: 'session/update',
    params: {
      sessionId,
      update: {
        sessionUpdate: 'agent_message_chunk',
        content: { type: 'text', text },
      },
    },
  });
}

function handleRequest(msg) {
  const { id, method, params } = msg;
  if (method === 'initialize') {
    send({
      jsonrpc: '2.0',
      id,
      result: { agentCapabilities: { loadSession: true } },
    });
    return;
  }
  if (method === 'session/new') {
    send({ jsonrpc: '2.0', id, result: { sessionId } });
    return;
  }
  if (method === 'session/load') {
    if (loadFail) {
      send({
        jsonrpc: '2.0',
        id,
        error: { code: -32000, message: 'load failed' },
      });
      return;
    }
    replayUpdate('replayed');
    send({ jsonrpc: '2.0', id, result: {} });
    return;
  }
  if (method === 'session/prompt') {
    replayUpdate('pong');
    send({ jsonrpc: '2.0', id, result: { stopReason: 'end_turn' } });
    return;
  }
  if (method === 'session/cancel') {
    send({ jsonrpc: '2.0', id, result: {} });
    return;
  }
  send({
    jsonrpc: '2.0',
    id,
    error: { code: -32601, message: 'unknown method: ' + method },
  });
}

const rl = readline.createInterface({ input: process.stdin, terminal: false });
rl.on('line', (line) => {
  const trimmed = line.trim();
  if (!trimmed) return;
  let msg;
  try {
    msg = JSON.parse(trimmed);
  } catch {
    return;
  }
  if (msg.method && msg.id === undefined) {
    return;
  }
  if (msg.id !== undefined && msg.method) {
    handleRequest(msg);
  }
});
process.stdin.on('end', () => process.exit(0));
"#;

fn scratch_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ajax-web-spawn-tests-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_fake_acp_script(dir: &Path) -> PathBuf {
    let path = dir.join("fake-acp-agent.js");
    let mut file = fs::File::create(&path).unwrap();
    file.write_all(FAKE_ACP_SCRIPT.as_bytes()).unwrap();
    path
}

fn pump_until_pong_or_turn_end(hub: &WebSessionHub, handle: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        hub.pump(handle);
        let (events, _) = hub.read_from(handle, 0);
        let done = events.iter().any(|event| match event {
            SessionServerEvent::TurnEnd { .. } => true,
            SessionServerEvent::Message { text, .. } => text == "pong",
            _ => false,
        });
        if done {
            return;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for pong or turn_end; events={events:?}");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn log_contains_text(hub: &WebSessionHub, handle: &str, needle: &str) -> bool {
    let (events, _) = hub.read_from(handle, 0);
    events.iter().any(|event| match event {
        SessionServerEvent::Message { text, .. } => text.contains(needle),
        _ => false,
    })
}

fn cursor_agent_present() -> bool {
    Command::new("agent")
        .arg("--help")
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[test]
fn g1_respawns_after_child_death_and_prompt_works() {
    let dir = scratch_dir("g1");
    let script = write_fake_acp_script(&dir);
    let handle = "web/g1-respawn";
    let hub = WebSessionHub::new(dir.clone());

    with_test_acp_program(&script, || {
        let client = hub.acquire(handle, &dir, "auto").expect("first acquire");
        let pid1 = client.lock().unwrap().child_id();

        {
            let mut guard = client.lock().unwrap();
            guard.kill_host_for_test();
            assert!(guard.host_exited());
        }

        let client2 = hub.acquire(handle, &dir, "auto").expect("second acquire");
        let pid2 = client2.lock().unwrap().child_id();
        assert_ne!(pid1, pid2);
        assert!(!client2.lock().unwrap().host_exited());

        client2
            .lock()
            .unwrap()
            .begin_prompt("hi")
            .expect("begin_prompt");
        pump_until_pong_or_turn_end(&hub, handle, Duration::from_secs(5));
        assert!(hub.generation(handle) > 0);
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn g1_load_fail_appends_context_reset_note() {
    let dir = scratch_dir("load-fail");
    let script = write_fake_acp_script(&dir);
    let handle = "web/g1-load-fail";
    let hub = WebSessionHub::new(dir.clone());

    with_test_acp_program(&script, || {
        let client = hub.acquire(handle, &dir, "auto").expect("first acquire");
        hub.record(
            handle,
            SessionServerEvent::Message {
                role: "user".to_string(),
                text: "seed".to_string(),
            },
        );
        client.lock().unwrap().kill_host_for_test();

        with_test_acp_extra_args(&["--load-fail"], || {
            hub.acquire(handle, &dir, "auto")
                .expect("acquire after load-fail spawn");
        });

        assert!(log_contains_text(&hub, handle, CONTEXT_RESET_NOTE));
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn g1_successful_load_drains_replay_from_transcript() {
    let dir = scratch_dir("load-drain");
    let script = write_fake_acp_script(&dir);
    let handle = "web/g1-load-drain";
    let hub = WebSessionHub::new(dir.clone());

    with_test_acp_program(&script, || {
        let client = hub.acquire(handle, &dir, "auto").expect("first acquire");
        hub.record(
            handle,
            SessionServerEvent::Message {
                role: "user".to_string(),
                text: "seed".to_string(),
            },
        );
        hub.record(
            handle,
            SessionServerEvent::Message {
                role: "agent".to_string(),
                text: "prior".to_string(),
            },
        );
        let (_, cursor) = hub.read_from(handle, 0);
        client.lock().unwrap().kill_host_for_test();

        hub.acquire(handle, &dir, "auto")
            .expect("acquire after successful load");

        let (delta, _) = hub.read_from(handle, cursor);
        assert!(
            !delta.iter().any(|event| matches!(
                event,
                SessionServerEvent::Message { text, .. } if text == "replayed"
            )),
            "replayed session/update must not reach the transcript"
        );
        let (full, _) = hub.read_from(handle, 0);
        assert!(!full.iter().any(|event| matches!(
            event,
            SessionServerEvent::Message { text, .. } if text == "replayed"
        )));
        assert!(!log_contains_text(&hub, handle, CONTEXT_RESET_NOTE));
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn live_cursor_initialize_advertises_load_session() {
    if !cursor_agent_present() {
        eprintln!("skip: agent not on PATH");
        return;
    }

    let dir = scratch_dir("live-init");
    let (client, report) = AcpStdioClient::spawn(&dir, None, None).expect("spawn live agent acp");
    assert!(
        report.load_session_advertised,
        "initialize must advertise loadSession on current Cursor"
    );
    assert!(!client.session_id().is_empty());
    drop(client);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn live_cursor_prompt_and_session_load() {
    if std::env::var("AJAX_ACP_SMOKE").ok().as_deref() != Some("1") {
        eprintln!("skip: AJAX_ACP_SMOKE not set");
        return;
    }
    if !cursor_agent_present() {
        eprintln!("skip: agent not on PATH");
        return;
    }

    let dir = scratch_dir("live-smoke");
    let (mut client, _report) =
        AcpStdioClient::spawn(&dir, None, None).expect("spawn live agent acp");
    let session_id = client.session_id().to_string();

    client
        .begin_prompt("Reply with exactly the word pong and nothing else.")
        .expect("begin_prompt");

    let deadline = Instant::now() + Duration::from_secs(45);
    let mut saw_pong = false;
    loop {
        if let Some(event) = client.wait_event(Duration::from_millis(200)) {
            use super::client::AcpClientEvent;
            match event {
                AcpClientEvent::SessionUpdate(params) => {
                    let text = params
                        .pointer("/update/content/text")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    if text.contains("pong") {
                        saw_pong = true;
                    }
                }
                AcpClientEvent::RequestFinished {
                    method: "session/prompt",
                    ..
                } => {
                    break;
                }
                _ => {}
            }
        }
        if saw_pong {
            break;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for pong from live agent");
        }
    }

    drop(client);

    let (client2, report2) =
        AcpStdioClient::spawn(&dir, None, Some(&session_id)).expect("respawn with session/load");
    assert!(
        report2.resumed,
        "session/load should succeed for session_id={session_id}"
    );
    drop(client2);

    let _ = fs::remove_dir_all(dir);
}

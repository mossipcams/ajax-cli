//! Spawn-level ACP reliability: fake stdio agent (G1) and optional live Cursor smoke.

use super::client::{with_test_acp_program, AcpClientEvent, AcpStdioClient};
use super::hub::WebSessionHub;
use crate::slices::web_session::SessionServerEvent;
use ajax_core::models::AgentClient;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const CONTEXT_RESET_NOTE: &str =
    "Model context reset after restart. Prior turns are still visible here.";

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

fn fake_acp_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_acp.js")
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
    let script = fake_acp_fixture();
    let handle = "web/g1-respawn";
    let hub = WebSessionHub::new(dir.clone());

    with_test_acp_program(&script, || {
        hub.acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("first acquire");
        let pid1 = hub.child_id(handle).expect("pid1");
        hub.kill_host_for_test(handle);

        hub.acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("second acquire");
        let pid2 = hub.child_id(handle).expect("pid2");
        assert_ne!(pid1, pid2);

        hub.submit_prompt(handle, "hi".to_string())
            .expect("submit_prompt");
        pump_until_pong_or_turn_end(&hub, handle, Duration::from_secs(5));
        assert!(hub.generation(handle) > 0);
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn g1_load_fail_appends_context_reset_note() {
    let dir = scratch_dir("load-fail");
    let script = fake_acp_fixture();
    let handle = "web/g1-load-fail";
    let hub = WebSessionHub::new(dir.clone());

    with_test_acp_program(&script, || {
        hub.acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("first acquire");
        hub.record(
            handle,
            SessionServerEvent::Message {
                role: "user".to_string(),
                text: "seed".to_string(),
            },
        );
        hub.kill_host_for_test(handle);

        super::client::with_test_acp_extra_args(&["--load-fail"], || {
            hub.acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire after load-fail spawn");
        });

        assert!(log_contains_text(&hub, handle, CONTEXT_RESET_NOTE));
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn g1_successful_load_drains_replay_from_transcript() {
    let dir = scratch_dir("load-drain");
    let script = fake_acp_fixture();
    let handle = "web/g1-load-drain";
    let hub = WebSessionHub::new(dir.clone());

    with_test_acp_program(&script, || {
        hub.acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("first acquire");
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
        hub.kill_host_for_test(handle);

        hub.acquire(handle, &dir, "auto", AgentClient::Cursor)
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
    let (client, report) =
        AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn live agent acp");
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
        AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn live agent acp");
    let session_id = client.session_id().to_string();

    client
        .begin_prompt("Reply with exactly the word pong and nothing else.")
        .expect("begin_prompt");

    let deadline = Instant::now() + Duration::from_secs(45);
    let mut saw_pong = false;
    loop {
        if let Some(event) = client.wait_event(Duration::from_millis(200)) {
            match event {
                AcpClientEvent::SessionUpdate(params) => {
                    let text = serde_json::to_value(&params)
                        .ok()
                        .and_then(|value| value.pointer("/update/content/text").cloned())
                        .and_then(|value| value.as_str().map(str::to_string))
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
        AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, Some(&session_id))
            .expect("respawn with session/load");
    assert!(report2.resumed, "session/load should report resumed");
    drop(client2);

    let _ = fs::remove_dir_all(dir);
}

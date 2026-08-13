//! Optional live Cursor ACP smoke tests (PATH + `AJAX_ACP_SMOKE=1`).

use super::client::{AcpClientEvent, AcpStdioClient};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

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

fn cursor_agent_present() -> bool {
    Command::new("agent")
        .arg("--help")
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
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
    assert!(report2.resumed, "session/load should report resumed");
    drop(client2);

    let _ = fs::remove_dir_all(dir);
}

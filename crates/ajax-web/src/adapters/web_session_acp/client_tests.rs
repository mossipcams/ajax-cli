//! Unit and fake-stdio integration tests for [`super::client`].

use super::client::{
    cursor_acp_args_for_program, cursor_acp_program_candidates, load_session_advertised,
    session_new_params, AcpClientEvent, AcpStdioClient,
};
use super::{with_test_acp_extra_args, with_test_acp_program};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn fake_acp_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_acp.js")
}

fn scratch_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ajax-web-acp-tests-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn session_update_text(params: &Value) -> Option<&str> {
    params
        .pointer("/update/content/text")
        .and_then(Value::as_str)
}

fn pump_until_pong_or_prompt_finished(client: &AcpStdioClient, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(event) = client.poll_event() {
            match event {
                AcpClientEvent::SessionUpdate(params) => {
                    if session_update_text(&params) == Some("pong") {
                        return;
                    }
                }
                AcpClientEvent::RequestFinished {
                    method: "session/prompt",
                    ..
                } => return,
                _ => {}
            }
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for pong or session/prompt finished");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn load_session_advertised_from_initialize_result() {
    let value = json!({ "agentCapabilities": { "loadSession": true } });
    assert!(load_session_advertised(&value));
    assert!(!load_session_advertised(&json!({})));
    assert!(!load_session_advertised(&json!({
        "agentCapabilities": { "loadSession": false }
    })));
}

/// Cursor validates `session/new` params and rejects a missing
/// `mcpServers` with an opaque JSON-RPC "Internal error", so the session
/// could never start. Keep the key present and an array.
#[test]
fn session_new_params_carry_mcp_servers_array() {
    let params = session_new_params(Path::new("/repo/worktree"));
    assert_eq!(
        params.get("cwd").and_then(Value::as_str),
        Some("/repo/worktree")
    );
    assert_eq!(
        params.get("mcpServers").and_then(Value::as_array),
        Some(&vec![])
    );
}

#[test]
fn cursor_acp_command_prefers_agent_binary() {
    let candidates = cursor_acp_program_candidates();
    assert_eq!(candidates[0].0, "agent");
    assert_eq!(candidates[0].1, &["acp"][..]);
    assert_eq!(candidates[1].0, "cursor");
    assert_eq!(candidates[1].1, &["agent", "acp"][..]);
}

#[test]
fn cursor_acp_args_insert_model_before_acp() {
    assert_eq!(
        cursor_acp_args_for_program(&["acp"], Some("composer-2.5")),
        vec!["--model", "composer-2.5", "acp"]
    );
    assert_eq!(
        cursor_acp_args_for_program(&["agent", "acp"], Some("gpt-5.6-sol-medium")),
        vec!["agent", "--model", "gpt-5.6-sol-medium", "acp"]
    );
    assert_eq!(
        cursor_acp_args_for_program(&["acp"], Some("auto")),
        vec!["acp"]
    );
    assert_eq!(cursor_acp_args_for_program(&["acp"], None), vec!["acp"]);
}

#[test]
fn fake_spawn_reports_load_session_advertised() {
    let dir = scratch_dir("spawn-advertised");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let (_client, report) = AcpStdioClient::spawn(&dir, None, None).expect("spawn fake acp");
        assert!(report.load_session_advertised);
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn fake_begin_prompt_receives_pong_and_turn_end() {
    let dir = scratch_dir("prompt-pong");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let (mut client, _report) =
            AcpStdioClient::spawn(&dir, None, None).expect("spawn fake acp");
        client.begin_prompt("ping").expect("begin_prompt");
        pump_until_pong_or_prompt_finished(&client, Duration::from_secs(5));
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn fake_second_begin_prompt_while_in_flight_returns_err() {
    let dir = scratch_dir("prompt-in-flight");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let (mut client, _report) =
            AcpStdioClient::spawn(&dir, None, None).expect("spawn fake acp");
        client.begin_prompt("first").expect("first begin_prompt");
        let err = client
            .begin_prompt("second")
            .expect_err("second prompt must fail");
        assert!(err.contains("prompt already in flight"));
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn fake_resume_drains_replayed_session_updates() {
    let dir = scratch_dir("resume-drain");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let (client, first_report) = AcpStdioClient::spawn(&dir, None, None).expect("first spawn");
        assert!(!first_report.resumed);
        let session_id = client.session_id().to_string();
        drop(client);

        let (client2, second_report) =
            AcpStdioClient::spawn(&dir, None, Some(&session_id)).expect("resume spawn");
        assert!(second_report.resumed);
        assert_eq!(client2.session_id(), session_id);
        assert!(
            client2.poll_event().is_none(),
            "replayed session/update must be drained after session/load"
        );
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn fake_load_fail_falls_back_to_new_session() {
    let dir = scratch_dir("load-fail");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let (client, _first_report) = AcpStdioClient::spawn(&dir, None, None).expect("first spawn");
        let resume_id = client.session_id().to_string();
        drop(client);

        with_test_acp_extra_args(&["--load-fail"], || {
            let (mut client2, report) =
                AcpStdioClient::spawn(&dir, None, Some(&resume_id)).expect("spawn after load fail");
            assert!(!report.resumed);
            assert!(!client2.session_id().is_empty());
            client2.begin_prompt("after-fail").expect("begin_prompt");
            pump_until_pong_or_prompt_finished(&client2, Duration::from_secs(5));
        });
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn host_exited_and_kill_host_for_test() {
    let dir = scratch_dir("host-exited");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let (mut client, _report) =
            AcpStdioClient::spawn(&dir, None, None).expect("spawn fake acp");
        assert!(!client.host_exited());
        let _pid = client.child_id();
        client.kill_host_for_test();
        assert!(client.host_exited());
    });

    let _ = fs::remove_dir_all(dir);
}

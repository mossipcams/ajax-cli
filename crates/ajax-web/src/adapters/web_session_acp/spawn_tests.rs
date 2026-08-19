//! Spawn-level ACP reliability: optional live Cursor smoke.

use super::client::{AcpClientEvent, AcpStdioClient};
use super::{with_test_acp_extra_args, with_test_acp_program};
use ajax_core::adapters::{cursor_unspecified_spawn_satisfied, CURSOR_DEFAULT_SPAWN_MODEL};
use ajax_core::models::AgentClient;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn fake_acp_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_acp.js")
}

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

// Regression for #979: parameterized picker applies Grok High as split options, not Fast.
#[test]
fn cursor_parameterized_picker_applies_grok_high_without_fast_issue_979() {
    let dir = scratch_dir("model-cursor-parameterized-grok-979");
    let script = fake_acp_fixture();
    let catalog_id = "cursor-grok-4.6-high";

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(
            &[
                "--cursor-parameterized-models",
                "--cli-default-model",
                "--ignore-spawn-model-once",
            ],
            || {
                let (client, report) = AcpStdioClient::spawn_with_operator_pin(
                    AgentClient::Cursor,
                    &dir,
                    catalog_id,
                    None,
                )
                .expect("spawn");
                assert!(
                    report.model_apply_error.is_none(),
                    "parameterized apply must satisfy Grok High: {:?}",
                    report.model_apply_error
                );
                assert_eq!(report.applied_model, "grok-4.6[effort=high,fast=false]");
                assert_ne!(report.applied_model, "composer-2.5[fast=true]");
                assert_ne!(report.applied_model, "grok-4.6[effort=high,fast=true]");
                let deadline = Instant::now() + Duration::from_secs(5);
                while Instant::now() < deadline {
                    if let Some(AcpClientEvent::SessionUpdate(update)) =
                        client.wait_event(Duration::from_millis(100))
                    {
                        let text = serde_json::to_string(&update).unwrap();
                        assert!(
                            !text.contains("model:session/set_config_option:cursor-grok-4.6-high"),
                            "must not send catalog id as set_config_option value: {text}"
                        );
                        if text.contains("model:session/set_config_option:false") {
                            return;
                        }
                    }
                }
                panic!("parameterized apply never set fast=false");
            },
        );
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression for #979: parameterized Auto/unspecified clears Fast before refusing.
#[test]
fn cursor_parameterized_unspecified_clears_fast_on_attach_issue_979() {
    let dir = scratch_dir("model-cursor-parameterized-unspecified-979");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(
            &[
                "--cursor-parameterized-models",
                "--cli-default-model",
                "--ignore-spawn-model-once",
            ],
            || {
                let (_client, report) =
                    AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn");
                assert!(
                    report.model_apply_error.is_none(),
                    "unspecified attach must clear Fast: {:?}",
                    report.model_apply_error
                );
                assert!(
                    cursor_unspecified_spawn_satisfied(&report.applied_model),
                    "applied {:?} must satisfy unspecified spawn default {CURSOR_DEFAULT_SPAWN_MODEL}",
                    report.applied_model
                );
                assert_ne!(report.applied_model, "composer-2.5[fast=true]");
                assert_ne!(report.applied_model, "grok-4.6[effort=high,fast=true]");
            },
        );
    });

    let _ = fs::remove_dir_all(dir);
}

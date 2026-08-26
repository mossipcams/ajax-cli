//! Spawn-level ACP reliability: optional live Cursor smoke.

use super::client::{acp_args_for_program, AcpClientEvent, AcpStdioClient};
use super::{with_test_acp_extra_args, with_test_acp_program};
use agent_client_protocol::schema::v1::{ContentBlock, TextContent};
use ajax_core::adapters::{
    acp_launch_for_agent, cursor_catalog_to_acp_in_band_token, cursor_catalog_to_acp_spawn_token,
    cursor_unspecified_spawn_satisfied, CURSOR_DEFAULT_SPAWN_MODEL,
};
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

fn cursor_launch() -> ajax_core::adapters::AcpLaunch {
    acp_launch_for_agent(AgentClient::Cursor).expect("cursor acp launch")
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
        .begin_prompt(&[ContentBlock::Text(TextContent::new(
            "Reply with exactly the word pong and nothing else.",
        ))])
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
                assert_eq!(report.applied_model, "grok-4.6");
                assert_ne!(report.applied_model, "composer-2.5");
                assert_ne!(report.applied_model, "composer-2.5[fast=true]");
                let options = report
                    .config_options
                    .as_deref()
                    .expect("parameterized apply must advertise config options");
                assert!(
                    super::config_options::pin_satisfied(Some(options), catalog_id, true),
                    "parameterized apply must satisfy Grok High pin"
                );
                let fast = super::config_options::model_config_boolean_option(options)
                    .expect("parameterized harness must advertise Fast");
                assert_eq!(
                    super::config_options::read_fast_current_value(fast),
                    Some(false),
                    "Grok High must apply Fast=false"
                );
                let deadline = Instant::now() + Duration::from_millis(500);
                while Instant::now() < deadline {
                    let Some(event) = client.wait_event(Duration::from_millis(50)) else {
                        continue;
                    };
                    if let AcpClientEvent::SessionUpdate(update) = event {
                        let text = serde_json::to_string(&update).unwrap();
                        assert!(
                            !text.contains("model:session/set_config_option:cursor-grok-4.6-high"),
                            "must not send catalog id as set_config_option value: {text}"
                        );
                    }
                }
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

// Regression for #989: operator-pin recovery must not session/new while the prior child lives.
#[test]
fn spawn_with_operator_pin_recovery_waits_for_prior_child_shutdown_issue_989() {
    let dir = scratch_dir("spawn-recover-exclusive-989");
    let script = fake_acp_fixture();
    let lock = dir.join(".fake-acp-exclusive-lock");
    let _ = fs::remove_file(&lock);
    let catalog_id = "composer-2.5-fast";

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(
            &[
                "--exclusive-session-new",
                "--cursor-models",
                "--cli-default-model",
                "--ignore-spawn-model-once",
                "--refuse-in-band-once",
            ],
            || {
                let (_client, report) = AcpStdioClient::spawn_with_operator_pin(
                    AgentClient::Cursor,
                    &dir,
                    catalog_id,
                    None,
                )
                .expect("recovery spawn must not fail session/new with incoming_transport_closed");
                assert!(
                    report.model_apply_error.is_none(),
                    "composer fast pin must apply after recovery: {:?}",
                    report.model_apply_error
                );
                assert_eq!(report.applied_model, "composer-2.5[fast=true]");
            },
        );
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression for #991: pipe-form Cursor picks reconstruct catalog ids on spawn argv.
#[test]
fn cursor_spawn_pipe_form_reconstructs_catalog_id_on_argv_issue_991() {
    let launch = cursor_launch();

    let cases = [
        ("grok-4.6|effort=high|fast=false", "cursor-grok-4.6-high"),
        ("composer-2.5|fast=true", "composer-2.5-fast"),
        (
            "claude-opus-5|effort=medium|fast=false",
            "claude-opus-5-medium",
        ),
        ("claude-opus-5|effort=high|fast=false", "claude-opus-5-high"),
        ("gpt-5.6-sol|effort=high|fast=false", "gpt-5.6-sol-high"),
    ];
    for (pipe_form, catalog_id) in cases {
        let spawn = cursor_catalog_to_acp_spawn_token(pipe_form);
        assert_eq!(
            spawn, catalog_id,
            "pipe form {pipe_form} must reconstruct {catalog_id}"
        );
        assert!(
            !spawn.contains("-thinking-"),
            "spawn argv must not infer thinking variants for {pipe_form}"
        );
        assert_eq!(
            acp_args_for_program(launch, &["acp"], Some(pipe_form)),
            vec!["--model", catalog_id, "acp"]
        );
        assert!(
            !catalog_id.contains('['),
            "spawn argv must not synthesize bracket tokens for {pipe_form}"
        );
    }

    assert_eq!(
        cursor_catalog_to_acp_in_band_token("grok-4.6|effort=high|fast=false"),
        "grok-4.6[effort=high,fast=false]"
    );
}

// Regression for #1079: spawn argv never receives pipe-form or bracket tokens.
#[test]
fn cursor_spawn_rejects_pipe_and_bracket_on_argv_issue_1079() {
    use ajax_core::adapters::{
        acp_args_for_candidate, acp_launch_for_agent, acp_spawn_model_for_argv,
        cursor_catalog_to_acp_spawn_token, CURSOR_DEFAULT_SPAWN_MODEL,
    };

    let pass_through = [
        "claude-opus-5",
        "claude-opus-5-medium",
        "claude-opus-5-thinking-high",
        "composer-2.5",
        CURSOR_DEFAULT_SPAWN_MODEL,
    ];
    for catalog_id in pass_through {
        let spawn = cursor_catalog_to_acp_spawn_token(catalog_id);
        assert_eq!(
            spawn, catalog_id,
            "{catalog_id} must pass through unchanged"
        );
        assert!(
            !spawn.contains('|') && !spawn.contains('['),
            "spawn argv must not contain pipe or bracket for {catalog_id}"
        );
    }

    let bracket = "claude-opus-5[thinking=true,context=300k,effort=high,fast=false]";
    let reconstructed = "claude-opus-5-thinking-high";
    assert_eq!(
        cursor_catalog_to_acp_spawn_token(bracket),
        reconstructed,
        "bracket handshake id must reconstruct to exploded catalog id"
    );

    let pipe_thinking = "claude-opus-5|thinking=true|effort=high|fast=false";
    assert_eq!(
        cursor_catalog_to_acp_spawn_token(pipe_thinking),
        reconstructed,
        "pipe-form with thinking must reconstruct to exploded catalog id"
    );

    let launch = acp_launch_for_agent(AgentClient::Cursor).expect("cursor");
    for raw in ["claude-opus-5", bracket, pipe_thinking] {
        let spawn = acp_spawn_model_for_argv(launch, Some(raw)).expect("spawn model");
        assert!(
            !spawn.contains('|') && !spawn.contains('['),
            "acp_spawn_model_for_argv must not pass pipe/bracket for {raw:?}: {spawn}"
        );
    }
    assert_eq!(
        acp_spawn_model_for_argv(launch, Some("claude-opus-5")),
        Some("claude-opus-5".to_string())
    );
    assert_eq!(
        acp_args_for_candidate(launch, &["acp"], Some(bracket)),
        vec![
            "--model".to_string(),
            reconstructed.to_string(),
            "acp".to_string(),
        ]
    );
}

// Live Cursor: Product-scope stored pins and handshake currentValue must session/new.
#[test]
fn live_cursor_spawn_product_scope_pins_issue_1079() {
    if std::env::var("AJAX_ACP_SMOKE").ok().as_deref() != Some("1") {
        eprintln!("skip: AJAX_ACP_SMOKE not set");
        return;
    }
    if !cursor_agent_present() {
        eprintln!("skip: agent not on PATH");
        return;
    }

    let pins = [
        "claude-opus-5",
        "claude-opus-5[thinking=true,context=300k,effort=high,fast=false]",
        "claude-opus-5|thinking=true|effort=high|fast=false",
        "claude-opus-5-thinking-high",
    ];
    for pin in pins {
        let dir = scratch_dir("live-1079");
        eprintln!("live issue_1079 spawn_with_operator_pin pin={pin}");
        let result = AcpStdioClient::spawn_with_operator_pin(AgentClient::Cursor, &dir, pin, None);
        match result {
            Ok((client, report)) => {
                eprintln!(
                    "  session={} applied={} apply_err={:?}",
                    client.session_id(),
                    report.applied_model,
                    report.model_apply_error
                );
                assert!(
                    !client.session_id().is_empty(),
                    "session/new must complete for pin {pin}"
                );
                drop(client);
            }
            Err(error) => {
                let _ = fs::remove_dir_all(&dir);
                panic!("live session/new failed for pin {pin}: {error}");
            }
        }
        let _ = fs::remove_dir_all(dir);
    }
}

//! Unit and fake-stdio integration tests for [`super::client`].

use super::client::{acp_args_for_program, AcpClientEvent, AcpStdioClient};
use super::sdk_connection::preferred_permission_config;
use super::{with_test_acp_extra_args, with_test_acp_program};
use agent_client_protocol::schema::{
    v1::{
        AgentCapabilities, ContentBlock, InitializeResponse, NewSessionRequest,
        SessionConfigOption, SessionConfigOptionCategory, SessionConfigSelectOption,
        SessionNotification, SessionUpdate,
    },
    ProtocolVersion,
};
use ajax_core::adapters::{cursor_catalog_to_acp_spawn_token, CURSOR_DEFAULT_MODEL};
use ajax_core::models::AgentClient;
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
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

fn session_update_text(params: &SessionNotification) -> Option<&str> {
    let chunk = match &params.update {
        SessionUpdate::UserMessageChunk(chunk)
        | SessionUpdate::AgentMessageChunk(chunk)
        | SessionUpdate::AgentThoughtChunk(chunk) => chunk,
        _ => return None,
    };
    match &chunk.content {
        ContentBlock::Text(text) => Some(text.text.as_str()),
        _ => None,
    }
}

#[test]
fn client_capabilities_do_not_claim_unimplemented_filesystem_or_terminal_support() {
    let capabilities = super::sdk_connection::client_capabilities();
    assert!(!capabilities.fs.read_text_file);
    assert!(!capabilities.fs.write_text_file);
    assert!(!capabilities.terminal);
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
    let advertised = InitializeResponse::new(ProtocolVersion::V1)
        .agent_capabilities(AgentCapabilities::new().load_session(true));
    assert!(advertised.agent_capabilities.load_session);
    assert!(
        !InitializeResponse::new(ProtocolVersion::V1)
            .agent_capabilities
            .load_session
    );
    assert!(
        !InitializeResponse::new(ProtocolVersion::V1)
            .agent_capabilities(AgentCapabilities::new().load_session(false))
            .agent_capabilities
            .load_session
    );
}

#[test]
fn trusted_permission_config_must_be_exact_and_advertised() {
    let options = vec![
        SessionConfigOption::select(
            "mode",
            "Mode",
            "default",
            vec![
                SessionConfigSelectOption::new("default", "Default"),
                SessionConfigSelectOption::new("agent-full-access", "Full Access"),
                SessionConfigSelectOption::new("bypassPermissions", "Bypass Permissions"),
            ],
        )
        .category(SessionConfigOptionCategory::Mode),
        SessionConfigOption::select(
            "effort",
            "Thinking",
            "high",
            vec![SessionConfigSelectOption::new(
                "agent-full-access",
                "Misleading value",
            )],
        )
        .category(SessionConfigOptionCategory::ThoughtLevel),
    ];

    assert_eq!(
        preferred_permission_config(AgentClient::Codex, Some(&options)),
        Some(("mode", "agent-full-access"))
    );
    assert_eq!(
        preferred_permission_config(AgentClient::Claude, Some(&options)),
        Some(("mode", "bypassPermissions"))
    );
    assert_eq!(
        preferred_permission_config(AgentClient::Pi, Some(&options)),
        None
    );
    assert_eq!(
        preferred_permission_config(AgentClient::Cursor, Some(&options)),
        None
    );
    assert_eq!(
        preferred_permission_config(
            AgentClient::Codex,
            Some(&[SessionConfigOption::select(
                "mode",
                "Mode",
                "default",
                vec![SessionConfigSelectOption::new("full-access", "Full Access",)],
            )]),
        ),
        None,
        "display names and similar IDs must not trigger a security mode"
    );
    assert_eq!(
        preferred_permission_config(AgentClient::Codex, Some(&options[1..])),
        None,
        "values advertised by non-mode config options must be ignored"
    );
    assert_eq!(preferred_permission_config(AgentClient::Codex, None), None);
}

/// Cursor validates `session/new` params and rejects a missing
/// `mcpServers` with an opaque JSON-RPC "Internal error", so the session
/// could never start. Keep the key present and an array.
#[test]
fn session_new_params_carry_mcp_servers_array() {
    let params = serde_json::to_value(NewSessionRequest::new("/repo/worktree")).unwrap();
    assert_eq!(
        params.get("cwd").and_then(Value::as_str),
        Some("/repo/worktree")
    );
    assert_eq!(
        params.get("mcpServers").and_then(Value::as_array),
        Some(&vec![])
    );
}

fn cursor_launch() -> ajax_core::adapters::AcpLaunch {
    ajax_core::adapters::acp_launch_for_agent(AgentClient::Cursor).expect("cursor acp launch")
}

#[test]
fn cursor_acp_command_prefers_agent_binary() {
    let candidates = cursor_launch().candidates;
    assert_eq!(candidates[0].0, "agent");
    assert_eq!(candidates[0].1, &["acp"][..]);
    assert_eq!(candidates[1].0, "cursor");
    assert_eq!(candidates[1].1, &["agent", "acp"][..]);
}

#[test]
fn cursor_acp_args_insert_model_before_acp() {
    let launch = cursor_launch();
    assert_eq!(
        acp_args_for_program(launch, &["acp"], Some("composer-2.5")),
        vec!["--model", "composer-2.5[fast=false]", "acp"]
    );
    assert_eq!(
        acp_args_for_program(launch, &["agent", "acp"], Some("gpt-5.6-sol-medium")),
        vec![
            "agent",
            "--model",
            "gpt-5.6-sol[effort=medium,fast=false]",
            "acp"
        ]
    );
    assert_eq!(
        acp_args_for_program(launch, &["acp"], Some("auto")),
        vec![
            "--model",
            cursor_catalog_to_acp_spawn_token(CURSOR_DEFAULT_MODEL).as_str(),
            "acp"
        ]
    );
    assert_eq!(
        acp_args_for_program(launch, &["acp"], None),
        vec![
            "--model",
            cursor_catalog_to_acp_spawn_token(CURSOR_DEFAULT_MODEL).as_str(),
            "acp"
        ]
    );
}

// The bridges take no `--model` on argv; a pinned model must not leak onto them.
#[test]
fn bridge_acp_args_never_carry_a_model_flag() {
    for agent in [AgentClient::Codex, AgentClient::Claude, AgentClient::Pi] {
        let launch = ajax_core::adapters::acp_launch_for_agent(agent).expect("bridge acp launch");
        assert!(
            !launch.model_pins_at_spawn(),
            "{agent:?} must not pin at spawn"
        );
        assert!(
            acp_args_for_program(launch, launch.candidates[0].1, Some("composer-2.5")).is_empty(),
            "{agent:?} argv must stay bare"
        );
    }
}

// Native first: a harness that grows its own `acp` subcommand must be used
// directly instead of its packaged adapter. Recorded per harness in core.
#[test]
fn every_bridge_harness_names_the_cli_that_could_speak_acp_natively() {
    use ajax_core::adapters::acp_launch_for_agent;

    assert_eq!(
        acp_launch_for_agent(AgentClient::Cursor)
            .expect("cursor")
            .native_program,
        None,
        "cursor's candidates are already its own binary"
    );
    for (agent, program) in [
        (AgentClient::Codex, "codex"),
        (AgentClient::Claude, "claude"),
        (AgentClient::Pi, "pi"),
    ] {
        assert_eq!(
            acp_launch_for_agent(agent).expect("bridge").native_program,
            Some(program),
            "{agent:?} should prefer its own CLI once it advertises acp"
        );
    }
}

// Each harness family takes its model a different way; the bridges would
// silently keep their own default if the client skipped the in-band call.
#[test]
fn spawn_selects_the_model_in_band_for_bridge_harnesses() {
    use ajax_core::adapters::{acp_launch_for_agent, AcpModelSelection};

    for agent in [AgentClient::Codex, AgentClient::Claude, AgentClient::Pi] {
        assert_eq!(
            acp_launch_for_agent(agent).expect("bridge").model_selection,
            AcpModelSelection::ConfigOption,
            "{agent:?} selects its model through a config option"
        );
    }

    let script = fake_acp_fixture();
    for agent in [AgentClient::Codex, AgentClient::Claude] {
        let dir = scratch_dir(&format!("model-in-band-{agent:?}"));
        with_test_acp_program(&script, || {
            let (_client, report) =
                AcpStdioClient::spawn(agent, &dir, Some("composer-2.5"), None).expect("spawn");
            assert_eq!(report.applied_model, "composer-2.5");
            assert!(
                report.model_apply_error.is_none(),
                "{agent:?} apply error: {:?}",
                report.model_apply_error
            );
        });
        let _ = fs::remove_dir_all(dir);
    }
}

// Regression for #952: Cursor must apply in-band when session/new advertises model.
#[test]
fn cursor_applies_in_band_when_model_is_advertised_issue_952() {
    let dir = scratch_dir("model-cursor-in-band-952");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let (client, report) =
            AcpStdioClient::spawn(AgentClient::Cursor, &dir, Some("composer-2.5"), None)
                .expect("spawn");
        assert_eq!(report.applied_model, "composer-2.5");
        assert!(report.model_apply_error.is_none());
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Some(AcpClientEvent::SessionUpdate(update)) =
                client.wait_event(Duration::from_millis(100))
            {
                let text = serde_json::to_string(&update).unwrap();
                if text.contains("model:session/set_config_option:composer-2.5") {
                    return;
                }
            }
        }
        panic!("cursor never applied its model in band when advertised");
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression for #954: Cursor spawn catalog ids must not be sent as handshake values.
#[test]
fn cursor_spawn_catalog_id_skips_in_band_when_not_advertised_issue_954() {
    let dir = scratch_dir("model-cursor-catalog-954");
    let script = fake_acp_fixture();
    let catalog_id = "cursor-grok-4.6-high";

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--cursor-models"], || {
            let (client, report) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, Some(catalog_id), None)
                    .expect("spawn");
            assert!(
                report.model_apply_error.is_none(),
                "catalog id in a different id space must not refuse: {:?}",
                report.model_apply_error
            );
            assert_eq!(report.applied_model, "grok-4.6[effort=high,fast=false]");
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                if let Some(AcpClientEvent::SessionUpdate(update)) =
                    client.wait_event(Duration::from_millis(100))
                {
                    let text = serde_json::to_string(&update).unwrap();
                    assert!(
                        !text.contains("model:session/set_config_option:cursor-grok-4.6-high"),
                        "must not send catalog id as handshake value: {text}"
                    );
                }
            }
        });
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression for #979: mapped spawn token must run Grok High, not Composer Fast.
#[test]
fn cursor_spawn_catalog_pin_runs_mapped_acp_model_issue_979() {
    let dir = scratch_dir("model-cursor-cli-default-979");
    let script = fake_acp_fixture();
    let catalog_id = CURSOR_DEFAULT_MODEL;
    let mapped = cursor_catalog_to_acp_spawn_token(catalog_id);

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--cli-default-model", "--cursor-models"], || {
            let (_client, report) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, Some(catalog_id), None)
                    .expect("spawn");
            assert!(
                report.model_apply_error.is_none(),
                "mapped spawn must run {mapped:?}, not Composer Fast: {:?}",
                report.model_apply_error
            );
            assert_eq!(report.applied_model, mapped);
            assert_ne!(report.applied_model, "composer-2.5[fast=true]");
        });
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression for #979: resume with wrong applied model must recover like fresh attach.
#[test]
fn cursor_spawn_recovers_after_resume_composer_fast_issue_979() {
    let dir = scratch_dir("model-cursor-recover-resume-979");
    let script = fake_acp_fixture();
    let catalog_id = "cursor-grok-4.6-high";
    let mapped = cursor_catalog_to_acp_spawn_token(catalog_id);

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(
            &[
                "--resume",
                "--cli-default-model",
                "--cursor-models",
                "--ignore-spawn-model-once",
                "--refuse-in-band-once",
            ],
            || {
                let (_client, report) = AcpStdioClient::spawn_with_operator_pin(
                    AgentClient::Cursor,
                    &dir,
                    catalog_id,
                    Some("fake-sess-1"),
                )
                .expect("spawn");
                assert!(
                    report.model_apply_error.is_none(),
                    "recovery must run {mapped:?}, not Composer Fast: {:?}",
                    report.model_apply_error
                );
                assert_eq!(report.applied_model, mapped);
                assert_ne!(report.applied_model, "composer-2.5[fast=true]");
                assert!(
                    !report.resumed,
                    "recovery must respawn with session/new, not resume/load"
                );
            },
        );
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression for #979: respawn once when spawn argv and in-band apply both leave CLI default.
#[test]
fn cursor_spawn_recovers_after_cli_default_and_refused_in_band_issue_979() {
    let dir = scratch_dir("model-cursor-recover-979");
    let script = fake_acp_fixture();
    let catalog_id = "cursor-grok-4.6-high";
    let mapped = cursor_catalog_to_acp_spawn_token(catalog_id);

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(
            &[
                "--cli-default-model",
                "--cursor-models",
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
                .expect("spawn");
                assert!(
                    report.model_apply_error.is_none(),
                    "recovery must run {mapped:?}, not Composer Fast: {:?}",
                    report.model_apply_error
                );
                assert_eq!(report.applied_model, mapped);
                assert_ne!(report.applied_model, "composer-2.5[fast=true]");
            },
        );
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression for #979: unspecified Cursor attach must not accept Composer Fast.
#[test]
fn cursor_unspecified_spawn_recovers_onto_mapped_default_issue_979() {
    let dir = scratch_dir("model-cursor-recover-default-979");
    let script = fake_acp_fixture();
    let mapped = cursor_catalog_to_acp_spawn_token(CURSOR_DEFAULT_MODEL);

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(
            &[
                "--cli-default-model",
                "--cursor-models",
                "--ignore-spawn-model-once",
                "--refuse-in-band-once",
            ],
            || {
                let (_client, report) = AcpStdioClient::spawn_with_operator_pin(
                    AgentClient::Cursor,
                    &dir,
                    "auto",
                    None,
                )
                .expect("spawn");
                assert!(
                    report.model_apply_error.is_none(),
                    "recovery must run {mapped:?}, not Composer Fast: {:?}",
                    report.model_apply_error
                );
                assert_eq!(report.applied_model, mapped);
                assert_ne!(report.applied_model, "composer-2.5[fast=true]");
            },
        );
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression for #979: unspecified Cursor attach must not accept Composer Fast.
#[test]
fn cursor_unspecified_spawn_runs_mapped_default_not_composer_fast_issue_979() {
    let dir = scratch_dir("model-cursor-unspecified-979");
    let script = fake_acp_fixture();
    let mapped = cursor_catalog_to_acp_spawn_token(CURSOR_DEFAULT_MODEL);

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--cli-default-model", "--cursor-models"], || {
            let (_client, report) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn");
            assert!(
                report.model_apply_error.is_none(),
                "unspecified spawn must run {mapped:?}, not Composer Fast: {:?}",
                report.model_apply_error
            );
            assert_eq!(report.applied_model, mapped);
            assert_ne!(report.applied_model, "composer-2.5[fast=true]");
        });
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression for #984: Sol High catalog pin must spawn mapped ACP token, not passthrough or Composer Fast.
#[test]
fn cursor_spawn_catalog_pin_runs_sol_high_mapped_acp_model_issue_984() {
    let dir = scratch_dir("model-cursor-sol-high-984");
    let script = fake_acp_fixture();
    let catalog_id = "gpt-5.6-sol-high";
    let mapped = cursor_catalog_to_acp_spawn_token(catalog_id);

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--cli-default-model", "--cursor-models"], || {
            let (_client, report) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, Some(catalog_id), None)
                    .expect("spawn");
            assert_ne!(mapped, catalog_id, "catalog id must map before spawn");
            assert_eq!(mapped, "gpt-5.6-sol[effort=high,fast=false]");
            assert!(
                report.model_apply_error.is_none(),
                "mapped spawn must run {mapped:?}, not Composer Fast: {:?}",
                report.model_apply_error
            );
            assert_eq!(report.applied_model, mapped);
            assert_ne!(report.applied_model, "composer-2.5[fast=true]");
        });
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression for #954: ConfigOption-only harnesses still refuse unadvertised pins.
#[test]
fn bridge_errors_when_pin_not_advertised_issue_954() {
    let dir = scratch_dir("model-bridge-not-advertised-954");
    let script = fake_acp_fixture();
    let not_advertised = "cursor-grok-4.6-high";

    with_test_acp_program(&script, || {
        let (_client, report) =
            AcpStdioClient::spawn(AgentClient::Codex, &dir, Some(not_advertised), None)
                .expect("spawn");
        assert!(
            report.model_apply_error.is_some(),
            "ConfigOption harness must error when pin is not advertised"
        );
        assert!(report
            .model_apply_error
            .as_deref()
            .is_some_and(|error| error.contains(not_advertised)));
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn fake_spawn_reports_load_session_advertised() {
    let dir = scratch_dir("spawn-advertised");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let (_client, report) =
            AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn fake acp");
        assert!(report.load_session_advertised);
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn fake_spawn_sends_no_nonstandard_initialized_notification() {
    let dir = scratch_dir("no-initialized-notification");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let (client, _) =
            AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn fake");
        let deadline = Instant::now() + Duration::from_millis(300);
        while Instant::now() < deadline {
            if let Some(AcpClientEvent::SessionUpdate(update)) =
                client.wait_event(Duration::from_millis(50))
            {
                assert!(
                    !session_update_text(&update)
                        .unwrap_or_default()
                        .starts_with("notification:"),
                    "unexpected ACP notification: {update:?}"
                );
            }
        }
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression for #880: ACP peers must agree on the initialize protocolVersion.
#[test]
fn fake_spawn_rejects_an_unsupported_protocol_version() {
    let dir = scratch_dir("unsupported-protocol");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--protocol-v2"], || {
            let error = match AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None) {
                Ok(_) => panic!("ACP v1 client must reject a v2 response"),
                Err(error) => error,
            };
            assert!(error.contains("protocol version"), "{error}");
        });
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn invalid_initialize_response_includes_the_agent_stderr_hint() {
    let dir = scratch_dir("invalid-initialize-stderr");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--bad-initialize"], || {
            let error = match AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None) {
                Ok(_) => panic!("invalid initialize response must fail"),
                Err(error) => error,
            };
            assert!(error.contains("agent login required"), "{error}");
        });
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression for #880: invalid stdout is a protocol error, not ignorable noise.
#[test]
fn fake_malformed_stdout_reports_an_error() {
    let dir = scratch_dir("malformed-stdout");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--malformed"], || {
            let (client, _) = AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None)
                .expect("spawn fake ACP");
            let deadline = Instant::now() + Duration::from_secs(2);
            while Instant::now() < deadline {
                if matches!(
                    client.wait_event(Duration::from_millis(100)),
                    Some(AcpClientEvent::Error(_))
                ) {
                    return;
                }
            }
            panic!("malformed ACP stdout was silently ignored");
        });
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn fake_begin_prompt_receives_pong_and_turn_end() {
    let dir = scratch_dir("prompt-pong");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let (mut client, _report) =
            AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn fake acp");
        client.begin_prompt("ping").expect("begin_prompt");
        pump_until_pong_or_prompt_finished(&client, Duration::from_secs(5));
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression for #880: the browser-facing approval remains a boolean, but the
// ACP peer must receive the selected standard permission option.
#[test]
fn fake_permission_request_returns_a_selected_acp_outcome() {
    let dir = scratch_dir("permission-selected");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--permission"], || {
            let (mut client, _) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn fake");
            client.begin_prompt("permission").expect("begin prompt");

            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                match client.wait_event(Duration::from_millis(100)) {
                    Some(AcpClientEvent::ClientRequest { id, method, params }) => {
                        assert_eq!(method, "session/request_permission");
                        assert_eq!(params.pointer("/toolCall/title"), Some(&json!("Run tests")));
                        assert_eq!(
                            params.pointer("/options/0/optionId"),
                            Some(&json!("allow-once"))
                        );
                        client
                            .respond_client_request(&id, json!({ "approved": true }))
                            .expect("approve permission");
                    }
                    Some(AcpClientEvent::SessionUpdate(update))
                        if session_update_text(&update)
                            == Some("permission:selected:allow-once") =>
                    {
                        return;
                    }
                    Some(_) | None => {}
                }
            }
            panic!("standard ACP permission response was not observed");
        });
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn fake_cancel_resolves_a_pending_permission_as_cancelled() {
    let dir = scratch_dir("permission-cancelled");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--permission"], || {
            let (mut client, _) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn fake");
            client.begin_prompt("permission").expect("begin prompt");

            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                match client.wait_event(Duration::from_millis(100)) {
                    Some(AcpClientEvent::ClientRequest { .. }) => {
                        client.cancel().expect("cancel");
                    }
                    Some(AcpClientEvent::SessionUpdate(update))
                        if session_update_text(&update) == Some("permission:cancelled:") =>
                    {
                        return;
                    }
                    Some(_) | None => {}
                }
            }
            panic!("pending ACP permission was not cancelled");
        });
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn fake_second_begin_prompt_while_in_flight_returns_err() {
    let dir = scratch_dir("prompt-in-flight");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let (mut client, _report) =
            AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn fake acp");
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
        let (client, first_report) =
            AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("first spawn");
        assert!(!first_report.resumed);
        let session_id = client.session_id().to_string();
        drop(client);

        let (client2, second_report) =
            AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, Some(&session_id))
                .expect("resume spawn");
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
fn failed_session_resume_falls_back_to_session_load() {
    let dir = scratch_dir("resume-falls-back-to-load");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let (client, _) =
            AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("first spawn");
        let session_id = client.session_id().to_string();
        drop(client);

        with_test_acp_extra_args(&["--resume-fail"], || {
            let (client, report) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, Some(&session_id))
                    .expect("restore spawn");
            assert!(report.resumed, "session/load should follow a failed resume");
            assert_eq!(client.session_id(), session_id);
            assert!(client.poll_event().is_none(), "load replay must be drained");
        });
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn fake_load_fail_falls_back_to_new_session() {
    let dir = scratch_dir("load-fail");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let (client, _first_report) =
            AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("first spawn");
        let resume_id = client.session_id().to_string();
        drop(client);

        with_test_acp_extra_args(&["--load-fail"], || {
            let (mut client2, report) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, Some(&resume_id))
                    .expect("spawn after load fail");
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
            AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn fake acp");
        assert!(!client.host_exited());
        let _pid = client.child_id();
        client.kill_host_for_test();
        assert!(client.host_exited());
    });

    let _ = fs::remove_dir_all(dir);
}

//! Unit and fake-stdio integration tests for [`super::client`].

use super::client::{AcpClientEvent, AcpStdioClient};
use super::sdk_connection::preferred_permission_config;
use super::{with_test_acp_extra_args, with_test_acp_program};
use agent_client_protocol::schema::{
    v1::{
        AgentCapabilities, ContentBlock, InitializeResponse, NewSessionRequest,
        RequestPermissionOutcome, RequestPermissionResponse, SelectedPermissionOutcome,
        SessionConfigOption, SessionConfigOptionCategory, SessionConfigSelectOption,
        SessionNotification, SessionUpdate, TextContent,
    },
    ProtocolVersion,
};
use ajax_core::models::AgentClient;
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn fake_acp_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_acp.js")
}

fn prompt_blocks(text: &str) -> Vec<ContentBlock> {
    vec![ContentBlock::Text(TextContent::new(text))]
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
fn client_capabilities_advertise_parameterized_model_picker_issue_979() {
    let capabilities = super::sdk_connection::client_capabilities();
    assert_eq!(
        capabilities
            .meta
            .as_ref()
            .and_then(|meta| meta.get("parameterizedModelPicker"))
            .and_then(|value| value.as_bool()),
        Some(true)
    );
}

#[test]
fn client_capabilities_do_not_claim_unimplemented_filesystem_or_terminal_support() {
    let capabilities = super::sdk_connection::client_capabilities();
    assert!(!capabilities.fs.read_text_file);
    assert!(!capabilities.fs.write_text_file);
    assert!(!capabilities.terminal);
}

#[test]
fn client_capabilities_advertise_form_elicitation_only() {
    let capabilities = super::sdk_connection::client_capabilities();
    let elicitation = capabilities
        .elicitation
        .as_ref()
        .expect("elicitation capabilities");
    assert!(elicitation.form.is_some());
    assert!(elicitation.url.is_none());
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

    for agent in [
        AgentClient::Codex,
        AgentClient::Claude,
        AgentClient::Pi,
        AgentClient::Cursor,
    ] {
        assert_eq!(
            preferred_permission_config(Some(&options)),
            Some(("mode".to_string(), "agent-full-access")),
            "harness {agent:?} should pick the first advertised full-access value"
        );
    }
    assert_eq!(
        preferred_permission_config(Some(&[SessionConfigOption::select(
            "mode",
            "Mode",
            "default",
            vec![SessionConfigSelectOption::new("full-access", "Full Access",)],
        )])),
        None,
        "display names and similar IDs must not trigger a security mode"
    );
    assert_eq!(
        preferred_permission_config(Some(&options[1..])),
        None,
        "values advertised by non-mode config options must be ignored"
    );
    assert_eq!(preferred_permission_config(None), None);
}

#[test]
fn trusted_permission_config_picks_first_advertised_full_access_value() {
    let cursor_options = vec![SessionConfigOption::select(
        "mode",
        "Mode",
        "default",
        vec![
            SessionConfigSelectOption::new("default", "Default"),
            SessionConfigSelectOption::new("agent", "Agent"),
        ],
    )
    .category(SessionConfigOptionCategory::Mode)];
    assert_eq!(
        preferred_permission_config(Some(&cursor_options)),
        Some(("mode".to_string(), "agent"))
    );

    let claude_only = vec![SessionConfigOption::select(
        "mode",
        "Mode",
        "default",
        vec![
            SessionConfigSelectOption::new("default", "Default"),
            SessionConfigSelectOption::new("bypassPermissions", "Bypass Permissions"),
        ],
    )
    .category(SessionConfigOptionCategory::Mode)];
    assert_eq!(
        preferred_permission_config(Some(&claude_only)),
        Some(("mode".to_string(), "bypassPermissions"))
    );

    let already_applied = vec![SessionConfigOption::select(
        "mode",
        "Mode",
        "agent-full-access",
        vec![
            SessionConfigSelectOption::new("default", "Default"),
            SessionConfigSelectOption::new("agent-full-access", "Full Access"),
        ],
    )
    .category(SessionConfigOptionCategory::Mode)];
    assert_eq!(preferred_permission_config(Some(&already_applied)), None);

    let code_only = vec![SessionConfigOption::select(
        "mode",
        "Mode",
        "default",
        vec![
            SessionConfigSelectOption::new("default", "Default"),
            SessionConfigSelectOption::new("code", "Code"),
        ],
    )
    .category(SessionConfigOptionCategory::Mode)];
    assert_eq!(
        preferred_permission_config(Some(&code_only)),
        Some(("mode".to_string(), "code"))
    );
}

// Regression: Cursor spawn must apply advertised full-access mode via official
// session/set_config_option {configId: mode, value: agent}, not session/set_mode.
#[test]
fn cursor_applies_full_access_mode_in_band_when_advertised() {
    let dir = scratch_dir("mode-cursor-agent");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--cursor-mode"], || {
            let (client, _report) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn");
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                if let Some(AcpClientEvent::SessionUpdate(update)) =
                    client.wait_event(Duration::from_millis(100))
                {
                    let text = serde_json::to_string(&update).unwrap();
                    if text.contains("model:session/set_config_option:mode:agent") {
                        return;
                    }
                }
            }
            panic!("cursor never applied full-access mode in band when advertised");
        });
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn request_permission_response_serializes_official_acp_outcome_shape() {
    let selected = RequestPermissionResponse::new(RequestPermissionOutcome::Selected(
        SelectedPermissionOutcome::new("allow-once"),
    ));
    assert_eq!(
        serde_json::to_value(selected).unwrap(),
        serde_json::json!({"outcome": {"outcome": "selected", "optionId": "allow-once"}})
    );

    let cancelled = RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled);
    assert_eq!(
        serde_json::to_value(cancelled).unwrap(),
        serde_json::json!({"outcome": {"outcome": "cancelled"}})
    );
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
        client
            .begin_prompt(&prompt_blocks("ping"))
            .expect("begin_prompt");
        pump_until_pong_or_prompt_finished(&client, Duration::from_secs(5));
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression for #880: trusted Ajax Chat auto-approves ACP permission requests on
// the host without surfacing an operator prompt.
#[test]
fn fake_permission_request_auto_selects_allow_once_without_operator_response() {
    let dir = scratch_dir("permission-auto-allow-once");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--permission"], || {
            let (mut client, _) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn fake");
            client
                .begin_prompt(&prompt_blocks("permission"))
                .expect("begin prompt");

            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                match client.wait_event(Duration::from_millis(100)) {
                    Some(AcpClientEvent::ClientRequest { method, .. }) => {
                        panic!("auto-answered permission must not surface: {method}");
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
fn fake_permission_request_prefers_allow_always_when_advertised() {
    let dir = scratch_dir("permission-auto-allow-always");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--permission-allow-always"], || {
            let (mut client, _) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn fake");
            client
                .begin_prompt(&prompt_blocks("permission"))
                .expect("begin prompt");

            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                match client.wait_event(Duration::from_millis(100)) {
                    Some(AcpClientEvent::ClientRequest { method, .. }) => {
                        panic!("auto-answered permission must not surface: {method}");
                    }
                    Some(AcpClientEvent::SessionUpdate(update))
                        if session_update_text(&update)
                            == Some("permission:selected:allow-always") =>
                    {
                        return;
                    }
                    Some(_) | None => {}
                }
            }
            panic!("allow-always ACP permission response was not observed");
        });
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn fake_permission_reject_only_options_cancel_without_inventing_allow_id() {
    let dir = scratch_dir("permission-reject-only");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--permission-reject-only"], || {
            let (mut client, _) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn fake");
            client
                .begin_prompt(&prompt_blocks("permission"))
                .expect("begin prompt");

            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                match client.wait_event(Duration::from_millis(100)) {
                    Some(AcpClientEvent::ClientRequest { method, .. }) => {
                        panic!("auto-answered permission must not surface: {method}");
                    }
                    Some(AcpClientEvent::SessionUpdate(update))
                        if session_update_text(&update) == Some("permission:cancelled:") =>
                    {
                        return;
                    }
                    Some(AcpClientEvent::SessionUpdate(update))
                        if session_update_text(&update)
                            .unwrap_or_default()
                            .starts_with("permission:selected:") =>
                    {
                        panic!("reject-only permission must not invent an allow id");
                    }
                    Some(_) | None => {}
                }
            }
            panic!("reject-only ACP permission was not cancelled");
        });
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn fake_cancel_ends_in_flight_turn_after_auto_approved_permission() {
    let dir = scratch_dir("permission-cancel-turn");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--permission", "--permission-hold"], || {
            let (mut client, _) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn fake");
            client
                .begin_prompt(&prompt_blocks("permission"))
                .expect("begin prompt");

            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                if let Some(AcpClientEvent::SessionUpdate(update)) =
                    client.wait_event(Duration::from_millis(100))
                {
                    if session_update_text(&update) == Some("permission:selected:allow-once") {
                        break;
                    }
                }
            }

            client.cancel().expect("cancel");

            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                if matches!(
                    client.wait_event(Duration::from_millis(100)),
                    Some(AcpClientEvent::RequestFinished {
                        method: "session/prompt",
                        ..
                    })
                ) {
                    return;
                }
            }
            panic!("cancel did not end the in-flight turn after auto-approved permission");
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
        client
            .begin_prompt(&prompt_blocks("first"))
            .expect("first begin_prompt");
        let err = client
            .begin_prompt(&prompt_blocks("second"))
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
            client2
                .begin_prompt(&prompt_blocks("after-fail"))
                .expect("begin_prompt");
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

fn wait_for_elicitation_request(client: &AcpStdioClient, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(AcpClientEvent::ElicitationRequest { request_id, .. }) = client.poll_event() {
            return request_id;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for elicitation request");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn pump_until_elicitation_update(client: &AcpStdioClient, needle: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(event) = client.poll_event() {
            match event {
                AcpClientEvent::ElicitationRequest { .. } if needle == "request" => return,
                AcpClientEvent::SessionUpdate(update)
                    if session_update_text(&update) == Some(needle) =>
                {
                    return;
                }
                _ => {}
            }
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for elicitation update: {needle}");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn fake_form_elicitation_surfaces_request_and_accepts_with_schema_content() {
    let dir = scratch_dir("elicitation-form-accept");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--elicitation-form"], || {
            let (mut client, _) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn fake");
            client
                .begin_prompt(&prompt_blocks("elicitation"))
                .expect("begin prompt");
            let request_id = wait_for_elicitation_request(&client, Duration::from_secs(5));

            client
                .respond_elicitation(
                    &request_id,
                    agent_client_protocol::schema::v1::ElicitationAction::Accept(
                        agent_client_protocol::schema::v1::ElicitationAcceptAction::new().content(
                            std::collections::BTreeMap::from([(
                                "target".to_string(),
                                agent_client_protocol::schema::v1::ElicitationContentValue::String(
                                    "staging".into(),
                                ),
                            )]),
                        ),
                    ),
                )
                .expect("accept elicitation");

            pump_until_elicitation_update(&client, "elicitation:accept", Duration::from_secs(5));
        });
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn fake_form_elicitation_decline_and_cancel_end_turn() {
    let dir = scratch_dir("elicitation-form-decline");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--elicitation-form"], || {
            let (mut client, _) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn fake");
            client
                .begin_prompt(&prompt_blocks("elicitation"))
                .expect("begin prompt");
            let request_id = wait_for_elicitation_request(&client, Duration::from_secs(5));

            client
                .respond_elicitation(
                    &request_id,
                    agent_client_protocol::schema::v1::ElicitationAction::Decline,
                )
                .expect("decline elicitation");
            pump_until_elicitation_update(&client, "elicitation:decline", Duration::from_secs(5));

            client
                .begin_prompt(&prompt_blocks("elicitation-2"))
                .expect("second prompt");
            let request_id = wait_for_elicitation_request(&client, Duration::from_secs(5));
            client
                .respond_elicitation(
                    &request_id,
                    agent_client_protocol::schema::v1::ElicitationAction::Cancel,
                )
                .expect("cancel elicitation");
            pump_until_elicitation_update(&client, "elicitation:cancel", Duration::from_secs(5));
        });
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn fake_url_elicitation_is_refused_without_advertising_url_mode() {
    let dir = scratch_dir("elicitation-url-refused");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--elicitation-url"], || {
            let (mut client, _) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn fake");
            client
                .begin_prompt(&prompt_blocks("elicitation-url"))
                .expect("begin prompt");
            pump_until_elicitation_update(
                &client,
                "elicitation:error:-32602",
                Duration::from_secs(5),
            );
        });
    });

    let _ = fs::remove_dir_all(dir);
}

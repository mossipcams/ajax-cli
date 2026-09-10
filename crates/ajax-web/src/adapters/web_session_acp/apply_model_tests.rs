//! Unit tests for [`super::apply_model`].

use super::apply_model::read_applied_model;
use super::is_unspecified_model;
use agent_client_protocol::schema::v1::{
    NewSessionResponse, SessionConfigOption, SessionConfigSelectOption,
};
use serde_json::json;

#[test]
fn read_applied_model_from_config_options_shape() {
    let options = vec![SessionConfigOption::select(
        "model",
        "Model",
        "composer-2.5",
        vec![SessionConfigSelectOption::new("composer-2.5", "Composer")],
    )];
    let result = json!({ "sessionId": "s1" });
    assert_eq!(read_applied_model(&result, Some(&options)), "composer-2.5");
}

#[test]
fn read_applied_model_falls_back_to_legacy_catalog_shape() {
    let result = json!({
        "sessionId": "s1",
        "models": {
            "currentModelId": "gpt-5.6-sol[medium]",
            "availableModels": [
                { "modelId": "gpt-5.6-sol[medium]", "name": "GPT-5.6-Sol (medium)" }
            ]
        }
    });
    assert_eq!(read_applied_model(&result, None), "gpt-5.6-sol[medium]");
}

#[test]
fn unspecified_model_sentinel() {
    assert!(is_unspecified_model(None));
    assert!(is_unspecified_model(Some("")));
    assert!(is_unspecified_model(Some("auto")));
    assert!(is_unspecified_model(Some("default")));
    assert!(!is_unspecified_model(Some("composer-2.5")));
}

#[test]
fn session_config_option_json_shape_for_fake_acp() {
    let opt = SessionConfigOption::select(
        "model",
        "Model",
        "harness-default",
        vec![SessionConfigSelectOption::new("harness-default", "Default")],
    );
    let resp = NewSessionResponse::new("fake-sess-1").config_options(vec![opt]);
    let json = serde_json::to_value(&resp).expect("json");
    assert_eq!(
        json.pointer("/configOptions/0/type")
            .and_then(|v| v.as_str()),
        Some("select")
    );
}

#[test]
fn operator_pin_satisfied_matches_thinking_bracket_issue_1013() {
    use super::apply_model::operator_pin_satisfied;

    assert!(operator_pin_satisfied(
        "claude-opus-5-thinking-medium",
        "claude-opus-5[thinking=true,effort=medium,fast=false]",
        true,
    ));
}

#[test]
fn applied_model_prefers_handshake_evidence_over_desired_pin_issue_952() {
    let options = vec![SessionConfigOption::select(
        "model",
        "Model",
        "harness-default",
        vec![
            SessionConfigSelectOption::new("harness-default", "Default"),
            SessionConfigSelectOption::new("composer-2.5", "Composer"),
        ],
    )];
    let handshake = json!({ "sessionId": "s1" });
    assert_eq!(
        read_applied_model(&handshake, Some(&options)),
        "harness-default"
    );
    assert_ne!(
        read_applied_model(&handshake, Some(&options)),
        "composer-2.5"
    );
}

fn with_model_dependent_session(
    pin: &str,
    resume: Option<&str>,
    check: impl FnOnce(super::SpawnReport, Vec<serde_json::Value>),
) {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
        time::SystemTime,
    };

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "ajax-model-dependent-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT_DIR.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&dir).unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("testdata/web_session/model_dependent_acp.cjs");
    let (client, report) = super::with_test_acp_program(&fixture, || {
        super::AcpStdioClient::spawn(
            ajax_core::models::AgentClient::Codex,
            &dir,
            Some(pin),
            resume,
        )
        .expect("fake ACP spawn")
    });
    let requests = fs::read_to_string(dir.join("model-dependent-requests.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    drop(client);
    fs::remove_dir_all(dir).unwrap();
    check(report, requests);
}

#[test]
fn model_dependent_controls_apply_after_base_on_new_resume_and_load_issue_1145() {
    for (resume, handshake) in [
        (None, "session/new"),
        (Some("existing"), "session/resume"),
        (Some("load-only"), "session/load"),
    ] {
        for effort in ["low", "high", "max"] {
            let pin = format!("gpt-5.6-sol|reasoning_effort={effort}|fast-mode=false");
            with_model_dependent_session(&pin, resume, |report, requests| {
                assert!(
                    report.model_apply_error.is_none(),
                    "{handshake}: {:?}",
                    report.model_apply_error
                );
                assert_eq!(report.applied_model, "gpt-5.6-sol");
                assert_eq!(report.resumed, resume.is_some());
                assert!(requests
                    .iter()
                    .any(|request| request["method"] == handshake));
                let applied: Vec<_> = requests
                    .iter()
                    .filter(|request| request["method"] == "session/set_config_option")
                    .map(|request| (&request["params"]["configId"], &request["params"]["value"]))
                    .collect();
                assert_eq!(
                    applied,
                    vec![
                        (&json!("model"), &json!("gpt-5.6-sol")),
                        (&json!("reasoning_effort"), &json!(effort)),
                        (&json!("fast-mode"), &json!(false)),
                    ]
                );
                assert_eq!(
                    super::applied_model_id_for_persist(report.config_options.as_deref().unwrap())
                        .unwrap(),
                    pin
                );
            });
        }
    }
}

#[test]
fn model_dependent_unsupported_setting_reports_confirmed_base_issue_1145() {
    for setting in ["reasoning_effort=unsupported", "unknown=high"] {
        let pin = format!("gpt-5.6-sol|{setting}");
        with_model_dependent_session(&pin, None, |report, requests| {
            assert!(report
                .model_apply_error
                .as_deref()
                .is_some_and(|error| error.contains("did not advertise")));
            assert_eq!(report.applied_model, "gpt-5.6-sol");
            assert_eq!(
                read_applied_model(&json!({}), report.config_options.as_deref()),
                "gpt-5.6-sol"
            );
            let applied: Vec<_> = requests
                .iter()
                .filter(|request| request["method"] == "session/set_config_option")
                .collect();
            assert_eq!(applied.len(), 1, "unsupported settings must never be sent");
            assert_eq!(applied[0]["params"]["configId"], "model");
        });
    }
}

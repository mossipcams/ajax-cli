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

//! Tests for applying the operator pin to a Pi RPC session at spawn and for
//! the Pi variant's `apply_model_pin` / `apply_config_option`, running the
//! fake Pi RPC fixture through [`SessionClient::spawn_with_operator_pin`].

use std::path::{Path, PathBuf};

use agent_client_protocol::schema::v1::{
    SessionConfigKind, SessionConfigOption, SessionConfigOptionValue,
};

use super::client::SpawnReport;
use super::session_client::{with_test_pi_program, SessionClient};
use ajax_core::models::AgentClient;

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_pi_rpc.js")
}

fn worktree_path() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn spawn_pi_with_pin(pin: &str) -> (SessionClient, SpawnReport) {
    SessionClient::spawn_with_operator_pin(AgentClient::Pi, worktree_path(), pin, None)
        .expect("fake pi rpc spawn must succeed")
}

fn thought_level_current(options: Option<&Vec<SessionConfigOption>>) -> Option<String> {
    let options = options?;
    options
        .iter()
        .find(|option| &*option.id.0 == "thought_level")
        .map(|option| match &option.kind {
            SessionConfigKind::Select(select) => select.current_value.0.to_string(),
            _ => "not-a-select".to_string(),
        })
}

#[test]
fn spawn_pin_applies_model_and_thought_level() {
    with_test_pi_program(&fixture_path(), &[], || {
        let (mut client, report) = spawn_pi_with_pin("fake-provider/fake-model|thought_level=high");

        assert!(report.model_apply_error.is_none());
        assert_eq!(report.applied_model, "fake-provider/fake-model");
        assert_eq!(
            thought_level_current(report.config_options.as_ref()),
            Some("high".to_string())
        );

        let _ = client.shutdown();
    });
}

#[test]
fn spawn_pin_model_error_never_fails_spawn() {
    with_test_pi_program(&fixture_path(), &[], || {
        let (mut client, report) = spawn_pi_with_pin("fake-provider/does-not-exist");

        let error = report
            .model_apply_error
            .as_deref()
            .expect("the model failure must be reported");
        assert!(
            error.contains("Model not found"),
            "expected a Pi model error, got {error}"
        );
        assert_eq!(
            report.applied_model, "fake-provider/fake-model",
            "the applied model must stay unchanged after a failed pin"
        );

        let _ = client.shutdown();
    });
}

#[test]
fn spawn_pin_model_error_keeps_later_options_applying() {
    with_test_pi_program(&fixture_path(), &[], || {
        let (mut client, report) =
            spawn_pi_with_pin("fake-provider/does-not-exist|thought_level=low");

        let error = report
            .model_apply_error
            .as_deref()
            .expect("the model failure must be reported");
        assert!(
            error.contains("Model not found"),
            "expected a Pi model error, got {error}"
        );
        assert_eq!(
            thought_level_current(report.config_options.as_ref()),
            Some("low".to_string()),
            "the later thought_level step must still apply"
        );

        let _ = client.shutdown();
    });
}

#[test]
fn spawn_unspecified_pin_applies_nothing() {
    with_test_pi_program(&fixture_path(), &[], || {
        for pin in ["default", ""] {
            let (mut client, report) = spawn_pi_with_pin(pin);

            assert!(
                report.model_apply_error.is_none(),
                "unspecified pin {pin:?} must not error"
            );
            assert_eq!(report.applied_model, "fake-provider/fake-model");
            assert_eq!(
                thought_level_current(report.config_options.as_ref()),
                Some("medium".to_string()),
                "unspecified pin {pin:?} must leave thinking untouched"
            );

            let _ = client.shutdown();
        }
    });
}

#[test]
fn spawn_pin_ignores_unknown_option_ids() {
    with_test_pi_program(&fixture_path(), &[], || {
        let (mut client, report) = spawn_pi_with_pin("fake-provider/fake-model|vocabulary=large");

        assert!(report.model_apply_error.is_none());
        assert_eq!(
            thought_level_current(report.config_options.as_ref()),
            Some("medium".to_string()),
            "unknown option ids must be ignored without error"
        );

        let _ = client.shutdown();
    });
}

#[test]
fn apply_config_option_updates_thought_level() {
    with_test_pi_program(&fixture_path(), &[], || {
        let (mut client, _) = spawn_pi_with_pin("");

        let outcome = client
            .apply_config_option("thought_level", SessionConfigOptionValue::value_id("low"))
            .expect("apply_config_option should report rather than fail");
        assert!(outcome.error.is_none());
        assert_eq!(
            thought_level_current(outcome.config_options.as_ref()),
            Some("low".to_string())
        );

        let _ = client.shutdown();
    });
}

#[test]
fn apply_config_option_unknown_id_reports_error() {
    with_test_pi_program(&fixture_path(), &[], || {
        let (mut client, _) = spawn_pi_with_pin("");

        let outcome = client
            .apply_config_option("nope", SessionConfigOptionValue::value_id("x"))
            .expect("apply_config_option should report rather than fail");
        let error = outcome
            .error
            .as_deref()
            .expect("unknown option ids must be reported");
        assert!(
            error.contains("unsupported Pi option nope"),
            "unexpected error: {error}"
        );

        let _ = client.shutdown();
    });
}

#[test]
fn apply_config_option_model_boolean_reports_error() {
    with_test_pi_program(&fixture_path(), &[], || {
        let (mut client, _) = spawn_pi_with_pin("");

        let outcome = client
            .apply_config_option("model", SessionConfigOptionValue::Boolean { value: true })
            .expect("apply_config_option should report rather than fail");
        assert!(
            outcome.error.is_some(),
            "a Boolean model value must be reported as unsupported"
        );

        let _ = client.shutdown();
    });
}

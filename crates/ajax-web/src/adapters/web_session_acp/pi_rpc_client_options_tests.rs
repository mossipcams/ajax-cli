//! Tests for the Pi model / thinking-level config options exposed by
//! [`super::pi_rpc_client::PiRpcClient`], running the fake Pi RPC fixture as a
//! directly-executable child (the file has a node shebang) and asserting the
//! options' shape plus the `set_model` / `set_thinking_level` round trips.

use std::path::{Path, PathBuf};
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory, SessionConfigSelectOption,
    SessionConfigSelectOptions,
};

use super::pi_rpc_client::PiRpcClient;

/// Generous per-request bound: node startup plus one scripted record burst.
const TIMEOUT: Duration = Duration::from_secs(10);

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_pi_rpc.js")
}

fn spawn_client() -> PiRpcClient {
    let cwd = Path::new(env!("CARGO_MANIFEST_DIR"));
    PiRpcClient::spawn(&fixture_path(), &[], cwd, None, TIMEOUT)
        .expect("fake pi rpc fixture must spawn (is node installed?)")
}

/// Owned snapshot of one select option, so callers avoid lifetimes into the
/// borrowed `SessionConfigOption`.
struct SelectView {
    current_value: String,
    values: Vec<String>,
    names: Vec<String>,
    category: Option<SessionConfigOptionCategory>,
}

fn select_view(options: &[SessionConfigOption], id: &str) -> SelectView {
    let option = options
        .iter()
        .find(|option| option.id.0.as_ref() == id)
        .unwrap_or_else(|| panic!("config option {id} must be advertised"));
    let SessionConfigKind::Select(select) = &option.kind else {
        panic!("config option {id} must be a select");
    };
    let entries: &[SessionConfigSelectOption] = match &select.options {
        SessionConfigSelectOptions::Ungrouped(entries) => entries,
        _ => panic!("config option {id} must be ungrouped"),
    };
    SelectView {
        current_value: select.current_value.0.to_string(),
        values: entries.iter().map(|e| e.value.0.to_string()).collect(),
        names: entries.iter().map(|e| e.name.clone()).collect(),
        category: option.category.clone(),
    }
}

#[test]
fn config_options_advertise_model_and_thinking_level() {
    let client = spawn_client();
    let options = client.config_options();

    assert_eq!(options.len(), 2);

    let model = select_view(&options, "model");
    assert!(matches!(
        model.category,
        Some(SessionConfigOptionCategory::Model)
    ));
    assert_eq!(model.current_value, "fake-provider/fake-model");
    assert_eq!(model.values.len(), 2);
    assert!(model
        .values
        .contains(&"fake-provider/fake-model".to_string()));
    assert!(model
        .values
        .contains(&"fake-provider/fake-nonreasoning".to_string()));

    let thinking = select_view(&options, "thought_level");
    assert!(matches!(
        thinking.category,
        Some(SessionConfigOptionCategory::ThoughtLevel)
    ));
    assert_eq!(thinking.current_value, "medium");
    assert_eq!(thinking.values, ["off", "low", "medium", "high"]);
    assert_eq!(thinking.names, ["off", "low", "medium", "high"]);
}

#[test]
fn applied_model_defaults_to_handshake_model() {
    let client = spawn_client();
    assert_eq!(client.applied_model(), "fake-provider/fake-model");
}

#[test]
fn set_model_updates_applied_model_and_current_value() {
    let client = spawn_client();
    client
        .set_model("fake-provider/fake-nonreasoning")
        .expect("set_model must succeed for a catalogued model");
    assert_eq!(client.applied_model(), "fake-provider/fake-nonreasoning");

    let options = client.config_options();
    let model = select_view(&options, "model");
    assert_eq!(model.current_value, "fake-provider/fake-nonreasoning");
}

#[test]
fn set_model_failure_passes_error_and_keeps_state() {
    let client = spawn_client();
    let err = client
        .set_model("fake-provider/does-not-exist")
        .expect_err("set_model must fail for an unknown model");
    assert!(err.contains("Model not found"), "unexpected error: {err}");
    assert_eq!(client.applied_model(), "fake-provider/fake-model");

    let options = client.config_options();
    let model = select_view(&options, "model");
    assert_eq!(model.current_value, "fake-provider/fake-model");
}

#[test]
fn set_model_without_separator_is_rejected() {
    let client = spawn_client();
    let err = client
        .set_model("noslash")
        .expect_err("set_model must fail without a provider/id value");
    assert_eq!(err, "model must be provider/id");
    assert_eq!(client.applied_model(), "fake-provider/fake-model");
}

#[test]
fn set_thinking_level_updates_current_value() {
    let client = spawn_client();
    client
        .set_thinking_level("high")
        .expect("set_thinking_level must succeed for a listed level");

    let options = client.config_options();
    let thinking = select_view(&options, "thought_level");
    assert_eq!(thinking.current_value, "high");
}

#[test]
fn set_model_and_thinking_level_fail_after_shutdown() {
    let client = spawn_client();
    client.shutdown();
    assert!(client.set_model("fake-provider/fake-model").is_err());
    assert!(client.set_thinking_level("high").is_err());
}

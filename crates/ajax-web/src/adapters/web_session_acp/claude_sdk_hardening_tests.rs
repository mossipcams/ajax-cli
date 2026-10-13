//! Hardening regression tests for the Claude SDK transport (C4d): model
//! values are validated against the sidecar's advertised catalog before being
//! forwarded, and the sidecar no longer loads an arbitrary SDK module from an
//! environment variable in production.

use std::path::{Path, PathBuf};
use std::time::Duration;

use agent_client_protocol::schema::v1::{SessionConfigKind, SessionConfigSelectOption};

use super::claude_sdk_client::ClaudeSdkClient;

/// Generous per-step bound: node startup plus one scripted record burst.
const TIMEOUT: Duration = Duration::from_secs(10);

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// `(sidecar script, fake sdk module)` paths under the crate.
fn sidecar_and_fake_sdk() -> (PathBuf, PathBuf) {
    let dir = manifest_dir();
    (
        dir.join("sidecar/claude_sdk_sidecar.mjs"),
        dir.join("tests/fixtures/fake_claude_sdk.mjs"),
    )
}

/// Spawn the real sidecar script (`node <sidecar> --sdk-module <fake>`) wired
/// to the fake Claude Agent SDK. Fails loudly when spawn fails.
fn spawn_client() -> ClaudeSdkClient {
    let (sidecar, fake_sdk) = sidecar_and_fake_sdk();
    let args = vec![
        sidecar.to_string_lossy().into_owned(),
        "--sdk-module".to_string(),
        fake_sdk.to_string_lossy().into_owned(),
    ];
    ClaudeSdkClient::spawn(
        Path::new("node"),
        &args,
        manifest_dir(),
        None,
        None,
        TIMEOUT,
    )
    .expect("claude sdk sidecar must spawn against the fake fixture (is node installed?)")
}

/// Current value of the `model` config option.
fn model_option_value(
    options: &[agent_client_protocol::schema::v1::SessionConfigOption],
) -> String {
    let option = options
        .iter()
        .find(|option| option.id.0 == "model".into())
        .expect("the fake SDK advertises a model config option");
    match &option.kind {
        SessionConfigKind::Select(select) => select.current_value.0.to_string(),
        other => panic!("model option should be a select, got {other:?}"),
    }
}

/// The advertised `model` entries of the client's config options.
fn model_option_entries(
    options: &[agent_client_protocol::schema::v1::SessionConfigOption],
) -> Vec<SessionConfigSelectOption> {
    let option = options
        .iter()
        .find(|option| option.id.0 == "model".into())
        .expect("the fake SDK advertises a model config option");
    match &option.kind {
        SessionConfigKind::Select(select) => match &select.options {
            agent_client_protocol::schema::v1::SessionConfigSelectOptions::Ungrouped(entries) => {
                entries.clone()
            }
            other => panic!("model option should be ungrouped, got {other:?}"),
        },
        other => panic!("model option should be a select, got {other:?}"),
    }
}

#[test]
fn set_model_rejects_value_the_fake_sdk_did_not_advertise() {
    let client = spawn_client();
    let before_applied = client.applied_model();
    assert_eq!(before_applied, "default");
    let before_option = model_option_value(&client.config_options());

    let err = client
        .set_model("not-a-model")
        .expect_err("unknown model values must be rejected locally");

    assert!(
        err.contains("unknown model"),
        "expected an 'unknown model' error, got: {err}"
    );
    assert_eq!(client.applied_model(), before_applied);
    assert_eq!(model_option_value(&client.config_options()), before_option);
}

#[test]
fn set_model_accepts_a_value_advertised_by_the_fake_sdk() {
    let client = spawn_client();
    // "haiku" must be advertised by the fake fixture for this test to mean anything.
    let entries: Vec<String> = model_option_entries(&client.config_options())
        .iter()
        .map(|entry| entry.value.0.to_string())
        .collect();
    assert!(
        entries.contains(&"haiku".to_string()),
        "fake SDK must advertise 'haiku': {entries:?}"
    );

    client
        .set_model("haiku")
        .expect("an advertised model value must pass validation and the round trip");

    assert_eq!(client.applied_model(), "haiku");
}

#[test]
fn set_model_accepts_the_default_value() {
    let client = spawn_client();
    client
        .set_model("default")
        .expect("the fake SDK's default model value must pass validation");
    assert_eq!(client.applied_model(), "default");
}

const SIDECAR_SOURCE: &str = include_str!("../../../sidecar/claude_sdk_sidecar.mjs");

#[test]
fn sidecar_has_no_environment_sdk_module_route() {
    // Built with concat! so the marker never appears verbatim in this test file.
    let marker = concat!("AJAX_", "CLAUDE_", "SDK_", "MODULE");
    assert!(
        !SIDECAR_SOURCE.contains(marker),
        "the sidecar must not load the SDK module from an environment variable; \
         only the --sdk-module argv route (plus package/npm locations) is allowed"
    );
}

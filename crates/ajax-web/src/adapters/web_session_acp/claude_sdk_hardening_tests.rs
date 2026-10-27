use std::path::{Path, PathBuf};
use std::time::Duration;

use super::claude_sdk_client::ClaudeSdkClient;

const TIMEOUT: Duration = Duration::from_secs(10);

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn sidecar_and_fake_sdk() -> (PathBuf, PathBuf) {
    let dir = manifest_dir();
    (
        dir.join("sidecar/claude_sdk_sidecar.mjs"),
        dir.join("tests/fixtures/fake_claude_sdk.mjs"),
    )
}

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

fn model_option_value(options: &[super::NeutralConfigOption]) -> String {
    let option = options
        .iter()
        .find(|option| option.id == "model")
        .expect("the fake SDK advertises a model config option");
    match &option.kind {
        super::NeutralConfigKind::Select { current_value, .. } => current_value.clone(),
        other => panic!("model option should be a select, got {other:?}"),
    }
}

fn model_option_entries(options: &[super::NeutralConfigOption]) -> Vec<super::NeutralConfigChoice> {
    let option = options
        .iter()
        .find(|option| option.id == "model")
        .expect("the fake SDK advertises a model config option");
    match &option.kind {
        super::NeutralConfigKind::Select { choices, .. } => choices.clone(),
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
    let entries: Vec<String> = model_option_entries(&client.config_options())
        .iter()
        .map(|entry| entry.value.clone())
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
    let marker = concat!("AJAX_", "CLAUDE_", "SDK_", "MODULE");
    assert!(
        !SIDECAR_SOURCE.contains(marker),
        "the sidecar must not load the SDK module from an environment variable; \
         only the --sdk-module argv route (plus package/npm locations) is allowed"
    );
}

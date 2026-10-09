//! Tests for [`super::claude_sdk_spawn`]: the Claude Agent SDK spawn path,
//! fail-closed restore, operator-pin application and the spawn report, running
//! the real sidecar against the fake Claude Agent SDK fixture.

use std::path::Path;

use agent_client_protocol::schema::v1::{SessionConfigKind, SessionConfigOption};
use ajax_core::models::AgentClient;

use super::claude_sdk_spawn::{spawn_claude_sdk, uses_claude_sdk, with_test_claude_sdk_module};
use super::client::{AcpSpawnError, RestoreFailure};

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// The fake Claude Agent SDK fixture path under the crate.
fn fake_sdk() -> std::path::PathBuf {
    manifest_dir().join("tests/fixtures/fake_claude_sdk.mjs")
}

/// Run `f` with the fake SDK module override installed and the crate as cwd.
fn with_fake_sdk<R>(f: impl FnOnce(&Path) -> R) -> R {
    let dir = manifest_dir();
    with_test_claude_sdk_module(&fake_sdk(), || f(dir))
}

#[test]
fn missing_sdk_error_names_the_install_command() {
    let dir = manifest_dir();
    let missing = dir.join("tests/fixtures/does-not-exist.mjs");
    let result = with_test_claude_sdk_module(&missing, || spawn_claude_sdk(dir, "default", None));
    match result {
        Err(AcpSpawnError::Message(text)) => {
            assert!(text.contains("no loadable claude-agent-sdk module"));
            assert!(text.contains("npm install -g @anthropic-ai/claude-agent-sdk"));
        }
        Ok(_) => panic!("expected a spawn error for the missing SDK module"),
        Err(_) => panic!("expected a message spawn error for the missing SDK module"),
    }
}

/// The current value of one select config option, panicking when it is absent.
fn current_value(options: &[SessionConfigOption], id: &str) -> String {
    let option = options
        .iter()
        .find(|option| option.id.0.as_ref() == id)
        .unwrap_or_else(|| panic!("config option {id} must be advertised"));
    let SessionConfigKind::Select(select) = &option.kind else {
        panic!("config option {id} must be a select");
    };
    select.current_value.0.to_string()
}

#[test]
fn spawn_returns_client_and_report() {
    with_fake_sdk(|cwd| {
        let (client, report) = spawn_claude_sdk(cwd, "", None).expect("spawn must succeed");

        assert!(!client.session_id().is_empty());
        assert_eq!(report.applied_model, "default");
        assert!(report.restore_advertised);
        assert!(!report.close_advertised);
        assert!(!report.resumed);
        assert!(!report.prompt_capabilities.image);
        assert!(!report.prompt_capabilities.embedded_context);

        let options = report
            .config_options
            .as_ref()
            .expect("config options must be advertised");
        let ids: Vec<&str> = options.iter().map(|option| option.id.0.as_ref()).collect();
        assert!(ids.contains(&"model"), "model option id: {ids:?}");
        assert!(ids.contains(&"effort"), "effort option id: {ids:?}");
    });
}

#[test]
fn resume_existing_session_reports_resumed() {
    with_fake_sdk(|cwd| {
        let (client, report) = spawn_claude_sdk(cwd, "", Some("r-1")).expect("resume must succeed");

        assert!(report.resumed);
        assert_eq!(client.session_id(), "r-1");
    });
}

#[test]
fn resume_missing_session_fails_closed() {
    with_fake_sdk(|cwd| match spawn_claude_sdk(cwd, "", Some("missing-1")) {
        Ok(_) => panic!("resuming a nonexistent session must fail"),
        Err(AcpSpawnError::Restore(RestoreFailure::Rejected { session_id, .. })) => {
            assert_eq!(session_id, "missing-1");
        }
        Err(other) => panic!("expected a restore failure, got: {other:?}"),
    });
}

#[test]
fn pin_applies_model_and_effort() {
    with_fake_sdk(|cwd| {
        let (client, report) =
            spawn_claude_sdk(cwd, "haiku|effort=high", None).expect("spawn must succeed");

        assert_eq!(report.applied_model, "haiku");
        assert_eq!(client.applied_model(), "haiku");
        assert!(report.model_apply_error.is_none());
        let options = report.config_options.expect("config options advertised");
        assert_eq!(current_value(&options, "effort"), "high");
    });
}

#[test]
fn pin_unknown_model_reports_error_but_effort_still_applies() {
    with_fake_sdk(|cwd| {
        let (_client, report) = spawn_claude_sdk(cwd, "nope|effort=low", None)
            .expect("a pin error must not fail the spawn");

        let err = report
            .model_apply_error
            .expect("unknown model must be reported");
        assert!(err.contains("unknown model"), "error text: {err}");
        assert_eq!(report.applied_model, "default");
        let options = report.config_options.expect("config options advertised");
        assert_eq!(current_value(&options, "effort"), "low");
    });
}

#[test]
fn pin_default_or_empty_applies_nothing() {
    with_fake_sdk(|cwd| {
        for pin in ["default", ""] {
            let (_client, report) = spawn_claude_sdk(cwd, pin, None).expect("spawn must succeed");

            assert!(
                report.model_apply_error.is_none(),
                "pin {pin:?} must not report an error"
            );
        }
    });
}

#[test]
fn pin_unknown_option_id_is_ignored() {
    with_fake_sdk(|cwd| {
        let (client, report) =
            spawn_claude_sdk(cwd, "haiku|thought_level=high", None).expect("spawn must succeed");

        assert_eq!(report.applied_model, "haiku");
        assert_eq!(client.applied_model(), "haiku");
        assert!(report.model_apply_error.is_none());
        let options = report.config_options.expect("config options advertised");
        assert_eq!(current_value(&options, "effort"), "medium");
    });
}

#[test]
fn uses_claude_sdk_gating() {
    // Outside the override no agent takes the SDK path.
    assert!(!uses_claude_sdk(AgentClient::Claude));

    with_fake_sdk(|_cwd| {
        assert!(uses_claude_sdk(AgentClient::Claude));
        assert!(!uses_claude_sdk(AgentClient::Cursor));
        assert!(!uses_claude_sdk(AgentClient::Codex));
        assert!(!uses_claude_sdk(AgentClient::Pi));
    });
}

#[test]
fn sdk_module_prefers_the_direct_package_under_a_npm_root() {
    let root = manifest_dir().join("tests/fixtures/npm_roots/direct");
    let module = super::claude_sdk_spawn::sdk_module_from_npm_root(&root)
        .expect("direct fixture must resolve");
    assert_eq!(
        module,
        root.join("@anthropic-ai")
            .join("claude-agent-sdk")
            .join("sdk.mjs")
    );
}

#[test]
fn sdk_module_falls_back_to_the_acp_nested_package() {
    let root = manifest_dir().join("tests/fixtures/npm_roots/nested_only");
    let module = super::claude_sdk_spawn::sdk_module_from_npm_root(&root)
        .expect("nested fixture must resolve");
    assert_eq!(
        module,
        root.join("@agentclientprotocol")
            .join("claude-agent-acp")
            .join("node_modules")
            .join("@anthropic-ai")
            .join("claude-agent-sdk")
            .join("sdk.mjs")
    );
}

#[test]
fn sdk_module_prefers_direct_over_nested() {
    let root = manifest_dir().join("tests/fixtures/npm_roots/both");
    let module = super::claude_sdk_spawn::sdk_module_from_npm_root(&root)
        .expect("both fixture must resolve");
    assert_eq!(
        module,
        root.join("@anthropic-ai")
            .join("claude-agent-sdk")
            .join("sdk.mjs")
    );
}

#[test]
fn sdk_module_is_none_when_nothing_exists() {
    let root = manifest_dir().join("tests/fixtures/npm_roots/empty");
    assert!(super::claude_sdk_spawn::sdk_module_from_npm_root(&root).is_none());
}

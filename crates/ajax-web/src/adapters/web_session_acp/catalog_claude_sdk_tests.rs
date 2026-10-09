//! Tests for the Claude Agent SDK model-catalog path of [`super::catalog`],
//! running the fake Claude SDK fixture and asserting that the Claude model
//! catalog comes from the SDK handshake config options.

use std::path::{Path, PathBuf};

use super::catalog::read_agent_model_catalog;
use super::claude_sdk_spawn::{uses_claude_sdk, with_test_claude_sdk_module};
use ajax_core::models::AgentClient;

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_claude_sdk.mjs")
}

fn worktree_path() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn claude_sdk_catalog_reads_handshake_options() {
    with_test_claude_sdk_module(&fixture_path(), || {
        let catalog = read_agent_model_catalog(AgentClient::Claude, worktree_path());

        assert!(catalog
            .models
            .contains(&("default".to_string(), "Default".to_string())));
        assert!(catalog
            .models
            .contains(&("haiku".to_string(), "Haiku".to_string())));
        assert_eq!(catalog.default_model.as_deref(), Some("default"));
        let reasoning = catalog
            .reasoning
            .as_ref()
            .unwrap_or_else(|| panic!("expected a reasoning group from the handshake"));
        assert_eq!(reasoning.id, "effort");
        assert_eq!(reasoning.options.len(), 5);
        assert_eq!(
            reasoning
                .options
                .iter()
                .map(|(value, _)| value.as_str())
                .collect::<Vec<_>>(),
            vec!["low", "medium", "high", "xhigh", "max"]
        );
        assert_eq!(reasoning.current.as_deref(), Some("medium"));
    });
}

#[test]
fn claude_sdk_catalog_with_missing_module_is_empty() {
    let missing = worktree_path().join("tests/fixtures/definitely_not_a_claude_sdk.mjs");
    with_test_claude_sdk_module(&missing, || {
        let catalog = read_agent_model_catalog(AgentClient::Claude, worktree_path());

        assert!(catalog.models.is_empty());
        assert!(catalog.default_model.is_none());
        assert!(catalog.reasoning.is_none());
    });
}

#[test]
fn production_claude_does_not_take_the_sdk_path() {
    assert!(!uses_claude_sdk(AgentClient::Claude));
}

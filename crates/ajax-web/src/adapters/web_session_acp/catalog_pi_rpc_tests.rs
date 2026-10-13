//! Tests for the Pi RPC model-catalog path of [`super::catalog`], running the
//! fake Pi RPC fixture and asserting that the Pi model catalog comes from the
//! Pi RPC handshake config options instead of an ACP session/new.

use std::path::{Path, PathBuf};

use super::catalog::read_agent_model_catalog;
use super::session_client::{uses_pi_rpc, with_test_pi_program};
use ajax_core::models::AgentClient;

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_pi_rpc.js")
}

fn worktree_path() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn pi_rpc_catalog_reads_handshake_options() {
    with_test_pi_program(&fixture_path(), &[], || {
        let catalog = read_agent_model_catalog(AgentClient::Pi, worktree_path());

        assert!(catalog.models.contains(&(
            "fake-provider/fake-model".to_string(),
            "Fake Model".to_string(),
        )));
        assert_eq!(
            catalog.default_model.as_deref(),
            Some("fake-provider/fake-model")
        );
        let reasoning = catalog
            .reasoning
            .as_ref()
            .unwrap_or_else(|| panic!("expected a reasoning group from the handshake"));
        assert_eq!(reasoning.id, "thought_level");
        assert_eq!(reasoning.options.len(), 4);
        assert_eq!(reasoning.current.as_deref(), Some("medium"));
    });
}

#[test]
fn pi_rpc_catalog_missing_program_is_empty() {
    with_test_pi_program(Path::new("/nonexistent/fake-pi-rpc"), &[], || {
        let catalog = read_agent_model_catalog(AgentClient::Pi, worktree_path());

        assert!(catalog.models.is_empty());
        assert!(catalog.default_model.is_none());
        assert!(catalog.reasoning.is_none());
    });
}

#[test]
fn non_pi_agents_stay_on_the_acp_path() {
    with_test_pi_program(&fixture_path(), &[], || {
        assert!(uses_pi_rpc(AgentClient::Pi));
        assert!(!uses_pi_rpc(AgentClient::Cursor));
        assert!(!uses_pi_rpc(AgentClient::Codex));
        assert!(!uses_pi_rpc(AgentClient::Claude));
    });
}

#[test]
fn native_catalog_matches_the_acp_parser() {
    let neutral = vec![
        super::NeutralConfigOption {
            id: "model".to_string(),
            name: "Model".to_string(),
            description: None,
            category: Some(super::NeutralConfigCategory::Model),
            kind: super::NeutralConfigKind::Select {
                current_value: "p/a".to_string(),
                choices: vec![
                    super::NeutralConfigChoice {
                        value: "p/a".to_string(),
                        name: "A".to_string(),
                        description: None,
                    },
                    super::NeutralConfigChoice {
                        value: "p/b".to_string(),
                        name: "B".to_string(),
                        description: None,
                    },
                ],
            },
        },
        super::NeutralConfigOption {
            id: "thought_level".to_string(),
            name: "Thinking".to_string(),
            description: None,
            category: Some(super::NeutralConfigCategory::ThoughtLevel),
            kind: super::NeutralConfigKind::Select {
                current_value: "high".to_string(),
                choices: vec![
                    super::NeutralConfigChoice {
                        value: "low".to_string(),
                        name: "Low".to_string(),
                        description: None,
                    },
                    super::NeutralConfigChoice {
                        value: "high".to_string(),
                        name: "High".to_string(),
                        description: None,
                    },
                ],
            },
        },
    ];

    assert_eq!(
        super::catalog::catalog_from_neutral(&neutral),
        super::catalog::parse_session_new_catalog(&serde_json::json!({
            "configOptions": serde_json::to_value(super::acp_options(&neutral)).unwrap()
        }))
    );
    let catalog = super::catalog::catalog_from_neutral(&neutral);
    assert_eq!(
        catalog.models,
        vec![
            ("p/a".to_string(), "A".to_string()),
            ("p/b".to_string(), "B".to_string())
        ]
    );
    assert_eq!(catalog.default_model.as_deref(), Some("p/a"));
    assert_eq!(
        catalog.reasoning.as_ref().unwrap().current.as_deref(),
        Some("high")
    );

    let thinking_only = std::slice::from_ref(&neutral[1]);
    let catalog = super::catalog::catalog_from_neutral(thinking_only);
    assert!(catalog.models.is_empty());
    assert!(catalog.default_model.is_none());
    assert!(catalog.reasoning.is_some());
}

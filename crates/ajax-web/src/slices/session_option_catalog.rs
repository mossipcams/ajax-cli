//! Last-advertised harness config options for New Task / idle pickers (AoE contract).

use crate::adapters::web_session_acp::{
    cached_harness_config_options, config_option_descriptors, remember_harness_config_options,
    AcpStdioClient, ConfigOptionDescriptor,
};
use ajax_core::models::AgentClient;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionOptionCatalogResponse {
    pub agent: String,
    #[serde(rename = "configOptions")]
    pub config_options: Vec<ConfigOptionDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub harness_version: String,
}

pub fn agent_client_from_name(agent: &str) -> AgentClient {
    crate::slices::session_models::agent_client_from_name(agent)
}

/// Last-advertised `configOptions` for `agent`, probing when the cache is empty.
pub fn list_session_option_catalog(agent: &str) -> SessionOptionCatalogResponse {
    let key = agent.trim().to_ascii_lowercase();
    let key = if key.is_empty() {
        "cursor".to_string()
    } else {
        key
    };
    let client = agent_client_from_name(&key);
    let version = crate::slices::session_models::harness_version(client);

    if let Some(options) = cached_harness_config_options(client) {
        if !options.is_empty() {
            return SessionOptionCatalogResponse {
                agent: key,
                config_options: options,
                error: None,
                harness_version: version,
            };
        }
    }

    if client == AgentClient::Other {
        return SessionOptionCatalogResponse {
            agent: key.clone(),
            config_options: Vec::new(),
            error: Some(format!("unsupported agent {key}")),
            harness_version: version,
        };
    }

    if let Some(missing) = missing_acp_program(client) {
        return SessionOptionCatalogResponse {
            agent: key,
            config_options: Vec::new(),
            error: Some(missing),
            harness_version: version,
        };
    }

    match probe_harness_config_options(client) {
        ProbeOutcome::SpawnFailed => SessionOptionCatalogResponse {
            agent: key.clone(),
            config_options: Vec::new(),
            error: Some(format!("{key} could not start to read config options")),
            harness_version: version,
        },
        ProbeOutcome::EmptyOptions => SessionOptionCatalogResponse {
            agent: key.clone(),
            config_options: Vec::new(),
            error: Some(format!("{key} started but advertised no config options")),
            harness_version: version,
        },
        ProbeOutcome::Ok(probed) => SessionOptionCatalogResponse {
            agent: key,
            config_options: probed,
            error: None,
            harness_version: version,
        },
    }
}

enum ProbeOutcome {
    Ok(Vec<ConfigOptionDescriptor>),
    SpawnFailed,
    EmptyOptions,
}

fn probe_harness_config_options(agent: AgentClient) -> ProbeOutcome {
    let Ok((_client, report)) = AcpStdioClient::spawn(agent, &std::env::temp_dir(), None, None)
    else {
        return ProbeOutcome::SpawnFailed;
    };
    let Some(raw) = report.config_options.as_deref() else {
        return ProbeOutcome::EmptyOptions;
    };
    if raw.is_empty() {
        return ProbeOutcome::EmptyOptions;
    }
    remember_harness_config_options(agent, raw);
    ProbeOutcome::Ok(config_option_descriptors(raw))
}

fn missing_acp_program(client: AgentClient) -> Option<String> {
    let launch = ajax_core::adapters::acp_launch_for_agent(client)?;
    let found = launch
        .candidates
        .iter()
        .any(|(program, _)| crate::adapters::program::resolve_program(program).is_some());
    (!found).then(|| {
        format!(
            "{} is not installed — {}",
            launch.candidates[0].0, launch.install_hint
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::web_session_acp::{
        clear_option_catalog_cache, with_test_acp_extra_args, with_test_acp_program,
    };
    use std::{fs, path::PathBuf};

    fn fake_acp_fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_acp.js")
    }

    #[test]
    fn unsupported_agent_reports_error() {
        let response = list_session_option_catalog("other");
        assert!(response.error.is_some());
        assert!(response.config_options.is_empty());
    }

    #[test]
    fn probe_spawn_failure_is_distinct_from_empty_advertised_options() {
        with_test_acp_program(std::path::Path::new("/no/such/ajax-acp-probe"), || {
            clear_option_catalog_cache();
            let response = list_session_option_catalog("codex");
            assert!(response.config_options.is_empty());
            let error = response.error.expect("spawn failure error");
            assert!(error.contains("could not start"));
            assert!(!error.contains("advertised no config options"));
        });
    }

    #[test]
    fn probe_empty_advertised_options_keeps_separate_message() {
        let dir = std::env::temp_dir().join(format!(
            "ajax-web-option-catalog-empty-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let script = dir.join("empty_config_options.js");
        fs::write(
            &script,
            r#"#!/usr/bin/env node
'use strict';
const readline = require('readline');
readline.createInterface({ input: process.stdin }).on('line', (line) => {
  const msg = JSON.parse(line);
  if (msg.method === 'initialize') {
    process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: msg.id, result: { protocolVersion: 1 } }) + '\n');
  } else if (msg.method === 'session/new') {
    process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: msg.id, result: { sessionId: 's1', configOptions: [] } }) + '\n');
  }
});
"#,
        )
        .unwrap();

        with_test_acp_program(&script, || {
            clear_option_catalog_cache();
            let response = list_session_option_catalog("codex");
            assert!(response.config_options.is_empty());
            let error = response.error.expect("empty options error");
            assert!(error.contains("advertised no config options"));
            assert!(!error.contains("could not start"));
        });

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn probe_success_returns_advertised_options() {
        let script = fake_acp_fixture();
        with_test_acp_program(&script, || {
            with_test_acp_extra_args(&["--cursor-models"], || {
                clear_option_catalog_cache();
                let response = list_session_option_catalog("cursor");
                assert!(response.error.is_none(), "{:?}", response.error);
                assert!(!response.config_options.is_empty());
            });
        });
    }
}

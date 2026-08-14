//! Per-harness model catalog for task creation and orchestration chat.
//!
//! Cursor lists its models through the CLI (`agent models`). The other
//! harnesses advertise theirs on the ACP `session/new` handshake, so their
//! catalog costs one short-lived bridge process — cached like Cursor's.

use ajax_core::{adapters::CURSOR_DEFAULT_MODEL, models::AgentClient};
use serde::Serialize;
use std::{
    collections::HashMap,
    process::Command,
    sync::Mutex,
    time::{Duration, Instant},
};

const CACHE_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionModelOption {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionModelsResponse {
    pub models: Vec<SessionModelOption>,
    /// Model the harness runs when the operator picks nothing.
    pub default: String,
    /// Agent this catalog belongs to, echoed so the browser can cache per agent.
    pub agent: String,
}

struct Cache {
    fetched_at: Instant,
    response: SessionModelsResponse,
}

static CACHE: Mutex<Option<HashMap<String, Cache>>> = Mutex::new(None);

/// Agent name as the browser sends it, mapped to a client Ajax can start.
pub fn agent_client_from_name(agent: &str) -> AgentClient {
    match agent.trim().to_ascii_lowercase().as_str() {
        "codex" => AgentClient::Codex,
        "claude" => AgentClient::Claude,
        "pi" => AgentClient::Pi,
        "cursor" | "" => AgentClient::Cursor,
        _ => AgentClient::Other,
    }
}

/// List the models `agent` can run. Soft-fails to the harness default alone.
pub fn list_session_models(agent: &str) -> SessionModelsResponse {
    let key = agent.trim().to_ascii_lowercase();
    let key = if key.is_empty() {
        "cursor".to_string()
    } else {
        key
    };

    if let Ok(guard) = CACHE.lock() {
        if let Some(cache) = guard.as_ref().and_then(|entries| entries.get(&key)) {
            if cache.fetched_at.elapsed() < CACHE_TTL {
                return cache.response.clone();
            }
        }
    }

    let response = fetch_catalog(&key);

    if let Ok(mut guard) = CACHE.lock() {
        guard.get_or_insert_with(HashMap::new).insert(
            key,
            Cache {
                fetched_at: Instant::now(),
                response: response.clone(),
            },
        );
    }

    response
}

fn fetch_catalog(agent: &str) -> SessionModelsResponse {
    let client = agent_client_from_name(agent);
    if client == AgentClient::Cursor {
        let models = fetch_models_from_agent().unwrap_or_else(|| {
            vec![SessionModelOption {
                id: "auto".to_string(),
                label: "Auto".to_string(),
            }]
        });
        return SessionModelsResponse {
            models,
            default: CURSOR_DEFAULT_MODEL.to_string(),
            agent: agent.to_string(),
        };
    }

    // The bridges only advertise their catalog inside a session, so this costs
    // one short-lived ACP process per cache miss.
    let catalog =
        crate::adapters::web_session_acp::read_agent_model_catalog(client, &std::env::temp_dir());
    let models = catalog
        .models
        .into_iter()
        .map(|(id, label)| SessionModelOption { id, label })
        .collect::<Vec<_>>();
    let default = catalog
        .default_model
        .or_else(|| models.first().map(|model| model.id.clone()))
        .unwrap_or_default();
    SessionModelsResponse {
        models,
        default,
        agent: agent.to_string(),
    }
}

fn fetch_models_from_agent() -> Option<Vec<SessionModelOption>> {
    let output = Command::new("agent").arg("models").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let models = parse_agent_models_output(&stdout);
    if models.is_empty() {
        None
    } else {
        Some(models)
    }
}

/// Parse `agent models` text: lines like `id - Label` after an optional header.
pub fn parse_agent_models_output(stdout: &str) -> Vec<SessionModelOption> {
    let mut models = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() || line.eq_ignore_ascii_case("Available models") {
            continue;
        }
        let Some((id, label)) = line.split_once(" - ") else {
            continue;
        };
        let id = id.trim();
        let label = label.trim();
        if id.is_empty() || label.is_empty() {
            continue;
        }
        if id.chars().any(|c| c.is_whitespace() || c.is_control()) {
            continue;
        }
        models.push(SessionModelOption {
            id: id.to_string(),
            label: label.to_string(),
        });
    }
    if !models.iter().any(|m| m.id == "auto") {
        models.insert(
            0,
            SessionModelOption {
                id: "auto".to_string(),
                label: "Auto".to_string(),
            },
        );
    }
    models
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_agent_models_output_reads_id_label_lines() {
        let models = parse_agent_models_output(
            "Available models\n\nauto - Auto (default)\ncomposer-2.5 - Composer 2.5\n",
        );
        assert_eq!(
            models,
            vec![
                SessionModelOption {
                    id: "auto".to_string(),
                    label: "Auto (default)".to_string(),
                },
                SessionModelOption {
                    id: "composer-2.5".to_string(),
                    label: "Composer 2.5".to_string(),
                },
            ]
        );
    }

    #[test]
    fn parse_agent_models_output_injects_auto_when_missing() {
        let models = parse_agent_models_output("composer-2.5 - Composer 2.5\n");
        assert_eq!(models[0].id, "auto");
        assert_eq!(models[1].id, "composer-2.5");
    }
}

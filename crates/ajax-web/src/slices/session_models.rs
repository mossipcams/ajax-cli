//! Cursor model catalog for orchestration chat (`agent models`).

use serde::Serialize;
use std::{
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
}

struct Cache {
    fetched_at: Instant,
    models: Vec<SessionModelOption>,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// List Cursor models for the session picker. Soft-fails to Auto alone.
pub fn list_session_models() -> SessionModelsResponse {
    if let Ok(guard) = CACHE.lock() {
        if let Some(cache) = guard.as_ref() {
            if cache.fetched_at.elapsed() < CACHE_TTL {
                return SessionModelsResponse {
                    models: cache.models.clone(),
                };
            }
        }
    }

    let models = fetch_models_from_agent().unwrap_or_else(|| {
        vec![SessionModelOption {
            id: "auto".to_string(),
            label: "Auto".to_string(),
        }]
    });

    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some(Cache {
            fetched_at: Instant::now(),
            models: models.clone(),
        });
    }

    SessionModelsResponse { models }
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

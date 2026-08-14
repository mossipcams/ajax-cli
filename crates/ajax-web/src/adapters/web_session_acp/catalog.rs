//! Model catalog read from a harness's own ACP handshake.
//!
//! Each bridge advertises its models on `session/new`, in one of two shapes:
//! Codex uses `models.availableModels`, Claude and Pi use a `configOptions`
//! entry with `id: "model"`. Cursor is not read here — it lists models through
//! its CLI (`agent models`), which needs no process handshake.

use super::client::AcpStdioClient;
use ajax_core::models::AgentClient;
use serde_json::Value;
use std::path::Path;

/// One selectable model, plus the id the harness would use on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentModelCatalog {
    pub models: Vec<(String, String)>,
    pub default_model: Option<String>,
}

impl AgentModelCatalog {
    fn empty() -> Self {
        Self {
            models: Vec::new(),
            default_model: None,
        }
    }
}

/// Start the harness's ACP process in `cwd` just long enough to read its
/// catalog. Returns an empty catalog when the harness cannot start or answers
/// with no models — the caller then offers the harness default only.
pub fn read_agent_model_catalog(agent: AgentClient, cwd: &Path) -> AgentModelCatalog {
    let Ok((client, _report)) = AcpStdioClient::spawn(agent, cwd, None, None) else {
        return AgentModelCatalog::empty();
    };
    let catalog = parse_session_new_catalog(client.session_new_result());
    drop(client);
    catalog
}

/// Pull the model catalog out of a `session/new` result, in either ACP shape.
pub fn parse_session_new_catalog(result: &Value) -> AgentModelCatalog {
    if let Some(models) = result.get("models") {
        let available = models
            .get("availableModels")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| {
                        let id = entry.get("modelId").and_then(Value::as_str)?;
                        let label = entry
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or(id)
                            .to_string();
                        Some((id.to_string(), label))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !available.is_empty() {
            return AgentModelCatalog {
                models: available,
                default_model: models
                    .get("currentModelId")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            };
        }
    }

    let Some(option) = result
        .get("configOptions")
        .and_then(Value::as_array)
        .and_then(|options| {
            options
                .iter()
                .find(|option| option.get("id").and_then(Value::as_str) == Some("model"))
        })
    else {
        return AgentModelCatalog::empty();
    };

    let models = option
        .get("options")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let id = entry.get("value").and_then(Value::as_str)?;
                    let label = entry
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or(id)
                        .to_string();
                    Some((id.to_string(), label))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    AgentModelCatalog {
        default_model: option
            .get("currentValue")
            .and_then(Value::as_str)
            .map(str::to_string),
        models,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_codex_available_models_shape() {
        let catalog = parse_session_new_catalog(&json!({
            "sessionId": "s1",
            "models": {
                "currentModelId": "gpt-5.6-sol[medium]",
                "availableModels": [
                    { "modelId": "gpt-5.6-sol[low]", "name": "GPT-5.6-Sol (low)" },
                    { "modelId": "gpt-5.6-sol[medium]", "name": "GPT-5.6-Sol (medium)" }
                ]
            }
        }));

        assert_eq!(
            catalog.models,
            vec![
                (
                    "gpt-5.6-sol[low]".to_string(),
                    "GPT-5.6-Sol (low)".to_string()
                ),
                (
                    "gpt-5.6-sol[medium]".to_string(),
                    "GPT-5.6-Sol (medium)".to_string()
                ),
            ]
        );
        assert_eq!(
            catalog.default_model.as_deref(),
            Some("gpt-5.6-sol[medium]")
        );
    }

    #[test]
    fn reads_config_options_shape_used_by_claude_and_pi() {
        let catalog = parse_session_new_catalog(&json!({
            "sessionId": "s1",
            "configOptions": [
                { "id": "mode", "options": [{ "value": "plan", "name": "Plan" }] },
                {
                    "id": "model",
                    "currentValue": "opencode-go/kimi-k3",
                    "options": [
                        { "value": "opencode-go/kimi-k3", "name": "Kimi K3" },
                        { "value": "opencode-go/glm-5.2", "name": "GLM-5.2" }
                    ]
                }
            ]
        }));

        assert_eq!(
            catalog.models,
            vec![
                ("opencode-go/kimi-k3".to_string(), "Kimi K3".to_string()),
                ("opencode-go/glm-5.2".to_string(), "GLM-5.2".to_string()),
            ]
        );
        assert_eq!(
            catalog.default_model.as_deref(),
            Some("opencode-go/kimi-k3")
        );
    }

    #[test]
    fn a_harness_that_advertises_nothing_yields_an_empty_catalog() {
        let catalog = parse_session_new_catalog(&json!({ "sessionId": "s1" }));
        assert!(catalog.models.is_empty());
        assert!(catalog.default_model.is_none());
    }
}

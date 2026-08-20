//! Model catalog read from a harness's own ACP handshake.
//!
//! Each bridge advertises its models on `session/new`, in one of two shapes:
//! Codex uses `models.availableModels`, Claude and Pi use a `configOptions`
//! entry with `id: "model"`. Cursor is not read here — it lists models through
//! its CLI (`agent models`), which needs no process handshake.

use super::client::AcpStdioClient;
use ajax_core::adapters::parse_cursor_model_intent;
use ajax_core::models::AgentClient;
use serde_json::Value;
use std::{collections::HashMap, path::Path};

/// A selectable option group: `(id, label)` pairs plus the harness's current
/// choice. Used for the model list and for the reasoning level beside it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AgentOptionGroup {
    /// Config id the harness answers to, e.g. `effort` or `reasoning_effort`.
    pub id: String,
    pub label: String,
    pub options: Vec<(String, String)>,
    pub current: Option<String>,
}

/// One selectable model, plus the id the harness would use on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentModelCatalog {
    pub models: Vec<(String, String)>,
    pub default_model: Option<String>,
    /// Reasoning level, when the harness exposes one as its own option.
    pub reasoning: Option<AgentOptionGroup>,
}

impl AgentModelCatalog {
    fn empty() -> Self {
        Self {
            models: Vec::new(),
            default_model: None,
            reasoning: None,
        }
    }
}

/// Read one `configOptions` entry into a group.
fn option_group(option: &Value) -> Option<AgentOptionGroup> {
    let id = option.get("id").and_then(Value::as_str)?;
    let options = option
        .get("options")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(|entry| {
            let value = entry.get("value").and_then(Value::as_str)?;
            let label = entry
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(value)
                .to_string();
            Some((value.to_string(), label))
        })
        .collect::<Vec<_>>();
    if options.is_empty() {
        return None;
    }
    Some(AgentOptionGroup {
        id: id.to_string(),
        label: option
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Reasoning")
            .to_string(),
        options,
        current: option
            .get("currentValue")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

/// Find a config option by category, falling back to its id — the harnesses
/// name the reasoning option differently (`effort`, `reasoning_effort`,
/// `thought_level`) but agree on the category.
fn config_option_in<'a>(result: &'a Value, category: &str) -> Option<&'a Value> {
    let options = result.get("configOptions").and_then(Value::as_array)?;
    options
        .iter()
        .find(|option| option.get("category").and_then(Value::as_str) == Some(category))
        .or_else(|| {
            options
                .iter()
                .find(|option| option.get("id").and_then(Value::as_str) == Some(category))
        })
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

/// Pull the model catalog out of a `session/new` result.
///
/// The harnesses advertise a `model` config option and, separately, a
/// `thought_level` one — that second axis is the reasoning level, which Cursor
/// instead bakes into its model ids. `models.availableModels` is the older shape
/// and is used only when no `model` config option is offered.
pub fn parse_session_new_catalog(result: &Value) -> AgentModelCatalog {
    let reasoning = config_option_in(result, "thought_level").and_then(option_group);

    if let Some(group) = config_option_in(result, "model").and_then(option_group) {
        return AgentModelCatalog {
            models: group.options,
            default_model: group.current,
            reasoning,
        };
    }

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
                reasoning,
            };
        }
    }

    AgentModelCatalog {
        reasoning,
        ..AgentModelCatalog::empty()
    }
}

/// Display names Cursor advertises on its `model` config option, keyed by base id.
///
/// New Task keeps ids and effort/fast axes from `agent models`, but row labels
/// should match the connected switcher (`choice.name` values such as
/// `Grok 4.6`, not CLI strings like `Cursor Grok 4.6`). Returns empty on failure
/// so callers fail open to the CLI labels they already have.
#[cfg(not(test))]
pub fn read_cursor_acp_model_labels(cwd: &Path) -> HashMap<String, String> {
    let Ok((client, _report)) = AcpStdioClient::spawn(AgentClient::Cursor, cwd, None, None) else {
        return HashMap::new();
    };
    let labels = cursor_model_labels_from_session_new(client.session_new_result());
    drop(client);
    labels
}

/// Test builds must not spawn a Cursor ACP child from the catalog route.
#[cfg(test)]
pub fn read_cursor_acp_model_labels(_cwd: &Path) -> HashMap<String, String> {
    HashMap::new()
}

/// Pull base-id display labels out of a Cursor `session/new` result.
pub fn cursor_model_labels_from_session_new(result: &Value) -> HashMap<String, String> {
    let Some(group) = config_option_in(result, "model").and_then(option_group) else {
        return HashMap::new();
    };
    let mut labels = HashMap::new();
    for (value, name) in group.options {
        let Some(intent) = parse_cursor_model_intent(&value) else {
            continue;
        };
        let clean = intent.effort.is_none() && !intent.fast.unwrap_or(false);
        match labels.get(&intent.base) {
            None => {
                labels.insert(intent.base, name);
            }
            Some(_) if clean => {
                labels.insert(intent.base, name);
            }
            _ => {}
        }
    }
    labels
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

    // Claude and Pi keep the reasoning level in its own option; the picker has
    // to show it, because the model id alone does not carry it.
    #[test]
    fn reads_the_reasoning_level_as_its_own_group() {
        let catalog = parse_session_new_catalog(&json!({
            "sessionId": "s1",
            "configOptions": [
                {
                    "id": "model",
                    "category": "model",
                    "currentValue": "opus",
                    "options": [
                        { "value": "opus", "name": "Opus" },
                        { "value": "haiku", "name": "Haiku" }
                    ]
                },
                {
                    "id": "effort",
                    "category": "thought_level",
                    "name": "Effort",
                    "currentValue": "high",
                    "options": [
                        { "value": "low", "name": "Low" },
                        { "value": "high", "name": "High" }
                    ]
                }
            ]
        }));

        let reasoning = catalog.reasoning.expect("reasoning group");
        assert_eq!(reasoning.id, "effort");
        assert_eq!(reasoning.label, "Effort");
        assert_eq!(reasoning.current.as_deref(), Some("high"));
        assert_eq!(
            reasoning.options,
            vec![
                ("low".to_string(), "Low".to_string()),
                ("high".to_string(), "High".to_string()),
            ]
        );
        assert_eq!(catalog.default_model.as_deref(), Some("opus"));
    }

    #[test]
    fn a_harness_that_advertises_nothing_yields_an_empty_catalog() {
        let catalog = parse_session_new_catalog(&json!({ "sessionId": "s1" }));
        assert!(catalog.models.is_empty());
        assert!(catalog.default_model.is_none());
    }

    #[test]
    fn cursor_model_labels_prefer_clean_base_names_over_bracket_variants() {
        let labels = cursor_model_labels_from_session_new(&json!({
            "sessionId": "s1",
            "configOptions": [
                {
                    "id": "model",
                    "category": "model",
                    "currentValue": "grok-4.6",
                    "options": [
                        { "value": "composer-2.5", "name": "Composer 2.5" },
                        { "value": "grok-4.6", "name": "Grok 4.6" },
                        { "value": "gpt-5.6-sol", "name": "GPT-5.6-Sol" },
                        {
                            "value": "gpt-5.6-sol[effort=high,fast=false]",
                            "name": "GPT-5.6-Sol High"
                        },
                        {
                            "value": "claude-opus-5-thinking-high",
                            "name": "Claude Opus 5 Thinking"
                        }
                    ]
                }
            ]
        }));

        assert_eq!(
            labels.get("composer-2.5").map(String::as_str),
            Some("Composer 2.5")
        );
        assert_eq!(labels.get("grok-4.6").map(String::as_str), Some("Grok 4.6"));
        assert_eq!(
            labels.get("gpt-5.6-sol").map(String::as_str),
            Some("GPT-5.6-Sol")
        );
        assert_eq!(
            labels.get("claude-opus-5-thinking").map(String::as_str),
            Some("Claude Opus 5 Thinking")
        );
    }
}

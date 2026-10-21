use super::client::AcpStdioClient;
use ajax_core::models::AgentClient;
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AgentOptionGroup {
    pub id: String,
    pub label: String,
    pub options: Vec<(String, String)>,
    pub current: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentModelCatalog {
    pub models: Vec<(String, String)>,
    pub default_model: Option<String>,
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

pub fn read_agent_model_catalog(agent: AgentClient, cwd: &Path) -> AgentModelCatalog {
    match super::session_client::transport_for(agent) {
        ajax_core::adapters::HarnessTransport::CursorSdk => AgentModelCatalog::empty(),
        ajax_core::adapters::HarnessTransport::ClaudeSdk => read_claude_sdk_catalog(cwd),
        ajax_core::adapters::HarnessTransport::PiRpc => read_pi_rpc_catalog(cwd),
        ajax_core::adapters::HarnessTransport::Acp => {
            let Ok((client, _report)) = AcpStdioClient::spawn(agent, cwd, None, None) else {
                return AgentModelCatalog::empty();
            };
            let catalog = parse_session_new_catalog(client.session_new_result());
            drop(client);
            catalog
        }
    }
}

/// The Cursor SDK model list plus the non-secret credential status
/// ("env" | "store") reported by the sidecar; `None` when it predates the field.
pub struct CursorSdkCatalog {
    pub models: Vec<super::cursor_sdk_client::CursorCatalogModel>,
    pub credential: Option<String>,
}

/// Read the Cursor model catalog from a Cursor SDK sidecar handshake.
/// Empty when the sidecar cannot be launched or fails to start.
pub fn read_cursor_sdk_catalog(cwd: &Path) -> CursorSdkCatalog {
    let empty = CursorSdkCatalog {
        models: Vec::new(),
        credential: None,
    };
    let Ok((program, extra_args)) = super::cursor_sdk_spawn::cursor_launch() else {
        return empty;
    };
    let Ok(client) = super::cursor_sdk_client::CursorSdkClient::spawn(
        &program,
        &extra_args,
        cwd,
        None,
        None,
        super::client::HANDSHAKE_TIMEOUT,
    ) else {
        return empty;
    };
    let models = client.model_catalog();
    let credential = client.cursor_credential_source();
    client.shutdown();
    CursorSdkCatalog { models, credential }
}

/// `Some(catalog)` when Cursor runs on the SDK transport and its catalog
/// could be read; `None` otherwise (the caller falls back).
pub fn read_cursor_catalog_if_sdk(cwd: &Path) -> Option<CursorSdkCatalog> {
    if super::session_client::transport_for(AgentClient::Cursor)
        != ajax_core::adapters::HarnessTransport::CursorSdk
    {
        return None;
    }
    let catalog = read_cursor_sdk_catalog(cwd);
    (!catalog.models.is_empty()).then_some(catalog)
}

/// Build the model catalog for a Pi RPC agent from its handshake config
/// options, parsed natively from the neutral values instead of an ACP
/// session/new round trip.
pub fn read_pi_rpc_catalog(cwd: &Path) -> AgentModelCatalog {
    let Ok((program, extra_args)) = super::session_client::pi_program_and_args() else {
        return AgentModelCatalog::empty();
    };
    let Ok(client) = super::pi_rpc_client::PiRpcClient::spawn(
        &program,
        &extra_args,
        cwd,
        None,
        super::client::HANDSHAKE_TIMEOUT,
    ) else {
        return AgentModelCatalog::empty();
    };
    let options = client.config_options();
    client.shutdown();
    catalog_from_neutral(&options)
}

/// Build a model catalog directly from neutral config options, mirroring the
/// lookups and group construction of [`parse_session_new_catalog`] without any
/// ACP JSON.
pub(super) fn catalog_from_neutral(options: &[super::NeutralConfigOption]) -> AgentModelCatalog {
    let reasoning = neutral_option_in(
        options,
        &super::NeutralConfigCategory::ThoughtLevel,
        "thought_level",
    )
    .and_then(neutral_option_group);

    if let Some(group) = neutral_option_in(options, &super::NeutralConfigCategory::Model, "model")
        .and_then(neutral_option_group)
    {
        return AgentModelCatalog {
            models: group.options,
            default_model: group.current,
            reasoning,
        };
    }

    AgentModelCatalog {
        reasoning,
        ..AgentModelCatalog::empty()
    }
}

/// Find the first option with the given category, else the first with the
/// fallback id (mirrors `config_option_in`).
fn neutral_option_in<'a>(
    options: &'a [super::NeutralConfigOption],
    category: &super::NeutralConfigCategory,
    fallback_id: &str,
) -> Option<&'a super::NeutralConfigOption> {
    options
        .iter()
        .find(|option| option.category.as_ref() == Some(category))
        .or_else(|| options.iter().find(|option| option.id == fallback_id))
}

/// Convert a select-style neutral option into an [`AgentOptionGroup`], or
/// `None` for non-select kinds and empty choice lists (mirrors `option_group`).
fn neutral_option_group(option: &super::NeutralConfigOption) -> Option<AgentOptionGroup> {
    let (current, choices) = match &option.kind {
        super::NeutralConfigKind::Select {
            current_value,
            choices,
        } => (current_value, choices),
        _ => return None,
    };
    if choices.is_empty() {
        return None;
    }
    Some(AgentOptionGroup {
        id: option.id.clone(),
        label: option.name.clone(),
        options: choices
            .iter()
            .map(|choice| (choice.value.clone(), choice.name.clone()))
            .collect(),
        current: Some(current.clone()),
    })
}

/// Build the model catalog for a Claude Agent SDK agent from its handshake
/// config options, parsed natively from the neutral values.
pub fn read_claude_sdk_catalog(cwd: &Path) -> AgentModelCatalog {
    let Ok((program, extra_args)) = super::claude_sdk_spawn::claude_launch() else {
        return AgentModelCatalog::empty();
    };
    let Ok(client) = super::claude_sdk_client::ClaudeSdkClient::spawn(
        &program,
        &extra_args,
        cwd,
        None,
        None,
        super::client::HANDSHAKE_TIMEOUT,
    ) else {
        return AgentModelCatalog::empty();
    };
    let options = client.config_options();
    client.shutdown();
    catalog_from_neutral(&options)
}

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
}

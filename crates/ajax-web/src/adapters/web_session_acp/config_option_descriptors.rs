//! Browser-facing snapshots of live ACP session config options.

use super::config_options::category_name;
use agent_client_protocol::schema::v1::{
    SessionConfigKind, SessionConfigOption, SessionConfigSelectOptions,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One advertised choice for a select config option (browser snapshot).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigOptionChoiceDescriptor {
    pub value: String,
    pub name: String,
}

/// Lightweight descriptor for live session config options (AoE-style).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigOptionDescriptor {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub current_value: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<ConfigOptionChoiceDescriptor>,
}

pub fn config_option_descriptors(options: &[SessionConfigOption]) -> Vec<ConfigOptionDescriptor> {
    options
        .iter()
        .filter_map(|option| {
            let (kind, current_value, choices) = match &option.kind {
                SessionConfigKind::Select(select) => {
                    let choices = match &select.options {
                        SessionConfigSelectOptions::Ungrouped(items) => items
                            .iter()
                            .map(|item| ConfigOptionChoiceDescriptor {
                                value: item.value.0.to_string(),
                                name: item.name.clone(),
                            })
                            .collect(),
                        SessionConfigSelectOptions::Grouped(groups) => groups
                            .iter()
                            .flat_map(|group| group.options.iter())
                            .map(|item| ConfigOptionChoiceDescriptor {
                                value: item.value.0.to_string(),
                                name: item.name.clone(),
                            })
                            .collect(),
                        _ => Vec::new(),
                    };
                    (
                        "select".to_string(),
                        Value::String(select.current_value.0.to_string()),
                        choices,
                    )
                }
                SessionConfigKind::Boolean(boolean) => (
                    "boolean".to_string(),
                    Value::Bool(boolean.current_value),
                    Vec::new(),
                ),
                _ => return None,
            };
            Some(ConfigOptionDescriptor {
                id: option.id.0.to_string(),
                category: option
                    .category
                    .as_ref()
                    .map(|category| category_name(category).to_string()),
                name: option.name.clone(),
                kind,
                current_value,
                choices,
            })
        })
        .collect()
}

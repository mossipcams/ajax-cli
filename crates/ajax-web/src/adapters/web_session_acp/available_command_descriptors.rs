//! Browser-facing snapshots of live ACP slash commands.

use agent_client_protocol::schema::v1::{AvailableCommand, AvailableCommandInput};
use serde::{Deserialize, Serialize};

/// One advertised slash command for protocol v2 snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableCommandDescriptor {
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_hint: Option<String>,
}

pub fn available_command_descriptors(
    commands: &[AvailableCommand],
) -> Vec<AvailableCommandDescriptor> {
    commands
        .iter()
        .map(|command| AvailableCommandDescriptor {
            name: command.name.clone(),
            description: command.description.clone(),
            input_hint: command.input.as_ref().and_then(input_hint),
        })
        .collect()
}

fn input_hint(input: &AvailableCommandInput) -> Option<String> {
    match input {
        AvailableCommandInput::Unstructured(unstructured) => {
            let hint = unstructured.hint.trim();
            (!hint.is_empty()).then(|| hint.to_string())
        }
        _ => None,
    }
}

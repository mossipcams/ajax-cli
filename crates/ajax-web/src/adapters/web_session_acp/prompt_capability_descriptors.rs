//! Browser-facing snapshots of live ACP prompt capabilities from initialize.

use agent_client_protocol::schema::v1::PromptCapabilities;
use serde::{Deserialize, Serialize};

/// Advertised prompt content capabilities for protocol v2 snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptCapabilityDescriptor {
    #[serde(default, skip_serializing_if = "is_false")]
    pub image: bool,
    #[serde(default, skip_serializing_if = "is_false", rename = "embeddedContext")]
    pub embedded_context: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

pub fn prompt_capability_descriptor(
    capabilities: &PromptCapabilities,
) -> PromptCapabilityDescriptor {
    PromptCapabilityDescriptor {
        image: capabilities.image,
        embedded_context: capabilities.embedded_context,
    }
}

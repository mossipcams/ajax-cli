//! Map ACP output content blocks (image, resource_link, embedded resource) to
//! browser wire types. Text stays on `SessionServerEvent::Message::text` and
//! `ToolContent::Text`; audio and ACP terminals are omitted.

use agent_client_protocol::schema::v1::{
    BlobResourceContents, ContentBlock, EmbeddedResourceResource, ImageContent, ResourceLink,
    TextResourceContents,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ToolContent;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputContentBlockWire {
    ResourceLink {
        name: String,
        uri: String,
        #[serde(default, rename = "mimeType", skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    Image {
        #[serde(rename = "mimeType")]
        mime_type: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        uri: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<String>,
    },
    Resource {
        uri: String,
        #[serde(default, rename = "mimeType", skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        blob: Option<String>,
    },
}

pub fn map_output_block(block: &ContentBlock) -> Option<OutputContentBlockWire> {
    match block {
        ContentBlock::Text(_) | ContentBlock::Audio(_) => None,
        ContentBlock::Image(image) => Some(compact_image_wire(image)),
        ContentBlock::ResourceLink(link) => Some(map_resource_link(link)),
        ContentBlock::Resource(embedded) => match &embedded.resource {
            EmbeddedResourceResource::TextResourceContents(text) => Some(map_text_resource(text)),
            EmbeddedResourceResource::BlobResourceContents(blob) => Some(map_blob_resource(blob)),
            _ => None,
        },
        _ => None,
    }
}

pub fn map_output_block_to_tool(block: &ContentBlock) -> Option<ToolContent> {
    map_output_block(block).map(ToolContent::from_output_block)
}

pub fn extract_output_block(value: &Value) -> Option<OutputContentBlockWire> {
    match value.get("type").and_then(Value::as_str) {
        Some("text") | Some("audio") => None,
        Some("image") => {
            let mime_type = value
                .get("mimeType")
                .or_else(|| value.get("mime_type"))
                .and_then(Value::as_str)?
                .to_string();
            let uri = optional_string(value, &["uri"]);
            let data = optional_string(value, &["data"]);
            Some(OutputContentBlockWire::Image {
                mime_type,
                uri: uri.clone(),
                data: if uri.is_some() { None } else { data },
            })
        }
        Some("resource_link") => {
            let name = value.get("name").and_then(Value::as_str)?.to_string();
            let uri = value.get("uri").and_then(Value::as_str)?.to_string();
            Some(OutputContentBlockWire::ResourceLink {
                name,
                uri,
                mime_type: optional_string(value, &["mimeType", "mime_type"]),
                title: optional_string(value, &["title"]),
                description: optional_string(value, &["description"]),
            })
        }
        Some("resource") => {
            let resource = value.get("resource").unwrap_or(value);
            let uri = resource.get("uri").and_then(Value::as_str)?.to_string();
            let mime_type = optional_string(resource, &["mimeType", "mime_type"]);
            let text = optional_string(resource, &["text"]);
            Some(OutputContentBlockWire::Resource {
                uri,
                mime_type,
                text,
                blob: None,
            })
        }
        _ => None,
    }
}

pub fn extract_tool_content_item(item: &Value) -> Option<ToolContent> {
    if item.get("type").and_then(Value::as_str) == Some("diff") {
        return None;
    }
    if item.get("type").and_then(Value::as_str) == Some("terminal") {
        return None;
    }
    let block = item.get("content").unwrap_or(item);
    extract_output_block(block)
        .map(ToolContent::from_output_block)
        .or_else(|| {
            let text = extract_text(block);
            (!text.trim().is_empty()).then_some(ToolContent::Text { text })
        })
}

fn compact_image_wire(image: &ImageContent) -> OutputContentBlockWire {
    OutputContentBlockWire::Image {
        mime_type: image.mime_type.clone(),
        uri: image.uri.clone(),
        data: if image.uri.is_some() {
            None
        } else {
            Some(image.data.clone())
        },
    }
}

fn map_resource_link(link: &ResourceLink) -> OutputContentBlockWire {
    OutputContentBlockWire::ResourceLink {
        name: link.name.clone(),
        uri: link.uri.clone(),
        mime_type: link.mime_type.clone(),
        title: link.title.clone(),
        description: link.description.clone(),
    }
}

fn map_text_resource(text: &TextResourceContents) -> OutputContentBlockWire {
    OutputContentBlockWire::Resource {
        uri: text.uri.clone(),
        mime_type: text.mime_type.clone(),
        text: Some(text.text.clone()),
        blob: None,
    }
}

fn map_blob_resource(blob: &BlobResourceContents) -> OutputContentBlockWire {
    OutputContentBlockWire::Resource {
        uri: blob.uri.clone(),
        mime_type: blob.mime_type.clone(),
        text: None,
        blob: None,
    }
}

fn optional_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|text| !text.is_empty())
}

fn extract_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Object(map) => map
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        _ => String::new(),
    }
}

impl ToolContent {
    pub fn from_output_block(block: OutputContentBlockWire) -> Self {
        match block {
            OutputContentBlockWire::Image {
                mime_type,
                uri,
                data,
            } => ToolContent::Image {
                mime_type,
                uri,
                data,
            },
            OutputContentBlockWire::ResourceLink {
                name,
                uri,
                mime_type,
                title,
                description,
            } => ToolContent::ResourceLink {
                name,
                uri,
                mime_type,
                title,
                description,
            },
            OutputContentBlockWire::Resource {
                uri,
                mime_type,
                text,
                blob,
            } => ToolContent::Resource {
                uri,
                mime_type,
                text,
                blob,
            },
        }
    }
}

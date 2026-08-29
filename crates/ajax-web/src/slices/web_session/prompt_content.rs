//! Validate browser prompt content blocks and map them to ACP `ContentBlock`s.

use crate::adapters::web_session_acp::PromptCapabilityDescriptor;
use agent_client_protocol::schema::v1::{
    BlobResourceContents, ContentBlock, EmbeddedResource, EmbeddedResourceResource, ImageContent,
    ResourceLink, TextContent, TextResourceContents,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PromptContentBlockWire {
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
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
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

#[derive(Debug, Clone)]
pub struct PromptPayload {
    pub transcript_text: String,
    pub blocks: Vec<ContentBlock>,
}

pub fn build_prompt_payload(
    text: &str,
    wire_blocks: &[PromptContentBlockWire],
    caps: &PromptCapabilityDescriptor,
) -> Result<PromptPayload, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() && wire_blocks.is_empty() {
        return Err("prompt text or content is required".to_string());
    }
    reject_disallowed_wire_blocks(wire_blocks, caps)?;
    let mut blocks = Vec::with_capacity(wire_blocks.len() + 1);
    if !trimmed.is_empty() {
        blocks.push(ContentBlock::Text(TextContent::new(trimmed)));
    }
    for block in wire_blocks {
        blocks.push(wire_to_content_block(block, caps)?);
    }
    Ok(PromptPayload {
        transcript_text: transcript_summary(trimmed, wire_blocks),
        blocks,
    })
}

fn wire_to_content_block(
    block: &PromptContentBlockWire,
    caps: &PromptCapabilityDescriptor,
) -> Result<ContentBlock, String> {
    match block {
        PromptContentBlockWire::ResourceLink {
            name,
            uri,
            mime_type,
            title,
            description,
        } => {
            let name = name.trim();
            let uri = uri.trim();
            if name.is_empty() || uri.is_empty() {
                return Err("resource_link name and uri are required".to_string());
            }
            let mut link = ResourceLink::new(name, uri);
            if let Some(mime_type) = mime_type
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                link = link.mime_type(mime_type);
            }
            if let Some(title) = title.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                link = link.title(title);
            }
            if let Some(description) = description
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                link = link.description(description);
            }
            Ok(ContentBlock::ResourceLink(link))
        }
        PromptContentBlockWire::Image { data, mime_type } => {
            if !caps.image {
                return Err("agent did not advertise image prompt content".to_string());
            }
            let data = data.trim();
            let mime_type = mime_type.trim();
            if data.is_empty() || mime_type.is_empty() {
                return Err("image data and mimeType are required".to_string());
            }
            Ok(ContentBlock::Image(ImageContent::new(data, mime_type)))
        }
        PromptContentBlockWire::Resource {
            uri,
            mime_type,
            text,
            blob,
        } => {
            if !caps.embedded_context {
                return Err("agent did not advertise embeddedContext prompt content".to_string());
            }
            let uri = uri.trim();
            if uri.is_empty() {
                return Err("embedded resource uri is required".to_string());
            }
            let has_text = text
                .as_deref()
                .map(str::trim)
                .is_some_and(|s| !s.is_empty());
            let has_blob = blob
                .as_deref()
                .map(str::trim)
                .is_some_and(|s| !s.is_empty());
            if has_text == has_blob {
                return Err("embedded resource requires exactly one of text or blob".to_string());
            }
            let resource = if has_text {
                let mut contents = TextResourceContents::new(text.as_ref().unwrap().trim(), uri);
                if let Some(mime_type) = mime_type
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    contents = contents.mime_type(mime_type);
                }
                EmbeddedResourceResource::TextResourceContents(contents)
            } else {
                let mut contents = BlobResourceContents::new(blob.as_ref().unwrap().trim(), uri);
                if let Some(mime_type) = mime_type
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    contents = contents.mime_type(mime_type);
                }
                EmbeddedResourceResource::BlobResourceContents(contents)
            };
            Ok(ContentBlock::Resource(EmbeddedResource::new(resource)))
        }
    }
}

pub fn reject_disallowed_wire_blocks(
    wire_blocks: &[PromptContentBlockWire],
    caps: &PromptCapabilityDescriptor,
) -> Result<(), String> {
    for block in wire_blocks {
        match block {
            PromptContentBlockWire::ResourceLink { .. } => {}
            PromptContentBlockWire::Image { .. } if caps.image => {}
            PromptContentBlockWire::Image { .. } => {
                return Err("agent did not advertise image prompt content".to_string());
            }
            PromptContentBlockWire::Resource { .. } if caps.embedded_context => {}
            PromptContentBlockWire::Resource { .. } => {
                return Err("agent did not advertise embeddedContext prompt content".to_string());
            }
        }
    }
    Ok(())
}

pub fn default_prompt_capabilities() -> PromptCapabilityDescriptor {
    crate::adapters::web_session_acp::prompt_capability_descriptor(
        &agent_client_protocol::schema::v1::PromptCapabilities::default(),
    )
}

fn transcript_summary(text: &str, wire_blocks: &[PromptContentBlockWire]) -> String {
    let labels: Vec<String> = wire_blocks.iter().filter_map(block_label).collect();
    if labels.is_empty() {
        return text.to_string();
    }
    let attachments = format!("[attached: {}]", labels.join(", "));
    if text.is_empty() {
        attachments
    } else {
        format!("{text}\n\n{attachments}")
    }
}

fn block_label(block: &PromptContentBlockWire) -> Option<String> {
    match block {
        PromptContentBlockWire::ResourceLink { name, .. } => {
            let name = name.trim();
            (!name.is_empty()).then(|| name.to_string())
        }
        PromptContentBlockWire::Image { mime_type, .. } => Some(image_label(mime_type.trim())),
        PromptContentBlockWire::Resource { uri, mime_type, .. } => {
            let uri = uri.trim();
            if uri.is_empty() {
                return None;
            }
            let name = uri.rsplit('/').next().unwrap_or(uri);
            mime_type
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|mime| format!("{name} ({mime})"))
                .or_else(|| Some(name.to_string()))
        }
    }
}

fn image_label(mime_type: &str) -> String {
    if mime_type.starts_with("image/") {
        format!(
            "image ({})",
            mime_type.strip_prefix("image/").unwrap_or(mime_type)
        )
    } else {
        format!("image ({mime_type})")
    }
}

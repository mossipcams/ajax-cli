//! Validate browser prompt content blocks and map them to ACP `ContentBlock`s.

use crate::adapters::web_session_acp::PromptCapabilityDescriptor;
use agent_client_protocol::schema::v1::{
    BlobResourceContents, ContentBlock, EmbeddedResource, EmbeddedResourceResource, ImageContent,
    ResourceLink, TextContent, TextResourceContents,
};
use base64::Engine;
use serde::{Deserialize, Serialize};

/// Per-frame WebSocket ceiling for session client prompts (mirrors browser transport).
pub(crate) const MAX_PROMPT_FRAME_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const MAX_IMAGE_BLOCKS: usize = 8;
const PROMPT_FRAME_HEADROOM_BYTES: usize = 4096;
const PLACEHOLDER_CLIENT_MESSAGE_ID: &str = "00000000-0000-4000-8000-000000000000";

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
    validate_prompt_content(trimmed, wire_blocks, caps)?;
    if trimmed.is_empty() && wire_blocks.is_empty() {
        return Err("prompt text or content blocks are required".to_string());
    }
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

pub fn validate_prompt_content(
    text: &str,
    wire_blocks: &[PromptContentBlockWire],
    caps: &PromptCapabilityDescriptor,
) -> Result<(), String> {
    reject_disallowed_wire_blocks(wire_blocks, caps)?;
    let image_count = wire_blocks
        .iter()
        .filter(|block| matches!(block, PromptContentBlockWire::Image { .. }))
        .count();
    if image_count > MAX_IMAGE_BLOCKS {
        return Err(format!(
            "prompt exceeds maximum of {MAX_IMAGE_BLOCKS} image blocks"
        ));
    }
    let frame_bytes = estimate_prompt_frame_bytes(text, wire_blocks)?;
    if frame_bytes > MAX_PROMPT_FRAME_BYTES {
        return Err("prompt frame too large".to_string());
    }
    for block in wire_blocks {
        if let PromptContentBlockWire::Image { data, mime_type } = block {
            validate_image_block(data, mime_type, text, wire_blocks)?;
        }
    }
    Ok(())
}

fn estimate_prompt_frame_bytes(
    text: &str,
    wire_blocks: &[PromptContentBlockWire],
) -> Result<usize, String> {
    #[derive(Serialize)]
    struct PromptFrame<'a> {
        #[serde(rename = "type")]
        ty: &'static str,
        text: &'a str,
        #[serde(rename = "clientMessageId")]
        client_message_id: &'static str,
        #[serde(rename = "contentBlocks", skip_serializing_if = "prompt_blocks_empty")]
        content_blocks: &'a [PromptContentBlockWire],
    }
    let frame = PromptFrame {
        ty: "prompt",
        text: text.trim(),
        client_message_id: PLACEHOLDER_CLIENT_MESSAGE_ID,
        content_blocks: wire_blocks,
    };
    serde_json::to_string(&frame)
        .map(|json| json.len())
        .map_err(|error| format!("prompt frame encode failed: {error}"))
}

fn prompt_blocks_empty(blocks: &&[PromptContentBlockWire]) -> bool {
    blocks.is_empty()
}

fn max_image_base64_chars(
    text: &str,
    wire_blocks: &[PromptContentBlockWire],
) -> Result<usize, String> {
    let non_image_blocks: Vec<_> = wire_blocks
        .iter()
        .filter(|block| !matches!(block, PromptContentBlockWire::Image { .. }))
        .cloned()
        .collect();
    let empty_image = PromptContentBlockWire::Image {
        data: String::new(),
        mime_type: "image/jpeg".to_string(),
    };
    let mut probe = non_image_blocks;
    probe.push(empty_image);
    let frame_without_image_data = estimate_prompt_frame_bytes(text, &probe)?;
    let budget = MAX_PROMPT_FRAME_BYTES
        .saturating_sub(PROMPT_FRAME_HEADROOM_BYTES)
        .saturating_sub(frame_without_image_data);
    Ok(budget)
}

fn validate_image_block(
    data: &str,
    mime_type: &str,
    text: &str,
    wire_blocks: &[PromptContentBlockWire],
) -> Result<(), String> {
    let data = data.trim();
    let mime_type = mime_type.trim();
    if data.is_empty() || mime_type.is_empty() {
        return Err("image data and mimeType are required".to_string());
    }
    if !mime_type.starts_with("image/") {
        return Err(format!("unsupported image mimeType: {mime_type}"));
    }
    if !supported_image_mime(mime_type) {
        return Err(format!("unsupported image mimeType: {mime_type}"));
    }
    let max_base64 = max_image_base64_chars(text, wire_blocks)?;
    if data.len() > max_base64 {
        return Err("image payload exceeds prompt frame budget".to_string());
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| "image data is not valid base64".to_string())?;
    if decoded.is_empty() {
        return Err("image data is empty".to_string());
    }
    if !image_magic_matches_mime(&decoded, mime_type) {
        return Err(format!("image bytes do not match mimeType {mime_type}"));
    }
    Ok(())
}

fn supported_image_mime(mime_type: &str) -> bool {
    matches!(
        mime_type,
        "image/jpeg" | "image/jpg" | "image/png" | "image/gif" | "image/webp"
    )
}

fn image_magic_matches_mime(bytes: &[u8], mime_type: &str) -> bool {
    match mime_type {
        "image/jpeg" | "image/jpg" => bytes.starts_with(&[0xFF, 0xD8, 0xFF]),
        "image/png" => bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]),
        "image/gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        "image/webp" => {
            bytes.len() >= 12 && bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP")
        }
        _ => false,
    }
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
    if text.is_empty() {
        return format!("[attached: {}]", labels.join(", "));
    }
    format!("{text}\n\n[attached: {}]", labels.join(", "))
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

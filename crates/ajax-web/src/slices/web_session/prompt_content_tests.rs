//! Prompt content validation and transcript shaping.

use super::prompt_content::{
    build_prompt_payload, default_prompt_capabilities, reject_disallowed_wire_blocks,
    validate_prompt_content, PromptContentBlockWire, MAX_IMAGE_BLOCKS, MAX_PROMPT_FRAME_BYTES,
};
use crate::adapters::web_session_acp::PromptCapabilityDescriptor;
use agent_client_protocol::schema::v1::{ContentBlock, PromptCapabilities};
use base64::Engine;

fn caps(image: bool, embedded_context: bool) -> PromptCapabilityDescriptor {
    PromptCapabilityDescriptor {
        image,
        embedded_context,
    }
}

fn tiny_png_base64() -> String {
    base64::engine::general_purpose::STANDARD
        .encode([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])
}

#[test]
fn text_only_prompt_builds_single_text_block() {
    let payload = build_prompt_payload("hello", &[], &default_prompt_capabilities()).unwrap();
    assert_eq!(payload.transcript_text, "hello");
    assert_eq!(payload.blocks.len(), 1);
    assert!(matches!(
        &payload.blocks[0],
        ContentBlock::Text(text) if text.text == "hello"
    ));
}

#[test]
fn resource_link_is_always_allowed() {
    let payload = build_prompt_payload(
        "see file",
        &[PromptContentBlockWire::ResourceLink {
            name: "notes.md".to_string(),
            uri: "file:///tmp/notes.md".to_string(),
            mime_type: Some("text/markdown".to_string()),
            title: None,
            description: None,
        }],
        &default_prompt_capabilities(),
    )
    .unwrap();
    assert!(payload.transcript_text.contains("[attached: notes.md]"));
    assert_eq!(payload.blocks.len(), 2);
}

#[test]
fn empty_prompt_without_blocks_is_rejected() {
    let err = build_prompt_payload("", &[], &default_prompt_capabilities()).unwrap_err();
    assert!(err.contains("required"));
}

#[test]
fn image_only_prompt_omits_empty_text_block() {
    let block = PromptContentBlockWire::Image {
        data: tiny_png_base64(),
        mime_type: "image/png".to_string(),
    };
    let payload = build_prompt_payload("", &[block], &caps(true, false)).unwrap();
    assert_eq!(payload.transcript_text, "[attached: image (png)]");
    assert_eq!(payload.blocks.len(), 1);
    assert!(matches!(&payload.blocks[0], ContentBlock::Image(_)));
}

#[test]
fn image_requires_advertised_capability() {
    let block = PromptContentBlockWire::Image {
        data: tiny_png_base64(),
        mime_type: "image/png".to_string(),
    };
    assert!(reject_disallowed_wire_blocks(&[block.clone()], &caps(false, false)).is_err());
    let payload = build_prompt_payload("photo", &[block], &caps(true, false)).unwrap();
    assert!(matches!(&payload.blocks[1], ContentBlock::Image(_)));
}

#[test]
fn embedded_resource_requires_advertised_capability() {
    let block = PromptContentBlockWire::Resource {
        uri: "file:///tmp/readme.txt".to_string(),
        mime_type: Some("text/plain".to_string()),
        text: Some("hello".to_string()),
        blob: None,
    };
    assert!(reject_disallowed_wire_blocks(&[block.clone()], &caps(false, false)).is_err());
    let payload = build_prompt_payload("context", &[block], &caps(false, true)).unwrap();
    assert!(matches!(&payload.blocks[1], ContentBlock::Resource(_)));
}

#[test]
fn prompt_capability_descriptor_reflects_initialize_response() {
    let descriptor = crate::adapters::web_session_acp::prompt_capability_descriptor(
        &PromptCapabilities::new().image(true).embedded_context(true),
    );
    assert!(descriptor.image);
    assert!(descriptor.embedded_context);
}

#[test]
fn bounded_frame_limit_is_8_mib() {
    assert_eq!(MAX_PROMPT_FRAME_BYTES, 8 * 1024 * 1024);
    assert_eq!(MAX_IMAGE_BLOCKS, 8);
}

#[test]
fn oversized_prompt_frame_is_rejected() {
    let block = PromptContentBlockWire::Image {
        data: "a".repeat(MAX_PROMPT_FRAME_BYTES),
        mime_type: "image/jpeg".to_string(),
    };
    let err = validate_prompt_content("", &[block], &caps(true, false)).unwrap_err();
    assert!(err.contains("frame too large") || err.contains("frame budget"));
}

#[test]
fn invalid_image_base64_is_rejected() {
    let block = PromptContentBlockWire::Image {
        data: "!!!not-base64!!!".to_string(),
        mime_type: "image/png".to_string(),
    };
    let err = validate_prompt_content("photo", &[block], &caps(true, false)).unwrap_err();
    assert!(err.contains("valid base64"));
}

#[test]
fn unsupported_image_mime_is_rejected() {
    let png =
        base64::engine::general_purpose::STANDARD.encode([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A]);
    let block = PromptContentBlockWire::Image {
        data: png,
        mime_type: "image/bmp".to_string(),
    };
    let err = validate_prompt_content("photo", &[block], &caps(true, false)).unwrap_err();
    assert!(err.contains("unsupported image mimeType"));
}

#[test]
fn too_many_image_blocks_is_rejected() {
    let png = base64::engine::general_purpose::STANDARD
        .encode([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    let blocks: Vec<_> = (0..=MAX_IMAGE_BLOCKS)
        .map(|_| PromptContentBlockWire::Image {
            data: png.clone(),
            mime_type: "image/png".to_string(),
        })
        .collect();
    let err = validate_prompt_content("", &blocks, &caps(true, false)).unwrap_err();
    assert!(err.contains("image blocks"));
}

#[test]
fn image_mime_must_match_bytes() {
    let jpeg = base64::engine::general_purpose::STANDARD.encode([0xFF, 0xD8, 0xFF, 0xE0]);
    let block = PromptContentBlockWire::Image {
        data: jpeg,
        mime_type: "image/png".to_string(),
    };
    let err = validate_prompt_content("photo", &[block], &caps(true, false)).unwrap_err();
    assert!(err.contains("do not match mimeType"));
}

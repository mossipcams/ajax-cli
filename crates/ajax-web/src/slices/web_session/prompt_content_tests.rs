//! Prompt content validation and transcript shaping.

use super::prompt_content::{
    build_prompt_payload, default_prompt_capabilities, reject_disallowed_wire_blocks,
    PromptContentBlockWire,
};
use crate::adapters::web_session_acp::PromptCapabilityDescriptor;
use agent_client_protocol::schema::v1::{ContentBlock, PromptCapabilities};

fn caps(image: bool, embedded_context: bool) -> PromptCapabilityDescriptor {
    PromptCapabilityDescriptor {
        image,
        embedded_context,
    }
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
fn image_requires_advertised_capability() {
    let block = PromptContentBlockWire::Image {
        data: "aGVsbG8=".to_string(),
        mime_type: "image/png".to_string(),
    };
    assert!(reject_disallowed_wire_blocks(&[block.clone()], &caps(false, false)).is_err());
    let payload = build_prompt_payload("photo", &[block], &caps(true, false)).unwrap();
    assert!(matches!(&payload.blocks[1], ContentBlock::Image(_)));
}

// ajax-cli#1110: an advertised image is a complete prompt without caption text.
#[test]
fn attachment_only_image_builds_payload_without_empty_text_block() {
    let block = PromptContentBlockWire::Image {
        data: "aGVsbG8=".to_string(),
        mime_type: "image/png".to_string(),
    };
    let payload = build_prompt_payload("   ", &[block], &caps(true, false)).unwrap();

    assert_eq!(payload.transcript_text, "[attached: image (png)]");
    assert_eq!(payload.blocks.len(), 1);
    assert!(matches!(&payload.blocks[0], ContentBlock::Image(_)));

    let error = build_prompt_payload("   ", &[], &caps(true, false)).unwrap_err();
    assert_eq!(error, "prompt text or content is required");
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

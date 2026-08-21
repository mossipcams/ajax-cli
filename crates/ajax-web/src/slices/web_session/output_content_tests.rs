use super::output_content::{
    extract_output_block, extract_tool_content_item, map_output_block, map_output_block_to_tool,
    OutputContentBlockWire,
};
use super::ToolContent;
use agent_client_protocol::schema::v1::{
    ContentBlock, EmbeddedResource, EmbeddedResourceResource, ImageContent, ResourceLink,
    TextResourceContents,
};

#[test]
fn map_image_prefers_uri_over_data_on_wire() {
    let block = ContentBlock::Image(
        ImageContent::new("aGVsbG8=", "image/png").uri("https://example.com/screenshot.png"),
    );
    assert_eq!(
        map_output_block(&block),
        Some(OutputContentBlockWire::Image {
            mime_type: "image/png".to_string(),
            uri: Some("https://example.com/screenshot.png".to_string()),
            data: None,
        })
    );
}

#[test]
fn map_image_keeps_data_when_no_uri() {
    let block = ContentBlock::Image(ImageContent::new("aGVsbG8=", "image/png"));
    assert_eq!(
        map_output_block(&block),
        Some(OutputContentBlockWire::Image {
            mime_type: "image/png".to_string(),
            uri: None,
            data: Some("aGVsbG8=".to_string()),
        })
    );
}

#[test]
fn map_resource_link_and_embedded_resource() {
    let link = ContentBlock::ResourceLink(ResourceLink::new("README.md", "file:///README.md"));
    assert!(matches!(
        map_output_block(&link),
        Some(OutputContentBlockWire::ResourceLink { .. })
    ));

    let embedded = ContentBlock::Resource(EmbeddedResource::new(
        EmbeddedResourceResource::TextResourceContents(
            TextResourceContents::new("hello", "file:///notes.txt").mime_type("text/plain"),
        ),
    ));
    assert_eq!(
        map_output_block(&embedded),
        Some(OutputContentBlockWire::Resource {
            uri: "file:///notes.txt".to_string(),
            mime_type: Some("text/plain".to_string()),
            text: Some("hello".to_string()),
            blob: None,
        })
    );
}

#[test]
fn map_tool_content_skips_terminal_and_maps_image() {
    let terminal = serde_json::json!({ "type": "terminal", "terminalId": "term-1" });
    assert!(extract_tool_content_item(&terminal).is_none());

    let image = serde_json::json!({
        "type": "content",
        "content": {
            "type": "image",
            "mimeType": "image/png",
            "uri": "https://example.com/a.png"
        }
    });
    assert_eq!(
        extract_tool_content_item(&image),
        Some(ToolContent::Image {
            mime_type: "image/png".to_string(),
            uri: Some("https://example.com/a.png".to_string()),
            data: None,
        })
    );
}

#[test]
fn map_typed_tool_image_block() {
    let block = ContentBlock::Image(ImageContent::new("ZGF0YQ==", "image/jpeg"));
    assert_eq!(
        map_output_block_to_tool(&block),
        Some(ToolContent::Image {
            mime_type: "image/jpeg".to_string(),
            uri: None,
            data: Some("ZGF0YQ==".to_string()),
        })
    );
}

#[test]
fn extract_untyped_resource_link() {
    let value = serde_json::json!({
        "type": "resource_link",
        "name": "diagram.png",
        "uri": "file:///tmp/diagram.png",
        "mimeType": "image/png"
    });
    assert_eq!(
        extract_output_block(&value),
        Some(OutputContentBlockWire::ResourceLink {
            name: "diagram.png".to_string(),
            uri: "file:///tmp/diagram.png".to_string(),
            mime_type: Some("image/png".to_string()),
            title: None,
            description: None,
        })
    );
}

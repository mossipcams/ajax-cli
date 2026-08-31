//! Live ACP prompt capability capture and snapshot exposure.

use super::prompt_content::PromptContentBlockWire;
use super::test_support::{fake_acp_fixture, scratch_dir, BlockingSessionDirectory};
use super::{apply_client_message, SessionClientMessage, SessionServerEvent};
use crate::adapters::web_session_acp::{
    prompt_capability_descriptor, with_test_acp_extra_args, with_test_acp_program, AcpStdioClient,
    PromptCapabilityDescriptor,
};
use agent_client_protocol::schema::v1::PromptCapabilities;
use ajax_core::models::AgentClient;
use base64::Engine;

fn tiny_png_base64() -> String {
    base64::engine::general_purpose::STANDARD
        .encode([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])
}

#[test]
fn attach_snapshot_includes_initialize_prompt_capabilities() {
    let dir = scratch_dir("attach-prompt-caps");
    let handle = "web/attach-prompt-caps";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--prompt-capabilities"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            let rt = tokio::runtime::Runtime::new().unwrap();
            let attach = rt.block_on(directory.inner().attach_snapshot(
                handle,
                "auto".to_string(),
                None,
            ));
            assert_eq!(
                attach.snapshot.prompt_capabilities,
                Some(PromptCapabilityDescriptor {
                    image: true,
                    embedded_context: true,
                })
            );
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn attach_without_advertised_prompt_capabilities_exposes_defaults() {
    let dir = scratch_dir("prompt-caps-default");
    let handle = "web/prompt-caps-default";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let attach = rt.block_on(directory.inner().attach_snapshot(
            handle,
            "auto".to_string(),
            None,
        ));
        assert_eq!(
            attach.snapshot.prompt_capabilities,
            Some(PromptCapabilityDescriptor {
                image: false,
                embedded_context: false,
            })
        );
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn rich_prompt_records_attachment_summary_without_base64() {
    let dir = scratch_dir("rich-prompt");
    let handle = "web/rich-prompt";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    let png_data = tiny_png_base64();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--prompt-capabilities"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            let rt = tokio::runtime::Runtime::new().unwrap();
            let mut generation = 0;
            rt.block_on(apply_client_message(
                directory.inner(),
                handle,
                &dir,
                SessionClientMessage::Prompt {
                    text: "see this".to_string(),
                    content_blocks: vec![
                        PromptContentBlockWire::ResourceLink {
                            name: "notes.md".to_string(),
                            uri: "file:///tmp/notes.md".to_string(),
                            mime_type: Some("text/markdown".to_string()),
                            title: None,
                            description: None,
                        },
                        PromptContentBlockWire::Image {
                            data: png_data.clone(),
                            mime_type: "image/png".to_string(),
                        },
                    ],
                    client_message_id: "cm-rich".to_string(),
                },
                &mut generation,
                None,
            ))
            .expect("prompt");
            directory.pump(handle);
            let (events, _) = directory.read_from(handle, 0);
            assert!(events.iter().any(|event| matches!(
                event,
                SessionServerEvent::Message { role, text, .. }
                    if role == "user"
                        && text.contains("[attached:")
                        && !text.contains(&png_data)
            )));
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn spawn_report_reflects_initialize_prompt_capabilities() {
    let dir = scratch_dir("spawn-prompt-caps");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--prompt-capabilities"], || {
            let (_client, report) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn");
            assert_eq!(
                report.prompt_capabilities,
                prompt_capability_descriptor(
                    &PromptCapabilities::new().image(true).embedded_context(true),
                )
            );
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

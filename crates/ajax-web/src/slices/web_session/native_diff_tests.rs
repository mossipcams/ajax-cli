//! End-to-end proof that a Claude `Write` tool call reaches the wire with its
//! diff block and keeps it after the later result update: the transcript is
//! merged per call id the way the browser projection does (the last non-empty
//! `content` wins), and the settled content must contain both the diff and the
//! tool result text.

use std::path::Path;
use std::time::Duration;

use ajax_core::models::AgentClient;

use crate::adapters::web_session_acp::with_test_claude_sdk_module;

use super::test_support::{self, BlockingSessionDirectory};
use super::{SessionServerEvent, ToolContent};

const HANDLE: &str = "web/native-diff-write";

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn claude_write_tool_call_keeps_its_diff_after_the_result_update() {
    let state_dir = test_support::scratch_dir("native-diff-write");
    with_test_claude_sdk_module(
        &manifest_dir().join("tests/fixtures/fake_claude_sdk.mjs"),
        || {
            let worktree = state_dir.join("wt");
            std::fs::create_dir_all(&worktree).expect("create worktree dir");
            let dir = BlockingSessionDirectory::new(state_dir);
            dir.acquire(HANDLE, &worktree, "auto", AgentClient::Claude)
                .expect("acquire must succeed");
            dir.submit_prompt_with_id(HANDLE, "p1".into(), "TOOL go".to_string())
                .expect("submit p1");

            // Wait until the Write call settles as completed in the transcript.
            test_support::pump_until(&dir, HANDLE, Duration::from_secs(20), |events| {
                events.iter().any(|event| {
                    matches!(event, SessionServerEvent::ToolCall { status, .. } if status == "completed")
                })
            });

            let (transcript, _cursor) = dir.read_from(HANDLE, 0);

            // Merge per call id the way the browser projection does: the last
            // non-empty `content` wins.
            let mut merged: std::collections::BTreeMap<String, Vec<ToolContent>> =
                std::collections::BTreeMap::new();
            for event in &transcript {
                if let SessionServerEvent::ToolCall {
                    call_id, content, ..
                } = event
                {
                    if !content.is_empty() {
                        merged.insert(call_id.clone(), content.clone());
                    }
                }
            }

            let final_content = merged
                .values()
                .find(|content| {
                    content
                        .iter()
                        .any(|item| matches!(item, ToolContent::Diff { path, .. } if path == "x"))
                })
                .expect("the Write call must reach the wire with its diff block");

            assert!(
                final_content.contains(&ToolContent::Diff {
                    path: "x".to_string(),
                    old_text: None,
                    new_text: "x".to_string()
                }),
                "the settled content must keep the Write diff; got {final_content:?}"
            );
            assert!(
                final_content.iter().any(|item| {
                    matches!(item, ToolContent::Text { text } if text == "wrote x")
                }),
                "the settled content must carry the tool result text; got {final_content:?}"
            );
        },
    );
}

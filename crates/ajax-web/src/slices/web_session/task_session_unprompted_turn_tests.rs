use std::path::{Path, PathBuf};
use std::time::Duration;

use ajax_core::models::AgentClient;

use crate::adapters::web_session_acp::with_test_claude_sdk_module;

use super::test_support::{self, BlockingSessionDirectory};
use super::SessionServerEvent;

const HANDLE: &str = "web/claude-wakeup";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_claude_sdk.mjs")
}

fn agent_message_texts(events: &[SessionServerEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|event| match event {
            SessionServerEvent::Message { role, text, .. } if role == "agent" => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn unprompted_wakeup_turns_stay_separate_issue_1559() {
    with_test_claude_sdk_module(&fixture(), || {
        let state_dir = test_support::scratch_dir("claude-wakeup");
        let dir = BlockingSessionDirectory::new(state_dir.clone());
        let worktree = state_dir.join("wt");
        std::fs::create_dir_all(&worktree).expect("create worktree dir");
        dir.acquire(HANDLE, &worktree, "auto", AgentClient::Claude)
            .expect("acquire must succeed");
        dir.submit_prompt_with_id(HANDLE, "p1".into(), "WAKEUP go".into())
            .expect("submit p1");

        test_support::pump_until(&dir, HANDLE, Duration::from_secs(10), |events| {
            events.iter().any(|event| {
                matches!(
                    event,
                    SessionServerEvent::Message { role, text, .. }
                        if role == "agent" && text == "C."
                )
            })
        });

        let stored =
            crate::adapters::web_session_store::load::<SessionServerEvent>(&state_dir, HANDLE);
        let messages: Vec<(&str, &str)> = stored
            .events
            .iter()
            .filter_map(|event| match event {
                SessionServerEvent::Message {
                    role,
                    text,
                    item_id,
                    ..
                } if role == "agent" => Some((text.as_str(), item_id.as_str())),
                _ => None,
            })
            .collect();
        let texts: Vec<&str> = messages.iter().map(|(text, _)| *text).collect();
        assert_eq!(
            texts,
            ["A.", "B.", "C."],
            "{:?}",
            agent_message_texts(&stored.events)
        );
        let mut item_ids: Vec<&str> = messages.iter().map(|(_, id)| *id).collect();
        item_ids.sort_unstable();
        item_ids.dedup();
        assert_eq!(
            item_ids.len(),
            3,
            "each wakeup turn needs its own item: {messages:?}"
        );

        let b_index = stored
            .events
            .iter()
            .position(
                |event| matches!(event, SessionServerEvent::Message { text, .. } if text == "B."),
            )
            .expect("B. message");
        let c_index = stored
            .events
            .iter()
            .position(
                |event| matches!(event, SessionServerEvent::Message { text, .. } if text == "C."),
            )
            .expect("C. message");
        assert!(stored.events[b_index..c_index]
            .iter()
            .any(|event| matches!(event, SessionServerEvent::TurnEnd { .. })));

        let _ = std::fs::remove_dir_all(&state_dir);
    });
}

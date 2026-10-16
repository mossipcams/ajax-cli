//! Claude SDK elicitation regression tests: an `onElicitation` call from the
//! fake SDK must reach the browser as an elicitation request, and both an
//! accept answer and a turn cancel must let the waiting turn finish.

use std::path::{Path, PathBuf};
use std::time::Duration;

use ajax_core::models::AgentClient;

use crate::adapters::web_session_acp::with_test_claude_sdk_module;

use super::test_support::{self, BlockingSessionDirectory};
use super::SessionServerEvent;

const HANDLE: &str = "web/claude-elicit";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_claude_sdk.mjs")
}

/// Submit the `ELICIT` prompt and pump until the sidecar's elicitation
/// request is published to the browser. Returns its request id.
fn acquire_elicitation_request(dir: &BlockingSessionDirectory) -> String {
    let request_id = std::cell::RefCell::new(None);
    test_support::pump_until(dir, HANDLE, Duration::from_secs(10), |events| {
        if request_id.borrow().is_none() {
            *request_id.borrow_mut() = events.iter().find_map(|event| match event {
                SessionServerEvent::ElicitationRequest { request_id, .. } => {
                    Some(request_id.clone())
                }
                _ => None,
            });
        }
        request_id.borrow().is_some()
    });
    request_id.into_inner().expect("elicitation request id")
}

fn setup_and_prompt() -> (std::path::PathBuf, BlockingSessionDirectory) {
    let state_dir = test_support::scratch_dir("claude-elicit");
    let dir = BlockingSessionDirectory::new(state_dir.clone());
    let worktree = state_dir.join("wt");
    std::fs::create_dir_all(&worktree).expect("create worktree dir");
    dir.acquire(HANDLE, &worktree, "auto", AgentClient::Claude)
        .expect("acquire must succeed");
    dir.submit_prompt_with_id(HANDLE, "p1".into(), "ELICIT go".into())
        .expect("submit p1");
    (state_dir, dir)
}

#[test]
fn claude_elicitation_accept_finishes_the_turn() {
    with_test_claude_sdk_module(&fixture(), || {
        let (state_dir, dir) = setup_and_prompt();
        let request_id = acquire_elicitation_request(&dir);

        // Answering the elicitation must let the waiting turn finish with
        // the accepted content.
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(dir.inner().answer_elicitation(
            HANDLE,
            &request_id,
            "accept",
            Some(serde_json::json!({ "name": "Ada" })),
        ))
        .expect("answer elicitation");

        test_support::pump_until(&dir, HANDLE, Duration::from_secs(10), |events| {
            events.iter().any(|event| {
                matches!(
                    event,
                    SessionServerEvent::Message { role, text, .. }
                        if role == "agent" && text.contains(r#"elicit:accept:{"name":"Ada"}"#)
                )
            })
        });

        let _ = std::fs::remove_dir_all(&state_dir);
    });
}

#[test]
fn claude_cancel_resolves_a_pending_elicitation() {
    with_test_claude_sdk_module(&fixture(), || {
        let (state_dir, dir) = setup_and_prompt();
        acquire_elicitation_request(&dir);

        // Cancelling the turn must resolve the pending elicitation.
        dir.cancel(HANDLE, false).expect("cancel");

        test_support::pump_until(&dir, HANDLE, Duration::from_secs(10), |events| {
            events.iter().any(|event| {
                matches!(
                    event,
                    SessionServerEvent::Message { role, text, .. }
                        if role == "agent" && text.contains("elicit:cancel")
                )
            })
        });

        let _ = std::fs::remove_dir_all(&state_dir);
    });
}

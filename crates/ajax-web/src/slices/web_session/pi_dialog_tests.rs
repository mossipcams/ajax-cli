//! Pi RPC extension UI dialog regression test: a `confirm` dialog emitted as
//! an `extension_ui_request` must reach the browser as an elicitation request,
//! and answering it must let the waiting turn finish. Expected to fail while
//! the host ignores `extension_ui_request` events (the fixture then never
//! settles, so no ElicitationRequest is ever recorded).

use std::path::Path;
use std::time::Duration;

use ajax_core::models::AgentClient;
use ajax_core::registry::SqliteRegistryStore;

use crate::adapters::web_session_acp::with_test_pi_program;
use crate::adapters::web_session_store::prompt_ledger::{try_load, PromptPhase};

use super::test_support::{self, BlockingSessionDirectory};
use super::SessionServerEvent;

const HANDLE: &str = "web/pi-dialog";

#[test]
fn pi_confirm_dialog_reaches_the_browser_and_answer_finishes_the_turn() {
    let state_dir = test_support::scratch_dir("pi-dialog");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_pi_rpc.js");
    with_test_pi_program(&fixture, &["--ui-confirm"], || {
        let dir = BlockingSessionDirectory::new(state_dir.clone());
        let worktree = state_dir.join("wt");
        std::fs::create_dir_all(&worktree).expect("create worktree dir");
        dir.acquire(HANDLE, &worktree, "auto", AgentClient::Pi)
            .expect("acquire must succeed");
        dir.submit_prompt_with_id(HANDLE, "p1".into(), "go".into())
            .expect("submit p1");

        // (1) The confirm dialog must surface to the browser as an
        // elicitation request. Fails today: the host ignores
        // extension_ui_request, so no ElicitationRequest is recorded.
        let request_id = std::cell::RefCell::new(None);
        test_support::pump_until(&dir, HANDLE, Duration::from_secs(5), |events| {
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
        let request_id = request_id.into_inner().expect("elicitation request id");

        // (2) Answering the dialog must finish the waiting turn.
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(dir.inner().answer_elicitation(
            HANDLE,
            &request_id,
            "accept",
            Some(serde_json::json!({ "confirmed": true })),
        ))
        .expect("answer elicitation");

        test_support::pump_until(&dir, HANDLE, Duration::from_secs(5), |events| {
            events.iter().any(|event| {
                matches!(
                    event,
                    SessionServerEvent::Message { role, text, .. }
                        if role == "agent" && text.contains("confirmed:true")
                )
            })
        });

        let store = SqliteRegistryStore::new(state_dir.join("ajax.db"));
        let ledger = try_load(&store, &state_dir, HANDLE).expect("ledger load");
        assert_eq!(
            ledger.entry("p1").map(|entry| entry.phase),
            Some(PromptPhase::Completed),
            "the prompt must no longer be in flight after the dialog is answered"
        );

        let _ = std::fs::remove_dir_all(&state_dir);
    });
}

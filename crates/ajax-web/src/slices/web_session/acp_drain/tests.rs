use super::{classify_prompt_terminal, PromptTerminalOutcome};
use serde_json::json;

#[test]
fn classify_prompt_terminal_success_from_command_result() {
    assert_eq!(
        classify_prompt_terminal(&Ok(json!({ "stopReason": "end_turn" }))),
        PromptTerminalOutcome::Success
    );
}

#[test]
fn classify_prompt_terminal_cancelled_from_cancellation_shaped_error() {
    assert_eq!(
        classify_prompt_terminal(&Err("context canceled".to_string())),
        PromptTerminalOutcome::Cancelled
    );
}

#[test]
fn classify_prompt_terminal_failed_from_genuine_error() {
    assert_eq!(
        classify_prompt_terminal(&Err("prompt failed".to_string())),
        PromptTerminalOutcome::Failed
    );
}

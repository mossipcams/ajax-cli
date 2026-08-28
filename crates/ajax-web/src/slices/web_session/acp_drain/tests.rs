use super::{classify_prompt_terminal, PromptTerminalOutcome};
use serde_json::json;

#[test]
fn classify_prompt_terminal_success_from_command_result() {
    assert_eq!(
        classify_prompt_terminal(&Ok(json!({ "stopReason": "end_turn" })), false),
        PromptTerminalOutcome::Success
    );
}

#[test]
fn classify_prompt_terminal_cancelled_when_host_requested_cancel() {
    assert_eq!(
        classify_prompt_terminal(&Err("context canceled".to_string()), true),
        PromptTerminalOutcome::Cancelled
    );
}

#[test]
fn classify_prompt_terminal_failed_from_unsolicited_cancellation_shaped_error() {
    assert_eq!(
        classify_prompt_terminal(
            &Err(
                "RetriableError: [canceled] http/2 stream closed with error code CANCEL (0x8)"
                    .to_string()
            ),
            false
        ),
        PromptTerminalOutcome::Failed
    );
}

#[test]
fn classify_prompt_terminal_failed_from_genuine_error() {
    assert_eq!(
        classify_prompt_terminal(&Err("prompt failed".to_string()), false),
        PromptTerminalOutcome::Failed
    );
}

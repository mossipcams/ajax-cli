//! Structural pane wait hints for capability-gated fallback.
//!
//! Visible-pane text is weak evidence. This module only recognizes idle
//! question prompts and permission menus anchored to the screen bottom.
//! It never classifies busy chrome or stream-json activity.

use crate::{
    agent_capability::{profile_for_agent_client, CapabilityFact},
    live::{LiveObservation, LiveStatusKind},
    models::AgentClient,
};

/// Prompt window for bottom-anchored chrome recognition.
const PROMPT_WINDOW: usize = 10;

/// A weak wait hint derived from visible pane chrome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaneWaitHint {
    WaitingQuestion,
    WaitingPermission,
}

/// Recognize a wait hint from visible-pane chrome only.
///
/// Returns `None` for empty panes, busy indicators, and stream-json events.
pub fn recognize_wait_hint(agent: AgentClient, visible_pane: &str) -> Option<PaneWaitHint> {
    let trimmed = visible_pane.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lines = meaningful_lines(trimmed);
    recognize_prompt(agent, &lines).map(|hint| match hint {
        RawPromptHint::IdlePrompt => PaneWaitHint::WaitingQuestion,
        RawPromptHint::ApprovalPrompt => PaneWaitHint::WaitingPermission,
    })
}

/// Capability-gated pane wait observation for refresh fallback.
///
/// Returns `None` when the agent profile supplies native wait evidence or the
/// pane chrome does not match an allowed wait hint.
pub fn maybe_pane_wait(agent: AgentClient, visible_pane: &str) -> Option<LiveObservation> {
    let hint = recognize_wait_hint(agent, visible_pane)?;
    let profile = profile_for_agent_client(agent);
    match hint {
        PaneWaitHint::WaitingPermission
            if profile.allows_pane_fallback(CapabilityFact::PermissionWait) =>
        {
            Some(LiveObservation::new(
                LiveStatusKind::WaitingForApproval,
                "waiting for approval",
            ))
        }
        PaneWaitHint::WaitingQuestion
            if profile.allows_pane_fallback(CapabilityFact::QuestionWait) =>
        {
            Some(LiveObservation::new(
                LiveStatusKind::WaitingForInput,
                "waiting for input",
            ))
        }
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RawPromptHint {
    IdlePrompt,
    ApprovalPrompt,
}

fn meaningful_lines(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect()
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn bottom_lines<'a>(lines: &'a [&'a str], window: usize) -> &'a [&'a str] {
    &lines[lines.len().saturating_sub(window)..]
}

type PromptRecognizer = fn(&[&str]) -> Option<RawPromptHint>;

fn recognize_prompt(agent: AgentClient, lines: &[&str]) -> Option<RawPromptHint> {
    let (primary, fallback): (PromptRecognizer, PromptRecognizer) = match agent {
        AgentClient::Claude => (recognize_claude_prompt, recognize_codex_prompt),
        AgentClient::Codex => (recognize_codex_prompt, recognize_claude_prompt),
        AgentClient::Other => (recognize_claude_prompt, recognize_codex_prompt),
    };
    primary(lines).or_else(|| fallback(lines))
}

fn recognize_claude_prompt(lines: &[&str]) -> Option<RawPromptHint> {
    let bottom = bottom_lines(lines, PROMPT_WINDOW);

    if claude_permission_menu(bottom) {
        return Some(RawPromptHint::ApprovalPrompt);
    }

    if lines
        .last()
        .is_some_and(|line| matches!(line.trim(), "❯" | ">"))
    {
        return Some(RawPromptHint::IdlePrompt);
    }

    let has_bare_prompt = bottom.iter().any(|line| matches!(line.trim(), "❯" | ">"));
    let has_strong_chrome = bottom.iter().any(|line| is_strong_claude_chrome_line(line));
    if has_bare_prompt && has_strong_chrome {
        return Some(RawPromptHint::IdlePrompt);
    }

    None
}

fn is_strong_claude_chrome_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    if lower.contains("bypass permissions") || lower.contains("shift+tab") {
        return true;
    }

    let trimmed = line.trim();
    trimmed.contains('│')
        && (trimmed.contains('%') || trimmed.contains('█') || trimmed.contains('░'))
}

fn claude_permission_menu(bottom: &[&str]) -> bool {
    let has_choice_marker = bottom.iter().any(|line| {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('❯') {
            return false;
        }
        !trimmed.trim_start_matches('❯').trim().is_empty()
    });
    let has_cue = bottom.iter().any(|line| {
        let lower = line.to_ascii_lowercase();
        if lower.contains("shift+tab") || lower.contains("bypass permissions") {
            return false;
        }
        contains_any(
            &lower,
            &[
                "run this command?",
                "do you want",
                "allow",
                "approve",
                "permission",
                "proceed?",
                "esc to cancel",
            ],
        )
    });
    has_choice_marker && has_cue
}

fn recognize_codex_prompt(lines: &[&str]) -> Option<RawPromptHint> {
    let bottom = bottom_lines(lines, PROMPT_WINDOW);

    let has_composer = bottom.iter().any(|line| line.starts_with('›'));
    let has_model_line = lines
        .last()
        .is_some_and(|line| line.starts_with("gpt-") && line.contains("~/"));

    if has_composer && has_model_line {
        return Some(RawPromptHint::IdlePrompt);
    }

    None
}

pub(crate) fn profile_allows_any_pane_wait_fallback(agent: AgentClient) -> bool {
    let profile = profile_for_agent_client(agent);
    profile.allows_pane_fallback(CapabilityFact::PermissionWait)
        || profile.allows_pane_fallback(CapabilityFact::QuestionWait)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_permission_chrome_is_waiting_permission() {
        let pane = "Do you want to run this command?\n\n❯ 1. Yes\n  2. No\n\nEsc to cancel";
        assert_eq!(
            recognize_wait_hint(AgentClient::Claude, pane),
            Some(PaneWaitHint::WaitingPermission)
        );
    }

    #[test]
    fn cursor_other_idle_prompt_is_waiting_question() {
        let pane = "some earlier output\nmore output\n❯";
        assert_eq!(
            recognize_wait_hint(AgentClient::Other, pane),
            Some(PaneWaitHint::WaitingQuestion)
        );
    }

    #[test]
    fn gated_fallback_skips_when_claude_has_native_wait() {
        let pane = "Do you want to run this command?\n\n❯ 1. Yes\n  2. No\n\nEsc to cancel";
        assert_eq!(maybe_pane_wait(AgentClient::Claude, pane), None);
    }

    #[test]
    fn busy_footer_does_not_produce_wait_hint() {
        let pane = "❯ fix the tests\n\n✶ Cogitating… (12s · ↑ 1.2k tokens · esc to interrupt)";
        assert_eq!(recognize_wait_hint(AgentClient::Claude, pane), None);
    }

    #[test]
    fn stream_json_thinking_does_not_produce_wait_hint() {
        let pane = "{\"type\":\"thinking\"}";
        assert_eq!(recognize_wait_hint(AgentClient::Other, pane), None);
    }
}

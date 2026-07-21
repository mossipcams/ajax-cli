//! Structural pane recognizers.
//!
//! Pane text is *weak* evidence. This module is the only place pane captures
//! are interpreted, and it can only ever produce three hints:
//!
//! - [`PaneHint::Busy`] — the agent is visibly working right now.
//! - [`PaneHint::IdlePrompt`] — the agent is visibly sitting at an input
//!   prompt.
//! - [`PaneHint::ApprovalPrompt`] — the agent is visibly showing a
//!   permission/approval choice.
//!
//! Pane text never asserts completion, failure, or stuck states (done,
//! command failed, blocked, merge conflict, CI failed, auth/rate-limit/
//! context-limit). Those belong to structured sources: the runtime wrapper
//! exit snapshot, provider hooks, provider lifecycle events, and git/`gh`
//! substrate evidence. This is the architectural fix for content-vs-chrome
//! false positives: agents routinely *write* words like "merge conflict",
//! "exit code 1", or "did you mean?" in prose while working, and keyword
//! matching cannot tell that prose apart from terminal UI.
//!
//! Every hint is positionally anchored to the visible screen bottom:
//!
//! - busy indicators must sit in the footer region (last [`FOOTER_WINDOW`]
//!   non-empty lines), where agents draw their live status line;
//! - prompt recognition is anchored to the bottom [`PROMPT_WINDOW`] lines and
//!   requires the agent's actual prompt chrome, not keywords;
//! - Cursor stream-json events are parsed as JSON (structured), newest event
//!   in the bottom region wins.
//!
//! Callers capture the *visible* pane only (no `-S` scrollback), so nothing
//! here ever classifies history.

use crate::models::AgentClient;

/// A weak, positionally-anchored hint derived from the visible pane.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaneHint {
    Busy,
    IdlePrompt,
    ApprovalPrompt,
}

/// How a hint was recognized. Structured recognition (stream-json, anchored
/// prompt chrome) earns medium confidence; bare busy chrome stays low.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Recognition {
    Structured,
    Chrome,
}

/// Busy indicators must sit in the agent's footer region: the last few
/// non-empty lines of the visible screen, where Claude and Codex draw their
/// live status line while working. Matches the historical `BUSY_WINDOW`:
/// with visible-pane-only capture this is still strictly the screen bottom.
const FOOTER_WINDOW: usize = 8;

/// Prompt and stream-json recognition looks at the bottom region of the
/// visible screen.
const PROMPT_WINDOW: usize = 10;

/// Recognize the strongest honest hint from a visible-pane capture.
///
/// Precedence: structured stream-json (newest event), then busy footer
/// (busy beats a visible composer prompt — Codex keeps its composer visible
/// while working), then approval menus, then idle prompts. The selected
/// agent's recognizer runs first; the other known agents' recognizers act as
/// a cross-check for mixed or misregistered sessions. Unknown agents get the
/// same structural cross-check — never keyword matching.
pub fn recognize_pane(agent: AgentClient, visible_pane: &str) -> Option<(PaneHint, Recognition)> {
    let trimmed = visible_pane.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lines = meaningful_lines(trimmed);

    if let Some(hint) = recognize_stream_json(&lines) {
        return Some((hint, Recognition::Structured));
    }

    if has_busy_footer(&lines) {
        return Some((PaneHint::Busy, Recognition::Chrome));
    }

    recognize_prompt(agent, &lines).map(|hint| (hint, Recognition::Structured))
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

// -- Busy footer -----------------------------------------------------------

/// A busy hint requires a live status line in the footer region. Both Claude
/// and Codex draw "esc to interrupt" while working; Claude draws a spinner
/// with a token counter; Codex draws a "Working (Ns …)" status line.
fn has_busy_footer(lines: &[&str]) -> bool {
    bottom_lines(lines, FOOTER_WINDOW).iter().any(|line| {
        let lower = line.to_ascii_lowercase();
        lower.contains("to interrupt")
            || (line.contains('…') && lower.contains("tokens)"))
            || lower.contains("working (")
            || lower.contains("codex is working")
            || lower.contains("claude is working")
    })
}

// -- Agent prompt recognition ----------------------------------------------

type PromptRecognizer = fn(&[&str]) -> Option<PaneHint>;

fn recognize_prompt(agent: AgentClient, lines: &[&str]) -> Option<PaneHint> {
    let (primary, fallback): (PromptRecognizer, PromptRecognizer) = match agent {
        AgentClient::Claude => (recognize_claude_prompt, recognize_codex_prompt),
        AgentClient::Codex => (recognize_codex_prompt, recognize_claude_prompt),
        AgentClient::Other => (recognize_claude_prompt, recognize_codex_prompt),
    };
    primary(lines).or_else(|| fallback(lines))
}

/// Claude idle prompt: a bare `❯`/`>` at the bottom, or a composer line plus
/// strong Claude chrome (status bar) in the bottom region.
fn recognize_claude_prompt(lines: &[&str]) -> Option<PaneHint> {
    let bottom = bottom_lines(lines, PROMPT_WINDOW);

    if claude_permission_menu(bottom) {
        return Some(PaneHint::ApprovalPrompt);
    }

    if lines
        .last()
        .is_some_and(|line| matches!(line.trim(), "❯" | ">"))
    {
        return Some(PaneHint::IdlePrompt);
    }

    let has_composer_line = bottom.iter().any(|line| is_claude_composer_line(line));
    let has_strong_chrome = bottom.iter().any(|line| is_strong_claude_chrome_line(line));
    if has_composer_line && has_strong_chrome {
        return Some(PaneHint::IdlePrompt);
    }

    None
}

fn is_claude_composer_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('❯') || trimmed.starts_with('>')
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

/// A Claude permission menu is a *selected choice* (`❯` followed by option
/// text) plus a permission cue, both inside the bottom region. The cue list
/// is deliberately narrow: it matches Claude's permission dialog wording, not
/// arbitrary prose questions.
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

/// Codex idle composer: a `›` composer line near the bottom plus the model/
/// cwd status line (`gpt-* … ~/…`) as the final line.
fn recognize_codex_prompt(lines: &[&str]) -> Option<PaneHint> {
    let bottom = bottom_lines(lines, PROMPT_WINDOW);

    let has_composer = bottom.iter().any(|line| line.starts_with('›'));
    let has_model_line = lines
        .last()
        .is_some_and(|line| line.starts_with("gpt-") && line.contains("~/"));

    if has_composer && has_model_line {
        return Some(PaneHint::IdlePrompt);
    }

    None
}

// -- Cursor stream-json ----------------------------------------------------

/// The newest stream-json event in the bottom region decides. Terminal
/// events (run finished/failed) short-circuit to `None` *without* scanning
/// older events: pane text never asserts completion or failure (the wrapper
/// exit snapshot owns those), and falling through to a stale `thinking`
/// event would resurrect a busy hint for a run that already ended.
fn recognize_stream_json(lines: &[&str]) -> Option<PaneHint> {
    for line in bottom_lines(lines, PROMPT_WINDOW).iter().rev() {
        match stream_json_hint(line) {
            StreamJsonHint::NotStreamJson => continue,
            StreamJsonHint::Terminal => return None,
            StreamJsonHint::Hint(hint) => return Some(hint),
        }
    }
    None
}

enum StreamJsonHint {
    /// The line is not a stream-json event; keep scanning older lines.
    NotStreamJson,
    /// The line is a stream-json event with a terminal or unrecognized
    /// outcome; the newest event decides, so stop scanning.
    Terminal,
    Hint(PaneHint),
}

fn stream_json_hint(line: &str) -> StreamJsonHint {
    let trimmed = line.trim();
    if !trimmed.starts_with('{') {
        return StreamJsonHint::NotStreamJson;
    }

    let Some(value) = serde_json::from_str::<serde_json::Value>(trimmed).ok() else {
        // A JSON-looking line that does not parse is prose, not an event.
        return StreamJsonHint::NotStreamJson;
    };
    let Some(event_type) = value.get("type").and_then(serde_json::Value::as_str) else {
        return StreamJsonHint::NotStreamJson;
    };

    match event_type.to_ascii_lowercase().as_str() {
        "system" if value.get("subtype").and_then(serde_json::Value::as_str) == Some("init") => {
            StreamJsonHint::Hint(PaneHint::Busy)
        }
        "thinking" => StreamJsonHint::Hint(PaneHint::Busy),
        "tool_call" => stream_json_tool_call_hint(&value),
        "assistant" => stream_json_assistant_hint(&value),
        "request" => StreamJsonHint::Hint(stream_json_request_hint(&value)),
        // "result" always carries a terminal run outcome. "status" carries
        // one unless it is a mid-run RUNNING/CREATING heartbeat.
        "result" => StreamJsonHint::Terminal,
        "status" => match value
            .get("status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_ascii_uppercase()
            .as_str()
        {
            "RUNNING" | "CREATING" => StreamJsonHint::Hint(PaneHint::Busy),
            _ => StreamJsonHint::Terminal,
        },
        _ => StreamJsonHint::NotStreamJson,
    }
}

fn stream_json_tool_call_hint(value: &serde_json::Value) -> StreamJsonHint {
    if let Some(status) = value.get("status").and_then(serde_json::Value::as_str) {
        return match status {
            "running" | "in_progress" => StreamJsonHint::Hint(PaneHint::Busy),
            // A finished tool call is mid-run, not run-terminal: the agent
            // typically continues, so keep scanning for a newer activity
            // event above it.
            _ => StreamJsonHint::NotStreamJson,
        };
    }

    match value.get("subtype").and_then(serde_json::Value::as_str) {
        Some("started") => StreamJsonHint::Hint(PaneHint::Busy),
        Some("completed") => StreamJsonHint::NotStreamJson,
        _ => StreamJsonHint::NotStreamJson,
    }
}

/// An assistant message that ends in a question mark is waiting on the
/// operator; anything else is mid-work output.
fn stream_json_assistant_hint(value: &serde_json::Value) -> StreamJsonHint {
    let Some(text) = stream_json_assistant_text(value) else {
        return StreamJsonHint::NotStreamJson;
    };
    if text.trim_end().ends_with('?') {
        return StreamJsonHint::Hint(PaneHint::IdlePrompt);
    }
    StreamJsonHint::Hint(PaneHint::Busy)
}

fn stream_json_request_hint(value: &serde_json::Value) -> PaneHint {
    let prompt = value
        .get("message")
        .and_then(serde_json::Value::as_str)
        .or_else(|| value.get("prompt").and_then(serde_json::Value::as_str))
        .or_else(|| value.get("text").and_then(serde_json::Value::as_str))
        .unwrap_or_default();

    if cursor_mentions_approval(prompt) {
        PaneHint::ApprovalPrompt
    } else {
        PaneHint::IdlePrompt
    }
}

fn stream_json_assistant_text(value: &serde_json::Value) -> Option<String> {
    let content = value.get("message")?.get("content")?.as_array()?;
    let text = content
        .iter()
        .filter_map(|block| {
            if block.get("type").and_then(serde_json::Value::as_str) != Some("text") {
                return None;
            }
            block.get("text").and_then(serde_json::Value::as_str)
        })
        .collect::<Vec<_>>()
        .join("");

    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn cursor_mentions_approval(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    contains_any(
        &lower,
        &[
            "approval required",
            "requires approval",
            "waiting for approval",
            "allow command",
            "proceed?",
            "do you want to proceed",
            "approve to proceed",
            "y/n",
            "[y/n]",
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::{recognize_pane, PaneHint, Recognition};
    use crate::models::AgentClient;

    fn hint(agent: AgentClient, pane: &str) -> Option<PaneHint> {
        recognize_pane(agent, pane).map(|(hint, _)| hint)
    }

    // -- True positives: anchored chrome ------------------------------------

    #[test]
    fn claude_idle_prompt_with_status_bar_is_idle() {
        let pane = "  ⎿  Done.\n\n❯\n  ⏵⏵ bypass permissions on (shift+tab to cycle)";
        assert_eq!(hint(AgentClient::Claude, pane), Some(PaneHint::IdlePrompt));
    }

    #[test]
    fn claude_bare_prompt_on_last_line_is_idle() {
        let pane = "some earlier output\nmore output\n❯";
        assert_eq!(hint(AgentClient::Claude, pane), Some(PaneHint::IdlePrompt));
    }

    #[test]
    fn claude_filled_composer_with_chrome_is_idle() {
        let pane = "done.\n\n────────────────────────────────────────\n❯\u{00a0}watch CI and tell me when it's green\n────────────────────────────────────────\n  Opus 4.8 │ ajax-pwa █░░░░░░░░░ 18%\n  ⏵⏵ bypass permissions on (shift+tab to cycle) · ← for agents\n";
        assert_eq!(hint(AgentClient::Claude, pane), Some(PaneHint::IdlePrompt));
    }

    #[test]
    fn claude_permission_menu_is_approval() {
        let pane = "Do you want to run this command?\n\n❯ 1. Yes\n  2. No\n\nEsc to cancel";
        assert_eq!(
            hint(AgentClient::Claude, pane),
            Some(PaneHint::ApprovalPrompt)
        );
    }

    #[test]
    fn claude_busy_footer_is_busy() {
        let pane = "❯ fix the tests\n\n✶ Cogitating… (12s · ↑ 1.2k tokens · esc to interrupt)";
        assert_eq!(hint(AgentClient::Claude, pane), Some(PaneHint::Busy));
    }

    #[test]
    fn claude_busy_footer_beats_stale_prompt_above() {
        let pane =
            "❯\n  ⏵⏵ bypass permissions on\n✶ Thinking… (3s · ↑ 200 tokens · esc to interrupt)";
        assert_eq!(hint(AgentClient::Claude, pane), Some(PaneHint::Busy));
    }

    #[test]
    fn codex_working_status_with_visible_composer_is_busy() {
        let pane =
            "› Improve documentation\n\n• Working (5s • esc to interrupt)\n\ngpt-5.5 high · ~/repo";
        assert_eq!(hint(AgentClient::Codex, pane), Some(PaneHint::Busy));
    }

    #[test]
    fn codex_idle_composer_is_idle() {
        let pane = "› Improve documentation\n\ngpt-5.5 high · ~/repo";
        assert_eq!(hint(AgentClient::Codex, pane), Some(PaneHint::IdlePrompt));
    }

    #[test]
    fn cursor_thinking_event_is_busy_and_structured() {
        let pane = "{\"type\":\"thinking\"}";
        assert_eq!(
            recognize_pane(AgentClient::Other, pane),
            Some((PaneHint::Busy, Recognition::Structured))
        );
    }

    #[test]
    fn cursor_tool_call_running_is_busy() {
        let pane = "{\"type\":\"tool_call\",\"status\":\"running\",\"name\":\"shell\"}";
        assert_eq!(hint(AgentClient::Other, pane), Some(PaneHint::Busy));
    }

    #[test]
    fn cursor_request_is_idle_or_approval() {
        let ask = "{\"type\":\"request\",\"message\":\"Which file should I edit?\"}";
        assert_eq!(hint(AgentClient::Other, ask), Some(PaneHint::IdlePrompt));
        let approve = "{\"type\":\"request\",\"message\":\"Allow command: rm -rf build? [y/n]\"}";
        assert_eq!(
            hint(AgentClient::Other, approve),
            Some(PaneHint::ApprovalPrompt)
        );
    }

    #[test]
    fn cursor_assistant_question_is_idle() {
        let pane = "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"Which approach do you prefer?\"}]}}";
        assert_eq!(hint(AgentClient::Other, pane), Some(PaneHint::IdlePrompt));
    }

    #[test]
    fn cursor_terminal_events_yield_no_hint() {
        let result = "{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false}";
        assert_eq!(hint(AgentClient::Other, result), None);
        let status = "{\"type\":\"status\",\"status\":\"FINISHED\"}";
        assert_eq!(hint(AgentClient::Other, status), None);
    }

    #[test]
    fn newest_stream_json_event_wins() {
        let pane = "{\"type\":\"request\",\"message\":\"Proceed?\"}\n{\"type\":\"thinking\"}";
        assert_eq!(hint(AgentClient::Other, pane), Some(PaneHint::Busy));
    }

    /// Regression: a terminal event must short-circuit, not fall through to
    /// a stale busy event above it — otherwise a finished run keeps
    /// classifying as working.
    #[test]
    fn terminal_stream_json_event_suppresses_stale_busy_events() {
        let pane = "{\"type\":\"thinking\"}\n{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false}";
        assert_eq!(hint(AgentClient::Other, pane), None);
        let pane = "{\"type\":\"tool_call\",\"status\":\"running\",\"name\":\"shell\"}\n{\"type\":\"status\",\"status\":\"FINISHED\"}";
        assert_eq!(hint(AgentClient::Other, pane), None);
    }

    #[test]
    fn assistant_statement_is_busy() {
        let pane = "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"Rerunning the suite now.\"}]}}";
        assert_eq!(hint(AgentClient::Other, pane), Some(PaneHint::Busy));
    }

    #[test]
    fn cross_agent_structural_prompts_are_recognized() {
        let claude_idle = "Here is my plan.\n\n❯";
        assert_eq!(
            hint(AgentClient::Codex, claude_idle),
            Some(PaneHint::IdlePrompt)
        );
        let claude_menu = "Do you want to proceed?\n❯ 1. Yes\n  2. No\nEsc to cancel";
        assert_eq!(
            hint(AgentClient::Codex, claude_menu),
            Some(PaneHint::ApprovalPrompt)
        );
        let codex_idle = "› Fix the tests\n\ngpt-5.5 high · ~/repo";
        assert_eq!(
            hint(AgentClient::Claude, codex_idle),
            Some(PaneHint::IdlePrompt)
        );
        assert_eq!(
            hint(AgentClient::Other, claude_idle),
            Some(PaneHint::IdlePrompt)
        );
        assert_eq!(
            hint(AgentClient::Other, codex_idle),
            Some(PaneHint::IdlePrompt)
        );
    }

    // -- False-positive regression corpus ------------------------------------
    //
    // Every case below is a real-world shape that used to flip a task to an
    // actionable status it was not in. Pane text must now stay neutral or
    // busy in all of them.

    #[test]
    fn agent_prose_about_merge_conflict_is_not_conflict_evidence() {
        let pane = "I'll fix the merge conflict in lib.rs now.\n\n✶ Thinking… (4s · ↑ 300 tokens · esc to interrupt)";
        assert_eq!(hint(AgentClient::Claude, pane), Some(PaneHint::Busy));
    }

    #[test]
    fn agent_prose_about_ci_failure_is_not_ci_evidence() {
        let pane = "The CI failed on the lint job, rerunning with a fix.\n\n• Working (8s • esc to interrupt)\ngpt-5.5 high · ~/repo";
        assert_eq!(hint(AgentClient::Codex, pane), Some(PaneHint::Busy));
    }

    #[test]
    fn command_failure_output_above_idle_prompt_is_not_failure_evidence() {
        let pane = "test result: FAILED. 2 failed\nerror: command failed with exit code 1\n\n❯\n  ⏵⏵ bypass permissions on";
        assert_eq!(hint(AgentClient::Claude, pane), Some(PaneHint::IdlePrompt));
    }

    #[test]
    fn agent_quoting_a_question_while_busy_is_not_waiting() {
        let pane = "The user asked \"did you mean the parser?\" — checking both.\n\n✶ Thinking… (2s · ↑ 100 tokens · esc to interrupt)";
        assert_eq!(hint(AgentClient::Claude, pane), Some(PaneHint::Busy));
    }

    #[test]
    fn completion_prose_is_never_done_evidence() {
        let pane =
            "All done! Tests passed and the task complete.\nsuccessfully completed the refactor";
        assert_eq!(hint(AgentClient::Claude, pane), None);
    }

    #[test]
    fn prose_with_generic_busy_words_is_not_busy_evidence() {
        let pane =
            "I am thinking about running the tests next.\nstill working on your task description";
        assert_eq!(hint(AgentClient::Claude, pane), None);
    }

    #[test]
    fn shell_prompt_is_neutral() {
        let pane = "some output\nmatt@host ~/repo % ";
        assert_eq!(hint(AgentClient::Claude, pane), None);
    }

    #[test]
    fn empty_pane_is_neutral() {
        assert_eq!(hint(AgentClient::Claude, "   \n  "), None);
    }

    #[test]
    fn busy_footer_must_be_in_footer_region() {
        // A busy-looking line scrolled well above fresh output is not a live
        // busy signal.
        let mut lines = vec!["✶ Thinking… (30s · ↑ 9k tokens · esc to interrupt)"];
        lines.extend((0..8).map(|i| match i {
            7 => "final line of fresh agent output",
            _ => "intermediate agent output line",
        }));
        let pane = lines.join("\n");
        assert_eq!(hint(AgentClient::Claude, &pane), None);
    }

    #[test]
    fn ambiguous_pane_is_neutral_not_guessy() {
        let pane = "compiling ajax-core v0.51.7\n    Finished dev profile";
        assert_eq!(hint(AgentClient::Claude, pane), None);
    }
}

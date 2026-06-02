//! Per-agent parsing of an interactive agent's captured terminal pane into a
//! structured, confidence-scored operator prompt, plus the reverse mapping from
//! an operator answer back to the exact tmux keys for that agent.
//!
//! Safety contract: an adapter may decline to understand a pane (return `None`,
//! or a `Low`-confidence prompt), but it must never confidently emit the wrong
//! key. The generic [`AgentPromptAdapter::answer_keys`] refuses to act on
//! anything but a `High`-confidence, answerable approval. This is what lets a
//! blocked agent be answered from a phone — or from a delayed notification —
//! without the risk of a misparse landing the wrong keystroke in a live
//! session.

use crate::models::AgentClient;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptKind {
    /// The agent is asking the operator to approve or choose an action.
    Approval,
    /// The agent is at a free-text composer waiting for typed input.
    FreeText,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Low,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChoiceRole {
    Affirm,
    Deny,
    Neutral,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct PromptChoice {
    pub label: String,
    /// Literal tmux keys this choice sends (e.g. `"1"` or `"y"`).
    pub keys: String,
    /// Whether to append `Enter` after the keys.
    pub submit: bool,
    pub role: ChoiceRole,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct AgentPrompt {
    pub kind: PromptKind,
    pub question: String,
    pub command: Option<String>,
    pub choices: Vec<PromptChoice>,
    pub confidence: Confidence,
    /// Stable hash of the prompt-relevant pane lines. The answer path recomputes
    /// it from a fresh capture and refuses to act on a mismatch (stale answer).
    pub fingerprint: String,
}

/// An operator's typed intent. The web layer never sees keystrokes; it sends one
/// of these and the adapter resolves the keys.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "answer", rename_all = "snake_case")]
pub enum OperatorAnswer {
    Approve,
    Deny,
    Select { index: usize },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SendKeys {
    pub keys: String,
    pub submit: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnswerError {
    /// The prompt is not a high-confidence approval (e.g. free-text composer or
    /// a low-confidence parse). The operator must escalate to the terminal.
    NotAnswerable,
    /// The requested choice has no match in the parsed prompt.
    UnknownChoice,
}

pub trait AgentPromptAdapter {
    /// Parse cleaned pane lines (ANSI-stripped, dedup-collapsed) into a prompt.
    fn parse(&self, lines: &[String]) -> Option<AgentPrompt>;

    /// Resolve an operator answer into the keys to send. Generic over agents:
    /// the keys live on the choices the adapter parsed, so the safety floor is
    /// enforced in one place.
    fn answer_keys(
        &self,
        prompt: &AgentPrompt,
        answer: &OperatorAnswer,
    ) -> Result<SendKeys, AnswerError> {
        if prompt.confidence != Confidence::High || prompt.kind != PromptKind::Approval {
            return Err(AnswerError::NotAnswerable);
        }
        let choice = match answer {
            OperatorAnswer::Approve => prompt.choices.iter().find(|c| c.role == ChoiceRole::Affirm),
            OperatorAnswer::Deny => prompt.choices.iter().find(|c| c.role == ChoiceRole::Deny),
            OperatorAnswer::Select { index } => prompt.choices.get(*index),
        }
        .ok_or(AnswerError::UnknownChoice)?;
        Ok(SendKeys {
            keys: choice.keys.clone(),
            submit: choice.submit,
        })
    }
}

/// Return the adapter for a given agent. Unknown agents get the null adapter,
/// which never recognizes a prompt (→ safe escalation).
pub fn adapter_for(agent: AgentClient) -> &'static dyn AgentPromptAdapter {
    match agent {
        AgentClient::Codex => &CodexAdapter,
        AgentClient::Claude | AgentClient::Other => &NullAdapter,
    }
}

/// Convenience: parse a pane for the given agent.
pub fn parse_prompt(agent: AgentClient, lines: &[String]) -> Option<AgentPrompt> {
    adapter_for(agent).parse(lines)
}

pub struct NullAdapter;

impl AgentPromptAdapter for NullAdapter {
    fn parse(&self, _lines: &[String]) -> Option<AgentPrompt> {
        None
    }
}

pub struct CodexAdapter;

impl AgentPromptAdapter for CodexAdapter {
    fn parse(&self, lines: &[String]) -> Option<AgentPrompt> {
        parse_numbered_approval(lines)
            .or_else(|| parse_yes_no_approval(lines))
            .or_else(|| parse_composer(lines))
    }
}

// --- Codex parsing helpers -------------------------------------------------

const APPROVAL_CUES: &[&str] = &[
    "allow",
    "approve",
    "proceed",
    "permission",
    "want to run",
    "run this command",
    "run the following",
    "may i",
];

/// Numbered selection list, e.g.:
/// ```text
///   Allow Codex to run this command?
///     cargo test --all-features
///   ❯ 1. Yes, run it
///     2. Yes, and don't ask again this session
///     3. No, and tell Codex what to do differently
/// ```
fn parse_numbered_approval(lines: &[String]) -> Option<AgentPrompt> {
    let last_opt = (0..lines.len())
        .rev()
        .find(|&i| parse_option_line(&lines[i]).is_some())?;
    let mut start = last_opt;
    while start > 0 && parse_option_line(&lines[start - 1]).is_some() {
        start -= 1;
    }
    let options: Vec<(u32, String)> = lines[start..=last_opt]
        .iter()
        .filter_map(|line| parse_option_line(line))
        .collect();
    if options.len() < 2 {
        return None;
    }
    // Numbers must be consecutive starting at 1, or this isn't a real list.
    for (index, (num, _)) in options.iter().enumerate() {
        if *num as usize != index + 1 {
            return None;
        }
    }

    // Up to a few non-empty lines above the option block carry the cue/command.
    let above: Vec<String> = lines[..start]
        .iter()
        .rev()
        .filter(|line| !line.trim().is_empty())
        .take(4)
        .cloned()
        .collect();

    let cue_line = above.iter().find(|line| {
        let lower = line.to_ascii_lowercase();
        APPROVAL_CUES.iter().any(|cue| lower.contains(cue))
    })?;
    let command = above
        .iter()
        .find_map(|line| extract_backtick(line))
        .or_else(|| {
            above
                .iter()
                .find(|line| looks_command_ish(line))
                .map(|line| line.trim().to_string())
        });

    let mut choices = Vec::with_capacity(options.len());
    let mut affirm_found = false;
    let mut deny_found = false;
    for (num, label) in &options {
        let lower = label.to_ascii_lowercase();
        let role =
            if lower.starts_with("yes") || lower.contains("approve") || lower.contains("allow") {
                affirm_found = true;
                ChoiceRole::Affirm
            } else if lower.starts_with("no")
                || lower.contains("don't")
                || lower.contains("do not")
                || lower.contains("reject")
                || lower.contains("deny")
            {
                deny_found = true;
                ChoiceRole::Deny
            } else {
                ChoiceRole::Neutral
            };
        choices.push(PromptChoice {
            label: label.clone(),
            keys: num.to_string(),
            submit: true,
            role,
        });
    }
    // An approval we can't say "yes" to is not answerable.
    if !affirm_found {
        return None;
    }
    // Codex convention: the last option is the negative ("No, and tell Codex…").
    if !deny_found {
        if let Some(last) = choices.last_mut() {
            if last.role == ChoiceRole::Neutral {
                last.role = ChoiceRole::Deny;
            }
        }
    }

    let fingerprint_value = {
        let mut parts: Vec<&str> = Vec::new();
        if let Some(command) = &command {
            parts.push(command);
        }
        for (_, label) in &options {
            parts.push(label);
        }
        fingerprint(&parts)
    };

    Some(AgentPrompt {
        kind: PromptKind::Approval,
        question: cue_line.trim().to_string(),
        command,
        choices,
        confidence: Confidence::High,
        fingerprint: fingerprint_value,
    })
}

/// Inline yes/no approval, e.g. ``Run `cargo test`? [y/n]``.
fn parse_yes_no_approval(lines: &[String]) -> Option<AgentPrompt> {
    let idx = (0..lines.len())
        .rev()
        .find(|&i| has_yes_no_token(&lines[i]))?;
    let line = &lines[idx];
    let command = extract_backtick(line);
    let choices = vec![
        PromptChoice {
            label: "Yes".to_string(),
            keys: "y".to_string(),
            submit: true,
            role: ChoiceRole::Affirm,
        },
        PromptChoice {
            label: "No".to_string(),
            keys: "n".to_string(),
            submit: true,
            role: ChoiceRole::Deny,
        },
    ];
    Some(AgentPrompt {
        kind: PromptKind::Approval,
        question: line.trim().to_string(),
        command,
        choices,
        confidence: Confidence::High,
        fingerprint: fingerprint(&[line]),
    })
}

/// Codex free-text composer: a `›`/`❯` line plus the `gpt-… ~/…` footer.
/// Always low confidence — under triage-only this carries no answerable
/// affordance, only "the agent is waiting; open the terminal".
fn parse_composer(lines: &[String]) -> Option<AgentPrompt> {
    let has_footer = lines.iter().rev().take(3).any(|line| is_codex_footer(line));
    if !has_footer {
        return None;
    }
    let composer_idx = lines.iter().rposition(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with('›') || trimmed.starts_with('❯')
    })?;
    let question = lines[..composer_idx]
        .iter()
        .rev()
        .find(|line| !is_divider(line) && !line.trim().is_empty())
        .map(|line| strip_divider_text(line))
        .unwrap_or_else(|| "Codex is waiting for input".to_string());

    Some(AgentPrompt {
        kind: PromptKind::FreeText,
        question,
        command: None,
        choices: Vec::new(),
        confidence: Confidence::Low,
        fingerprint: fingerprint(&[&lines[composer_idx]]),
    })
}

fn parse_option_line(line: &str) -> Option<(u32, String)> {
    let stripped = strip_leading_marker(line);
    let digits: String = stripped.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    let num: u32 = digits.parse().ok()?;
    let rest = stripped[digits.len()..].trim_start();
    let rest = rest.strip_prefix('.').or_else(|| rest.strip_prefix(')'))?;
    let label = rest.trim();
    if label.is_empty() {
        return None;
    }
    Some((num, label.to_string()))
}

fn strip_leading_marker(line: &str) -> &str {
    line.trim_start_matches(|c: char| {
        c.is_whitespace() || matches!(c, '❯' | '>' | '›' | '•' | '·' | '*' | '◉' | '○')
    })
}

fn has_yes_no_token(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("[y/n]")
        || lower.contains("(y/n)")
        || lower.contains(" y/n")
        || lower.ends_with("y/n")
        || lower.contains("yes/no")
}

fn extract_backtick(line: &str) -> Option<String> {
    let start = line.find('`')?;
    let rest = &line[start + 1..];
    let end = rest.find('`')?;
    let command = rest[..end].trim();
    if command.is_empty() {
        None
    } else {
        Some(command.to_string())
    }
}

fn looks_command_ish(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.ends_with('?') {
        return false;
    }
    if trimmed.starts_with('$') {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    const PREFIXES: &[&str] = &[
        "cargo ", "npm ", "npx ", "pnpm ", "yarn ", "git ", "./", "sh ", "bash ", "rm ", "mv ",
        "cp ", "python", "node ", "make ", "docker ", "curl ", "grep ", "sed ",
    ];
    PREFIXES.iter().any(|prefix| lower.starts_with(prefix))
}

fn is_codex_footer(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("gpt-") && trimmed.contains("~/")
}

fn is_divider(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|c| matches!(c, '─' | '—' | '-' | '═' | '_' | ' '))
}

fn strip_divider_text(line: &str) -> String {
    line.trim()
        .trim_matches(|c: char| matches!(c, '─' | '—' | '═' | ' '))
        .trim()
        .to_string()
}

/// FNV-1a over the joined parts. Deterministic within a process (which is all the
/// stale-answer guard needs) and stable across builds.
fn fingerprint(parts: &[&str]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for byte in part.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= u64::from(b'\n');
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<String> {
        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn codex_numbered_approval_parses_command_and_roles() {
        let pane = lines(
            "Allow Codex to run this command?
             cargo test --all-features
             ❯ 1. Yes, run it
               2. Yes, and don't ask again this session
               3. No, and tell Codex what to do differently",
        );

        let prompt = CodexAdapter.parse(&pane).expect("numbered approval");

        assert_eq!(prompt.kind, PromptKind::Approval);
        assert_eq!(prompt.confidence, Confidence::High);
        assert_eq!(prompt.command.as_deref(), Some("cargo test --all-features"));
        assert_eq!(prompt.choices.len(), 3);
        assert_eq!(prompt.choices[0].role, ChoiceRole::Affirm);
        assert_eq!(prompt.choices[0].keys, "1");
        assert_eq!(prompt.choices[2].role, ChoiceRole::Deny);
        assert_eq!(prompt.choices[2].keys, "3");
    }

    #[test]
    fn numbered_approval_approve_selects_option_one_not_y() {
        let pane = lines(
            "Allow Codex to run this command?
             cargo test
             ❯ 1. Yes, run it
               2. No, and tell Codex what to do differently",
        );
        let prompt = CodexAdapter.parse(&pane).unwrap();

        let approve = CodexAdapter
            .answer_keys(&prompt, &OperatorAnswer::Approve)
            .unwrap();
        assert_eq!(approve.keys, "1");
        assert!(approve.submit);

        let deny = CodexAdapter
            .answer_keys(&prompt, &OperatorAnswer::Deny)
            .unwrap();
        assert_eq!(deny.keys, "2");
    }

    #[test]
    fn numbered_block_without_approval_cue_is_not_a_prompt() {
        let pane = lines(
            "Here is the plan:
               1. Refactor the parser
               2. Add tests
               3. Ship it",
        );
        assert!(CodexAdapter.parse(&pane).is_none());
    }

    #[test]
    fn codex_yes_no_approval_maps_to_y_and_n() {
        let pane = lines("Run `cargo test`? [y/n]");
        let prompt = CodexAdapter.parse(&pane).unwrap();

        assert_eq!(prompt.kind, PromptKind::Approval);
        assert_eq!(prompt.command.as_deref(), Some("cargo test"));
        assert_eq!(
            CodexAdapter
                .answer_keys(&prompt, &OperatorAnswer::Approve)
                .unwrap()
                .keys,
            "y"
        );
        assert_eq!(
            CodexAdapter
                .answer_keys(&prompt, &OperatorAnswer::Deny)
                .unwrap()
                .keys,
            "n"
        );
    }

    #[test]
    fn codex_composer_is_low_confidence_freetext_and_not_answerable() {
        let pane = lines(
            "─ Worked for 7m 39s ─────────────
             › Write tests for @filename
             gpt-5.4 high · ~/.ajax-dev/worktrees/x/release-please",
        );
        let prompt = CodexAdapter.parse(&pane).expect("composer");

        assert_eq!(prompt.kind, PromptKind::FreeText);
        assert_eq!(prompt.confidence, Confidence::Low);
        assert!(prompt.choices.is_empty());
        assert_eq!(
            CodexAdapter.answer_keys(&prompt, &OperatorAnswer::Approve),
            Err(AnswerError::NotAnswerable)
        );
    }

    #[test]
    fn garbled_pane_yields_no_prompt() {
        let pane = lines(
            "compiling crate foo
             warning: unused import
             Finished in 3.2s",
        );
        assert!(CodexAdapter.parse(&pane).is_none());
    }

    #[test]
    fn null_adapter_never_recognizes_a_prompt() {
        let pane = lines("Run `cargo test`? [y/n]");
        assert!(NullAdapter.parse(&pane).is_none());
        assert!(parse_prompt(AgentClient::Claude, &pane).is_none());
        assert!(parse_prompt(AgentClient::Codex, &pane).is_some());
    }

    #[test]
    fn fingerprint_changes_with_command() {
        let a = lines("Run `cargo test`? [y/n]");
        let b = lines("Run `rm -rf /`? [y/n]");
        let fa = CodexAdapter.parse(&a).unwrap().fingerprint;
        let fb = CodexAdapter.parse(&b).unwrap().fingerprint;
        assert_ne!(fa, fb);
    }

    #[test]
    fn operator_answer_deserializes_from_tagged_json() {
        let approve: OperatorAnswer = serde_json::from_str(r#"{"answer":"approve"}"#).unwrap();
        assert_eq!(approve, OperatorAnswer::Approve);
        let select: OperatorAnswer =
            serde_json::from_str(r#"{"answer":"select","index":2}"#).unwrap();
        assert_eq!(select, OperatorAnswer::Select { index: 2 });
    }
}

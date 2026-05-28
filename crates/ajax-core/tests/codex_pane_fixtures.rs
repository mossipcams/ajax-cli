//! Fixture tests anchored to real captured Codex panes (see
//! `tests/fixtures/codex/`). These guard the adapter against drift in the
//! actual interactive Codex TUI, not just synthetic shapes.

use ajax_core::{
    agent_prompt::{parse_prompt, Confidence, PromptKind},
    models::AgentClient,
};

/// Mirror the cleaning the live pane path applies before classification:
/// trim each line, drop blanks, collapse adjacent duplicates.
fn clean(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if out.last().is_some_and(|previous| previous == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

#[test]
fn real_codex_composer_pane_is_low_confidence_freetext() {
    let raw = include_str!("fixtures/codex/composer_idle.txt");
    let lines = clean(raw);

    let prompt =
        parse_prompt(AgentClient::Codex, &lines).expect("real Codex composer pane recognized");

    // The idle composer is waiting for input but must never be answerable from
    // the browser under triage-only — it escalates to the terminal.
    assert_eq!(prompt.kind, PromptKind::FreeText);
    assert_eq!(prompt.confidence, Confidence::Low);
    assert!(prompt.choices.is_empty());
}

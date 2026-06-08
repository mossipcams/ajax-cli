use crate::{
    adapters::{CommandRunError, CommandRunner, CommandSpec},
    agent_prompt::{self, AgentPrompt, ChoiceRole, Confidence, PromptKind},
    live::classify_pane,
    models::{AgentClient, LiveStatusKind},
};
use std::time::Duration;

const DEFAULT_WORKTRUNK_WINDOW: &str = "worktrunk";
const PANE_COMMAND_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaneSnapshot {
    pub sequence_changed: bool,
    pub lines: Vec<String>,
    pub truncated: bool,
    pub state: Option<PaneState>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaneState {
    pub kind: LiveStatusKind,
    pub summary: String,
    pub command: Option<String>,
    pub prompt: Option<String>,
    /// Answerable choices, in display order. Empty unless a high-confidence
    /// approval was recognized.
    pub choices: Vec<PaneChoice>,
    pub confidence: Option<Confidence>,
    /// Stale-answer guard fingerprint; present only when there is a structured
    /// prompt to answer.
    pub fingerprint: Option<String>,
}

/// An operator-facing choice. Carries no keystrokes — the answer path resolves
/// keys through the agent adapter so the wire never carries raw keys.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaneChoice {
    pub index: usize,
    pub label: String,
    pub role: ChoiceRole,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SendKeysOutcome {
    pub submitted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaneError {
    SessionMissing,
    InvalidKeys(String),
    CommandRun(CommandRunError),
}

pub fn snapshot(
    runner: &mut impl CommandRunner,
    session: &str,
    agent: AgentClient,
    since: Option<&[String]>,
    limit: usize,
) -> Result<PaneSnapshot, PaneError> {
    let output = runner
        .run(&capture_command(session))
        .map_err(map_tmux_error)?;
    let cleaned = clean_pane_lines(&output.stdout);
    let state = classify_state(&cleaned, agent);
    let truncated = cleaned.len() > limit;
    let lines = tail_lines(&cleaned, limit);

    if since.is_some_and(|previous| previous == lines.as_slice()) {
        return Ok(PaneSnapshot {
            sequence_changed: false,
            lines: Vec::new(),
            truncated,
            state,
        });
    }

    Ok(PaneSnapshot {
        sequence_changed: true,
        lines,
        truncated,
        state,
    })
}

fn capture_command(session: &str) -> CommandSpec {
    CommandSpec::new(
        "tmux",
        [
            "capture-pane",
            "-p",
            "-e",
            "-t",
            &format!("{session}:{DEFAULT_WORKTRUNK_WINDOW}"),
            "-S",
            "-200",
        ],
    )
    .with_timeout(PANE_COMMAND_TIMEOUT)
}

/// Re-capture the pane and parse the current structured prompt for `agent`,
/// including the keystrokes each choice maps to. The answer path uses this to
/// verify a prompt is still live (fingerprint match) before sending keys.
pub fn capture_prompt(
    runner: &mut impl CommandRunner,
    session: &str,
    agent: AgentClient,
) -> Result<Option<AgentPrompt>, PaneError> {
    let output = runner
        .run(&capture_command(session))
        .map_err(map_tmux_error)?;
    let cleaned = clean_pane_lines(&output.stdout);
    Ok(agent_prompt::parse_prompt(agent, &cleaned))
}

pub fn send_keys(
    runner: &mut impl CommandRunner,
    session: &str,
    keys: &str,
    submit: bool,
) -> Result<SendKeysOutcome, PaneError> {
    let trimmed = keys.trim();
    if trimmed.is_empty() {
        return Err(PaneError::InvalidKeys("keys must not be empty".to_string()));
    }
    if looks_like_tmux_key_token(trimmed) && !is_allowed_tmux_key_token(trimmed) {
        return Err(PaneError::InvalidKeys(format!(
            "unsupported tmux key token: {trimmed}"
        )));
    }

    let mut args = vec![
        "send-keys".to_string(),
        "-t".to_string(),
        format!("{session}:{DEFAULT_WORKTRUNK_WINDOW}"),
        keys.to_string(),
    ];
    if submit {
        args.push("Enter".to_string());
    }
    let command = CommandSpec {
        program: "tmux".to_string(),
        args,
        cwd: None,
        mode: crate::adapters::CommandMode::Capture,
        timeout: Some(PANE_COMMAND_TIMEOUT),
    };
    runner.run(&command).map_err(map_tmux_error)?;

    Ok(SendKeysOutcome { submitted: true })
}

fn map_tmux_error(error: CommandRunError) -> PaneError {
    match &error {
        CommandRunError::NonZeroExit { stderr, .. }
            if stderr_contains_missing_pane(stderr.as_str()) =>
        {
            PaneError::SessionMissing
        }
        _ => PaneError::CommandRun(error),
    }
}

fn stderr_contains_missing_pane(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    lower.contains("can't find pane")
        || lower.contains("can't find window")
        || lower.contains("can't find session")
        || lower.contains("no such pane")
}

fn is_allowed_tmux_key_token(keys: &str) -> bool {
    matches!(
        keys,
        "Enter"
            | "C-c"
            | "C-d"
            | "C-z"
            | "Up"
            | "Down"
            | "Left"
            | "Right"
            | "Tab"
            | "Escape"
            | "BSpace"
    )
}

fn looks_like_tmux_key_token(keys: &str) -> bool {
    keys.starts_with("C-")
        || matches!(
            keys,
            "Enter" | "Up" | "Down" | "Left" | "Right" | "Tab" | "Escape" | "BSpace"
        )
}

fn clean_pane_lines(stdout: &str) -> Vec<String> {
    let mut lines = Vec::new();

    for line in stdout.lines() {
        let stripped = strip_ansi(line);
        let trimmed = stripped.trim();
        if trimmed.is_empty() {
            continue;
        }
        if lines.last().is_some_and(|previous| previous == trimmed) {
            continue;
        }
        lines.push(trimmed.to_string());
    }

    lines
}

fn tail_lines(lines: &[String], limit: usize) -> Vec<String> {
    if limit == 0 {
        return Vec::new();
    }
    let start = lines.len().saturating_sub(limit);
    lines[start..].to_vec()
}

fn classify_state(lines: &[String], agent: AgentClient) -> Option<PaneState> {
    if lines.is_empty() {
        return None;
    }

    let joined = lines.join("\n");
    let mut observation = classify_pane(&joined);
    if observation.kind == LiveStatusKind::Unknown {
        observation.kind = LiveStatusKind::AgentRunning;
        observation.summary = "agent running".to_string();
    }

    let mut state = PaneState {
        kind: observation.kind,
        summary: observation.summary,
        command: None,
        prompt: None,
        choices: Vec::new(),
        confidence: None,
        fingerprint: None,
    };

    if let Some(prompt) = agent_prompt::parse_prompt(agent, lines) {
        overlay_prompt(&mut state, prompt);
    }

    Some(state)
}

/// Fold a structured agent prompt into the pane state. A high-confidence
/// approval is authoritative over the generic keyword classifier; a free-text
/// composer only contributes the prompt text (it stays non-answerable).
fn overlay_prompt(state: &mut PaneState, prompt: AgentPrompt) {
    state.confidence = Some(prompt.confidence);
    state.fingerprint = Some(prompt.fingerprint);
    match prompt.kind {
        PromptKind::Approval => {
            if prompt.confidence == Confidence::High {
                state.kind = LiveStatusKind::WaitingForApproval;
                state.summary = "waiting for approval".to_string();
            }
            state.command = prompt.command;
            state.choices = prompt
                .choices
                .iter()
                .enumerate()
                .map(|(index, choice)| PaneChoice {
                    index,
                    label: choice.label.clone(),
                    role: choice.role,
                })
                .collect();
        }
        PromptKind::FreeText => {
            state.prompt = Some(prompt.question);
        }
    }
}

/// Strip ANSI CSI escape sequences while preserving multibyte UTF-8. Operating
/// over `chars` (not raw bytes) matters: agent TUIs draw glyphs like `›`/`❯`
/// that the prompt classifier keys on, and a byte-wise strip mangles them.
fn strip_ansi(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();

    while let Some(current) = chars.next() {
        if current == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            continue;
        }
        output.push(current);
    }

    output
}

#[cfg(test)]
mod tests {
    use super::{
        send_keys, snapshot, PaneChoice, PaneSnapshot, PaneState, SendKeysOutcome,
        PANE_COMMAND_TIMEOUT,
    };
    use crate::{
        adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec},
        agent_prompt::{ChoiceRole, Confidence},
        models::{AgentClient, LiveStatusKind},
    };

    #[derive(Default)]
    struct StubRunner {
        outputs: Vec<Result<CommandOutput, CommandRunError>>,
        commands: Vec<CommandSpec>,
    }

    impl StubRunner {
        fn with_stdout(stdout: &str) -> Self {
            Self {
                outputs: vec![Ok(CommandOutput {
                    status_code: 0,
                    stdout: stdout.to_string(),
                    stderr: String::new(),
                })],
                commands: Vec::new(),
            }
        }
    }

    impl CommandRunner for StubRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.commands.push(command.clone());
            self.outputs.remove(0)
        }
    }

    #[test]
    fn snapshot_strips_ansi_collapses_adjacent_duplicates_and_limits_tail_lines() {
        let pane = "\u{1b}[31merror:\u{1b}[0m boom\n\
error: boom\n\
\n\
Running tests\n\
Running tests\n\
Do you want to proceed? [y/n]\n\
last line\n";
        let mut runner = StubRunner::with_stdout(pane);

        let snapshot = snapshot(
            &mut runner,
            "ajax-web-fix-login",
            AgentClient::Codex,
            None,
            3,
        )
        .unwrap();

        assert_eq!(
            snapshot,
            PaneSnapshot {
                sequence_changed: true,
                lines: vec![
                    "Running tests".to_string(),
                    "Do you want to proceed? [y/n]".to_string(),
                    "last line".to_string(),
                ],
                truncated: true,
                state: Some(PaneState {
                    kind: LiveStatusKind::WaitingForApproval,
                    summary: "waiting for approval".to_string(),
                    command: None,
                    prompt: None,
                    choices: vec![
                        PaneChoice {
                            index: 0,
                            label: "Yes".to_string(),
                            role: ChoiceRole::Affirm,
                        },
                        PaneChoice {
                            index: 1,
                            label: "No".to_string(),
                            role: ChoiceRole::Deny,
                        },
                    ],
                    confidence: Some(Confidence::High),
                    fingerprint: snapshot.state.as_ref().and_then(|s| s.fingerprint.clone()),
                }),
            }
        );
        assert!(snapshot
            .state
            .as_ref()
            .and_then(|s| s.fingerprint.as_ref())
            .is_some());
        assert_eq!(runner.commands.len(), 1);
        assert_eq!(runner.commands[0].timeout, Some(PANE_COMMAND_TIMEOUT));
        assert_eq!(
            runner.commands[0].args,
            vec![
                "capture-pane",
                "-p",
                "-e",
                "-t",
                "ajax-web-fix-login:worktrunk",
                "-S",
                "-200",
            ]
        );
    }

    #[test]
    fn snapshot_returns_empty_delta_when_cleaned_lines_have_not_changed_since_previous() {
        let mut runner = StubRunner::with_stdout("working\nstill working\n");

        let snapshot = snapshot(
            &mut runner,
            "ajax-web-fix-login",
            AgentClient::Codex,
            Some(&["working".to_string(), "still working".to_string()]),
            12,
        )
        .unwrap();

        assert_eq!(
            snapshot,
            PaneSnapshot {
                sequence_changed: false,
                lines: Vec::new(),
                truncated: false,
                state: Some(PaneState {
                    kind: LiveStatusKind::AgentRunning,
                    summary: "agent running".to_string(),
                    command: None,
                    prompt: None,
                    choices: Vec::new(),
                    confidence: None,
                    fingerprint: None,
                }),
            }
        );
    }

    #[test]
    fn snapshot_recognizes_codex_composer_through_ansi_and_unicode() {
        let pane = "\u{1b}[2m─ Worked for 7m ─\u{1b}[0m\n\
                    › Write tests for @filename\n\
                    gpt-5.4 high · ~/projects/web\n";
        let mut runner = StubRunner::with_stdout(pane);

        let snapshot = snapshot(
            &mut runner,
            "ajax-web-fix-login",
            AgentClient::Codex,
            None,
            12,
        )
        .unwrap();
        let state = snapshot.state.expect("state");

        assert_eq!(state.kind, LiveStatusKind::WaitingForInput);
        assert!(state.prompt.is_some());
        // The composer is low confidence: surfaced but not answerable.
        assert!(state.choices.is_empty());
        assert_eq!(state.confidence, Some(Confidence::Low));
    }

    #[test]
    fn send_keys_sends_literal_text_with_trailing_enter_when_submit_is_true() {
        let mut runner = StubRunner::with_stdout("");

        let outcome = send_keys(&mut runner, "ajax-web-fix-login", "approve it", true).unwrap();

        assert_eq!(outcome, SendKeysOutcome { submitted: true });
        assert_eq!(runner.commands.len(), 1);
        assert_eq!(runner.commands[0].timeout, Some(PANE_COMMAND_TIMEOUT));
        assert_eq!(
            runner.commands[0].args,
            vec![
                "send-keys",
                "-t",
                "ajax-web-fix-login:worktrunk",
                "approve it",
                "Enter",
            ]
        );
    }

    #[test]
    fn send_keys_accepts_allow_listed_tmux_key_tokens() {
        let mut runner = StubRunner::with_stdout("");

        let outcome = send_keys(&mut runner, "ajax-web-fix-login", "Enter", false).unwrap();

        assert_eq!(outcome, SendKeysOutcome { submitted: true });
        assert_eq!(
            runner.commands[0].args,
            vec!["send-keys", "-t", "ajax-web-fix-login:worktrunk", "Enter",]
        );
    }

    #[test]
    fn send_keys_rejects_unsupported_control_style_tokens() {
        let mut runner = StubRunner::with_stdout("");

        let error = send_keys(&mut runner, "ajax-web-fix-login", "C-x", false).unwrap_err();

        assert_eq!(
            error,
            super::PaneError::InvalidKeys("unsupported tmux key token: C-x".to_string())
        );
        assert!(runner.commands.is_empty());
    }
}

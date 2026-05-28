use crate::{
    adapters::{CommandRunError, CommandRunner, CommandSpec},
    live::classify_pane,
    models::LiveStatusKind,
};

const DEFAULT_WORKTRUNK_WINDOW: &str = "worktrunk";

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
    since: Option<&[String]>,
    limit: usize,
) -> Result<PaneSnapshot, PaneError> {
    let command = CommandSpec::new(
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
    );
    let output = runner.run(&command).map_err(map_tmux_error)?;
    let cleaned = clean_pane_lines(&output.stdout);
    let state = classify_state(&cleaned);
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

fn classify_state(lines: &[String]) -> Option<PaneState> {
    if lines.is_empty() {
        return None;
    }

    let joined = lines.join("\n");
    let mut observation = classify_pane(&joined);
    if observation.kind == LiveStatusKind::Unknown {
        observation.kind = LiveStatusKind::AgentRunning;
        observation.summary = "agent running".to_string();
    }
    Some(PaneState {
        kind: observation.kind,
        summary: observation.summary,
        command: None,
        prompt: None,
    })
}

fn strip_ansi(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut output = String::with_capacity(line.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == 0x1b {
            if index + 1 < bytes.len() && bytes[index + 1] == b'[' {
                index += 2;
                while index < bytes.len() {
                    let byte = bytes[index];
                    index += 1;
                    if (0x40..=0x7e).contains(&byte) {
                        break;
                    }
                }
                continue;
            }
            index += 1;
            continue;
        }

        output.push(bytes[index] as char);
        index += 1;
    }

    output
}

#[cfg(test)]
mod tests {
    use super::{send_keys, snapshot, PaneSnapshot, PaneState, SendKeysOutcome};
    use crate::{
        adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec},
        models::LiveStatusKind,
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

        let snapshot = snapshot(&mut runner, "ajax-web-fix-login", None, 3).unwrap();

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
                }),
            }
        );
        assert_eq!(runner.commands.len(), 1);
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
                }),
            }
        );
    }

    #[test]
    fn send_keys_sends_literal_text_with_trailing_enter_when_submit_is_true() {
        let mut runner = StubRunner::with_stdout("");

        let outcome = send_keys(&mut runner, "ajax-web-fix-login", "approve it", true).unwrap();

        assert_eq!(outcome, SendKeysOutcome { submitted: true });
        assert_eq!(runner.commands.len(), 1);
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

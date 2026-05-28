//! Browser pane snapshot and input surface.

use ajax_core::{
    adapters::CommandRunner,
    agent_prompt::{adapter_for, AnswerError, OperatorAnswer},
    commands::CommandContext,
    registry::Registry,
    slices::pane as core_pane,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

const PANE_LINE_LIMIT: usize = 12;
const INPUT_RATE_LIMIT: usize = 10;
const INPUT_RATE_WINDOW: Duration = Duration::from_secs(5);
const INPUT_DEDUP_WINDOW: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PaneSequenceState {
    entries: HashMap<String, StoredPane>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StoredPane {
    sequence: u64,
    lines: Vec<String>,
}

impl PaneSequenceState {
    pub fn sequence_for(&self, qualified_handle: &str) -> u64 {
        self.entries
            .get(qualified_handle)
            .map(|entry| entry.sequence)
            .unwrap_or(0)
    }
}

#[derive(Clone, Debug, Default)]
pub struct PaneInputState {
    dedup: HashMap<String, CachedInputResponse>,
    recent: HashMap<String, VecDeque<Instant>>,
}

#[derive(Clone, Debug)]
struct CachedInputResponse {
    stored_at: Instant,
    response: TaskInputResponse,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BrowserPaneState {
    pub kind: String,
    pub summary: String,
    pub command: Option<String>,
    pub prompt: Option<String>,
    /// Answerable choices in display order. Empty unless a high-confidence
    /// approval was recognized for this task's agent.
    #[serde(default)]
    pub choices: Vec<BrowserPaneChoice>,
    /// `"high"` or `"low"` when a structured prompt was recognized.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
    /// Stale-answer guard token the client echoes back when answering.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// Whether this prompt can be answered from the browser (high-confidence
    /// approval). `false` → the operator must open the terminal.
    pub answerable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BrowserPaneChoice {
    pub index: usize,
    pub label: String,
    pub role: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BrowserPaneSnapshot {
    pub sequence: u64,
    pub lines: Vec<String>,
    pub truncated: bool,
    pub tmux_exists: bool,
    pub state: Option<BrowserPaneState>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaneRouteError {
    TaskNotFound,
    SessionMissing,
    Command(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct TaskInputRequest {
    pub keys: String,
    pub submit: bool,
    pub request_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TaskInputResponse {
    pub sequence_hint: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TaskInputError {
    TaskNotFound,
    SessionMissing,
    RateLimited,
    InvalidRequest(String),
    Command(String),
}

pub fn browser_task_pane_view<R: Registry>(
    context: &CommandContext<R>,
    runner: &mut impl CommandRunner,
    sequences: &mut PaneSequenceState,
    qualified_handle: &str,
    since: Option<u64>,
) -> Result<BrowserPaneSnapshot, PaneRouteError> {
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
        .ok_or(PaneRouteError::TaskNotFound)?;

    let previous_lines = sequences
        .entries
        .get(qualified_handle)
        .map(|entry| entry.lines.as_slice());
    let snapshot = core_pane::snapshot(
        runner,
        &task.tmux_session,
        task.selected_agent,
        previous_lines,
        PANE_LINE_LIMIT,
    )
    .map_err(|error| match error {
        core_pane::PaneError::SessionMissing => PaneRouteError::SessionMissing,
        core_pane::PaneError::InvalidKeys(message) => PaneRouteError::Command(message),
        core_pane::PaneError::CommandRun(inner) => PaneRouteError::Command(inner.to_string()),
    })?;

    let entry = sequences
        .entries
        .entry(qualified_handle.to_string())
        .or_insert_with(|| StoredPane {
            sequence: 0,
            lines: Vec::new(),
        });
    if snapshot.sequence_changed {
        entry.sequence = entry.sequence.saturating_add(1);
        entry.lines = snapshot.lines;
    }

    let lines = if since.is_some_and(|value| value == entry.sequence) {
        Vec::new()
    } else {
        entry.lines.clone()
    };

    Ok(BrowserPaneSnapshot {
        sequence: entry.sequence,
        lines,
        truncated: snapshot.truncated,
        tmux_exists: true,
        state: snapshot.state.map(browser_pane_state),
    })
}

fn browser_pane_state(state: core_pane::PaneState) -> BrowserPaneState {
    use ajax_core::agent_prompt::{ChoiceRole, Confidence};

    let answerable =
        matches!(state.confidence, Some(Confidence::High)) && !state.choices.is_empty();
    BrowserPaneState {
        kind: format!("{:?}", state.kind),
        summary: state.summary,
        command: state.command,
        prompt: state.prompt,
        choices: state
            .choices
            .iter()
            .map(|choice| BrowserPaneChoice {
                index: choice.index,
                label: choice.label.clone(),
                role: match choice.role {
                    ChoiceRole::Affirm => "affirm",
                    ChoiceRole::Deny => "deny",
                    ChoiceRole::Neutral => "neutral",
                }
                .to_string(),
            })
            .collect(),
        confidence: state.confidence.map(|confidence| {
            match confidence {
                Confidence::High => "high",
                Confidence::Low => "low",
            }
            .to_string()
        }),
        fingerprint: state.fingerprint,
        answerable,
    }
}

pub fn send_task_input<R: Registry>(
    context: &CommandContext<R>,
    runner: &mut impl CommandRunner,
    sequences: &PaneSequenceState,
    inputs: &mut PaneInputState,
    qualified_handle: &str,
    request: TaskInputRequest,
    now: Instant,
) -> Result<TaskInputResponse, TaskInputError> {
    if request.request_id.trim().is_empty() {
        return Err(TaskInputError::InvalidRequest(
            "request_id is required".to_string(),
        ));
    }

    prune_expired_inputs(inputs, now);
    let dedup_key = format!("{qualified_handle}\u{1f}{}", request.request_id);
    if let Some(cached) = inputs.dedup.get(&dedup_key) {
        return Ok(cached.response.clone());
    }

    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
        .ok_or(TaskInputError::TaskNotFound)?;

    let recent = inputs
        .recent
        .entry(qualified_handle.to_string())
        .or_default();
    while recent
        .front()
        .is_some_and(|instant| now.duration_since(*instant) > INPUT_RATE_WINDOW)
    {
        recent.pop_front();
    }
    if recent.len() >= INPUT_RATE_LIMIT {
        return Err(TaskInputError::RateLimited);
    }

    core_pane::send_keys(runner, &task.tmux_session, &request.keys, request.submit).map_err(
        |error| match error {
            core_pane::PaneError::SessionMissing => TaskInputError::SessionMissing,
            core_pane::PaneError::InvalidKeys(message) => TaskInputError::InvalidRequest(message),
            core_pane::PaneError::CommandRun(inner) => TaskInputError::Command(inner.to_string()),
        },
    )?;

    recent.push_back(now);
    let response = TaskInputResponse {
        sequence_hint: sequences.sequence_for(qualified_handle),
    };
    inputs.dedup.insert(
        dedup_key,
        CachedInputResponse {
            stored_at: now,
            response: response.clone(),
        },
    );
    Ok(response)
}

/// A guarded answer to a structured agent prompt. Carries the operator's intent
/// (`approve` / `deny` / `select`) plus the `fingerprint` of the prompt they were
/// answering — the server re-captures the live pane and refuses to send keys if
/// the prompt has changed.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
pub struct TaskAnswerRequest {
    #[serde(flatten)]
    pub answer: OperatorAnswer,
    pub fingerprint: String,
    pub request_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TaskAnswerError {
    TaskNotFound,
    SessionMissing,
    /// The live pane no longer matches the answered prompt (or there is no
    /// prompt). The answer is discarded; the client re-surfaces current state.
    Stale,
    /// The current prompt is not a high-confidence approval — the operator must
    /// open the terminal.
    NotAnswerable,
    RateLimited,
    InvalidRequest(String),
    Command(String),
}

pub fn answer_task_prompt<R: Registry>(
    context: &CommandContext<R>,
    runner: &mut impl CommandRunner,
    sequences: &PaneSequenceState,
    inputs: &mut PaneInputState,
    qualified_handle: &str,
    request: TaskAnswerRequest,
    now: Instant,
) -> Result<TaskInputResponse, TaskAnswerError> {
    if request.request_id.trim().is_empty() {
        return Err(TaskAnswerError::InvalidRequest(
            "request_id is required".to_string(),
        ));
    }
    if request.fingerprint.trim().is_empty() {
        return Err(TaskAnswerError::InvalidRequest(
            "fingerprint is required".to_string(),
        ));
    }

    prune_expired_inputs(inputs, now);
    let dedup_key = format!("{qualified_handle}\u{1f}{}", request.request_id);
    if let Some(cached) = inputs.dedup.get(&dedup_key) {
        return Ok(cached.response.clone());
    }

    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
        .ok_or(TaskAnswerError::TaskNotFound)?;

    {
        let recent = inputs
            .recent
            .entry(qualified_handle.to_string())
            .or_default();
        while recent
            .front()
            .is_some_and(|instant| now.duration_since(*instant) > INPUT_RATE_WINDOW)
        {
            recent.pop_front();
        }
        if recent.len() >= INPUT_RATE_LIMIT {
            return Err(TaskAnswerError::RateLimited);
        }
    }

    // Re-capture the live prompt and verify it is still the one the operator
    // answered. A mismatch (or a vanished prompt) means the answer is stale.
    let prompt = core_pane::capture_prompt(runner, &task.tmux_session, task.selected_agent)
        .map_err(map_answer_pane_error)?
        .ok_or(TaskAnswerError::Stale)?;
    if prompt.fingerprint != request.fingerprint {
        return Err(TaskAnswerError::Stale);
    }

    let keys = adapter_for(task.selected_agent)
        .answer_keys(&prompt, &request.answer)
        .map_err(|error| match error {
            AnswerError::NotAnswerable => TaskAnswerError::NotAnswerable,
            AnswerError::UnknownChoice => {
                TaskAnswerError::InvalidRequest("unknown choice".to_string())
            }
        })?;

    core_pane::send_keys(runner, &task.tmux_session, &keys.keys, keys.submit)
        .map_err(map_answer_pane_error)?;

    inputs
        .recent
        .entry(qualified_handle.to_string())
        .or_default()
        .push_back(now);
    let response = TaskInputResponse {
        sequence_hint: sequences.sequence_for(qualified_handle),
    };
    inputs.dedup.insert(
        dedup_key,
        CachedInputResponse {
            stored_at: now,
            response: response.clone(),
        },
    );
    Ok(response)
}

fn map_answer_pane_error(error: core_pane::PaneError) -> TaskAnswerError {
    match error {
        core_pane::PaneError::SessionMissing => TaskAnswerError::SessionMissing,
        core_pane::PaneError::InvalidKeys(message) => TaskAnswerError::InvalidRequest(message),
        core_pane::PaneError::CommandRun(inner) => TaskAnswerError::Command(inner.to_string()),
    }
}

fn prune_expired_inputs(inputs: &mut PaneInputState, now: Instant) {
    inputs
        .dedup
        .retain(|_, cached| now.duration_since(cached.stored_at) <= INPUT_DEDUP_WINDOW);
    inputs.recent.retain(|_, entries| {
        while entries
            .front()
            .is_some_and(|instant| now.duration_since(*instant) > INPUT_RATE_WINDOW)
        {
            entries.pop_front();
        }
        !entries.is_empty()
    });
}
#[cfg(test)]
mod tests {
    use super::{browser_task_pane_view, PaneRouteError, PaneSequenceState};
    use ajax_core::{
        adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec},
        commands::CommandContext,
        config::{Config, ManagedRepo},
        models::{AgentClient, Task, TaskId},
        registry::{InMemoryRegistry, Registry},
    };

    fn context_with_task() -> CommandContext<InMemoryRegistry> {
        let config = Config {
            repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
            ..Config::default()
        };
        let mut registry = InMemoryRegistry::default();
        registry
            .create_task(Task::new(
                TaskId::new("web/fix-login"),
                "web",
                "fix-login",
                "Fix login",
                "ajax/fix-login",
                "main",
                "/repo/web__worktrees/ajax-fix-login",
                "ajax-web-fix-login",
                "worktrunk",
                AgentClient::Codex,
            ))
            .unwrap();
        CommandContext::new(config, registry)
    }

    struct PaneRunner {
        response: Result<CommandOutput, CommandRunError>,
    }

    impl CommandRunner for PaneRunner {
        fn run(&mut self, _command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.response.clone()
        }
    }

    #[test]
    fn pane_view_returns_empty_delta_when_since_matches_cached_sequence() {
        let context = context_with_task();
        let mut sequences = PaneSequenceState::default();
        let mut runner = PaneRunner {
            response: Ok(CommandOutput {
                status_code: 0,
                stdout: "agent running\n".to_string(),
                stderr: String::new(),
            }),
        };

        let first = browser_task_pane_view(
            &context,
            &mut runner,
            &mut sequences,
            "web/fix-login",
            Some(0),
        )
        .unwrap();
        let second = browser_task_pane_view(
            &context,
            &mut runner,
            &mut sequences,
            "web/fix-login",
            Some(first.sequence),
        )
        .unwrap();

        assert_eq!(first.sequence, 1);
        assert_eq!(second.sequence, 1);
        assert_eq!(second.lines, Vec::<String>::new());
    }

    #[test]
    fn pane_view_translates_missing_tmux_session_into_conflict_payload() {
        let context = context_with_task();
        let mut sequences = PaneSequenceState::default();
        let mut runner = PaneRunner {
            response: Err(CommandRunError::NonZeroExit {
                program: "tmux".to_string(),
                status_code: 1,
                stderr: "can't find session".to_string(),
                cwd: None,
            }),
        };

        let error = browser_task_pane_view(
            &context,
            &mut runner,
            &mut sequences,
            "web/fix-login",
            Some(0),
        )
        .unwrap_err();

        assert_eq!(error, PaneRouteError::SessionMissing);
    }

    use super::{
        answer_task_prompt, PaneInputState, TaskAnswerError, TaskAnswerRequest,
    };
    use ajax_core::agent_prompt::OperatorAnswer;
    use std::time::Instant;

    const YES_NO_PANE: &str = "Allow Codex to run this command?\nRun `cargo test`? [y/n]\n";

    /// Records every command and returns the same capture-pane stdout, so the
    /// `/answer` path sees a stable approval prompt and we can assert the keys
    /// that were sent.
    struct RecordingRunner {
        pane: String,
        commands: Vec<CommandSpec>,
    }

    impl CommandRunner for RecordingRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.commands.push(command.clone());
            Ok(CommandOutput {
                status_code: 0,
                stdout: self.pane.clone(),
                stderr: String::new(),
            })
        }
    }

    fn answer_request(answer: OperatorAnswer, fingerprint: &str) -> TaskAnswerRequest {
        TaskAnswerRequest {
            answer,
            fingerprint: fingerprint.to_string(),
            request_id: "req-1".to_string(),
        }
    }

    fn current_fingerprint(pane: &str, agent: AgentClient) -> String {
        let lines: Vec<String> = pane
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect();
        ajax_core::agent_prompt::parse_prompt(agent, &lines)
            .expect("prompt")
            .fingerprint
    }

    #[test]
    fn answer_approve_sends_adapter_keys_when_fingerprint_matches() {
        let context = context_with_task();
        let sequences = PaneSequenceState::default();
        let mut inputs = PaneInputState::default();
        let mut runner = RecordingRunner {
            pane: YES_NO_PANE.to_string(),
            commands: Vec::new(),
        };
        let fingerprint = current_fingerprint(YES_NO_PANE, AgentClient::Codex);

        answer_task_prompt(
            &context,
            &mut runner,
            &sequences,
            &mut inputs,
            "web/fix-login",
            answer_request(OperatorAnswer::Approve, &fingerprint),
            Instant::now(),
        )
        .expect("approve accepted");

        // First command captures the pane; the send-keys carries "y", not raw input.
        let send = runner
            .commands
            .iter()
            .find(|command| command.args.first().map(String::as_str) == Some("send-keys"))
            .expect("send-keys issued");
        assert!(send.args.contains(&"y".to_string()));
        assert!(send.args.contains(&"Enter".to_string()));
    }

    #[test]
    fn answer_rejects_stale_fingerprint_without_sending_keys() {
        let context = context_with_task();
        let sequences = PaneSequenceState::default();
        let mut inputs = PaneInputState::default();
        let mut runner = RecordingRunner {
            pane: YES_NO_PANE.to_string(),
            commands: Vec::new(),
        };

        let error = answer_task_prompt(
            &context,
            &mut runner,
            &sequences,
            &mut inputs,
            "web/fix-login",
            answer_request(OperatorAnswer::Approve, "stale-fingerprint"),
            Instant::now(),
        )
        .unwrap_err();

        assert_eq!(error, TaskAnswerError::Stale);
        assert!(
            !runner
                .commands
                .iter()
                .any(|command| command.args.first().map(String::as_str) == Some("send-keys")),
            "no keys may be sent for a stale answer"
        );
    }

    #[test]
    fn answer_refuses_non_answerable_free_text_composer() {
        let context = context_with_task();
        let sequences = PaneSequenceState::default();
        let mut inputs = PaneInputState::default();
        let composer = "› Write tests for @filename\ngpt-5.4 high · ~/.ajax-dev/x\n";
        let mut runner = RecordingRunner {
            pane: composer.to_string(),
            commands: Vec::new(),
        };
        let fingerprint = current_fingerprint(composer, AgentClient::Codex);

        let error = answer_task_prompt(
            &context,
            &mut runner,
            &sequences,
            &mut inputs,
            "web/fix-login",
            answer_request(OperatorAnswer::Approve, &fingerprint),
            Instant::now(),
        )
        .unwrap_err();

        assert_eq!(error, TaskAnswerError::NotAnswerable);
    }

    #[test]
    fn answer_dedups_repeated_request_id() {
        let context = context_with_task();
        let sequences = PaneSequenceState::default();
        let mut inputs = PaneInputState::default();
        let mut runner = RecordingRunner {
            pane: YES_NO_PANE.to_string(),
            commands: Vec::new(),
        };
        let fingerprint = current_fingerprint(YES_NO_PANE, AgentClient::Codex);

        for _ in 0..2 {
            answer_task_prompt(
                &context,
                &mut runner,
                &sequences,
                &mut inputs,
                "web/fix-login",
                answer_request(OperatorAnswer::Approve, &fingerprint),
                Instant::now(),
            )
            .expect("accepted");
        }

        let send_count = runner
            .commands
            .iter()
            .filter(|command| command.args.first().map(String::as_str) == Some("send-keys"))
            .count();
        assert_eq!(send_count, 1, "the repeat request_id must not resend keys");
    }
}

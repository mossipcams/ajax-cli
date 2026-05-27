pub use crate::live_application::{apply_authoritative_observation, apply_observation};
pub use crate::models::{LiveObservation, LiveStatusKind};

pub fn classify_pane(pane: &str) -> LiveObservation {
    let trimmed = pane.trim();
    if trimmed.is_empty() {
        return LiveObservation::new(LiveStatusKind::Unknown, "pane is empty");
    }

    let lines = meaningful_lines(trimmed);
    if looks_like_idle_codex_prompt(&lines) {
        return LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input");
    }

    classify_recent_evidence(&lines)
        .unwrap_or_else(|| LiveObservation::new(LiveStatusKind::Unknown, "unknown terminal state"))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PaneEvidence {
    Completion,
    ApprovalPrompt,
    InputPrompt,
    AuthRequired,
    RateLimited,
    ContextLimit,
    Blocked,
    MergeConflict,
    CiFailed,
    CommandFailed,
    CommandRunning,
    AgentRunning,
    TestsRunning,
}

impl PaneEvidence {
    fn observation(self) -> LiveObservation {
        match self {
            Self::Completion => LiveObservation::new(LiveStatusKind::Done, "done"),
            Self::ApprovalPrompt => {
                LiveObservation::new(LiveStatusKind::WaitingForApproval, "waiting for approval")
            }
            Self::InputPrompt => {
                LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input")
            }
            Self::AuthRequired => {
                LiveObservation::new(LiveStatusKind::AuthRequired, "authentication required")
            }
            Self::RateLimited => LiveObservation::new(LiveStatusKind::RateLimited, "rate limited"),
            Self::ContextLimit => {
                LiveObservation::new(LiveStatusKind::ContextLimit, "context limit reached")
            }
            Self::Blocked => LiveObservation::new(LiveStatusKind::Blocked, "blocked"),
            Self::MergeConflict => LiveObservation::new(
                LiveStatusKind::MergeConflict,
                "merge conflict needs attention",
            ),
            Self::CiFailed => LiveObservation::new(LiveStatusKind::CiFailed, "ci failed"),
            Self::CommandFailed => {
                LiveObservation::new(LiveStatusKind::CommandFailed, "command failed")
            }
            Self::CommandRunning => {
                LiveObservation::new(LiveStatusKind::CommandRunning, "command running")
            }
            Self::AgentRunning => {
                LiveObservation::new(LiveStatusKind::AgentRunning, "agent running")
            }
            Self::TestsRunning => {
                LiveObservation::new(LiveStatusKind::TestsRunning, "tests running")
            }
        }
    }
}

fn classify_recent_evidence(lines: &[&str]) -> Option<LiveObservation> {
    if lines
        .last()
        .is_some_and(|line| looks_like_shell_prompt(line))
    {
        if let Some(line) = lines.iter().rev().nth(1).copied() {
            if let Some(observation) = classify_line_evidence(line) {
                return Some(observation);
            }
        }

        return Some(LiveObservation::new(
            LiveStatusKind::ShellIdle,
            "shell idle",
        ));
    }

    lines.iter().rev().copied().find_map(classify_line_evidence)
}

fn classify_line_evidence(line: &str) -> Option<LiveObservation> {
    classify_cursor_stream_json_line(line)
        .or_else(|| pane_evidence(line).map(PaneEvidence::observation))
}

fn pane_evidence(line: &str) -> Option<PaneEvidence> {
    let lower = line.to_ascii_lowercase();

    if is_completion_line(&lower) {
        return Some(PaneEvidence::Completion);
    }

    if contains_any(
        &lower,
        &[
            "do you want to proceed",
            "approve to proceed",
            "allow command",
            "approval request",
            "y/n",
            "[y/n]",
        ],
    ) {
        return Some(PaneEvidence::ApprovalPrompt);
    }

    if contains_any(
        &lower,
        &[
            "please login",
            "please log in",
            "log in to",
            "login to continue",
            "authenticate",
            "auth required",
        ],
    ) {
        return Some(PaneEvidence::AuthRequired);
    }

    if contains_any(
        &lower,
        &["rate limit", "too many requests", "try again later"],
    ) {
        return Some(PaneEvidence::RateLimited);
    }

    if contains_any(&lower, &["context limit", "token limit", "context length"]) {
        return Some(PaneEvidence::ContextLimit);
    }

    if contains_any(
        &lower,
        &["blocked", "cannot continue", "manual intervention required"],
    ) {
        return Some(PaneEvidence::Blocked);
    }

    if contains_any(
        &lower,
        &[
            "merge conflict",
            "conflict (",
            "automatic merge failed",
            "fix conflicts",
        ],
    ) {
        return Some(PaneEvidence::MergeConflict);
    }

    if contains_any(
        &lower,
        &[
            "ci failed",
            "github actions failed",
            "check run failed",
            "workflow failed",
            "failing checks",
        ],
    ) {
        return Some(PaneEvidence::CiFailed);
    }

    if contains_any(
        &lower,
        &[
            "waiting for input",
            "what kind of ",
            "what do you want me to",
            "what you want me to do",
            "send me the problem",
            "did you mean",
            "specific task",
            "press enter",
            "continue?",
            "enter your choice",
            "select an option",
        ],
    ) {
        return Some(PaneEvidence::InputPrompt);
    }

    if contains_any(
        &lower,
        &[
            "test result: failed",
            "command failed",
            "exit code",
            "nonzeroexit",
            "failed with",
        ],
    ) {
        return Some(PaneEvidence::CommandFailed);
    }

    if contains_any(
        &lower,
        &["running command", "executing command", "$ cargo", "$ npm"],
    ) {
        return Some(PaneEvidence::CommandRunning);
    }

    if looks_like_active_agent_status(line) {
        return Some(PaneEvidence::AgentRunning);
    }

    if contains_any(
        &lower,
        &["cargo test", "running test", "running 0 tests", "running "],
    ) {
        return Some(PaneEvidence::TestsRunning);
    }

    None
}

fn meaningful_lines(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect()
}

fn looks_like_idle_codex_prompt(lines: &[&str]) -> bool {
    let recent_lines: Vec<_> = lines.iter().rev().take(4).copied().collect();

    recent_lines.iter().any(|line| line.starts_with('›'))
        && lines
            .last()
            .is_some_and(|line| line.starts_with("gpt-") && line.contains("~/"))
        && !recent_lines
            .iter()
            .any(|line| looks_like_active_agent_status(line))
}

fn looks_like_active_agent_status(line: &str) -> bool {
    contains_any(
        &line.to_ascii_lowercase(),
        &[
            "codex is working",
            "claude is working",
            "cursor agent",
            "cursor is working",
            "background terminal running",
            "thinking",
            "waiting for background terminal",
            "working (",
            "working on your task",
            "running tool",
            "using tool",
        ],
    )
}

fn classify_cursor_stream_json_line(line: &str) -> Option<LiveObservation> {
    let trimmed = line.trim();
    if !trimmed.starts_with('{') {
        return None;
    }

    let value = serde_json::from_str::<serde_json::Value>(trimmed).ok()?;
    let event_type = value.get("type")?.as_str()?.to_ascii_lowercase();

    match event_type.as_str() {
        "system" if value.get("subtype").and_then(serde_json::Value::as_str) == Some("init") => {
            Some(LiveObservation::new(
                LiveStatusKind::AgentRunning,
                "agent running",
            ))
        }
        "thinking" => Some(LiveObservation::new(
            LiveStatusKind::AgentRunning,
            "agent running",
        )),
        "tool_call" => classify_cursor_tool_call_line(&value),
        "assistant" => cursor_assistant_observation(&value),
        "result" => classify_cursor_result_line(&value),
        "request" => classify_cursor_request_line(&value),
        "status" => classify_cursor_status_line(&value),
        _ => None,
    }
}

fn classify_cursor_tool_call_line(value: &serde_json::Value) -> Option<LiveObservation> {
    if let Some(status) = value.get("status").and_then(serde_json::Value::as_str) {
        return match status {
            "running" | "in_progress" => Some(LiveObservation::new(
                LiveStatusKind::CommandRunning,
                format!("tool running: {}", cursor_tool_name(value)),
            )),
            "error" | "failed" => Some(LiveObservation::new(
                LiveStatusKind::CommandFailed,
                "command failed",
            )),
            _ => None,
        };
    }

    match value.get("subtype").and_then(serde_json::Value::as_str) {
        Some("started") => value.get("tool_call").map(|tool_call| {
            LiveObservation::new(
                LiveStatusKind::CommandRunning,
                format!("tool running: {}", cursor_nested_tool_name(tool_call)),
            )
        }),
        Some("completed") => None,
        _ => None,
    }
}

fn classify_cursor_result_line(value: &serde_json::Value) -> Option<LiveObservation> {
    if value.get("is_error").and_then(serde_json::Value::as_bool) == Some(true)
        || value.get("subtype").and_then(serde_json::Value::as_str) == Some("error")
    {
        return Some(LiveObservation::new(
            LiveStatusKind::CommandFailed,
            "command failed",
        ));
    }

    if value.get("subtype").and_then(serde_json::Value::as_str) == Some("success")
        || value.get("is_error").and_then(serde_json::Value::as_bool) == Some(false)
    {
        return Some(LiveObservation::new(LiveStatusKind::Done, "done"));
    }

    None
}

fn classify_cursor_request_line(value: &serde_json::Value) -> Option<LiveObservation> {
    let prompt = value
        .get("message")
        .and_then(serde_json::Value::as_str)
        .or_else(|| value.get("prompt").and_then(serde_json::Value::as_str))
        .or_else(|| value.get("text").and_then(serde_json::Value::as_str))
        .unwrap_or("waiting for operator input");

    if cursor_mentions_approval(prompt) {
        Some(LiveObservation::new(
            LiveStatusKind::WaitingForApproval,
            "waiting for approval",
        ))
    } else {
        Some(LiveObservation::new(
            LiveStatusKind::WaitingForInput,
            "waiting for input",
        ))
    }
}

fn classify_cursor_status_line(value: &serde_json::Value) -> Option<LiveObservation> {
    let status = value
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_ascii_uppercase();

    match status.as_str() {
        "RUNNING" | "CREATING" => Some(LiveObservation::new(
            LiveStatusKind::AgentRunning,
            "agent running",
        )),
        "FINISHED" => Some(LiveObservation::new(LiveStatusKind::Done, "done")),
        "ERROR" | "CANCELLED" | "EXPIRED" => Some(LiveObservation::new(
            LiveStatusKind::CommandFailed,
            "command failed",
        )),
        _ => None,
    }
}

fn cursor_assistant_observation(value: &serde_json::Value) -> Option<LiveObservation> {
    let text = cursor_assistant_text(value)?;
    if text.trim_end().ends_with('?') && !cursor_mentions_approval(&text) {
        return Some(LiveObservation::new(
            LiveStatusKind::WaitingForInput,
            "waiting for input",
        ));
    }

    let observation = classify_pane(&text);
    if observation.kind == LiveStatusKind::Unknown {
        Some(LiveObservation::new(
            LiveStatusKind::AgentRunning,
            "agent running",
        ))
    } else {
        Some(observation)
    }
}

fn cursor_assistant_text(value: &serde_json::Value) -> Option<String> {
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

fn cursor_tool_name(value: &serde_json::Value) -> String {
    value
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| "tool".to_string())
}

fn cursor_nested_tool_name(tool_call: &serde_json::Value) -> String {
    if let Some(read) = tool_call.get("readToolCall") {
        if let Some(path) = read
            .get("args")
            .and_then(|args| args.get("path"))
            .and_then(serde_json::Value::as_str)
        {
            return format!("read {path}");
        }
        return "read".to_string();
    }

    if let Some(shell) = tool_call
        .get("bashToolCall")
        .or_else(|| tool_call.get("shellToolCall"))
    {
        if let Some(command) = shell
            .get("args")
            .and_then(|args| args.get("command").or_else(|| args.get("cmd")))
            .and_then(serde_json::Value::as_str)
        {
            return format!("shell: {command}");
        }
        return "shell".to_string();
    }

    tool_call
        .as_object()
        .and_then(|fields| fields.keys().next())
        .map(|name| name.to_string())
        .unwrap_or_else(|| "tool".to_string())
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

fn is_completion_line(lower: &str) -> bool {
    contains_any(
        lower,
        &[
            "test result: ok",
            "tests passed",
            "all pre-pr checks passed",
            "successfully completed",
            "task complete",
            "all done",
        ],
    ) || lower.trim_matches(|character: char| !character.is_ascii_alphanumeric()) == "done"
}

pub fn classify_agent_status_value(value: &str) -> Option<LiveObservation> {
    match value.trim() {
        "working" => Some(LiveObservation::new(
            LiveStatusKind::AgentRunning,
            "agent running",
        )),
        "wait" => Some(LiveObservation::new(
            LiveStatusKind::WaitingForInput,
            "waiting for input",
        )),
        "ask" => Some(LiveObservation::new(
            LiveStatusKind::WaitingForApproval,
            "waiting for approval",
        )),
        "done" | "parked" => Some(LiveObservation::new(LiveStatusKind::Done, "done")),
        _ => None,
    }
}

pub fn reduce_agent_status_values<'a>(
    values: impl IntoIterator<Item = &'a str>,
) -> Option<LiveObservation> {
    values
        .into_iter()
        .filter_map(|value| {
            classify_agent_status_value(value).map(|observation| {
                let priority = agent_status_priority(observation.kind);
                (priority, observation)
            })
        })
        .max_by_key(|(priority, _observation)| *priority)
        .map(|(_priority, observation)| observation)
}

fn agent_status_priority(kind: LiveStatusKind) -> u8 {
    match kind {
        LiveStatusKind::AgentRunning => 5,
        LiveStatusKind::WaitingForInput => 4,
        LiveStatusKind::WaitingForApproval => 3,
        LiveStatusKind::Done => 2,
        _ => 0,
    }
}

pub fn reduce_live_observation(
    current: Option<&LiveObservation>,
    next: LiveObservation,
) -> LiveObservation {
    let Some(current) = current else {
        return next;
    };

    if next.kind.is_missing_substrate() {
        return next;
    }

    if current.kind.is_missing_substrate() {
        return current.clone();
    }

    if should_keep_current_status(current.kind, next.kind) {
        return current.clone();
    }

    next
}

fn should_keep_current_status(current: LiveStatusKind, next: LiveStatusKind) -> bool {
    if current == LiveStatusKind::Done {
        return is_incidental_observation(next);
    }

    if is_waiting_status(current) {
        return is_passive_observation(next);
    }

    if is_failure_status(current) {
        return is_incidental_observation(next);
    }

    false
}

fn is_waiting_status(kind: LiveStatusKind) -> bool {
    matches!(
        kind,
        LiveStatusKind::WaitingForApproval | LiveStatusKind::WaitingForInput
    )
}

fn is_failure_status(kind: LiveStatusKind) -> bool {
    matches!(
        kind,
        LiveStatusKind::AuthRequired
            | LiveStatusKind::RateLimited
            | LiveStatusKind::ContextLimit
            | LiveStatusKind::CiFailed
            | LiveStatusKind::CommandFailed
            | LiveStatusKind::Blocked
            | LiveStatusKind::MergeConflict
    )
}

fn is_passive_observation(kind: LiveStatusKind) -> bool {
    matches!(kind, LiveStatusKind::ShellIdle | LiveStatusKind::Unknown)
}

fn is_incidental_observation(kind: LiveStatusKind) -> bool {
    matches!(
        kind,
        LiveStatusKind::ShellIdle
            | LiveStatusKind::Unknown
            | LiveStatusKind::AgentRunning
            | LiveStatusKind::CommandRunning
            | LiveStatusKind::TestsRunning
    )
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn looks_like_shell_prompt(text: &str) -> bool {
    text.lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .is_some_and(|line| {
            let line = line.trim_end();
            line.ends_with('%') || line.ends_with('$') || line.ends_with('#')
        })
}

#[cfg(test)]
mod tests {
    use crate::models::{
        AgentClient, AgentRuntimeStatus, LiveObservation, LiveStatusKind, SideFlag, Task, TaskId,
    };

    use super::{
        apply_observation, classify_agent_status_value, classify_pane, reduce_agent_status_values,
    };

    fn base_task() -> Task {
        Task::new(
            TaskId::new("task-1"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            "/tmp/worktrees/web-fix-login",
            "ajax-web-fix-login",
            "worktrunk",
            AgentClient::Codex,
        )
    }

    #[test]
    fn pane_classifier_detects_agent_attention_states() {
        for (pane, expected) in [
            (
                "Do you want to proceed? y/n",
                LiveStatusKind::WaitingForApproval,
            ),
            (
                "Waiting for input. Press Enter to continue.",
                LiveStatusKind::WaitingForInput,
            ),
            ("Please login to continue", LiveStatusKind::AuthRequired),
            (
                "rate limit exceeded; try again later",
                LiveStatusKind::RateLimited,
            ),
            ("context limit reached", LiveStatusKind::ContextLimit),
            (
                "CONFLICT (content): merge conflict in src/lib.rs",
                LiveStatusKind::MergeConflict,
            ),
        ] {
            let observation = classify_pane(pane);

            assert_eq!(observation.kind, expected, "{pane}");
        }
    }

    #[test]
    fn pane_classifier_detects_codex_clarification_prompts_as_waiting_for_input() {
        for pane in [
            "\
› Math

⚠ Heads up, you have less than 25% of your weekly limit left.

• What kind of math do you want to work on? Send me the problem, equation, or
  topic.

› Use /skills to list available skills",
            "\
› trst

⚠ Heads up, you have less than 25% of your weekly limit left.

• I’m not sure what you want me to do with “trst”. Did you mean “test”, or is
  there a specific task in this repo you want me to handle?

› Use /skills to list available skills",
        ] {
            let observation = classify_pane(pane);

            assert_eq!(observation.kind, LiveStatusKind::WaitingForInput, "{pane}");
        }
    }

    #[test]
    fn pane_classifier_detects_conflict_and_ci_failure_evidence() {
        for (pane, expected) in [
            (
                "Automatic merge failed; fix conflicts and then commit the result.",
                LiveStatusKind::MergeConflict,
            ),
            (
                "CONFLICT (modify/delete): src/lib.rs deleted in HEAD and modified in feature",
                LiveStatusKind::MergeConflict,
            ),
            ("CI failed for this branch", LiveStatusKind::CiFailed),
            (
                "GitHub Actions failed: test.yml / build",
                LiveStatusKind::CiFailed,
            ),
            ("check run failed: cargo test", LiveStatusKind::CiFailed),
            ("workflow failed after 3m", LiveStatusKind::CiFailed),
            (
                "There are failing checks on the PR",
                LiveStatusKind::CiFailed,
            ),
        ] {
            let observation = classify_pane(pane);

            assert_eq!(observation.kind, expected, "{pane}");
        }
    }

    #[test]
    fn pane_classifier_detects_cursor_stream_json_and_activity() {
        for (pane, expected) in [
            (
                r#"{"type":"system","subtype":"init","agent":"cursor"}"#,
                LiveStatusKind::AgentRunning,
            ),
            (r#"{"type":"thinking"}"#, LiveStatusKind::AgentRunning),
            (
                r#"{"type":"tool_call","call_id":"1","name":"grep","status":"running"}"#,
                LiveStatusKind::CommandRunning,
            ),
            (
                r#"{"type":"tool_call","subtype":"started","call_id":"1","tool_call":{"readToolCall":{"args":{"path":"src/lib.rs"}}}}"#,
                LiveStatusKind::CommandRunning,
            ),
            (
                r#"{"type":"status","status":"RUNNING"}"#,
                LiveStatusKind::AgentRunning,
            ),
            (
                r#"{"type":"result","subtype":"success","is_error":false,"result":"done"}"#,
                LiveStatusKind::Done,
            ),
            (
                r#"{"type":"result","subtype":"error","is_error":true,"result":"auth failed"}"#,
                LiveStatusKind::CommandFailed,
            ),
            (
                r#"{"type":"request","request_id":"req-1","message":"Allow command?"}"#,
                LiveStatusKind::WaitingForApproval,
            ),
            (
                r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Which branch should I use?"}]}}"#,
                LiveStatusKind::WaitingForInput,
            ),
            (
                "cursor agent --print --output-format stream-json fix tests",
                LiveStatusKind::AgentRunning,
            ),
            (
                concat!(
                    r#"{"type":"thinking"}"#,
                    "\n",
                    r#"{"type":"tool_call","call_id":"1","name":"grep","status":"running"}"#,
                ),
                LiveStatusKind::CommandRunning,
            ),
        ] {
            let observation = classify_pane(pane);

            assert_eq!(observation.kind, expected, "{pane}");
        }
    }

    #[test]
    fn pane_classifier_detects_runtime_states() {
        for (pane, expected) in [
            (
                "cargo test --all-features\nrunning 37 tests",
                LiveStatusKind::TestsRunning,
            ),
            ("running command: npm test", LiveStatusKind::CommandRunning),
            ("test result: ok. 37 passed", LiveStatusKind::Done),
            (
                "codex is working on your task",
                LiveStatusKind::AgentRunning,
            ),
            (
                "Command failed with exit code 101",
                LiveStatusKind::CommandFailed,
            ),
            ("✓ Successfully completed task", LiveStatusKind::Done),
            ("matt@host project % ", LiveStatusKind::ShellIdle),
            ("", LiveStatusKind::Unknown),
        ] {
            let observation = classify_pane(pane);

            assert_eq!(observation.kind, expected, "{pane}");
        }
    }

    #[test]
    fn agent_status_values_map_to_live_observations() {
        for (value, expected) in [
            ("working", Some(LiveStatusKind::AgentRunning)),
            ("done", Some(LiveStatusKind::Done)),
            ("wait", Some(LiveStatusKind::WaitingForInput)),
            ("ask", Some(LiveStatusKind::WaitingForApproval)),
            ("parked", Some(LiveStatusKind::Done)),
            ("", None),
            ("nonsense", None),
        ] {
            let observation = classify_agent_status_value(value);

            assert_eq!(
                observation.map(|observation| observation.kind),
                expected,
                "{value:?}"
            );
        }
    }

    #[test]
    fn agent_status_values_reduce_by_tmux_agent_status_priority() {
        let observation = reduce_agent_status_values(["done", "wait", "working", "ask", "parked"]);

        assert_eq!(
            observation.map(|observation| observation.kind),
            Some(LiveStatusKind::AgentRunning)
        );

        let observation = reduce_agent_status_values(["parked", "done", "ask"]);

        assert_eq!(
            observation.map(|observation| observation.kind),
            Some(LiveStatusKind::WaitingForApproval)
        );
    }

    #[test]
    fn pane_classifier_uses_final_prompt_over_stale_running_history() {
        let pane = "\
The targeted checks pass. I’m continuing the cherry-pick now.
The rebased commit is created. I’m running the full pre-PR parity script now.
All pre-PR checks passed.
matt@Matts-MacBook-Pro ajax-tech-debt %";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::Done);
    }

    #[test]
    fn pane_classifier_uses_later_success_over_stale_failure_history() {
        let pane = "\
Earlier command failed with exit code 101.
I fixed the issue and reran the full suite.
All pre-PR checks passed.
matt@Matts-MacBook-Pro ajax-tech-debt %";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::Done);
    }

    #[test]
    fn pane_classifier_uses_final_prompt_over_stale_approval_history() {
        let pane = "\
Do you want to proceed? y/n
Approved and continued.
No more work is running.
matt@Matts-MacBook-Pro ajax-tech-debt %";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::ShellIdle);
    }

    #[test]
    fn pane_classifier_treats_plan_approval_prompt_as_waiting_for_approval() {
        let pane = "\
Task 1: Badge accessibility + duplication cleanup

- Test to write: add failing Vitest coverage.
- Code to implement: extract a small internal badge-rendering helper.
- Verify: run rtk npm test -- badges.test.ts.

Plan ready. Approve to proceed.";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::WaitingForApproval);
    }

    #[test]
    fn pane_classifier_treats_idle_codex_prompt_as_waiting_for_input() {
        let pane = "\
› Improve documentation in @filename

  gpt-5.5 high · ~/Desktop/Projects/ajax-cli__worktrees/ajax-spaghetti";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::WaitingForInput);
    }

    #[test]
    fn pane_classifier_treats_codex_working_prompt_as_agent_running() {
        let pane = "\
› Improve documentation in @filename

• codex is working

  gpt-5.5 high · ~/Desktop/Projects/ajax-cli__worktrees/ajax-spaghetti";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::AgentRunning);
    }

    #[test]
    fn pane_classifier_treats_codex_working_status_prompt_as_agent_running() {
        let pane = "\
• Working (3m 00s • esc to interrupt) · 1 background terminal running · /ps to…

› Improve documentation in @filename

  gpt-5.5 high · ~/Desktop/Projects/ajax-cli__worktrees/ajax-ci";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::AgentRunning);
    }

    #[test]
    fn pane_classifier_treats_codex_background_terminal_status_as_agent_running() {
        for pane in [
            "\
1 background terminal running · /ps to view · /stop to close

› Write tests for @filename

  gpt-5.5 high fast · ~/Desktop/Projects/autodoctor__worktrees/ajax-false-positive",
            "\
• Waiting for background terminal (20m 21s • esc to interrupt) · 1 background …

› Improve documentation in @filename

  gpt-5.5 high · ~/Desktop/Projects/ajax-cli__worktrees/ajax-ci",
        ] {
            let observation = classify_pane(pane);

            assert_eq!(observation.kind, LiveStatusKind::AgentRunning, "{pane}");
        }
    }

    #[test]
    fn pane_classifier_does_not_treat_negative_done_phrasing_as_complete() {
        let pane = "The task is not done yet; running cargo test now";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::TestsRunning);
    }

    #[test]
    fn pane_classifier_uses_current_failure_over_stale_success_history() {
        let pane = "\
All pre-PR checks passed.
Later validation found a regression.
Command failed with exit code 101
matt@Matts-MacBook-Pro ajax-tech-debt %";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::CommandFailed);
    }

    #[test]
    fn pane_classifier_does_not_treat_login_task_text_as_auth_required() {
        let pane = "\
Task: Fix login form alignment
Review the button spacing.
matt@Matts-MacBook-Pro ajax-fix-login %";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::ShellIdle);
    }

    #[test]
    fn missing_resource_observations_clear_agent_running() {
        for status in [
            LiveStatusKind::WorktreeMissing,
            LiveStatusKind::TmuxMissing,
            LiveStatusKind::WorktrunkMissing,
        ] {
            let mut task = base_task();
            task.agent_status = AgentRuntimeStatus::Running;
            task.add_side_flag(SideFlag::AgentRunning);

            apply_observation(&mut task, LiveObservation::new(status, "resource missing"));

            assert_eq!(task.agent_status, AgentRuntimeStatus::Unknown, "{status:?}");
            assert!(!task.has_side_flag(SideFlag::AgentRunning), "{status:?}");
        }
    }

    #[test]
    fn running_observation_does_not_override_missing_resources() {
        let mut task = base_task();
        task.add_side_flag(SideFlag::WorktreeMissing);

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Unknown);
        assert!(!task.has_side_flag(SideFlag::AgentRunning));
        assert!(task.has_side_flag(SideFlag::WorktreeMissing));
    }

    #[test]
    fn recovered_missing_resource_can_accept_new_live_status() {
        let mut task = base_task();

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::WorktreeMissing, "worktree missing"),
        );
        task.remove_side_flag(SideFlag::WorktreeMissing);
        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Running);
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::AgentRunning)
        );
        assert!(!task.has_side_flag(SideFlag::WorktreeMissing));
    }

    #[test]
    fn done_observation_is_not_downgraded_by_shell_idle() {
        let mut task = base_task();

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::Done, "done"),
        );
        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::ShellIdle, "shell idle"),
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Done);
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::Done)
        );
    }

    #[test]
    fn waiting_observation_is_not_downgraded_by_passive_terminal_evidence() {
        for status in [LiveStatusKind::ShellIdle, LiveStatusKind::Unknown] {
            let mut task = base_task();

            apply_observation(
                &mut task,
                LiveObservation::new(LiveStatusKind::WaitingForApproval, "waiting for approval"),
            );
            apply_observation(&mut task, LiveObservation::new(status, "passive evidence"));

            assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting, "{status:?}");
            assert!(task.has_side_flag(SideFlag::NeedsInput), "{status:?}");
            assert_eq!(
                task.live_status
                    .as_ref()
                    .map(|live_status| live_status.kind),
                Some(LiveStatusKind::WaitingForApproval),
                "{status:?}"
            );
        }
    }

    #[test]
    fn waiting_for_approval_is_cleared_by_resumed_activity() {
        for status in [
            LiveStatusKind::AgentRunning,
            LiveStatusKind::CommandRunning,
            LiveStatusKind::TestsRunning,
        ] {
            let mut task = base_task();

            apply_observation(
                &mut task,
                LiveObservation::new(LiveStatusKind::WaitingForApproval, "waiting for approval"),
            );
            apply_observation(&mut task, LiveObservation::new(status, "activity resumed"));

            assert_eq!(task.agent_status, AgentRuntimeStatus::Running, "{status:?}");
            assert!(!task.has_side_flag(SideFlag::NeedsInput), "{status:?}");
            assert!(task.has_side_flag(SideFlag::AgentRunning), "{status:?}");
            assert_eq!(
                task.live_status
                    .as_ref()
                    .map(|live_status| live_status.kind),
                Some(status),
                "{status:?}"
            );
        }
    }

    #[test]
    fn failed_observation_is_not_downgraded_by_later_output() {
        let mut task = base_task();

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::CommandFailed, "command failed"),
        );
        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::CommandRunning, "command running"),
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Blocked);
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::CommandFailed)
        );
    }

    #[test]
    fn merge_conflict_flag_is_cleared_by_later_input_prompt() {
        let mut task = base_task();

        apply_observation(
            &mut task,
            LiveObservation::new(
                LiveStatusKind::MergeConflict,
                "merge conflict needs attention",
            ),
        );
        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input"),
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting);
        assert!(task.has_side_flag(SideFlag::NeedsInput));
        assert!(!task.has_side_flag(SideFlag::Conflicted));
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::WaitingForInput)
        );
    }

    #[test]
    fn live_lifecycle_updates_ignore_invalid_transition_edges() {
        let mut task = base_task();
        task.lifecycle_status = crate::models::LifecycleStatus::Error;

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::WaitingForApproval, "waiting for approval"),
        );

        assert_eq!(task.lifecycle_status, crate::models::LifecycleStatus::Error);
        assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting);
        assert!(task.has_side_flag(SideFlag::NeedsInput));
    }

    #[test]
    fn ci_failed_observation_marks_task_blocked_and_tests_failed() {
        let mut task = base_task();

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::CiFailed, "ci failed"),
        );
        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Blocked);
        assert!(!task.has_side_flag(SideFlag::NeedsInput));
        assert!(task.has_side_flag(SideFlag::TestsFailed));
        assert!(!task.has_side_flag(SideFlag::AgentRunning));
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::CiFailed)
        );
    }

    #[test]
    fn active_live_observation_refreshes_activity_and_clears_stale() {
        let mut task = base_task();
        task.last_activity_at = std::time::SystemTime::UNIX_EPOCH;
        task.add_side_flag(SideFlag::Stale);

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert!(!task.has_side_flag(SideFlag::Stale));
        assert!(task.last_activity_at > std::time::SystemTime::UNIX_EPOCH);
    }

    #[test]
    fn live_projection_functions_do_not_mutate_lifecycle_or_substrate() {
        let task = base_task();
        let lifecycle_before = task.lifecycle_status;
        let git_before = task.git_status.clone();
        let tmux_before = task.tmux_status.clone();
        let worktrunk_before = task.worktrunk_status.clone();

        let classified = classify_pane("Do you want to proceed? y/n\n");
        let reduced = super::reduce_live_observation(
            task.live_status.as_ref(),
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert_eq!(classified.kind, LiveStatusKind::WaitingForApproval);
        assert_eq!(reduced.kind, LiveStatusKind::AgentRunning);
        assert_eq!(task.lifecycle_status, lifecycle_before);
        assert_eq!(task.git_status, git_before);
        assert_eq!(task.tmux_status, tmux_before);
        assert_eq!(task.worktrunk_status, worktrunk_before);
    }

    #[test]
    fn live_projection_module_does_not_own_lifecycle_mutation() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/live.rs"),
        )
        .unwrap();

        let transition_call = ["transition", "_lifecycle("].concat();
        let transition_reason = ["Lifecycle", "TransitionReason"].concat();

        assert!(!source.contains(&transition_call));
        assert!(!source.contains(&transition_reason));
    }
}

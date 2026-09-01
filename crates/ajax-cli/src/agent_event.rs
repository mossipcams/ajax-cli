use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use ajax_core::canonical_agent_event::{
    ActivityKind, AttentionReason, CanonicalEventDetail, CanonicalEventKind, ParsedEnvelope,
    TurnOutcome,
};
use clap::ArgMatches;
use serde::Serialize;

use crate::{agent_runtime, CliError};

static EVENT_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CanonicalAgentEvent {
    pub kind: CanonicalEventKind,
    pub detail: Option<CanonicalEventDetail>,
}

#[derive(Serialize)]
struct AgentEventEnvelope<'a> {
    schema_version: u32,
    event_id: String,
    task_id: &'a str,
    run_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_run_id: Option<String>,
    client: &'a str,
    native_event: &'a str,
    kind: CanonicalEventKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<CanonicalEventDetail>,
    occurred_at_unix_millis: u128,
    received_at_unix_millis: u128,
    source: &'static str,
}

pub(crate) struct AgentEventIdentity {
    pub task_id: String,
    pub run_id: String,
    pub events_dir: PathBuf,
}

pub(crate) enum AgentEventOutcome {
    NoIdentity,
    Ignored,
    RejectedByRuntime,
    Appended,
}

#[derive(Debug)]
pub(crate) enum AgentEventError {
    Runtime(CliError),
    Io(io::Error),
}

pub(crate) fn run_agent_event_command(matches: &ArgMatches) -> Result<String, CliError> {
    let client = matches
        .get_one::<String>("client")
        .map(String::as_str)
        .unwrap_or("");
    let event = matches
        .get_one::<String>("event")
        .map(String::as_str)
        .unwrap_or("");
    let payload = read_stdin_payload();
    let identity = resolve_agent_event_identity(client, &payload);
    match run_agent_event(identity.as_ref(), client, event, &payload) {
        Ok(AgentEventOutcome::NoIdentity) | Ok(AgentEventOutcome::Ignored) => {}
        Ok(AgentEventOutcome::RejectedByRuntime) => {}
        Ok(AgentEventOutcome::Appended) => {}
        Err(AgentEventError::Runtime(error)) => {
            return Err(error);
        }
        Err(AgentEventError::Io(error)) => {
            return Err(CliError::CommandFailed(format!(
                "agent event write failed: {error}"
            )));
        }
    }
    if client == "cursor" && event == "sessionStart" {
        if let Some(identity) = identity {
            return Ok(session_start_env_stdout(&identity));
        }
    }
    Ok(String::new())
}

pub(crate) fn run_agent_event(
    identity: Option<&AgentEventIdentity>,
    client: &str,
    event: &str,
    payload: &serde_json::Value,
) -> Result<AgentEventOutcome, AgentEventError> {
    let Some(identity) = identity else {
        return Ok(AgentEventOutcome::NoIdentity);
    };
    let Some(canonical) = translate_native_event(client, event, payload) else {
        return Ok(AgentEventOutcome::Ignored);
    };
    let observed_at = agent_runtime::now_millis().map_err(AgentEventError::Runtime)?;
    if !agent_runtime::runtime_hooks_accepted(
        &identity.events_dir,
        &identity.task_id,
        &canonical.kind,
        observed_at,
    ) {
        return Ok(AgentEventOutcome::RejectedByRuntime);
    }
    append_agent_event_jsonl(
        identity,
        client,
        event,
        &canonical,
        observed_at,
        observed_at,
    )
    .map_err(AgentEventError::Io)?;
    Ok(AgentEventOutcome::Appended)
}

pub(crate) fn translate_native_event(
    client: &str,
    event: &str,
    payload: &serde_json::Value,
) -> Option<CanonicalAgentEvent> {
    match (client, event) {
        ("claude", "UserPromptSubmit") => Some(turn_started()),
        ("claude", "PreToolUse") => {
            if claude_tool_name(payload) == "AskUserQuestion" {
                Some(attention_requested(AttentionReason::Question))
            } else {
                Some(activity_started(payload))
            }
        }
        ("claude", "PostToolUse") => {
            if claude_tool_name(payload) == "AskUserQuestion" {
                Some(attention_cleared())
            } else {
                Some(activity_finished(payload))
            }
        }
        ("claude", "Notification") => Some(claude_notification(payload)),
        ("claude", "Notification:permission_prompt") => {
            Some(attention_requested(AttentionReason::Permission))
        }
        ("claude", "Notification:elicitation_dialog")
        | ("claude", "Notification:agent_needs_input") => {
            Some(attention_requested(AttentionReason::Question))
        }
        ("claude", "Notification:idle_prompt") => {
            Some(attention_requested(AttentionReason::Question))
        }
        ("claude", "Notification:agent_completed") => Some(turn_settled(TurnOutcome::Completed)),
        ("claude", "Stop") => Some(claude_stop(payload)),
        // Rate-limit / API-error turn ends are not task failures; Failed would project Error.
        ("claude", "StopFailure") => Some(turn_settled(TurnOutcome::Interrupted)),
        ("claude", "SessionStart") => Some(session_opened()),
        ("claude", "SessionEnd") => Some(session_closed()),
        ("codex", "UserPromptSubmit") => Some(turn_started()),
        ("codex", "PreToolUse") => Some(activity_started(payload)),
        ("codex", "PostToolUse") => Some(activity_finished(payload)),
        ("codex", "PermissionRequest") => Some(attention_requested(AttentionReason::Permission)),
        ("codex", "Stop") => Some(turn_settled(TurnOutcome::Completed)),
        ("codex", "SessionStart") => Some(session_opened()),
        ("codex", "SessionEnd") => Some(session_closed()),
        ("cursor", "beforeSubmitPrompt") => Some(turn_started()),
        ("cursor", "preToolUse") => Some(activity_started(payload)),
        ("cursor", "postToolUse") => Some(activity_finished(payload)),
        ("cursor", "postToolUseFailure") => Some(activity_finished(payload)),
        ("cursor", "beforeShellExecution") | ("cursor", "beforeMCPExecution") => {
            Some(attention_requested(AttentionReason::Permission))
        }
        ("cursor", "subagentStart") => Some(child_started()),
        ("cursor", "subagentStop") => Some(child_settled()),
        ("cursor", "stop") => Some(cursor_stop(payload)),
        ("cursor", "Notification:permission_prompt") => {
            Some(attention_requested(AttentionReason::Permission))
        }
        ("cursor", "Notification:elicitation_dialog") => {
            Some(attention_requested(AttentionReason::Question))
        }
        ("cursor", "ElicitationResult") => Some(turn_started()),
        ("cursor", "sessionStart") => Some(session_opened()),
        ("cursor", "sessionEnd") => Some(session_closed()),
        ("pi", "before_agent_start") => Some(turn_started()),
        ("pi", "agent_settled") => Some(turn_settled(TurnOutcome::Completed)),
        _ => None,
    }
}

fn turn_started() -> CanonicalAgentEvent {
    CanonicalAgentEvent {
        kind: CanonicalEventKind::TurnStarted,
        detail: None,
    }
}

fn activity_started(payload: &serde_json::Value) -> CanonicalAgentEvent {
    CanonicalAgentEvent {
        kind: CanonicalEventKind::ActivityStarted,
        detail: Some(CanonicalEventDetail::Activity {
            activity: ActivityKind::Tool,
            activity_id: activity_id_from_payload(payload),
        }),
    }
}

fn activity_finished(payload: &serde_json::Value) -> CanonicalAgentEvent {
    CanonicalAgentEvent {
        kind: CanonicalEventKind::ActivityFinished,
        detail: Some(CanonicalEventDetail::Activity {
            activity: ActivityKind::Tool,
            activity_id: activity_id_from_payload(payload),
        }),
    }
}

fn attention_requested(reason: AttentionReason) -> CanonicalAgentEvent {
    CanonicalAgentEvent {
        kind: CanonicalEventKind::AttentionRequested,
        detail: Some(CanonicalEventDetail::Attention { attention: reason }),
    }
}

fn attention_cleared() -> CanonicalAgentEvent {
    CanonicalAgentEvent {
        kind: CanonicalEventKind::AttentionCleared,
        detail: None,
    }
}

fn child_started() -> CanonicalAgentEvent {
    CanonicalAgentEvent {
        kind: CanonicalEventKind::ChildStarted,
        detail: None,
    }
}

fn child_settled() -> CanonicalAgentEvent {
    CanonicalAgentEvent {
        kind: CanonicalEventKind::ChildSettled,
        detail: None,
    }
}

fn turn_settled(outcome: TurnOutcome) -> CanonicalAgentEvent {
    CanonicalAgentEvent {
        kind: CanonicalEventKind::TurnSettled,
        detail: Some(CanonicalEventDetail::Outcome { outcome }),
    }
}

fn session_opened() -> CanonicalAgentEvent {
    CanonicalAgentEvent {
        kind: CanonicalEventKind::SessionOpened,
        detail: None,
    }
}

fn session_closed() -> CanonicalAgentEvent {
    CanonicalAgentEvent {
        kind: CanonicalEventKind::SessionClosed,
        detail: None,
    }
}

fn claude_tool_name(payload: &serde_json::Value) -> &str {
    payload
        .get("tool_name")
        .and_then(|value| value.as_str())
        .unwrap_or("")
}

fn claude_notification(payload: &serde_json::Value) -> CanonicalAgentEvent {
    let message = payload
        .get("message")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if message.to_ascii_lowercase().contains("permission") {
        attention_requested(AttentionReason::Permission)
    } else {
        attention_requested(AttentionReason::Question)
    }
}

fn claude_stop(payload: &serde_json::Value) -> CanonicalAgentEvent {
    if payload
        .get("background_tasks")
        .and_then(|value| value.as_array())
        .is_some_and(|tasks| !tasks.is_empty())
    {
        CanonicalAgentEvent {
            kind: CanonicalEventKind::TurnStarted,
            detail: None,
        }
    } else {
        turn_settled(TurnOutcome::Completed)
    }
}

fn cursor_stop(payload: &serde_json::Value) -> CanonicalAgentEvent {
    let status = payload
        .get("status")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let outcome = match status {
        "error" => TurnOutcome::Failed,
        "aborted" => TurnOutcome::Interrupted,
        _ => TurnOutcome::Completed,
    };
    turn_settled(outcome)
}

fn activity_id_from_payload(payload: &serde_json::Value) -> Option<String> {
    ["tool_call_id", "tool_id", "id", "tool_name", "tool"]
        .iter()
        .find_map(|key| payload.get(*key).and_then(|value| value.as_str()))
        .map(str::to_string)
}

fn append_agent_event_jsonl(
    identity: &AgentEventIdentity,
    client: &str,
    native_event: &str,
    canonical: &CanonicalAgentEvent,
    occurred_at_unix_millis: u128,
    received_at_unix_millis: u128,
) -> io::Result<()> {
    fs::create_dir_all(&identity.events_dir)?;
    let seq = EVENT_SEQ.fetch_add(1, Ordering::Relaxed);
    let event_id = format!("{}-{}-{}", received_at_unix_millis, std::process::id(), seq);
    let parent_run_id = if identity.run_id == "primary" {
        None
    } else {
        Some("primary".to_string())
    };
    let envelope = AgentEventEnvelope {
        schema_version: 1,
        event_id,
        task_id: &identity.task_id,
        run_id: &identity.run_id,
        parent_run_id,
        client,
        native_event,
        kind: canonical.kind.clone(),
        detail: canonical.detail.clone(),
        occurred_at_unix_millis,
        received_at_unix_millis,
        source: "native_hook",
    };
    let line = serde_json::to_string(&envelope).map_err(io::Error::other)?;
    let stem = agent_runtime::task_file_stem(&identity.task_id);
    let jsonl_path = identity.events_dir.join(format!("{stem}.jsonl"));
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(jsonl_path)?;
    writeln!(file, "{line}")?;
    try_notify_socket(&notify_socket_path(&identity.events_dir), line.as_bytes());
    Ok(())
}

pub(crate) fn notify_socket_path(events_dir: &Path) -> PathBuf {
    #[cfg(test)]
    if let Some(path) = test_notify_socket_override() {
        return path;
    }
    if let Ok(path) = std::env::var("AJAX_AGENT_EVENTS_SOCKET") {
        if !path.is_empty() {
            return PathBuf::from(path);
        }
    }
    events_dir.join("notify.sock")
}

#[cfg(test)]
thread_local! {
    static TEST_NOTIFY_SOCKET_OVERRIDE: std::cell::RefCell<Option<PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn test_notify_socket_override() -> Option<PathBuf> {
    TEST_NOTIFY_SOCKET_OVERRIDE.with(|cell| cell.borrow().clone())
}

#[cfg(test)]
fn set_test_notify_socket_override(path: Option<PathBuf>) {
    TEST_NOTIFY_SOCKET_OVERRIDE.with(|cell| *cell.borrow_mut() = path);
}

#[cfg(unix)]
fn try_notify_socket(path: &Path, line: &[u8]) {
    use std::os::unix::net::UnixStream;

    if let Ok(mut stream) = UnixStream::connect(path) {
        let _ = stream.write_all(line);
        let _ = stream.write_all(b"\n");
    }
}

#[cfg(not(unix))]
fn try_notify_socket(_path: &Path, _line: &[u8]) {}

fn read_stdin_payload() -> serde_json::Value {
    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() || input.trim().is_empty() {
        return serde_json::Value::Null;
    }
    serde_json::from_str(&input).unwrap_or(serde_json::Value::Null)
}

pub(crate) fn parse_envelopes_from_jsonl(path: &Path) -> Vec<ParsedEnvelope> {
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    content
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn read_agent_event_identity() -> Option<AgentEventIdentity> {
    let task_id = std::env::var("AJAX_TASK_ID").ok()?;
    if task_id.is_empty() {
        return None;
    }
    let events_dir = std::env::var("AJAX_AGENT_EVENTS_DIR").ok()?;
    if events_dir.is_empty() {
        return None;
    }
    let run_id = std::env::var("AJAX_RUN_ID").unwrap_or_else(|_| "primary".to_string());
    Some(AgentEventIdentity {
        task_id,
        run_id,
        events_dir: PathBuf::from(events_dir),
    })
}

pub(crate) fn resolve_agent_event_identity(
    client: &str,
    payload: &serde_json::Value,
) -> Option<AgentEventIdentity> {
    if let Some(identity) = read_agent_event_identity() {
        return Some(identity);
    }

    let project_dir = cursor_project_dir(payload);

    if let Ok(events_dir) = std::env::var("AJAX_AGENT_EVENTS_DIR") {
        if !events_dir.is_empty() {
            if let Some(project_dir) = project_dir.as_deref() {
                if let Some(entry) = read_cwd_index_entry(Path::new(&events_dir), project_dir) {
                    return Some(cwd_index_entry_to_identity(entry));
                }
            }
        }
    }

    if client == "cursor" {
        if let Some(project_dir) = project_dir {
            return resolve_cursor_identity(
                &project_dir,
                payload,
                std::env::var_os("HOME").map(PathBuf::from).as_deref(),
                std::env::var_os("AJAX_HOME").map(PathBuf::from).as_deref(),
            );
        }
    }

    None
}

fn cursor_project_dir(payload: &serde_json::Value) -> Option<String> {
    if let Ok(project_dir) = std::env::var("CURSOR_PROJECT_DIR") {
        if !project_dir.is_empty() {
            return Some(project_dir);
        }
    }
    payload
        .get("workspace_roots")
        .and_then(|value| value.get(0))
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

fn read_cwd_index_entry(
    events_dir: &Path,
    project_dir: &str,
) -> Option<agent_runtime::CwdIndexEntry> {
    let project = Path::new(project_dir);
    let project = fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
    let index_path = agent_runtime::cwd_index_path(events_dir, &project);
    let content = fs::read_to_string(&index_path).ok()?;
    let entry: agent_runtime::CwdIndexEntry = serde_json::from_str(&content).ok()?;
    if entry.task_id.is_empty() || entry.events_dir.is_empty() {
        return None;
    }
    Some(entry)
}

fn cwd_index_entry_to_identity(entry: agent_runtime::CwdIndexEntry) -> AgentEventIdentity {
    AgentEventIdentity {
        task_id: entry.task_id,
        run_id: if entry.run_id.is_empty() {
            "primary".to_string()
        } else {
            entry.run_id
        },
        events_dir: PathBuf::from(entry.events_dir),
    }
}

pub(crate) fn resolve_cursor_identity(
    project_dir: &str,
    _payload: &serde_json::Value,
    home: Option<&Path>,
    ajax_home: Option<&Path>,
) -> Option<AgentEventIdentity> {
    for events_dir in cursor_identity_discovery_roots(project_dir, home, ajax_home) {
        if let Some(entry) = read_cwd_index_entry(&events_dir, project_dir) {
            return Some(cwd_index_entry_to_identity(entry));
        }
    }
    None
}

fn cursor_identity_discovery_roots(
    project_dir: &str,
    home: Option<&Path>,
    ajax_home: Option<&Path>,
) -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from(project_dir).join(".cache/ajax/agent-events")];
    if let Some(ajax_home) = ajax_home {
        roots.push(ajax_home.join("cache/agent-events"));
    }
    if let Some(home) = home {
        // Stable/XDG cache (RuntimePaths default), then profile homes.
        roots.push(home.join(".cache/ajax/agent-events"));
        roots.push(home.join(".ajax-dev/cache/agent-events"));
        roots.push(home.join(".ajax/cache/agent-events"));
    }
    roots
}

pub(crate) fn session_start_env_stdout(identity: &AgentEventIdentity) -> String {
    serde_json::json!({
        "env": {
            "AJAX_TASK_ID": identity.task_id,
            "AJAX_RUN_ID": identity.run_id,
            "AJAX_AGENT_EVENTS_DIR": identity.events_dir.to_string_lossy(),
        }
    })
    .to_string()
}

#[cfg(test)]
mod tests;

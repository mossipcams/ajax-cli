use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use ajax_core::canonical_agent_event::{
    fold_envelopes, project_snapshot, ActivityKind, AttentionReason, CanonicalEventDetail,
    CanonicalEventKind, ParsedEnvelope, TurnOutcome,
};
use clap::ArgMatches;
use serde::{Deserialize, Serialize};

use crate::{agent_runtime, CliError};

static EVENT_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub(crate) struct AgentEventSnapshot {
    pub task_id: String,
    pub run_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<String>,
    pub value: String,
    pub observed_at_unix_millis: u128,
}

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
    let _ = run_agent_event(identity.as_ref(), client, event, &payload);
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
) -> Result<(), ()> {
    let Some(identity) = identity else {
        return Ok(());
    };
    let Some(canonical) = translate_native_event(client, event, payload) else {
        return Ok(());
    };
    let observed_at = agent_runtime::now_millis().map_err(|_| ())?;
    if !agent_runtime::runtime_hooks_accepted(
        &identity.events_dir,
        &identity.task_id,
        &canonical.kind,
        observed_at,
    ) {
        return Ok(());
    }
    append_agent_event_jsonl(
        identity,
        client,
        event,
        &canonical,
        observed_at,
        observed_at,
    )
    .map_err(|_| ())?;
    if should_update_legacy_snapshot(&canonical.kind) {
        if let Some(value) = project_legacy_value(&canonical) {
            write_agent_event(identity, value, observed_at).map_err(|_| ())?;
        }
    }
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn translate_agent_event(
    client: &str,
    event: &str,
    payload: &serde_json::Value,
) -> Option<&'static str> {
    translate_native_event(client, event, payload)
        .and_then(|canonical| project_legacy_value(&canonical))
}

pub(crate) fn translate_native_event(
    client: &str,
    event: &str,
    payload: &serde_json::Value,
) -> Option<CanonicalAgentEvent> {
    match (client, event) {
        ("claude", "UserPromptSubmit") => Some(turn_started()),
        ("claude", "PreToolUse") => Some(activity_started(payload)),
        ("claude", "PostToolUse") => Some(activity_finished(payload)),
        ("claude", "Notification") => Some(claude_notification(payload)),
        ("claude", "Stop") => Some(claude_stop(payload)),
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
        ("cursor", "stop") => Some(cursor_stop(payload)),
        ("cursor", "sessionStart") => Some(session_opened()),
        ("cursor", "sessionEnd") => Some(session_closed()),
        ("pi", "before_agent_start") => Some(turn_started()),
        ("pi", "agent_settled") => Some(turn_settled(TurnOutcome::Completed)),
        _ => None,
    }
}

pub(crate) fn project_legacy_value(canonical: &CanonicalAgentEvent) -> Option<&'static str> {
    match (&canonical.kind, canonical.detail.as_ref()) {
        (CanonicalEventKind::TurnStarted | CanonicalEventKind::ActivityStarted, _) => {
            Some("working")
        }
        (
            CanonicalEventKind::AttentionRequested,
            Some(CanonicalEventDetail::Attention {
                attention: AttentionReason::Permission,
            }),
        ) => Some("ask"),
        (CanonicalEventKind::AttentionRequested, _) => Some("wait"),
        (CanonicalEventKind::AttentionCleared, _) => Some("working"),
        (
            CanonicalEventKind::TurnSettled,
            Some(CanonicalEventDetail::Outcome {
                outcome: TurnOutcome::Failed,
            }),
        ) => Some("failed"),
        (CanonicalEventKind::TurnSettled, _) => Some("done"),
        (CanonicalEventKind::SessionOpened | CanonicalEventKind::ChildStarted, _) => {
            Some("working")
        }
        (CanonicalEventKind::SessionClosed | CanonicalEventKind::ChildSettled, _) => Some("done"),
        (CanonicalEventKind::ActivityFinished | CanonicalEventKind::Heartbeat, _) => None,
    }
}

fn should_update_legacy_snapshot(kind: &CanonicalEventKind) -> bool {
    !matches!(
        kind,
        CanonicalEventKind::ActivityFinished | CanonicalEventKind::Heartbeat
    )
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

pub(crate) fn write_agent_event(
    identity: &AgentEventIdentity,
    value: &str,
    observed_at_unix_millis: u128,
) -> io::Result<()> {
    fs::create_dir_all(&identity.events_dir)?;
    let parent_run_id = if identity.run_id == "primary" {
        None
    } else {
        Some("primary".to_string())
    };
    let snapshot = AgentEventSnapshot {
        task_id: identity.task_id.clone(),
        run_id: identity.run_id.clone(),
        parent_run_id,
        value: value.to_string(),
        observed_at_unix_millis,
    };
    let encoded = serde_json::to_vec(&snapshot).map_err(io::Error::other)?;
    let stem = agent_runtime::task_file_stem(&identity.task_id);
    let latest_path = identity.events_dir.join(format!("{stem}.json"));
    let temporary_path = identity
        .events_dir
        .join(format!(".{stem}.tmp-{}", std::process::id()));
    fs::write(&temporary_path, &encoded)?;
    fs::rename(&temporary_path, &latest_path)
}

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

pub(crate) fn fold_and_project_jsonl(path: &Path) -> Option<(String, u128)> {
    let envelopes = parse_envelopes_from_jsonl(path);
    if envelopes.is_empty() {
        return None;
    }
    let max_received_at = envelopes
        .iter()
        .map(|event| event.received_at_unix_millis)
        .max()?;
    let snapshot = fold_envelopes(&envelopes);
    let value = project_snapshot(&snapshot)?;
    Some((value.to_string(), max_received_at))
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
mod tests {
    use std::fs;

    use ajax_core::canonical_agent_event::{CanonicalEventDetail, CanonicalEventKind, TurnOutcome};

    use crate::agent_runtime::{self, AgentRuntimeSnapshot, AgentRuntimeState};

    use super::{
        resolve_cursor_identity, run_agent_event, session_start_env_stdout, translate_agent_event,
        translate_native_event, write_agent_event, AgentEventIdentity, AgentEventSnapshot,
    };

    fn temp_events_fixture(label: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "ajax-agent-event-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let events_dir = root.join("agent-events");
        fs::create_dir_all(&events_dir).unwrap();
        (root, events_dir)
    }

    fn test_identity(dir: &std::path::Path, task_id: &str) -> AgentEventIdentity {
        AgentEventIdentity {
            task_id: task_id.to_string(),
            run_id: "primary".to_string(),
            events_dir: dir.to_path_buf(),
        }
    }

    #[test]
    fn translate_claude_stop_with_background_tasks_stays_working() {
        let with_tasks = serde_json::json!({"background_tasks":[{"id":1}]});
        assert_eq!(
            translate_agent_event("claude", "Stop", &with_tasks),
            Some("working")
        );

        let empty_tasks = serde_json::json!({"background_tasks":[]});
        assert_eq!(
            translate_agent_event("claude", "Stop", &empty_tasks),
            Some("done")
        );

        let missing_key = serde_json::json!({});
        assert_eq!(
            translate_agent_event("claude", "Stop", &missing_key),
            Some("done")
        );
    }

    #[test]
    fn claude_stop_with_background_tasks_does_not_settle() {
        let with_tasks = serde_json::json!({"background_tasks":[{"id":1}]});
        let canonical = translate_native_event("claude", "Stop", &with_tasks).unwrap();
        assert_eq!(canonical.kind, CanonicalEventKind::TurnStarted);
        assert_ne!(canonical.kind, CanonicalEventKind::TurnSettled);
        assert_eq!(
            translate_agent_event("claude", "Stop", &with_tasks),
            Some("working")
        );
    }

    #[test]
    fn cursor_stop_error_projects_failed() {
        let payload = serde_json::json!({"status":"error"});
        assert_eq!(
            translate_agent_event("cursor", "stop", &payload),
            Some("failed")
        );
        let canonical = translate_native_event("cursor", "stop", &payload).unwrap();
        assert_eq!(canonical.kind, CanonicalEventKind::TurnSettled);
        assert_eq!(
            canonical.detail,
            Some(CanonicalEventDetail::Outcome {
                outcome: TurnOutcome::Failed
            })
        );
    }

    #[test]
    fn translate_claude_notification_permission_vs_idle() {
        let permission = serde_json::json!({
            "message": "Claude needs your permission to run Bash"
        });
        assert_eq!(
            translate_agent_event("claude", "Notification", &permission),
            Some("ask")
        );

        let idle = serde_json::json!({"message": "waiting for your input"});
        assert_eq!(
            translate_agent_event("claude", "Notification", &idle),
            Some("wait")
        );
    }

    #[test]
    fn translate_codex_and_pi_verified_events() {
        let payload = serde_json::json!({});
        assert_eq!(
            translate_agent_event("codex", "UserPromptSubmit", &payload),
            Some("working")
        );
        assert_eq!(
            translate_agent_event("codex", "Stop", &payload),
            Some("done")
        );
        assert_eq!(
            translate_agent_event("codex", "PermissionRequest", &payload),
            Some("ask")
        );
        assert_eq!(
            translate_agent_event("cursor", "preToolUse", &payload),
            Some("working")
        );
        assert_eq!(
            translate_agent_event("cursor", "postToolUse", &payload),
            None
        );
        assert_eq!(
            translate_agent_event("pi", "agent_settled", &payload),
            Some("done")
        );
        assert_eq!(translate_agent_event("pi", "agent_end", &payload), None);
    }

    #[test]
    fn translate_session_start_end_projects_working_done() {
        let payload = serde_json::json!({});
        assert_eq!(
            translate_agent_event("claude", "SessionStart", &payload),
            Some("working")
        );
        assert_eq!(
            translate_agent_event("claude", "SessionEnd", &payload),
            Some("done")
        );
        assert_eq!(
            translate_agent_event("codex", "SessionStart", &payload),
            Some("working")
        );
        assert_eq!(
            translate_agent_event("codex", "SessionEnd", &payload),
            Some("done")
        );
        assert_eq!(
            translate_agent_event("cursor", "sessionStart", &payload),
            Some("working")
        );
        assert_eq!(
            translate_agent_event("cursor", "sessionEnd", &payload),
            Some("done")
        );
    }

    #[test]
    fn translate_ignores_unknown_events() {
        assert_eq!(
            translate_agent_event("cursor", "subagentStop", &serde_json::json!({})),
            None
        );
        assert_eq!(
            translate_agent_event("nope", "stop", &serde_json::json!({})),
            None
        );
    }

    #[test]
    fn write_appends_jsonl_and_updates_legacy_snapshot() {
        let (root, dir) = temp_events_fixture("jsonl-dual-write");
        write_test_runtime_snapshot(&dir, "web/fix-login", AgentRuntimeState::Running, 1);
        let identity = test_identity(&dir, "web/fix-login");

        run_agent_event(
            Some(&identity),
            "claude",
            "UserPromptSubmit",
            &serde_json::json!({}),
        )
        .unwrap();

        let stem = "web__fix-login";
        let jsonl_path = dir.join(format!("{stem}.jsonl"));
        let jsonl = fs::read_to_string(&jsonl_path).unwrap();
        let lines: Vec<&str> = jsonl.lines().collect();
        assert_eq!(lines.len(), 1);
        let envelope: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(envelope["schema_version"], 1);
        assert_eq!(envelope["kind"], "turn_started");

        let latest_path = dir.join(format!("{stem}.json"));
        let snapshot: AgentEventSnapshot =
            serde_json::from_str(&fs::read_to_string(&latest_path).unwrap()).unwrap();
        assert_eq!(snapshot.value, "working");

        fs::remove_dir_all(root).unwrap();
    }

    fn write_test_runtime_snapshot(
        events_dir: &std::path::Path,
        task_id: &str,
        state: AgentRuntimeState,
        observed_at_unix_millis: u128,
    ) {
        let runtime_root = events_dir.parent().unwrap().join("agent-runtime");
        fs::create_dir_all(&runtime_root).unwrap();
        let snapshot = AgentRuntimeSnapshot {
            task_id: task_id.to_string(),
            state,
            observed_at_unix_millis,
            pid: Some(42),
            exit_code: None,
            message: None,
        };
        let stem = agent_runtime::task_file_stem(task_id);
        let encoded = serde_json::to_vec(&snapshot).unwrap();
        fs::write(runtime_root.join(format!("{stem}.json")), encoded).unwrap();
    }

    #[test]
    fn activity_finished_appends_jsonl_without_clobbering_ask_snapshot() {
        let (root, dir) = temp_events_fixture("jsonl-no-clobber");
        write_test_runtime_snapshot(&dir, "web/fix-login", AgentRuntimeState::Running, 1);
        let identity = test_identity(&dir, "web/fix-login");
        let permission = serde_json::json!({
            "message": "Claude needs your permission to run Bash"
        });

        run_agent_event(Some(&identity), "claude", "Notification", &permission).unwrap();
        run_agent_event(
            Some(&identity),
            "claude",
            "PostToolUse",
            &serde_json::json!({}),
        )
        .unwrap();

        let stem = "web__fix-login";
        let jsonl = fs::read_to_string(dir.join(format!("{stem}.jsonl"))).unwrap();
        assert_eq!(jsonl.lines().count(), 2);

        let snapshot: AgentEventSnapshot =
            serde_json::from_str(&fs::read_to_string(dir.join(format!("{stem}.json"))).unwrap())
                .unwrap();
        assert_eq!(snapshot.value, "ask");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn write_agent_event_is_atomic_and_task_keyed() {
        let (root, dir) = temp_events_fixture("atomic");
        let identity = AgentEventIdentity {
            task_id: "web/fix-login".to_string(),
            run_id: "primary".to_string(),
            events_dir: dir.clone(),
        };

        write_agent_event(&identity, "done", 1).unwrap();
        write_agent_event(&identity, "working", 2).unwrap();

        let latest_path = dir.join("web__fix-login.json");
        let snapshot: AgentEventSnapshot =
            serde_json::from_str(&fs::read_to_string(&latest_path).unwrap()).unwrap();
        assert_eq!(snapshot.value, "working");
        assert_eq!(snapshot.run_id, "primary");
        assert_eq!(snapshot.parent_run_id, None);

        let tmp_files = fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .count();
        assert_eq!(tmp_files, 0);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn run_agent_event_noop_without_identity() {
        run_agent_event(
            None,
            "claude",
            "Stop",
            &serde_json::json!({"background_tasks":[]}),
        )
        .unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn socket_send_delivers_line_when_listener_present() {
        use std::io::{BufRead, BufReader};
        use std::os::unix::net::UnixListener;
        use std::sync::mpsc;
        use std::thread;
        use std::time::Duration;

        use super::set_test_notify_socket_override;

        let (root, dir) = temp_events_fixture("socket-notify");
        let socket_path = std::path::PathBuf::from(format!(
            "/tmp/ajax-notify-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let listener = UnixListener::bind(&socket_path).unwrap();

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let _ = tx.send(line);
            }
        });

        set_test_notify_socket_override(Some(socket_path.clone()));

        write_test_runtime_snapshot(&dir, "web/fix-login", AgentRuntimeState::Running, 1);
        let identity = test_identity(&dir, "web/fix-login");
        run_agent_event(
            Some(&identity),
            "claude",
            "UserPromptSubmit",
            &serde_json::json!({}),
        )
        .unwrap();

        set_test_notify_socket_override(None);

        let received = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let envelope: serde_json::Value = serde_json::from_str(received.trim()).unwrap();
        assert_eq!(envelope["schema_version"], 1);
        assert_eq!(envelope["kind"], "turn_started");

        let stem = "web__fix-login";
        let jsonl = fs::read_to_string(dir.join(format!("{stem}.jsonl"))).unwrap();
        assert_eq!(jsonl.lines().count(), 1);

        let _ = fs::remove_file(&socket_path);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn run_agent_event_appends_when_runtime_snapshot_running() {
        let (root, dir) = temp_events_fixture("runtime-running");
        write_test_runtime_snapshot(&dir, "web/fix-login", AgentRuntimeState::Running, 1);
        let identity = test_identity(&dir, "web/fix-login");

        run_agent_event(
            Some(&identity),
            "cursor",
            "beforeSubmitPrompt",
            &serde_json::json!({}),
        )
        .unwrap();

        let stem = "web__fix-login";
        let jsonl = fs::read_to_string(dir.join(format!("{stem}.jsonl"))).unwrap();
        assert_eq!(jsonl.lines().count(), 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn run_agent_event_rejects_after_stale_exit_for_non_settle_events() {
        let (root, dir) = temp_events_fixture("runtime-stale-exit");
        let stale_at = agent_runtime::now_millis().unwrap().saturating_sub(60_000);
        write_test_runtime_snapshot(
            &dir,
            "web/fix-login",
            AgentRuntimeState::ExitedSuccess,
            stale_at,
        );
        let identity = test_identity(&dir, "web/fix-login");

        run_agent_event(
            Some(&identity),
            "cursor",
            "preToolUse",
            &serde_json::json!({}),
        )
        .unwrap();

        let stem = "web__fix-login";
        assert!(!dir.join(format!("{stem}.jsonl")).exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn run_agent_event_accepts_fresh_exit_for_turn_settled() {
        let (root, dir) = temp_events_fixture("runtime-fresh-exit-settle");
        write_test_runtime_snapshot(
            &dir,
            "web/fix-login",
            AgentRuntimeState::ExitedSuccess,
            agent_runtime::now_millis().unwrap(),
        );
        let identity = test_identity(&dir, "web/fix-login");

        run_agent_event(Some(&identity), "cursor", "stop", &serde_json::json!({})).unwrap();

        let stem = "web__fix-login";
        let jsonl = fs::read_to_string(dir.join(format!("{stem}.jsonl"))).unwrap();
        assert_eq!(jsonl.lines().count(), 1);
        let envelope: serde_json::Value =
            serde_json::from_str(jsonl.lines().next().unwrap()).unwrap();
        assert_eq!(envelope["kind"], "turn_settled");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn run_agent_event_rejects_without_runtime_snapshot() {
        let (root, dir) = temp_events_fixture("runtime-missing");
        let identity = test_identity(&dir, "web/fix-login");

        run_agent_event(
            Some(&identity),
            "cursor",
            "beforeSubmitPrompt",
            &serde_json::json!({}),
        )
        .unwrap();

        let stem = "web__fix-login";
        assert!(!dir.join(format!("{stem}.jsonl")).exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cursor_event_resolves_identity_from_cwd_index_without_ajax_env() {
        let ajax_home = std::env::temp_dir().join(format!(
            "ajax-home-cwd-index-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let events_dir = ajax_home.join("cache/agent-events");
        fs::create_dir_all(&events_dir).unwrap();
        let project_dir = ajax_home.join("worktrees/web-fix-login");
        fs::create_dir_all(&project_dir).unwrap();
        agent_runtime::publish_cwd_index(&events_dir, "web/fix-login", "primary", &project_dir)
            .unwrap();

        let identity = resolve_cursor_identity(
            &project_dir.to_string_lossy(),
            &serde_json::json!({}),
            None,
            Some(&ajax_home),
        )
        .unwrap();
        assert_eq!(identity.task_id, "web/fix-login");
        assert_eq!(identity.run_id, "primary");
        assert_eq!(identity.events_dir, events_dir.canonicalize().unwrap());

        write_test_runtime_snapshot(&events_dir, "web/fix-login", AgentRuntimeState::Running, 1);
        run_agent_event(
            Some(&identity),
            "cursor",
            "beforeSubmitPrompt",
            &serde_json::json!({}),
        )
        .unwrap();

        let stem = "web__fix-login";
        let jsonl = fs::read_to_string(events_dir.join(format!("{stem}.jsonl"))).unwrap();
        assert_eq!(jsonl.lines().count(), 1);

        fs::remove_dir_all(ajax_home).unwrap();
    }

    #[test]
    fn cursor_resolves_identity_from_xdg_cache_ajax_without_ajax_home() {
        let home = std::env::temp_dir().join(format!(
            "ajax-xdg-home-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let events_dir = home.join(".cache/ajax/agent-events");
        fs::create_dir_all(&events_dir).unwrap();
        let project_dir = home.join("worktrees/web-fix-login");
        fs::create_dir_all(&project_dir).unwrap();
        agent_runtime::publish_cwd_index(&events_dir, "web/fix-login", "primary", &project_dir)
            .unwrap();

        let identity = resolve_cursor_identity(
            &project_dir.to_string_lossy(),
            &serde_json::json!({}),
            Some(&home),
            None,
        )
        .expect("stable XDG ~/.cache/ajax must resolve without AJAX_HOME");
        assert_eq!(identity.task_id, "web/fix-login");
        assert_eq!(identity.events_dir, events_dir.canonicalize().unwrap());

        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn cursor_session_start_stdout_includes_session_env() {
        let (_, events_dir) = temp_events_fixture("session-start-env");
        let identity = test_identity(&events_dir, "web/fix-login");
        let stdout = session_start_env_stdout(&identity);
        let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(parsed["env"]["AJAX_TASK_ID"], "web/fix-login");
        assert_eq!(parsed["env"]["AJAX_RUN_ID"], "primary");
        assert_eq!(
            parsed["env"]["AJAX_AGENT_EVENTS_DIR"].as_str().unwrap(),
            events_dir.to_string_lossy()
        );
    }

    #[test]
    fn cursor_without_index_still_noops() {
        let ajax_home = std::env::temp_dir().join(format!(
            "ajax-home-missing-index-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let events_dir = ajax_home.join("cache/agent-events");
        fs::create_dir_all(&events_dir).unwrap();
        let project_dir = ajax_home.join("worktrees/web-fix-login");
        fs::create_dir_all(&project_dir).unwrap();

        assert!(resolve_cursor_identity(
            &project_dir.to_string_lossy(),
            &serde_json::json!({}),
            None,
            Some(&ajax_home),
        )
        .is_none());

        write_test_runtime_snapshot(&events_dir, "web/fix-login", AgentRuntimeState::Running, 1);
        run_agent_event(None, "cursor", "beforeSubmitPrompt", &serde_json::json!({})).unwrap();

        let stem = "web__fix-login";
        assert!(!events_dir.join(format!("{stem}.jsonl")).exists());

        fs::remove_dir_all(ajax_home).unwrap();
    }
}

use std::{
    fs,
    io::{self, Read},
    path::PathBuf,
};

use clap::ArgMatches;
use serde::{Deserialize, Serialize};

use crate::{agent_runtime, CliError};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub(crate) struct AgentEventSnapshot {
    pub task_id: String,
    pub run_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<String>,
    pub value: String,
    pub observed_at_unix_millis: u128,
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
    let identity = read_agent_event_identity();
    let _ = run_agent_event(identity.as_ref(), client, event, &payload);
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
    let Some(value) = translate_agent_event(client, event, payload) else {
        return Ok(());
    };
    let observed_at = agent_runtime::now_millis().map_err(|_| ())?;
    write_agent_event(identity, value, observed_at).map_err(|_| ())?;
    Ok(())
}

pub(crate) fn translate_agent_event(
    client: &str,
    event: &str,
    payload: &serde_json::Value,
) -> Option<&'static str> {
    match (client, event) {
        ("claude", "UserPromptSubmit" | "PreToolUse" | "PostToolUse") => Some("working"),
        ("claude", "Notification") => {
            let message = payload
                .get("message")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            if message.to_ascii_lowercase().contains("permission") {
                Some("ask")
            } else {
                Some("wait")
            }
        }
        ("claude", "Stop") => {
            if payload
                .get("background_tasks")
                .and_then(|value| value.as_array())
                .is_some_and(|tasks| !tasks.is_empty())
            {
                Some("working")
            } else {
                Some("done")
            }
        }
        ("codex", "UserPromptSubmit" | "PreToolUse" | "PostToolUse") => Some("working"),
        ("codex", "Stop") => Some("done"),
        ("cursor", "beforeSubmitPrompt") => Some("working"),
        ("cursor", "stop") => Some("done"),
        ("pi", "before_agent_start") => Some("working"),
        ("pi", "agent_settled") => Some("done"),
        _ => None,
    }
}

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

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        run_agent_event, translate_agent_event, write_agent_event, AgentEventIdentity,
        AgentEventSnapshot,
    };

    fn temp_events_dir(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "ajax-agent-event-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
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
            translate_agent_event("pi", "agent_settled", &payload),
            Some("done")
        );
        assert_eq!(translate_agent_event("pi", "agent_end", &payload), None);
    }

    #[test]
    fn translate_ignores_unknown_events() {
        assert_eq!(
            translate_agent_event("claude", "SessionStart", &serde_json::json!({})),
            None
        );
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
    fn write_agent_event_is_atomic_and_task_keyed() {
        let dir = temp_events_dir("atomic");
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

        fs::remove_dir_all(dir).unwrap();
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
}

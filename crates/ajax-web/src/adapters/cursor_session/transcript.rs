//! Append-only orchestration-chat session metadata and events on disk.

use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

#[derive(Default)]
pub struct LoadedTranscript {
    pub events: Vec<Value>,
    pub acp_session_id: Option<String>,
    pub model: Option<String>,
}

/// Reload persisted transcript and meta from disk. Skips empty lines and a torn
/// trailing line that does not parse as JSON.
pub fn load_transcript(state_dir: &Path, qualified_handle: &str) -> LoadedTranscript {
    let path = jsonl_path(state_dir, qualified_handle);
    let Ok(raw) = fs::read_to_string(&path) else {
        return LoadedTranscript::default();
    };
    let mut lines: Vec<&str> = raw.lines().filter(|line| !line.trim().is_empty()).collect();
    if lines.is_empty() {
        return LoadedTranscript::default();
    }
    if serde_json::from_str::<Value>(lines.last().copied().unwrap_or("")).is_err() {
        lines.pop();
    }
    let mut loaded = LoadedTranscript::default();
    for line in lines {
        let Ok(val) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if val.get("kind") == Some(&json!("meta")) {
            if let Some(id) = val.get("acp_session_id").and_then(Value::as_str) {
                loaded.acp_session_id = Some(id.to_string());
            }
            if let Some(model) = val.get("model").and_then(Value::as_str) {
                loaded.model = Some(model.to_string());
            }
        } else if let Some(event) = val.get("event") {
            loaded.events.push(event.clone());
        }
    }
    loaded
}

pub fn append_session_meta(
    state_dir: &Path,
    qualified_handle: &str,
    acp_session_id: &str,
    model: &str,
) -> Result<(), String> {
    let path = jsonl_path(state_dir, qualified_handle);
    ensure_parent(&path)?;
    let line = serde_json::json!({
        "kind": "meta",
        "acp_session_id": acp_session_id,
        "model": model,
    });
    append_line(&path, &line)
}

pub fn append_event(state_dir: &Path, qualified_handle: &str, event: &Value) -> Result<(), String> {
    let path = jsonl_path(state_dir, qualified_handle);
    ensure_parent(&path)?;
    let line = serde_json::json!({ "event": event });
    append_line(&path, &line)
}

fn jsonl_path(state_dir: &Path, qualified_handle: &str) -> std::path::PathBuf {
    let encoded = qualified_handle.replace('%', "%25").replace('/', "%2F");
    state_dir
        .join("web-session")
        .join(format!("{encoded}.jsonl"))
}

fn ensure_parent(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn append_line(path: &Path, line: &Value) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    writeln!(file, "{line}").map_err(|error| error.to_string())
}

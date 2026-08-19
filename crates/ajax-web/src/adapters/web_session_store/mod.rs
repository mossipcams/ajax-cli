//! JSONL persistence for orchestration chat transcripts under `state_dir`.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

pub const MAX_LOG_EVENTS: usize = 2000;
// Compact occasionally so append-only writes remain bounded without rewriting
// the whole transcript for every streamed ACP chunk.
const MAX_LOG_BYTES: u64 = 64 * 1024;

const WEB_SESSION_DIR: &str = "web-session";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSession<T> {
    pub acp_session_id: Option<String>,
    pub model: String,
    pub events: Vec<T>,
    pub dropped: usize,
}

impl<T> Default for StoredSession<T> {
    fn default() -> Self {
        Self {
            acp_session_id: None,
            model: "auto".to_string(),
            events: Vec::new(),
            dropped: 0,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct DiskMeta {
    kind: String,
    v: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    acp_session_id: Option<String>,
    model: String,
    #[serde(default)]
    dropped: usize,
}

#[derive(Debug, Deserialize)]
struct DiskEventLine<T> {
    event: T,
}

pub fn load<T: DeserializeOwned>(state_dir: &Path, handle: &str) -> StoredSession<T> {
    let path = session_path(state_dir, handle);
    if !path.is_file() {
        return StoredSession::default();
    }
    let Ok(file) = File::open(&path) else {
        return StoredSession::default();
    };
    let lines: Vec<String> = BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter(|line| !line.trim().is_empty())
        .collect();
    if lines.is_empty() {
        return StoredSession::default();
    }
    let parse_end = if matches!(parse_line::<T>(&lines[lines.len() - 1]), ParsedLine::Skip) {
        lines.len().saturating_sub(1)
    } else {
        lines.len()
    };
    let mut session = StoredSession::default();
    for line in &lines[..parse_end] {
        match parse_line(line) {
            ParsedLine::Meta(meta) => {
                session.acp_session_id = meta.acp_session_id;
                session.model = meta.model;
                session.dropped = meta.dropped;
            }
            ParsedLine::Event(event) => session.events.push(event),
            ParsedLine::Skip => {}
        }
    }
    session
}

pub fn save_meta(state_dir: &Path, handle: &str, acp_session_id: Option<&str>, model: &str) {
    let mut session = load::<serde_json::Value>(state_dir, handle);
    session.acp_session_id = acp_session_id.map(str::to_string);
    session.model = model.to_string();
    persist(state_dir, handle, &session);
}

pub fn append_events<T: Serialize + serde::de::DeserializeOwned>(
    state_dir: &Path,
    handle: &str,
    new_events: &[T],
) {
    if new_events.is_empty() {
        return;
    }

    let path = session_path(state_dir, handle);
    if !path.is_file() {
        persist(state_dir, handle, &StoredSession::<T>::default());
    }
    let result = (|| -> Result<(), std::io::Error> {
        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        for event in new_events {
            let row = serde_json::json!({
                "kind": "event",
                "event": event,
            });
            let line = serde_json::to_string(&row).map_err(std::io::Error::other)?;
            writeln!(file, "{line}")?;
        }
        file.flush()
    })();
    if let Err(error) = result {
        tracing::warn!(%error, handle, "failed to append web session transcript");
        return;
    }

    let oversized = fs::metadata(&path)
        .map(|metadata| metadata.len() > MAX_LOG_BYTES)
        .unwrap_or(false);
    if !oversized {
        return;
    }
    let mut session = load::<T>(state_dir, handle);
    let excess = session.events.len().saturating_sub(MAX_LOG_EVENTS);
    if excess == 0 {
        return;
    }
    session.events.drain(..excess);
    session.dropped += excess;
    persist(state_dir, handle, &session);
}

enum ParsedLine<T> {
    Meta(DiskMeta),
    Event(T),
    Skip,
}

fn parse_line<T: DeserializeOwned>(line: &str) -> ParsedLine<T> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return ParsedLine::Skip;
    };
    let Some(kind) = value.get("kind").and_then(serde_json::Value::as_str) else {
        return ParsedLine::Skip;
    };
    match kind {
        "meta" => serde_json::from_str::<DiskMeta>(line)
            .map(ParsedLine::Meta)
            .unwrap_or(ParsedLine::Skip),
        "event" => serde_json::from_str::<DiskEventLine<T>>(line)
            .map(|row| ParsedLine::Event(row.event))
            .unwrap_or(ParsedLine::Skip),
        _ => ParsedLine::Skip,
    }
}

fn encode_handle(handle: &str) -> String {
    handle.replace('%', "%25").replace('/', "%2F")
}

fn decode_handle(encoded: &str) -> String {
    encoded.replace("%2F", "/").replace("%25", "%")
}

/// Qualified handles with a persisted JSONL transcript under `state_dir`.
pub fn list_persisted_handles(state_dir: &Path) -> Vec<String> {
    let dir = state_dir.join(WEB_SESSION_DIR);
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .strip_suffix(".jsonl")
                .map(decode_handle)
        })
        .collect()
}

/// Remove the persisted transcript for `handle`. Returns true when a file was deleted.
pub fn delete_session(state_dir: &Path, handle: &str) -> bool {
    let path = session_path(state_dir, handle);
    match fs::remove_file(&path) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            tracing::warn!(%error, handle, "failed to delete web session transcript");
            false
        }
    }
}

pub fn session_path(state_dir: &Path, handle: &str) -> PathBuf {
    state_dir
        .join(WEB_SESSION_DIR)
        .join(format!("{}.jsonl", encode_handle(handle)))
}

fn persist<T: Serialize>(state_dir: &Path, handle: &str, session: &StoredSession<T>) {
    if let Err(error) = rewrite_file(state_dir, handle, session) {
        tracing::warn!(%error, handle, "failed to persist web session transcript");
    }
}

fn rewrite_file<T: Serialize>(
    state_dir: &Path,
    handle: &str,
    session: &StoredSession<T>,
) -> Result<(), std::io::Error> {
    let dir = state_dir.join(WEB_SESSION_DIR);
    fs::create_dir_all(&dir)?;
    let path = session_path(state_dir, handle);
    let tmp_path = path.with_extension("jsonl.tmp");
    let mut file = fs::File::create(&tmp_path)?;
    let meta = DiskMeta {
        kind: "meta".to_string(),
        v: 1,
        acp_session_id: session.acp_session_id.clone(),
        model: session.model.clone(),
        dropped: session.dropped,
    };
    let meta_line = serde_json::to_string(&meta).map_err(std::io::Error::other)?;
    writeln!(file, "{meta_line}")?;
    for event in &session.events {
        let row = serde_json::json!({
            "kind": "event",
            "event": event,
        });
        let line = serde_json::to_string(&row).map_err(std::io::Error::other)?;
        writeln!(file, "{line}")?;
    }
    file.sync_all()?;
    fs::rename(tmp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests;

//! JSONL persistence for orchestration chat transcripts under `state_dir`.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

pub const MAX_LOG_EVENTS: usize = 2000;
// Compact occasionally so append-only writes remain bounded without rewriting
// the whole transcript for every streamed ACP chunk.
const MAX_LOG_BYTES: u64 = 64 * 1024;

pub(crate) const WEB_SESSION_DIR: &str = "web-session";

pub mod prompt_ledger;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSession<T> {
    pub acp_session_id: Option<String>,
    pub model: String,
    pub events: Vec<T>,
    pub dropped: usize,
    /// Interior rows that could not be read back; the history is incomplete.
    pub corrupt_lines: usize,
}

impl<T> Default for StoredSession<T> {
    fn default() -> Self {
        Self {
            acp_session_id: None,
            model: "auto".to_string(),
            events: Vec::new(),
            dropped: 0,
            corrupt_lines: 0,
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
    #[serde(default, skip_serializing_if = "is_zero")]
    corrupt_lines: usize,
}

fn is_zero(count: &usize) -> bool {
    *count == 0
}

#[derive(Debug, Deserialize)]
struct DiskEventLine<T> {
    event: T,
}

/// Read the stored session. A missing file is an empty session; a file that
/// exists but cannot be read is an error, never an empty session.
pub fn try_load<T: DeserializeOwned>(
    state_dir: &Path,
    handle: &str,
) -> io::Result<StoredSession<T>> {
    let file = match File::open(session_path(state_dir, handle)) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(StoredSession::default())
        }
        Err(error) => return Err(error),
    };
    let mut lines = Vec::new();
    for line in BufReader::new(file).split(b'\n') {
        // A row that is not UTF-8 is a corrupt row, not an unreadable file.
        let line = String::from_utf8_lossy(&line?).into_owned();
        if !line.trim().is_empty() {
            lines.push(line);
        }
    }
    // An unparsable final row is a write cut short by a crash, not corruption.
    let parse_end = match lines.last() {
        Some(last) if matches!(parse_line::<T>(last), ParsedLine::Corrupt) => lines.len() - 1,
        _ => lines.len(),
    };
    let mut session = StoredSession::default();
    let mut corrupt_lines = 0;
    for line in &lines[..parse_end] {
        match parse_line(line) {
            ParsedLine::Meta(meta) => {
                session.acp_session_id = meta.acp_session_id;
                session.model = meta.model;
                session.dropped = meta.dropped;
                session.corrupt_lines = meta.corrupt_lines;
            }
            ParsedLine::Event(event) => session.events.push(event),
            ParsedLine::Corrupt => corrupt_lines += 1,
            ParsedLine::Skip => {}
        }
    }
    session.corrupt_lines += corrupt_lines;
    Ok(session)
}

pub fn try_save_meta(
    state_dir: &Path,
    handle: &str,
    acp_session_id: Option<&str>,
    model: &str,
) -> io::Result<()> {
    // A failed load must not be rewritten as an empty session.
    let mut session = try_load::<serde_json::Value>(state_dir, handle)?;
    session.acp_session_id = acp_session_id.map(str::to_string);
    session.model = model.to_string();
    rewrite_file(state_dir, handle, &session)
}

/// Clear the stored ACP resume id so the next attach uses `session/new`.
pub fn clear_acp_session_id(state_dir: &Path, handle: &str) -> io::Result<()> {
    let mut session = try_load::<serde_json::Value>(state_dir, handle)?;
    if session.acp_session_id.is_none() {
        return Ok(());
    }
    session.acp_session_id = None;
    rewrite_file(state_dir, handle, &session)
}

pub fn try_append_events<T: Serialize + serde::de::DeserializeOwned>(
    state_dir: &Path,
    handle: &str,
    new_events: &[T],
) -> io::Result<()> {
    if new_events.is_empty() {
        return Ok(());
    }

    let path = session_path(state_dir, handle);
    if !path.is_file() {
        rewrite_file(state_dir, handle, &StoredSession::<T>::default())?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    for event in new_events {
        let row = serde_json::json!({
            "kind": "event",
            "event": event,
        });
        let line = serde_json::to_string(&row).map_err(io::Error::other)?;
        writeln!(file, "{line}")?;
    }
    file.flush()?;

    // The rows above are durable; a failed compaction only leaves the file large.
    if let Err(error) = compact::<T>(state_dir, handle, &path) {
        tracing::warn!(%error, handle, "failed to compact web session transcript");
    }
    Ok(())
}

fn compact<T: Serialize + DeserializeOwned>(
    state_dir: &Path,
    handle: &str,
    path: &Path,
) -> io::Result<()> {
    if fs::metadata(path)?.len() <= MAX_LOG_BYTES {
        return Ok(());
    }
    let mut session = try_load::<T>(state_dir, handle)?;
    let excess = session.events.len().saturating_sub(MAX_LOG_EVENTS);
    if excess == 0 {
        return Ok(());
    }
    session.events.drain(..excess);
    session.dropped += excess;
    rewrite_file(state_dir, handle, &session)
}

/// Operator-facing warning when stored rows could not be read back.
pub fn corruption_warning(corrupt_lines: usize) -> Option<String> {
    (corrupt_lines > 0).then(|| {
        format!(
            "{corrupt_lines} stored transcript rows were unreadable; this history is incomplete"
        )
    })
}

// Test fixtures: seed and inspect transcripts without error plumbing.
#[cfg(test)]
pub fn load<T: DeserializeOwned>(state_dir: &Path, handle: &str) -> StoredSession<T> {
    try_load(state_dir, handle).unwrap_or_default()
}

#[cfg(test)]
pub fn save_meta(state_dir: &Path, handle: &str, acp_session_id: Option<&str>, model: &str) {
    let _ = try_save_meta(state_dir, handle, acp_session_id, model);
}

#[cfg(test)]
pub fn append_events<T: Serialize + serde::de::DeserializeOwned>(
    state_dir: &Path,
    handle: &str,
    new_events: &[T],
) {
    let _ = try_append_events(state_dir, handle, new_events);
}

enum ParsedLine<T> {
    Meta(DiskMeta),
    Event(T),
    /// Not a readable row: the stored history lost something here.
    Corrupt,
    /// A well-formed row of a kind this build does not know.
    Skip,
}

fn parse_line<T: DeserializeOwned>(line: &str) -> ParsedLine<T> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return ParsedLine::Corrupt;
    };
    let Some(kind) = value.get("kind").and_then(serde_json::Value::as_str) else {
        return ParsedLine::Corrupt;
    };
    match kind {
        "meta" => serde_json::from_str::<DiskMeta>(line)
            .map(ParsedLine::Meta)
            .unwrap_or(ParsedLine::Corrupt),
        "event" => serde_json::from_str::<DiskEventLine<T>>(line)
            .map(|row| ParsedLine::Event(row.event))
            .unwrap_or(ParsedLine::Corrupt),
        _ => ParsedLine::Skip,
    }
}

pub(crate) fn encode_handle(handle: &str) -> String {
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
    let deleted_transcript = match fs::remove_file(&path) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            tracing::warn!(%error, handle, "failed to delete web session transcript");
            false
        }
    };
    let deleted_ledger = prompt_ledger::delete_ledger(state_dir, handle);
    deleted_transcript || deleted_ledger
}

pub fn session_path(state_dir: &Path, handle: &str) -> PathBuf {
    state_dir
        .join(WEB_SESSION_DIR)
        .join(format!("{}.jsonl", encode_handle(handle)))
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
        corrupt_lines: session.corrupt_lines,
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

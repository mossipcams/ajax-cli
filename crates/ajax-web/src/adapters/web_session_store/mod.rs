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

pub(crate) const WEB_SESSION_DIR: &str = "web-session";

pub mod prompt_ledger;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSession<T> {
    pub acp_session_id: Option<String>,
    pub model: String,
    pub events: Vec<T>,
    pub dropped: usize,
    pub context_epoch: u64,
}

impl<T> Default for StoredSession<T> {
    fn default() -> Self {
        Self {
            acp_session_id: None,
            model: "auto".to_string(),
            events: Vec::new(),
            dropped: 0,
            context_epoch: 0,
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
    #[serde(default)]
    context_epoch: u64,
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
                session.context_epoch = meta.context_epoch;
            }
            ParsedLine::Event(event) => session.events.push(event),
            ParsedLine::Skip => {}
        }
    }
    session
}

pub fn save_meta(
    state_dir: &Path,
    handle: &str,
    acp_session_id: Option<&str>,
    model: &str,
) -> Result<(), std::io::Error> {
    let mut session = load::<serde_json::Value>(state_dir, handle);
    session.acp_session_id = acp_session_id.map(str::to_string);
    session.model = model.to_string();
    rewrite_file(state_dir, handle, &session).map_err(|error| {
        tracing::warn!(%error, handle, "failed to persist web session transcript");
        error
    })
}

/// Persist session identity and an explicit context epoch (Start new context / harness Switch).
pub fn save_meta_with_context_epoch(
    state_dir: &Path,
    handle: &str,
    acp_session_id: Option<&str>,
    model: &str,
    context_epoch: u64,
) -> Result<(), std::io::Error> {
    let mut session = load::<serde_json::Value>(state_dir, handle);
    session.acp_session_id = acp_session_id.map(str::to_string);
    session.model = model.to_string();
    session.context_epoch = context_epoch;
    rewrite_file(state_dir, handle, &session).map_err(|error| {
        tracing::warn!(%error, handle, "failed to persist web session transcript");
        error
    })
}

/// Clear the stored ACP resume id so the next attach uses `session/new`.
pub fn clear_acp_session_id(state_dir: &Path, handle: &str) -> Result<(), std::io::Error> {
    let mut session = load::<serde_json::Value>(state_dir, handle);
    if session.acp_session_id.is_none() {
        return Ok(());
    }
    session.acp_session_id = None;
    rewrite_file(state_dir, handle, &session).map_err(|error| {
        tracing::warn!(%error, handle, "failed to persist web session transcript");
        error
    })
}

pub fn append_events<T: Serialize + serde::de::DeserializeOwned>(
    state_dir: &Path,
    handle: &str,
    new_events: &[T],
) -> Result<(), std::io::Error> {
    if new_events.is_empty() {
        return Ok(());
    }

    let path = session_path(state_dir, handle);
    if !path.is_file() {
        rewrite_file(state_dir, handle, &StoredSession::<T>::default()).map_err(|error| {
            tracing::warn!(%error, handle, "failed to persist web session transcript");
            error
        })?;
    }
    let result = (|| -> Result<(), std::io::Error> {
        #[cfg(test)]
        if force_append_fail() {
            return Err(std::io::Error::other("forced append_events failure"));
        }
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
        return Err(error);
    }

    let oversized = fs::metadata(&path)
        .map(|metadata| metadata.len() > MAX_LOG_BYTES)
        .unwrap_or(false);
    if !oversized {
        return Ok(());
    }
    let mut session = load::<T>(state_dir, handle);
    let excess = session.events.len().saturating_sub(MAX_LOG_EVENTS);
    if excess == 0 {
        return Ok(());
    }
    session.events.drain(..excess);
    session.dropped += excess;
    rewrite_file(state_dir, handle, &session).map_err(|error| {
        tracing::warn!(%error, handle, "failed to persist web session transcript");
        error
    })
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

#[cfg(test)]
fn persist<T: Serialize>(state_dir: &Path, handle: &str, session: &StoredSession<T>) {
    if let Err(error) = rewrite_file(state_dir, handle, session) {
        tracing::warn!(%error, handle, "failed to persist web session transcript");
    }
}

#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(test)]
static FORCE_SAVE_META_FAIL: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
static FORCE_APPEND_FAIL: AtomicBool = AtomicBool::new(false);

/// Test-scoped `save_meta` / `rewrite_file` failure injection; restores the prior flag on drop.
#[cfg(test)]
pub struct ForceSaveMetaFailGuard {
    previous: bool,
}

#[cfg(test)]
impl ForceSaveMetaFailGuard {
    pub fn enable() -> Self {
        let previous = FORCE_SAVE_META_FAIL.swap(true, Ordering::SeqCst);
        Self { previous }
    }
}

#[cfg(test)]
impl Drop for ForceSaveMetaFailGuard {
    fn drop(&mut self) {
        FORCE_SAVE_META_FAIL.store(self.previous, Ordering::SeqCst);
    }
}

#[cfg(test)]
fn force_save_meta_fail() -> bool {
    FORCE_SAVE_META_FAIL.load(Ordering::SeqCst)
}

/// Test-scoped `append_events` failure injection; restores the prior flag on drop.
#[cfg(test)]
pub struct ForceAppendFailGuard {
    previous: bool,
}

#[cfg(test)]
impl ForceAppendFailGuard {
    pub fn enable() -> Self {
        let previous = FORCE_APPEND_FAIL.swap(true, Ordering::SeqCst);
        Self { previous }
    }
}

#[cfg(test)]
impl Drop for ForceAppendFailGuard {
    fn drop(&mut self) {
        FORCE_APPEND_FAIL.store(self.previous, Ordering::SeqCst);
    }
}

#[cfg(test)]
fn force_append_fail() -> bool {
    FORCE_APPEND_FAIL.load(Ordering::SeqCst)
}

fn rewrite_file<T: Serialize>(
    state_dir: &Path,
    handle: &str,
    session: &StoredSession<T>,
) -> Result<(), std::io::Error> {
    #[cfg(test)]
    if force_save_meta_fail() {
        return Err(std::io::Error::other("forced save_meta failure"));
    }
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
        context_epoch: session.context_epoch,
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

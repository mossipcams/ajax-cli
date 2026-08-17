//! JSONL persistence for orchestration chat transcripts under `state_dir`.

use crate::slices::web_session::SessionServerEvent;
use serde::{Deserialize, Serialize};
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
pub struct StoredSession {
    pub acp_session_id: Option<String>,
    pub model: String,
    pub events: Vec<SessionServerEvent>,
    pub dropped: usize,
}

impl Default for StoredSession {
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
struct DiskEventLine {
    event: SessionServerEvent,
}

pub fn load(state_dir: &Path, handle: &str) -> StoredSession {
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
    let parse_end = if matches!(parse_line(&lines[lines.len() - 1]), ParsedLine::Skip) {
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
    let mut session = load(state_dir, handle);
    session.acp_session_id = acp_session_id.map(str::to_string);
    session.model = model.to_string();
    persist(state_dir, handle, &session);
}

pub fn append_events(state_dir: &Path, handle: &str, new_events: &[SessionServerEvent]) {
    if new_events.is_empty() {
        return;
    }

    let path = session_path(state_dir, handle);
    if !path.is_file() {
        persist(state_dir, handle, &StoredSession::default());
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
    let mut session = load(state_dir, handle);
    let excess = session.events.len().saturating_sub(MAX_LOG_EVENTS);
    if excess == 0 {
        // Over the byte cap but inside the event cap: there is nothing to trim,
        // so rewriting would produce a byte-identical file and leave it just as
        // oversized — which is what turned every later append into another full
        // rewrite + fsync, once per streamed ACP chunk.
        return;
    }
    session.events.drain(..excess);
    session.dropped += excess;
    persist(state_dir, handle, &session);
}

enum ParsedLine {
    Meta(DiskMeta),
    Event(SessionServerEvent),
    Skip,
}

fn parse_line(line: &str) -> ParsedLine {
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
        "event" => serde_json::from_str::<DiskEventLine>(line)
            .map(|row| ParsedLine::Event(row.event))
            .unwrap_or(ParsedLine::Skip),
        _ => ParsedLine::Skip,
    }
}

fn encode_handle(handle: &str) -> String {
    handle.replace('%', "%25").replace('/', "%2F")
}

fn session_path(state_dir: &Path, handle: &str) -> PathBuf {
    state_dir
        .join(WEB_SESSION_DIR)
        .join(format!("{}.jsonl", encode_handle(handle)))
}

fn persist(state_dir: &Path, handle: &str, session: &StoredSession) {
    if let Err(error) = rewrite_file(state_dir, handle, session) {
        tracing::warn!(%error, handle, "failed to persist web session transcript");
    }
}

fn rewrite_file(
    state_dir: &Path,
    handle: &str,
    session: &StoredSession,
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
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn note(text: &str) -> SessionServerEvent {
        SessionServerEvent::Message {
            role: "agent".to_string(),
            text: text.to_string(),
        }
    }

    fn scratch_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ajax-web-session-store-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn round_trip_events_and_meta() {
        let dir = scratch_dir("round-trip");
        let handle = "web/fix-login";
        let events = vec![note("one"), note("two")];
        append_events(&dir, handle, &events);
        save_meta(&dir, handle, Some("sess-abc"), "composer-2.5");
        let loaded = load(&dir, handle);
        assert_eq!(loaded.acp_session_id.as_deref(), Some("sess-abc"));
        assert_eq!(loaded.model, "composer-2.5");
        assert_eq!(loaded.events, events);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_file_is_empty() {
        let dir = scratch_dir("missing");
        let loaded = load(&dir, "web/none");
        assert_eq!(loaded, StoredSession::default());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn torn_last_line_is_skipped() {
        let dir = scratch_dir("torn");
        let handle = "web/fix-login";
        append_events(&dir, handle, &[note("kept")]);
        let path = session_path(&dir, handle);
        let mut contents = fs::read_to_string(&path).unwrap();
        contents.push_str("{\"kind\":\"event\",\"event\":{\"type\":\"mess");
        fs::write(&path, contents).unwrap();
        let loaded = load(&dir, handle);
        assert_eq!(loaded.events, vec![note("kept")]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn cap_trims_oldest_events() {
        let dir = scratch_dir("cap");
        let handle = "web/fix-login";
        let events: Vec<_> = (0..MAX_LOG_EVENTS + 5)
            .map(|i| note(&i.to_string()))
            .collect();
        append_events(&dir, handle, &events);
        let loaded = load(&dir, handle);
        assert_eq!(loaded.events.len(), MAX_LOG_EVENTS);
        assert_eq!(loaded.dropped, 5);
        assert_eq!(loaded.events[0], note("5"));
        assert_eq!(
            loaded.events[MAX_LOG_EVENTS - 1],
            note(&(MAX_LOG_EVENTS + 4).to_string())
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn incremental_appends_keep_the_transcript_file_identity() {
        use std::os::unix::fs::MetadataExt;

        let dir = scratch_dir("append-identity");
        let handle = "web/fix-login";
        append_events(&dir, handle, &[note("one")]);
        let path = session_path(&dir, handle);
        let first_inode = fs::metadata(&path).unwrap().ino();

        append_events(&dir, handle, &[note("two")]);

        assert_eq!(fs::metadata(&path).unwrap().ino(), first_inode);
        assert_eq!(load(&dir, handle).events, vec![note("one"), note("two")]);
        let _ = fs::remove_dir_all(dir);
    }

    /// A transcript passes `MAX_LOG_BYTES` long before it reaches
    /// `MAX_LOG_EVENTS`, and in that window there is nothing to trim. Rewriting
    /// anyway produced a byte-identical, still-oversized file, so every later
    /// append reloaded, reparsed, rewrote and fsynced the whole transcript —
    /// once per streamed ACP chunk, under the hub's session lock. The behavior
    /// contract is explicit: "Transcript events append to JSONL without a
    /// per-event full rewrite" (`docs/architecture/web-session-behavior.md`).
    #[cfg(unix)]
    #[test]
    fn append_past_the_byte_cap_does_not_rewrite_the_whole_transcript() {
        use std::os::unix::fs::MetadataExt;

        let dir = scratch_dir("oversized-append");
        let handle = "web/fix-login";
        let chunk = "x".repeat(1024);
        let events: Vec<_> = (0..200).map(|_| note(&chunk)).collect();
        assert!(events.len() < MAX_LOG_EVENTS, "count cap must not fire");

        append_events(&dir, handle, &events);

        let path = session_path(&dir, handle);
        assert!(
            fs::metadata(&path).unwrap().len() > MAX_LOG_BYTES,
            "fixture must leave the transcript over the byte cap"
        );

        let inode = fs::metadata(&path).unwrap().ino();
        append_events(&dir, handle, &[note("after")]);

        assert_eq!(
            fs::metadata(&path).unwrap().ino(),
            inode,
            "append over the byte cap rewrote the whole transcript"
        );
        assert_eq!(load(&dir, handle).events.len(), events.len() + 1);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn handle_with_slash_encodes_to_single_filename() {
        let dir = scratch_dir("encode");
        let handle = "web/fix/login";
        append_events(&dir, handle, &[note("ok")]);
        let path = session_path(&dir, handle);
        assert_eq!(
            path,
            dir.join("web-session").join("web%2Ffix%2Flogin.jsonl")
        );
        assert!(path.is_file());
        assert_eq!(load(&dir, handle).events, vec![note("ok")]);
        let _ = fs::remove_dir_all(dir);
    }
}

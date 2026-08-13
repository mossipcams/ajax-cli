//! JSONL persistence for orchestration chat transcripts under `state_dir`.

use crate::slices::web_session::SessionServerEvent;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

pub const MAX_LOG_EVENTS: usize = 2000;

const WEB_SESSION_DIR: &str = "web-session";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSession {
    pub acp_session_id: Option<String>,
    pub model: String,
    pub events: Vec<SessionServerEvent>,
}

impl Default for StoredSession {
    fn default() -> Self {
        Self {
            acp_session_id: None,
            model: "auto".to_string(),
            events: Vec::new(),
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
    let _ = rewrite_file(state_dir, handle, &session);
}

pub fn append_events(state_dir: &Path, handle: &str, new_events: &[SessionServerEvent]) {
    if new_events.is_empty() {
        return;
    }
    let mut session = load(state_dir, handle);
    session.events.extend_from_slice(new_events);
    if session.events.len() > MAX_LOG_EVENTS {
        let excess = session.events.len() - MAX_LOG_EVENTS;
        session.events.drain(..excess);
    }
    let _ = rewrite_file(state_dir, handle, &session);
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

fn rewrite_file(state_dir: &Path, handle: &str, session: &StoredSession) -> Result<(), ()> {
    let dir = state_dir.join(WEB_SESSION_DIR);
    fs::create_dir_all(&dir).map_err(|_| ())?;
    let path = session_path(state_dir, handle);
    let mut file = fs::File::create(&path).map_err(|_| ())?;
    let meta = DiskMeta {
        kind: "meta".to_string(),
        v: 1,
        acp_session_id: session.acp_session_id.clone(),
        model: session.model.clone(),
    };
    let meta_line = serde_json::to_string(&meta).map_err(|_| ())?;
    writeln!(file, "{meta_line}").map_err(|_| ())?;
    for event in &session.events {
        let row = serde_json::json!({
            "kind": "event",
            "event": event,
        });
        let line = serde_json::to_string(&row).map_err(|_| ())?;
        writeln!(file, "{line}").map_err(|_| ())?;
    }
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
        assert_eq!(loaded.events[0], note("5"));
        assert_eq!(
            loaded.events[MAX_LOG_EVENTS - 1],
            note(&(MAX_LOG_EVENTS + 4).to_string())
        );
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

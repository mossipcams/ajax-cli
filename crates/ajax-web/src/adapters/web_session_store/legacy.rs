//! Read-only legacy JSONL importer. Sources are retained after migration.
use super::{session_path, StoredSession};
use serde::{de::DeserializeOwned, Deserialize};
use std::{
    fs::File,
    io::{self, BufRead, BufReader},
    path::Path,
};

#[derive(Debug, Deserialize)]
struct DiskMeta {
    acp_session_id: Option<String>,
    model: String,
    #[serde(default)]
    dropped: usize,
    #[serde(default)]
    corrupt_lines: usize,
}

#[derive(Debug, Deserialize)]
struct DiskEventLine<T> {
    event: T,
}

/// Read the stored session. A missing file is an empty session; a file that
/// exists but cannot be read is an error, never an empty session.
pub(super) fn read<T: DeserializeOwned>(
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

//! Tails the delegate ACP event files that model-router harness children write
//! into their per-worktree events dir (see `adapters::web_session_acp`'s
//! `delegate_events`) and turns their tool calls into flat chat rows.

use std::{
    collections::HashMap,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use super::acp_map::map_acp_session_update;
use super::SessionServerEvent;
use serde_json::Value;

/// A single line longer than this is dropped as corrupt rather than buffered.
const MAX_LINE_BYTES: usize = 1024 * 1024;

#[derive(Default)]
pub(super) struct DelegateTail {
    /// Bytes of each file consumed through the last newline. A partial
    /// trailing line is simply not consumed; the next poll rereads it from
    /// this offset.
    offsets: HashMap<PathBuf, u64>,
}

impl DelegateTail {
    /// Reads every `*.jsonl` file in `dir` from its stored offset and maps new
    /// complete lines to tool-call rows. A missing dir yields an empty vec.
    pub(super) fn poll(&mut self, dir: &Path) -> Vec<SessionServerEvent> {
        let Ok(entries) = fs::read_dir(dir) else {
            return Vec::new();
        };

        let mut events = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|name| name.to_str()) else {
                continue;
            };

            let mut file = match File::open(&path) {
                Ok(file) => file,
                Err(_) => continue,
            };
            let len = file.metadata().map(|meta| meta.len()).unwrap_or(0);
            let stored = self.offsets.entry(path.clone()).or_insert(0);
            if len < *stored {
                // Truncated or replaced in place: restart from the top.
                *stored = 0;
            }

            let mut fresh = Vec::new();
            if file.seek(SeekFrom::Start(*stored)).is_err() || file.read_to_end(&mut fresh).is_err()
            {
                continue;
            }

            match fresh.iter().rposition(|&byte| byte == b'\n') {
                Some(pos) => {
                    for line in fresh[..pos].split(|&byte| byte == b'\n') {
                        map_line(stem, line, &mut events);
                    }
                    *stored += pos as u64 + 1;
                }
                None => {
                    // No newline arrived: consume nothing so the partial
                    // tail is reread next poll. A runaway unterminated line
                    // beyond MAX_LINE_BYTES must not be reread forever.
                    if fresh.len() > MAX_LINE_BYTES {
                        *stored += fresh.len() as u64;
                    }
                }
            }
        }

        events
    }
}

fn map_line(stem: &str, line: &[u8], events: &mut Vec<SessionServerEvent>) {
    if line.len() > MAX_LINE_BYTES {
        return;
    }
    let Ok(text) = std::str::from_utf8(line) else {
        return;
    };
    let Ok(record) = serde_json::from_str::<Value>(text) else {
        return;
    };
    if record.get("method").and_then(Value::as_str) != Some("session/update") {
        return;
    }
    let Some(params) = record.get("params") else {
        return;
    };

    for event in map_acp_session_update(params) {
        // Only flat tool rows go into the chat transcript; everything else the
        // delegate emitted is dropped.
        if let SessionServerEvent::ToolCall {
            call_id,
            title,
            kind,
            status,
            locations,
            content,
        } = event
        {
            events.push(SessionServerEvent::ToolCall {
                call_id: format!("{stem}:{call_id}"),
                title,
                kind,
                status,
                locations,
                content,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ajax-delegate-tail-{}-{}",
            std::process::id(),
            label
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn tool_call_line(id: &str, title: &str, status: &str) -> String {
        format!(
            "{{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{{\"sessionId\":\"s\",\"update\":{{\"sessionUpdate\":\"tool_call\",\"toolCallId\":\"{id}\",\"title\":\"{title}\",\"kind\":\"search\",\"status\":\"{status}\"}}}}}}"
        )
    }

    const TOOL_CALL_UPDATE_LINE: &str = "{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"s\",\"update\":{\"sessionUpdate\":\"tool_call_update\",\"toolCallId\":\"t1\",\"status\":\"completed\"}}}";

    fn as_tool_call(event: &SessionServerEvent) -> (&str, &str, &str) {
        match event {
            SessionServerEvent::ToolCall {
                call_id,
                title,
                status,
                ..
            } => (call_id, title, status),
            other => panic!("expected tool_call row, got {other:?}"),
        }
    }

    fn append_line(dir: &Path, file: &str, line: &str) {
        let mut handle = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(file))
            .expect("open jsonl");
        handle.write_all(line.as_bytes()).expect("write line");
    }

    #[test]
    fn tool_call_line_yields_prefixed_row() {
        let dir = temp_dir("basic");
        append_line(
            &dir,
            "run1.jsonl",
            &format!("{}\n", tool_call_line("t1", "grep", "pending")),
        );

        let events = DelegateTail::default().poll(&dir);
        assert_eq!(events.len(), 1);
        let (call_id, title, status) = as_tool_call(&events[0]);
        assert_eq!(call_id, "run1:t1");
        assert_eq!(title, "grep");
        assert_eq!(status, "pending");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn tool_call_update_carries_status_for_same_id() {
        let dir = temp_dir("update");
        append_line(
            &dir,
            "run1.jsonl",
            &format!(
                "{}\n{}\n",
                tool_call_line("t1", "grep", "pending"),
                TOOL_CALL_UPDATE_LINE
            ),
        );

        let events = DelegateTail::default().poll(&dir);
        assert_eq!(events.len(), 2);
        let (call_id, _, status) = as_tool_call(&events[1]);
        assert_eq!(call_id, "run1:t1");
        assert_eq!(status, "completed");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn agent_message_chunks_are_dropped() {
        let dir = temp_dir("messages");
        append_line(
            &dir,
            "run1.jsonl",
            "{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"s\",\"update\":{\"sessionUpdate\":\"agent_message_chunk\",\"content\":[{\"type\":\"text\",\"text\":\"hi\"}]}}}\n",
        );

        let events = DelegateTail::default().poll(&dir);
        assert!(events.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn incomplete_line_is_buffered_until_newline() {
        let dir = temp_dir("partial");
        append_line(
            &dir,
            "run1.jsonl",
            &tool_call_line("t1", "grep", "pending"), // no trailing newline
        );

        let mut tail = DelegateTail::default();
        assert!(tail.poll(&dir).is_empty(), "no newline, no event");

        append_line(&dir, "run1.jsonl", "\n");
        let events = tail.poll(&dir);
        assert_eq!(events.len(), 1, "newline completes the line");
        let (call_id, _, _) = as_tool_call(&events[0]);
        assert_eq!(call_id, "run1:t1");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_json_is_skipped_and_following_lines_map() {
        let dir = temp_dir("malformed");
        append_line(
            &dir,
            "run1.jsonl",
            &format!("not json\n{}\n", tool_call_line("t2", "read", "pending")),
        );

        let events = DelegateTail::default().poll(&dir);
        assert_eq!(events.len(), 1);
        let (call_id, _, _) = as_tool_call(&events[0]);
        assert_eq!(call_id, "run1:t2");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn consumed_lines_are_not_reemitted() {
        let dir = temp_dir("offsets");
        append_line(
            &dir,
            "run1.jsonl",
            &format!("{}\n", tool_call_line("t1", "grep", "pending")),
        );

        let mut tail = DelegateTail::default();
        assert_eq!(tail.poll(&dir).len(), 1);
        assert!(
            tail.poll(&dir).is_empty(),
            "second poll must not re-emit consumed lines"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_dir_yields_no_events() {
        let dir =
            std::env::temp_dir().join(format!("ajax-delegate-tail-missing-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        let events = DelegateTail::default().poll(&dir);
        assert!(events.is_empty());
    }
}

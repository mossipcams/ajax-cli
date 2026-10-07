use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

fn note(text: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "message",
        "role": "agent",
        "text": text,
    })
}

fn scratch_dir(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ajax-web-session-store-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn round_trip_events_and_meta() {
    let dir = scratch_dir("round-trip");
    let handle = "web/fix-login";
    let events = vec![note("one"), note("two")];
    append_events(&dir, handle, &events);
    save_meta(&dir, handle, Some("sess-abc"), "composer-2.5");
    let loaded: StoredSession<serde_json::Value> = load(&dir, handle);
    assert_eq!(loaded.acp_session_id.as_deref(), Some("sess-abc"));
    assert_eq!(loaded.model, "composer-2.5");
    assert_eq!(loaded.events, events);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn missing_file_is_empty() {
    let dir = scratch_dir("missing");
    let loaded: StoredSession<serde_json::Value> = load(&dir, "web/none");
    assert_eq!(loaded, StoredSession::default());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn torn_last_line_is_skipped() {
    let dir = scratch_dir("torn");
    let handle = "web/fix-login";
    append_events(&dir, handle, &[note("kept")]);
    let path = session_path(&dir, handle);
    let mut contents = std::fs::read_to_string(&path).unwrap();
    contents.push_str("{\"kind\":\"event\",\"event\":{\"type\":\"mess");
    std::fs::write(&path, contents).unwrap();
    let loaded: StoredSession<serde_json::Value> = load(&dir, handle);
    assert_eq!(loaded.events, vec![note("kept")]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn cap_trims_oldest_events() {
    let dir = scratch_dir("cap");
    let handle = "web/fix-login";
    let events: Vec<_> = (0..MAX_LOG_EVENTS + 5)
        .map(|i| note(&i.to_string()))
        .collect();
    append_events(&dir, handle, &events);
    let loaded: StoredSession<serde_json::Value> = load(&dir, handle);
    assert_eq!(loaded.events.len(), MAX_LOG_EVENTS);
    assert_eq!(loaded.dropped, 5);
    assert_eq!(loaded.events[0], note("5"));
    assert_eq!(
        loaded.events[MAX_LOG_EVENTS - 1],
        note(&(MAX_LOG_EVENTS + 4).to_string())
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn incremental_appends_keep_the_transcript_file_identity() {
    use std::os::unix::fs::MetadataExt;

    let dir = scratch_dir("append-identity");
    let handle = "web/fix-login";
    append_events(&dir, handle, &[note("one")]);
    let path = session_path(&dir, handle);
    let first_inode = std::fs::metadata(&path).unwrap().ino();

    append_events(&dir, handle, &[note("two")]);

    assert_eq!(std::fs::metadata(&path).unwrap().ino(), first_inode);
    assert_eq!(
        load::<serde_json::Value>(&dir, handle).events,
        vec![note("one"), note("two")]
    );
    let _ = std::fs::remove_dir_all(dir);
}

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
        std::fs::metadata(&path).unwrap().len() > MAX_LOG_BYTES,
        "fixture must leave the transcript over the byte cap"
    );

    let inode = std::fs::metadata(&path).unwrap().ino();
    append_events(&dir, handle, &[note("after")]);

    assert_eq!(
        std::fs::metadata(&path).unwrap().ino(),
        inode,
        "append over the byte cap rewrote the whole transcript"
    );
    assert_eq!(
        load::<serde_json::Value>(&dir, handle).events.len(),
        events.len() + 1
    );

    let _ = std::fs::remove_dir_all(dir);
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
    assert_eq!(
        load::<serde_json::Value>(&dir, handle).events,
        vec![note("ok")]
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn delete_session_removes_the_transcript_file() {
    let dir = scratch_dir("delete");
    let handle = "web/fix-login";
    append_events(&dir, handle, &[note("gone")]);
    assert!(session_path(&dir, handle).is_file());
    assert!(delete_session(&dir, handle));
    assert!(!session_path(&dir, handle).exists());
    assert!(!delete_session(&dir, handle));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn list_persisted_handles_decodes_slashy_handles() {
    let dir = scratch_dir("list");
    append_events(&dir, "web/fix/login", &[note("ok")]);
    append_events(&dir, "web/other", &[note("also")]);
    let mut handles = list_persisted_handles(&dir);
    handles.sort();
    assert_eq!(
        handles,
        vec!["web/fix/login".to_string(), "web/other".to_string()]
    );
    let _ = std::fs::remove_dir_all(dir);
}

fn write_rows(dir: &std::path::Path, handle: &str, rows: &[&str]) {
    let path = session_path(dir, handle);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, rows.join("\n") + "\n").unwrap();
}

const META_ROW: &str = r#"{"kind":"meta","v":1,"acp_session_id":"sess-abc","model":"auto"}"#;
const EVENT_ROW: &str = r#"{"kind":"event","event":{"type":"message","role":"agent","text":"x"}}"#;

#[test]
fn issue_1228_unreadable_file_is_an_error_not_an_empty_session() {
    let dir = scratch_dir("unreadable-1228");
    let handle = "web/unreadable";
    assert!(try_load::<serde_json::Value>(&dir, handle).is_ok());

    // A directory at the transcript path opens but cannot be read.
    let path = session_path(&dir, handle);
    std::fs::create_dir_all(&path).unwrap();
    assert!(try_load::<serde_json::Value>(&dir, handle).is_err());
    assert!(try_save_meta(&dir, handle, Some("sess-new"), "auto").is_err());
    assert!(clear_acp_session_id(&dir, handle).is_err());
    assert!(
        !path.with_extension("jsonl.tmp").exists(),
        "a failed load must not start a rewrite"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn issue_1240_interior_corrupt_rows_are_counted_and_survive_rewrite() {
    let dir = scratch_dir("corrupt-1240");
    let handle = "web/corrupt";
    write_rows(
        &dir,
        handle,
        &[META_ROW, EVENT_ROW, "{\"kind\":\"event\",\"eve", EVENT_ROW],
    );
    let loaded = try_load::<serde_json::Value>(&dir, handle).unwrap();
    assert_eq!(loaded.events.len(), 2);
    assert_eq!(loaded.corrupt_lines, 1);

    try_save_meta(&dir, handle, Some("sess-abc"), "auto").unwrap();
    let rewritten = try_load::<serde_json::Value>(&dir, handle).unwrap();
    assert_eq!(rewritten.events.len(), 2);
    assert_eq!(
        rewritten.corrupt_lines, 1,
        "the rewrite drops the bad row, so the count must be stored"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn issue_1240_truncated_final_row_is_not_corruption() {
    let dir = scratch_dir("truncated-tail-1240");
    let handle = "web/truncated-tail";
    write_rows(
        &dir,
        handle,
        &[META_ROW, EVENT_ROW, "{\"kind\":\"event\",\"eve"],
    );
    let loaded = try_load::<serde_json::Value>(&dir, handle).unwrap();
    assert_eq!(loaded.events.len(), 1);
    assert_eq!(loaded.corrupt_lines, 0);
    let _ = std::fs::remove_dir_all(dir);
}

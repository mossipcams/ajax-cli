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
fn legacy_meta_without_context_epoch_loads_as_zero() {
    let dir = scratch_dir("legacy-meta");
    let handle = "web/legacy-epoch";
    std::fs::create_dir_all(dir.join(WEB_SESSION_DIR)).unwrap();
    let meta = serde_json::json!({
        "kind": "meta",
        "v": 1,
        "acp_session_id": "sess-legacy",
        "model": "auto",
        "dropped": 0,
    });
    std::fs::write(session_path(&dir, handle), format!("{meta}\n")).unwrap();

    let loaded: StoredSession<serde_json::Value> = load(&dir, handle);
    assert_eq!(loaded.acp_session_id.as_deref(), Some("sess-legacy"));
    assert_eq!(loaded.context_epoch, 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn context_epoch_round_trips_with_session_id() {
    let dir = scratch_dir("epoch-round-trip");
    let handle = "web/epoch";
    let session = StoredSession {
        acp_session_id: Some("sess-epoch".to_string()),
        model: "composer-2.5".to_string(),
        events: vec![note("hello")],
        dropped: 0,
        context_epoch: 3,
    };
    persist(&dir, handle, &session);

    let loaded: StoredSession<serde_json::Value> = load(&dir, handle);
    assert_eq!(loaded.acp_session_id.as_deref(), Some("sess-epoch"));
    assert_eq!(loaded.context_epoch, 3);
    assert_eq!(loaded.events, vec![note("hello")]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn save_meta_preserves_existing_context_epoch() {
    let dir = scratch_dir("save-meta-epoch");
    let handle = "web/preserve-epoch";
    persist(
        &dir,
        handle,
        &StoredSession::<serde_json::Value> {
            acp_session_id: Some("sess-old".to_string()),
            model: "auto".to_string(),
            events: Vec::new(),
            dropped: 0,
            context_epoch: 2,
        },
    );
    save_meta(&dir, handle, Some("sess-new"), "gpt-5").unwrap();

    let loaded: StoredSession<serde_json::Value> = load(&dir, handle);
    assert_eq!(loaded.acp_session_id.as_deref(), Some("sess-new"));
    assert_eq!(loaded.model, "gpt-5");
    assert_eq!(loaded.context_epoch, 2);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn round_trip_events_and_meta() {
    let dir = scratch_dir("round-trip");
    let handle = "web/fix-login";
    let events = vec![note("one"), note("two")];
    append_events(&dir, handle, &events).unwrap();
    save_meta(&dir, handle, Some("sess-abc"), "composer-2.5").unwrap();
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
    append_events(&dir, handle, &[note("kept")]).unwrap();
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
    append_events(&dir, handle, &events).unwrap();
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
    append_events(&dir, handle, &[note("one")]).unwrap();
    let path = session_path(&dir, handle);
    let first_inode = std::fs::metadata(&path).unwrap().ino();

    append_events(&dir, handle, &[note("two")]).unwrap();

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

    append_events(&dir, handle, &events).unwrap();

    let path = session_path(&dir, handle);
    assert!(
        std::fs::metadata(&path).unwrap().len() > MAX_LOG_BYTES,
        "fixture must leave the transcript over the byte cap"
    );

    let inode = std::fs::metadata(&path).unwrap().ino();
    append_events(&dir, handle, &[note("after")]).unwrap();

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
    append_events(&dir, handle, &[note("ok")]).unwrap();
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
    append_events(&dir, handle, &[note("gone")]).unwrap();
    assert!(session_path(&dir, handle).is_file());
    assert!(delete_session(&dir, handle));
    assert!(!session_path(&dir, handle).exists());
    assert!(!delete_session(&dir, handle));
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(unix)]
fn make_dir_read_only(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o555);
    std::fs::set_permissions(path, perms).unwrap();
}

#[cfg(unix)]
#[test]
fn save_meta_returns_error_when_persist_fails_without_mutating_disk() {
    let dir = scratch_dir("save-meta-fail");
    let handle = "web/fail-save";
    append_events(&dir, handle, &[note("keep")]).unwrap();
    save_meta(&dir, handle, Some("sess-before"), "auto").unwrap();
    let before = load::<serde_json::Value>(&dir, handle);

    make_dir_read_only(&dir.join(WEB_SESSION_DIR));

    let result = save_meta(&dir, handle, Some("sess-after"), "gpt-5");
    assert!(
        result.is_err(),
        "save_meta must not claim success on persist failure"
    );

    let after = load::<serde_json::Value>(&dir, handle);
    assert_eq!(
        after, before,
        "failed save_meta must leave disk identity unchanged"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn clear_acp_session_id_returns_error_when_persist_fails_without_clearing_identity() {
    let dir = scratch_dir("clear-id-fail");
    let handle = "web/fail-clear";
    save_meta(&dir, handle, Some("sess-sticky"), "auto").unwrap();
    assert_eq!(
        load::<serde_json::Value>(&dir, handle)
            .acp_session_id
            .as_deref(),
        Some("sess-sticky")
    );

    make_dir_read_only(&dir.join(WEB_SESSION_DIR));

    let result = clear_acp_session_id(&dir, handle);
    assert!(
        result.is_err(),
        "clear_acp_session_id must not claim success on persist failure"
    );
    assert_eq!(
        load::<serde_json::Value>(&dir, handle)
            .acp_session_id
            .as_deref(),
        Some("sess-sticky"),
        "failed clear must leave the stored session id intact"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn append_events_returns_error_when_append_fails_without_durable_write() {
    let dir = scratch_dir("append-fail");
    let handle = "web/fail-append";
    append_events(&dir, handle, &[note("before")]).unwrap();
    let before = load::<serde_json::Value>(&dir, handle);

    let _fail = ForceAppendFailGuard::enable();
    let result = append_events(&dir, handle, &[note("after")]);
    drop(_fail);

    assert!(
        result.is_err(),
        "append_events must not claim success on append failure"
    );
    assert_eq!(
        load::<serde_json::Value>(&dir, handle),
        before,
        "failed append must leave the transcript unchanged"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn list_persisted_handles_decodes_slashy_handles() {
    let dir = scratch_dir("list");
    append_events(&dir, "web/fix/login", &[note("ok")]).unwrap();
    append_events(&dir, "web/other", &[note("also")]).unwrap();
    let mut handles = list_persisted_handles(&dir);
    handles.sort();
    assert_eq!(
        handles,
        vec!["web/fix/login".to_string(), "web/other".to_string()]
    );
    let _ = std::fs::remove_dir_all(dir);
}

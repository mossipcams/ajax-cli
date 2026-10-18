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
fn save_meta_updates_harness_on_existing_run() {
    let dir = scratch_dir("save-harness");
    let handle = "web/save-harness";
    let store = SqliteRegistryStore::new(dir.join("ajax.db"));
    try_save_meta(
        &store,
        &dir,
        handle,
        "claude",
        Some("claude-session"),
        "auto",
    )
    .unwrap();
    let original = store.latest_run_for_task(handle).unwrap().unwrap();

    try_save_meta(&store, &dir, handle, "codex", Some("codex-session"), "auto").unwrap();

    let updated = store.latest_run_for_task(handle).unwrap().unwrap();
    assert_eq!(updated.run_id, original.run_id);
    assert_eq!(updated.harness, "codex");
    assert_eq!(updated.native_session_id.as_deref(), Some("codex-session"));
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
fn torn_final_line_is_skipped_on_import() {
    let dir = scratch_dir("torn");
    let handle = "web/fix-login";
    let path = session_path(&dir, handle);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    // Legacy JSONL import source: one good event row plus a torn final line.
    std::fs::write(
        &path,
        format!(
            "{{\"kind\":\"event\",\"event\":{}}}\n{{\"kind\":\"event\",\"event\":{{\"type\":\"mess",
            serde_json::to_string(&note("kept")).unwrap()
        ),
    )
    .unwrap();
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

#[test]
fn incremental_appends_keep_earlier_events_in_order() {
    let dir = scratch_dir("append-identity");
    let handle = "web/fix-login";
    append_events(&dir, handle, &[note("one")]);

    append_events(&dir, handle, &[note("two")]);

    assert_eq!(
        load::<serde_json::Value>(&dir, handle).events,
        vec![note("one"), note("two")]
    );
    assert_eq!(load::<serde_json::Value>(&dir, handle).dropped, 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn append_past_the_event_cap_retains_exactly_max_log_events() {
    let dir = scratch_dir("oversized-append");
    let handle = "web/fix-login";
    let events: Vec<_> = (0..MAX_LOG_EVENTS + 5)
        .map(|i| note(&i.to_string()))
        .collect();

    append_events(&dir, handle, &events);

    let loaded: StoredSession<serde_json::Value> = load(&dir, handle);
    assert_eq!(loaded.events.len(), MAX_LOG_EVENTS);
    assert_eq!(loaded.dropped, 5);
    assert_eq!(
        loaded.events[MAX_LOG_EVENTS - 1],
        note(&(MAX_LOG_EVENTS + 4).to_string())
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn handle_with_slash_encodes_to_single_filename() {
    let dir = scratch_dir("encode");
    let handle = "web/fix/login";
    // Legacy JSONL import source written directly so the filename check holds.
    let path = session_path(&dir, handle);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        format!(
            "{{\"kind\":\"event\",\"event\":{}}}\n",
            serde_json::to_string(&note("ok")).unwrap()
        ),
    )
    .unwrap();
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
    let path = session_path(&dir, handle);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, format!("{}\n", EVENT_ROW)).unwrap();
    assert!(path.is_file());
    let store = ajax_core::registry::SqliteRegistryStore::new(dir.join("ajax.db"));
    assert!(delete_session(&store, &dir, handle));
    assert!(!path.exists());
    assert!(!delete_session(&store, &dir, handle));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn list_persisted_handles_decodes_slashy_handles() {
    let dir = scratch_dir("list");
    append_events(&dir, "web/fix/login", &[note("ok")]);
    append_events(&dir, "web/other", &[note("also")]);
    let store = ajax_core::registry::SqliteRegistryStore::new(dir.join("ajax.db"));
    let mut handles = list_persisted_handles(&store, &dir);
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
    let store = ajax_core::registry::SqliteRegistryStore::new(dir.join("ajax.db"));
    assert!(try_load::<serde_json::Value>(&store, &dir, handle, "unknown").is_ok());

    // A directory at the transcript path opens but cannot be read.
    let path = session_path(&dir, handle);
    std::fs::create_dir_all(&path).unwrap();
    assert!(try_load::<serde_json::Value>(&store, &dir, handle, "unknown").is_err());
    assert!(try_save_meta(&store, &dir, handle, "unknown", Some("sess-new"), "auto").is_err());
    assert!(clear_acp_session_id(&store, &dir, handle, "unknown").is_err());
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
    let store = ajax_core::registry::SqliteRegistryStore::new(dir.join("ajax.db"));
    write_rows(
        &dir,
        handle,
        &[META_ROW, EVENT_ROW, "{\"kind\":\"event\",\"eve", EVENT_ROW],
    );
    let loaded = try_load::<serde_json::Value>(&store, &dir, handle, "unknown").unwrap();
    assert_eq!(loaded.events.len(), 2);
    assert_eq!(loaded.corrupt_lines, 1);

    try_save_meta(&store, &dir, handle, "unknown", Some("sess-abc"), "auto").unwrap();
    let rewritten = try_load::<serde_json::Value>(&store, &dir, handle, "unknown").unwrap();
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
    let store = ajax_core::registry::SqliteRegistryStore::new(dir.join("ajax.db"));
    write_rows(
        &dir,
        handle,
        &[META_ROW, EVENT_ROW, "{\"kind\":\"event\",\"eve"],
    );
    let loaded = try_load::<serde_json::Value>(&store, &dir, handle, "unknown").unwrap();
    assert_eq!(loaded.events.len(), 1);
    assert_eq!(loaded.corrupt_lines, 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn forced_meta_write_failure_leaves_the_stored_session_id_unchanged() {
    let dir = scratch_dir("forced-meta-fail");
    let handle = "web/forced";
    save_meta(&dir, handle, Some("sess-old"), "auto");

    set_force_meta_write_fail(true);
    let store = ajax_core::registry::SqliteRegistryStore::new(dir.join("ajax.db"));
    assert!(try_save_meta(&store, &dir, handle, "unknown", Some("sess-new"), "auto").is_err());
    assert!(clear_acp_session_id(&store, &dir, handle, "unknown").is_err());
    set_force_meta_write_fail(false);

    assert_eq!(
        load::<serde_json::Value>(&dir, handle)
            .acp_session_id
            .as_deref(),
        Some("sess-old")
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn import_is_not_repeated_when_the_run_already_exists() {
    let dir = scratch_dir("no-reimport");
    let handle = "web/no-reimport";
    let store = ajax_core::registry::SqliteRegistryStore::new(dir.join("ajax.db"));

    write_rows(&dir, handle, &[META_ROW, EVENT_ROW]);
    let loaded = try_load::<serde_json::Value>(&store, &dir, handle, "unknown").unwrap();
    assert_eq!(loaded.events.len(), 1);
    let path = session_path(&dir, handle);
    assert!(!path.exists());
    assert!(path.with_extension("jsonl.migrated").exists());

    write_rows(&dir, handle, &[META_ROW, EVENT_ROW]);
    let reloaded = try_load::<serde_json::Value>(&store, &dir, handle, "unknown").unwrap();
    assert_eq!(reloaded.events.len(), 1);
    assert!(!path.exists());
    assert!(path.with_extension("jsonl.migrated").exists());
    assert!(path.with_extension("jsonl.migrated.1").exists());

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn delete_session_removes_numbered_migration_leftovers() {
    let dir = scratch_dir("delete-migrated");
    let session_dir = dir.join(WEB_SESSION_DIR);
    std::fs::create_dir_all(&session_dir).unwrap();
    let enc_a = encode_handle("web/a");
    let enc_ab = encode_handle("web/ab");
    for name in [
        format!("{enc_a}.jsonl.migrated"),
        format!("{enc_a}.jsonl.migrated.1"),
        format!("{enc_a}.jsonl.migrated.2"),
        format!("{enc_ab}.jsonl.migrated"),
    ] {
        std::fs::write(session_dir.join(&name), EVENT_ROW).unwrap();
    }
    let store = ajax_core::registry::SqliteRegistryStore::new(dir.join("ajax.db"));
    assert!(delete_session(&store, &dir, "web/a"));
    for n in ["migrated", "migrated.1", "migrated.2"] {
        let gone = session_dir.join(format!("{enc_a}.jsonl.{n}"));
        assert!(!gone.exists(), "{gone:?} must be deleted");
    }
    let kept = session_dir.join(format!("{enc_ab}.jsonl.migrated"));
    assert!(
        kept.exists(),
        "a longer handle sharing the prefix must be untouched"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn delete_session_still_removes_files_when_the_database_delete_fails() {
    let dir = scratch_dir("delete-db-fail");
    // A directory where the registry lives makes the database unusable.
    std::fs::create_dir_all(dir.join("ajax.db")).unwrap();
    let path = session_path(&dir, "web/a");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let migrated = path.with_extension("jsonl.migrated");
    std::fs::write(&migrated, EVENT_ROW).unwrap();
    assert!(migrated.is_file());

    let store = ajax_core::registry::SqliteRegistryStore::new(dir.join("ajax.db"));
    assert!(!delete_session(&store, &dir, "web/a"));
    assert!(
        !migrated.exists(),
        "files must be removed even when the db delete fails"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn list_persisted_handles_sees_numbered_migration_leftovers() {
    let dir = scratch_dir("list-migrated");
    let session_dir = dir.join(WEB_SESSION_DIR);
    std::fs::create_dir_all(&session_dir).unwrap();
    let enc = encode_handle("web/x");
    std::fs::write(
        session_dir.join(format!("{enc}.jsonl.migrated.3")),
        EVENT_ROW,
    )
    .unwrap();
    let store = ajax_core::registry::SqliteRegistryStore::new(dir.join("ajax.db"));
    assert_eq!(list_persisted_handles(&store, &dir), vec!["web/x"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn record_child_identity_round_trips_and_clears() {
    let dir = scratch_dir("child-identity");
    let handle = "web/child-identity";
    let store = ajax_core::registry::SqliteRegistryStore::new(dir.join("ajax.db"));

    record_child_identity(
        &store,
        &dir,
        handle,
        "unknown",
        Some((4242, "Mon Oct 12 10:00:00 2026".to_string())),
        "boot-a",
    )
    .unwrap();

    let run = store.latest_run_for_task(handle).unwrap().unwrap();
    assert_eq!(run.child, Some((4242, 4242)));
    assert_eq!(
        run.child_started,
        Some("Mon Oct 12 10:00:00 2026".to_string())
    );
    assert_eq!(run.owner_boot_id, Some("boot-a".to_string()));

    // A respawn with no live child clears the identity but keeps the owner.
    record_child_identity(&store, &dir, handle, "unknown", None, "boot-a").unwrap();

    let run = store.latest_run_for_task(handle).unwrap().unwrap();
    assert_eq!(run.child, None);
    assert_eq!(run.child_started, None);
    assert_eq!(run.owner_boot_id, Some("boot-a".to_string()));

    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn migrated_transcript_copy_is_owner_only() {
    use std::os::unix::fs::PermissionsExt;

    let dir = scratch_dir("owner-only-migrated");
    let handle = "web/owner-only";
    write_rows(&dir, handle, &[META_ROW, EVENT_ROW]);
    std::fs::set_permissions(
        session_path(&dir, handle),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();

    let store = ajax_core::registry::SqliteRegistryStore::new(dir.join("ajax.db"));
    try_load::<serde_json::Value>(&store, &dir, handle, "unknown").unwrap();

    let migrated = session_path(&dir, handle).with_extension("jsonl.migrated");
    assert!(migrated.exists());
    assert_eq!(
        std::fs::metadata(&migrated).unwrap().permissions().mode() & 0o777,
        0o600
    );

    let _ = std::fs::remove_dir_all(dir);
}

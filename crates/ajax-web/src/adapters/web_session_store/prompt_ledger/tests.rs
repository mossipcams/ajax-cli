use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

fn scratch_dir(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ajax-web-prompt-ledger-{label}-{}-{}",
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
fn round_trip_atomic_ledger() {
    let dir = scratch_dir("round-trip");
    let handle = "web/fix-login";
    let mut ledger = PromptLedger::default();
    ledger.upsert_queued(
        "p1".to_string(),
        "hello (1 attachment)".to_string(),
        "hello".to_string(),
        Vec::new(),
    );
    persist(&dir, handle, &ledger).expect("persist");
    let loaded = load(&dir, handle).expect("load");
    assert_eq!(loaded, ledger);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.join("ajax.db"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn ledger_and_events_commit_together() {
    let dir = scratch_dir("ledger-events");
    let handle = "web/fix-login";
    let mut ledger = PromptLedger::default();
    ledger.upsert_queued(
        "p1".to_string(),
        "hello".to_string(),
        "hello".to_string(),
        Vec::new(),
    );
    let events: [serde_json::Value; 2] = [
        serde_json::json!({"type": "delta", "text": "a"}),
        serde_json::json!({"type": "done"}),
    ];
    try_persist_with_events(
        &ajax_core::registry::SqliteRegistryStore::new(dir.join("ajax.db")),
        &dir,
        handle,
        &ledger,
        &events,
    )
    .expect("persist with events");

    let loaded = super::super::load::<serde_json::Value>(&dir, handle);
    let ledger_loaded = load(&dir, handle).expect("load ledger");
    assert!(ledger_loaded.owns_prompt("p1"));
    assert_eq!(loaded.events, events.into_iter().collect::<Vec<_>>());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn failed_ledger_commit_writes_no_events() {
    let dir = scratch_dir("ledger-events-fail");
    let handle = "web/fix-login";
    let store = ajax_core::registry::SqliteRegistryStore::new(dir.join("ajax.db"));
    let mut first = PromptLedger::default();
    first.upsert_queued(
        "p1".to_string(),
        "one".to_string(),
        "one".to_string(),
        Vec::new(),
    );
    persist(&dir, handle, &first).expect("persist p1");

    // Absent prompt is cancelled; the id then collides with the tombstone row.
    persist(&dir, handle, &PromptLedger::default()).expect("cancel p1");
    let mut again = PromptLedger::default();
    again.upsert_queued(
        "p1".to_string(),
        "again".to_string(),
        "again".to_string(),
        Vec::new(),
    );
    let events: [serde_json::Value; 1] = [serde_json::json!({"type": "delta"})];
    let result = super::try_persist_with_events(&store, &dir, handle, &again, &events);
    assert!(result.is_err(), "duplicate prompt id must fail");

    let loaded = super::super::load::<serde_json::Value>(&dir, handle);
    assert!(loaded.events.is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn events_alone_commit_when_ledger_is_unchanged() {
    let dir = scratch_dir("events-alone");
    let handle = "web/fix-login";
    let store = ajax_core::registry::SqliteRegistryStore::new(dir.join("ajax.db"));
    let mut ledger = PromptLedger::default();
    ledger.upsert_queued(
        "p1".to_string(),
        "one".to_string(),
        "one".to_string(),
        Vec::new(),
    );
    persist(&dir, handle, &ledger).expect("first persist");
    persist(&dir, handle, &ledger).expect("second persist (no events)");

    let events: [serde_json::Value; 1] = [serde_json::json!({"type": "delta", "text": "solo"})];
    super::try_persist_with_events(&store, &dir, handle, &ledger, &events)
        .expect("events-only persist");

    let loaded = super::super::load::<serde_json::Value>(&dir, handle);
    assert_eq!(loaded.events, events.into_iter().collect::<Vec<_>>());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn persist_failure_surfaces_error() {
    let dir = scratch_dir("persist-fail");
    let handle = "web/fix-login";
    let _fail = ForcePersistFailGuard::enable();
    let result = persist(&dir, handle, &PromptLedger::default());
    drop(_fail);
    assert!(result.is_err());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn recover_marks_dispatching_interrupted_and_returns_queued() {
    let mut ledger = PromptLedger::default();
    ledger.upsert_queued(
        "queued".to_string(),
        "queued".to_string(),
        "queued".to_string(),
        Vec::new(),
    );
    ledger.upsert_queued(
        "orphan".to_string(),
        "orphan".to_string(),
        "orphan".to_string(),
        Vec::new(),
    );
    assert!(ledger.mark_dispatching("orphan"));
    let (queued, interrupted) = ledger.recover_after_restart();
    assert_eq!(interrupted, vec!["orphan".to_string()]);
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].client_message_id, "queued");
    assert_eq!(
        ledger.entry("orphan").map(|entry| entry.phase),
        Some(PromptPhase::Interrupted)
    );
}

#[test]
fn owns_prompt_is_dedupe_authority() {
    let mut ledger = PromptLedger::default();
    assert!(!ledger.owns_prompt("p1"));
    ledger.upsert_queued(
        "p1".to_string(),
        "one".to_string(),
        "one".to_string(),
        Vec::new(),
    );
    assert!(ledger.owns_prompt("p1"));
    ledger.mark_completed("p1");
    assert!(ledger.owns_prompt("p1"));
    let entry = ledger.entry("p1").expect("tombstone");
    assert_eq!(entry.phase, PromptPhase::Completed);
    assert!(entry.transcript_text.is_empty());
    assert!(entry.prompt_text.is_empty());
    assert!(entry.content_blocks.is_empty());
}

#[test]
fn delete_ledger_removes_sidecar_file() {
    let dir = scratch_dir("delete");
    let handle = "web/fix-login";
    let path = ledger_path(&dir, handle);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, r#"{"kind":"prompt_ledger","v":1,"entries":[]}"#).unwrap();
    assert!(ledger_path(&dir, handle).is_file());
    assert!(delete_ledger(&dir, handle));
    assert!(!ledger_path(&dir, handle).exists());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn load_rejects_malformed_ledger() {
    let dir = scratch_dir("malformed");
    let handle = "web/fix-login";
    let path = ledger_path(&dir, handle);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "{not json").unwrap();
    assert_eq!(load(&dir, handle), Err(LedgerLoadError::Malformed));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn load_rejects_unsupported_version() {
    let dir = scratch_dir("new-version");
    let handle = "web/fix-login";
    let path = ledger_path(&dir, handle);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, r#"{"kind":"prompt_ledger","v":99,"entries":[]}"#).unwrap();
    assert_eq!(
        load(&dir, handle),
        Err(LedgerLoadError::UnsupportedVersion { found: 99 })
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn load_rejects_wrong_kind() {
    let dir = scratch_dir("wrong-kind");
    let handle = "web/fix-login";
    let path = ledger_path(&dir, handle);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, r#"{"kind":"other","v":1,"entries":[]}"#).unwrap();
    assert!(matches!(
        load(&dir, handle),
        Err(LedgerLoadError::WrongKind { .. })
    ));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn legacy_ledger_is_imported_once_and_removed() {
    let dir = scratch_dir("legacy-import");
    let handle = "web/fix-login";
    let path = ledger_path(&dir, handle);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        r#"{"kind":"prompt_ledger","v":1,"entries":[{"clientMessageId":"p1","phase":"queued","transcript_text":"hi","prompt_text":"hi"}]}"#,
    )
    .unwrap();

    let loaded = load(&dir, handle).expect("load");
    assert_eq!(loaded.entries.len(), 1);
    let entry = &loaded.entries[0];
    assert_eq!(entry.client_message_id, "p1");
    assert_eq!(entry.phase, PromptPhase::Queued);
    assert_eq!(entry.prompt_text, "hi");

    let base = path.file_name().unwrap().to_str().unwrap();
    let migrated = path.with_file_name(format!("{base}.migrated"));
    assert!(!path.exists());
    assert!(
        !migrated.exists(),
        "an imported legacy ledger is durable in the database and must be deleted"
    );

    let again = load(&dir, handle).expect("load");
    assert_eq!(again, loaded);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn legacy_ledger_seen_after_import_is_renamed_not_imported() {
    let dir = scratch_dir("legacy-import-then-sidecar");
    let handle = "web/fix-login";

    let mut ledger = PromptLedger::default();
    ledger.upsert_queued(
        "p1".to_string(),
        "one (1 attachment)".to_string(),
        "one".to_string(),
        Vec::new(),
    );
    persist(&dir, handle, &ledger).expect("persist p1");

    // A legacy sidecar appearing after the run exists is not trusted content.
    let path = ledger_path(&dir, handle);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        r#"{"kind":"prompt_ledger","v":1,"entries":[{"clientMessageId":"p2","phase":"queued","transcript_text":"two","prompt_text":"two"}]}"#
    )
    .unwrap();

    let loaded = load(&dir, handle).expect("load");
    assert_eq!(loaded.entries.len(), 1);
    assert_eq!(loaded.entries[0].client_message_id, "p1");

    let base = path.file_name().unwrap().to_str().unwrap();
    let migrated = path.with_file_name(format!("{base}.migrated"));
    assert!(!path.exists());
    assert!(migrated.is_file());

    let _ = std::fs::remove_dir_all(dir);
}

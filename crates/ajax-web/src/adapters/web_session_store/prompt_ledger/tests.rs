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
        let mode = std::fs::metadata(ledger_path(&dir, handle))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
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
    persist(&dir, handle, &PromptLedger::default()).expect("persist");
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

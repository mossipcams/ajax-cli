use crate::registry::{NewRun, SqliteRegistryStore};

fn run(id: &str, task: &str) -> NewRun {
    NewRun {
        run_id: id.into(),
        task_id: task.into(),
        harness: "codex".into(),
        model: "model".into(),
        created_unix_ms: 1234,
    }
}

fn nonce() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

fn temp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "ajax-execution-history-{}-{name}-{}.sqlite",
        std::process::id(),
        nonce()
    ))
}

#[test]
fn open_creates_missing_parent_directory() {
    let missing_dir = std::env::temp_dir().join(format!(
        "ajax-execution-history-{}-nested-{}",
        std::process::id(),
        nonce()
    ));
    assert!(!missing_dir.exists());
    let nested = missing_dir.join("missing-dir.sqlite");
    let store = SqliteRegistryStore::new(nested.clone());
    assert_eq!(store.latest_run_for_task("registry-task").unwrap(), None);
    assert!(nested.parent().unwrap().is_dir());
}

#[test]
fn unreadable_history_round_trips_and_defaults_to_zero() {
    let store = SqliteRegistryStore::new(temp_path("unreadable"));
    store
        .create_run_with_history(&run("imported", "registry-task"), None, 1, &["e".into()], 3)
        .unwrap();
    let record = store.latest_run_for_task("registry-task").unwrap().unwrap();
    assert_eq!(record.unreadable_history_rows, 3);

    store.create_run(&run("plain", "registry-task")).unwrap();
    let record = store.latest_run_for_task("registry-task").unwrap().unwrap();
    assert_eq!(record.run_id, "plain");
    assert_eq!(record.unreadable_history_rows, 0);
}

#[test]
fn list_run_task_ids_is_distinct_and_sorted() {
    let store = SqliteRegistryStore::new(temp_path("task-ids"));
    store.create_run(&run("r1", "task-b")).unwrap();
    store.create_run(&run("r2", "task-a")).unwrap();
    store
        .create_run_with_history(&run("r3", "task-a"), None, 1, &[], 0)
        .unwrap();
    assert_eq!(store.list_run_task_ids().unwrap(), vec!["task-a", "task-b"]);
}

#[cfg(unix)]
#[test]
fn database_file_is_owner_only_and_reapplied_on_open() {
    use std::os::unix::fs::PermissionsExt;
    let path = temp_path("owner-only");
    let store = SqliteRegistryStore::new(path.clone());
    assert_eq!(store.latest_run_for_task("x").unwrap(), None);
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);

    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(store.latest_run_for_task("x").unwrap(), None);
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

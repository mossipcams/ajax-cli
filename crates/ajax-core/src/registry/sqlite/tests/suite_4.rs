use super::super::*;
use super::*;

#[test]
fn sqlite_store_rejects_stale_expected_revision_without_overwriting_newer_state() {
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-stale-revision.db",
        std::process::id()
    ));
    let store = SqliteRegistryStore::new(&path);
    let mut first = InMemoryRegistry::default();
    first
        .create_task(task("task-1", "web", "fix-login"))
        .unwrap();
    store.save(&first).unwrap();
    let revision = store.current_revision().unwrap();

    let mut newer = first.clone();
    newer.get_task_mut(&TaskId::new("task-1")).unwrap().title = "newer".to_string();
    store.save_if_revision(&newer, revision).unwrap();

    let error = store.save_if_revision(&first, revision).unwrap_err();
    assert_eq!(
        error,
        RegistrySnapshotError::RevisionConflict {
            expected: revision,
            actual: revision + 1,
        }
    );
    assert_eq!(
        store
            .load()
            .unwrap()
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .title,
        "newer"
    );
    std::fs::remove_file(path).unwrap();
}

use super::super::connection_pool::{fresh_opens, idle_count};
use super::super::migrations::SQLITE_SCHEMA_VERSION;
use super::super::*;
use super::*;

fn reuse_path(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "ajax-registry-reuse-{}-{tag}.db",
        std::process::id()
    ))
}

fn registry_with(task_ids: &[(&str, &str)]) -> InMemoryRegistry {
    let mut registry = InMemoryRegistry::default();
    for (id, handle) in task_ids {
        registry.create_task(task(id, "web", handle)).unwrap();
    }
    registry
}

fn persisted_task_ids(path: &std::path::Path) -> Vec<String> {
    let connection = rusqlite::Connection::open(path).unwrap();
    let mut statement = connection
        .prepare("SELECT task_id FROM registry_tasks ORDER BY task_id")
        .unwrap();
    statement
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn consecutive_store_calls_reuse_one_connection_and_migrate_once() {
    let path = reuse_path("reuse");
    let store = SqliteRegistryStore::new(&path);
    let registry = registry_with(&[("task-a", "alpha")]);

    store.save(&registry).unwrap();
    for _ in 0..10 {
        store.load().unwrap();
        store.current_revision().unwrap();
        store.save(&registry).unwrap();
    }

    assert_eq!(fresh_opens(&path), 1);
    assert_eq!(idle_count(&path), 1);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn deleted_database_file_is_reopened_and_new_writes_land_in_the_new_file() {
    let path = reuse_path("deleted");
    let store = SqliteRegistryStore::new(&path);
    store.save(&registry_with(&[("task-old", "old")])).unwrap();
    assert_eq!(fresh_opens(&path), 1);

    std::fs::remove_file(&path).unwrap();
    store.save(&registry_with(&[("task-new", "new")])).unwrap();

    assert_eq!(fresh_opens(&path), 2);
    assert_eq!(persisted_task_ids(&path), vec!["task-new"]);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn replaced_database_file_is_reopened_and_new_writes_land_in_the_replacement() {
    let path = reuse_path("replaced");
    let replacement_path = reuse_path("replacement");
    let store = SqliteRegistryStore::new(&path);
    store.save(&registry_with(&[("task-old", "old")])).unwrap();
    SqliteRegistryStore::new(&replacement_path)
        .save(&registry_with(&[("task-replacement", "replacement")]))
        .unwrap();

    std::fs::rename(&replacement_path, &path).unwrap();
    let loaded = store.load().unwrap();
    store
        .save(&registry_with(&[("task-after", "after")]))
        .unwrap();

    assert_eq!(fresh_opens(&path), 2);
    assert!(loaded.get_task(&TaskId::new("task-replacement")).is_some());
    assert!(loaded.get_task(&TaskId::new("task-old")).is_none());
    assert_eq!(persisted_task_ids(&path), vec!["task-after"]);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn connection_with_open_transaction_is_not_returned_to_the_pool() {
    let path = reuse_path("open-transaction");
    let store = SqliteRegistryStore::new(&path);
    store.current_revision().unwrap();
    assert_eq!(idle_count(&path), 1);

    let connection = store.open().unwrap();
    connection.execute_batch("BEGIN").unwrap();
    drop(connection);

    assert_eq!(idle_count(&path), 0);
    store.current_revision().unwrap();
    assert_eq!(fresh_opens(&path), 2);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn changed_user_version_on_disk_surfaces_incompatible_schema_and_drops_pool() {
    let path = reuse_path("user-version");
    let store = SqliteRegistryStore::new(&path);
    store.current_revision().unwrap();
    assert_eq!(idle_count(&path), 1);

    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("PRAGMA user_version = 99;")
        .unwrap();

    let expected = RegistrySnapshotError::IncompatibleSchema {
        found: 99,
        supported: SQLITE_SCHEMA_VERSION,
    };
    assert_eq!(store.current_revision().unwrap_err(), expected);
    assert_eq!(idle_count(&path), 0);
    assert_eq!(fresh_opens(&path), 1);

    assert_eq!(store.current_revision().unwrap_err(), expected);
    assert_eq!(fresh_opens(&path), 2);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn concurrent_store_calls_complete_without_deadlock_or_busy_errors() {
    let path = reuse_path("concurrent");
    let store = SqliteRegistryStore::new(&path);
    let registry = registry_with(&[("task-a", "alpha"), ("task-b", "beta")]);
    store.save(&registry).unwrap();

    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                for _ in 0..50 {
                    store.save(&registry).unwrap();
                    assert_eq!(store.load().unwrap().tasks.len(), 2);
                }
            });
        }
    });

    assert!(idle_count(&path) <= 4);
    std::fs::remove_file(&path).unwrap();
}

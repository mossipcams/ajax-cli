use crate::registry::{NewRun, RunUpdate, SqliteRegistryStore};

fn new_run(id: &str) -> NewRun {
    NewRun {
        run_id: id.into(),
        task_id: "registry-task".into(),
        harness: "codex".into(),
        model: "model".into(),
        created_unix_ms: 1234,
    }
}

fn store() -> (SqliteRegistryStore, std::path::PathBuf) {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "ajax-execution-child-{}-{nonce}.sqlite",
        std::process::id()
    ));
    (SqliteRegistryStore::new(&path), path)
}

#[test]
fn child_started_round_trips_with_child_and_boot_id() {
    let (store, _) = store();
    store.create_run(&new_run("run")).unwrap();
    store
        .commit_execution(&crate::registry::ExecutionCommit {
            run_id: "run".into(),
            run_update: Some(RunUpdate {
                child: Some(Some((4242, 4242))),
                child_started: Some(Some("Mon Oct 12 10:00:00 2026".into())),
                owner_boot_id: Some(Some("boot-a".into())),
                ..Default::default()
            }),
            ..Default::default()
        })
        .unwrap();

    let record = store.latest_run_for_task("registry-task").unwrap().unwrap();
    assert_eq!(record.child, Some((4242, 4242)));
    assert_eq!(
        record.child_started.as_deref(),
        Some("Mon Oct 12 10:00:00 2026")
    );
    assert_eq!(record.owner_boot_id.as_deref(), Some("boot-a"));

    let listed = store.runs_with_children().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].run_id, "run");
    assert_eq!(listed[0].child, Some((4242, 4242)));
    assert_eq!(
        listed[0].child_started.as_deref(),
        Some("Mon Oct 12 10:00:00 2026")
    );
}

#[test]
fn runs_without_a_child_are_not_listed() {
    let (store, _) = store();
    store.create_run(&new_run("run")).unwrap();
    assert!(store.runs_with_children().unwrap().is_empty());
}

#[test]
fn clearing_child_and_started_removes_run_from_listing() {
    let (store, _) = store();
    store.create_run(&new_run("run")).unwrap();
    store
        .commit_execution(&crate::registry::ExecutionCommit {
            run_id: "run".into(),
            run_update: Some(RunUpdate {
                child: Some(Some((4242, 4242))),
                child_started: Some(Some("Mon Oct 12 10:00:00 2026".into())),
                ..Default::default()
            }),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(store.runs_with_children().unwrap().len(), 1);

    store
        .commit_execution(&crate::registry::ExecutionCommit {
            run_id: "run".into(),
            run_update: Some(RunUpdate {
                child: Some(None),
                child_started: Some(None),
                ..Default::default()
            }),
            ..Default::default()
        })
        .unwrap();

    assert!(store.runs_with_children().unwrap().is_empty());
    let record = store.latest_run_for_task("registry-task").unwrap().unwrap();
    assert_eq!(record.child, None);
    assert_eq!(record.child_started, None);
}

#[test]
fn runs_with_children_preserves_creation_order() {
    let (store, _) = store();
    store.create_run(&new_run("first")).unwrap();
    store
        .create_run(&NewRun {
            run_id: "second".into(),
            ..new_run("unused")
        })
        .unwrap();
    for id in ["first", "second"] {
        store
            .commit_execution(&crate::registry::ExecutionCommit {
                run_id: id.into(),
                run_update: Some(RunUpdate {
                    child: Some(Some((1, 1))),
                    ..Default::default()
                }),
                ..Default::default()
            })
            .unwrap();
    }

    let listed = store.runs_with_children().unwrap();
    assert_eq!(
        listed
            .iter()
            .map(|record| record.run_id.as_str())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
}

#[test]
fn earlier_schema_ten_builds_get_missing_agent_runs_columns() {
    let (store, path) = store();
    // Opens the current schema without writing any run.
    assert!(store
        .latest_run_for_task("registry-task")
        .unwrap()
        .is_none());

    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "ALTER TABLE agent_runs DROP COLUMN unreadable_history_rows; \
             ALTER TABLE agent_runs DROP COLUMN child_started;",
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO agent_runs (run_id, task_id, harness, model, schema_version, created_unix_ms) \
             VALUES ('legacy', 'legacy-task', 'codex', 'model', 1, 42)",
            [],
        )
        .unwrap();

    let reopened = SqliteRegistryStore::new(&path);
    let record = reopened
        .latest_run_for_task("legacy-task")
        .unwrap()
        .unwrap();
    assert_eq!(record.run_id, "legacy");
    assert_eq!(record.unreadable_history_rows, 0);
    assert_eq!(record.child_started, None);
}

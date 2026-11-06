use super::super::*;
use super::*;

fn store_path(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{tag}.db",
        std::process::id()
    ))
}

#[test]
fn record_activity_updates_only_b_activity_fields_and_keeps_lifecycle_and_a_intact() {
    let path = store_path("activity-fields");
    let store = SqliteRegistryStore::new(&path);
    let mut registry = InMemoryRegistry::default();
    registry
        .create_task(task("task-a", "web", "alpha"))
        .unwrap();
    registry.create_task(task("task-b", "web", "beta")).unwrap();
    store.save(&registry).unwrap();
    let baseline_revision = store.current_revision().unwrap();

    let before = store.load().unwrap();
    let event_count_before = before.events.len();
    let a_before = before.get_task(&TaskId::new("task-a")).unwrap().clone();
    let b_before = before.get_task(&TaskId::new("task-b")).unwrap().clone();
    let b_lifecycle_before = before
        .get_task(&TaskId::new("task-b"))
        .unwrap()
        .lifecycle_status;

    let mut in_memory = store.load().unwrap();
    let b = in_memory.get_task_mut(&TaskId::new("task-b")).unwrap();
    b.last_activity_at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_900_000_000);
    b.agent_status = AgentRuntimeStatus::Done;
    b.live_status = Some(LiveObservation::new(
        LiveStatusKind::ShellIdle,
        "shell idle".to_string(),
    ));
    b.live_status_observed_at = Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_900_000_010));
    b.add_side_flag(SideFlag::Dirty);
    b.lifecycle_status = LifecycleStatus::Active;

    let next_revision = store.record_activity(b, &b_before).unwrap();
    assert_eq!(next_revision, baseline_revision + 1);

    let after = store.load().unwrap();
    assert_eq!(after.events.len(), event_count_before);
    let a_after = after.get_task(&TaskId::new("task-a")).unwrap();
    assert_eq!(a_after, &a_before);
    let b_after = after.get_task(&TaskId::new("task-b")).unwrap();
    assert_eq!(b_after.lifecycle_status, b_lifecycle_before);
    assert_ne!(b_after.lifecycle_status, LifecycleStatus::Active);
    assert_eq!(
        b_after.last_activity_at,
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_900_000_000)
    );
    assert_eq!(b_after.agent_status, AgentRuntimeStatus::Done);
    let live = b_after.live_status.as_ref().expect("live status persisted");
    assert_eq!(live.kind, LiveStatusKind::ShellIdle);
    assert_eq!(live.summary.as_str(), "shell idle");
    assert_eq!(
        b_after.live_status_observed_at,
        Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_900_000_010))
    );
    assert!(b_after.side_flags().any(|flag| flag == SideFlag::Dirty));

    let error = store
        .save_if_revision(&in_memory, baseline_revision)
        .unwrap_err();
    assert_eq!(
        error,
        RegistrySnapshotError::RevisionConflict {
            expected: baseline_revision,
            actual: next_revision,
        }
    );

    std::fs::remove_file(path).unwrap();
}

#[test]
fn record_activity_missing_task_errors_without_bumping_revision() {
    let path = store_path("activity-missing");
    let store = SqliteRegistryStore::new(&path);
    let mut registry = InMemoryRegistry::default();
    registry
        .create_task(task("task-a", "web", "alpha"))
        .unwrap();
    store.save(&registry).unwrap();
    let baseline_revision = store.current_revision().unwrap();

    let unsaved = task("task-ghost", "web", "ghost");
    let error = store.record_activity(&unsaved, &unsaved).unwrap_err();
    assert!(matches!(error, RegistrySnapshotError::Decode(_)));

    assert_eq!(store.current_revision().unwrap(), baseline_revision);

    std::fs::remove_file(path).unwrap();
}

#[test]
fn record_activity_refuses_when_another_writer_changed_the_task() {
    let path = store_path("activity-cas");
    let store = SqliteRegistryStore::new(&path);
    let mut registry = InMemoryRegistry::default();
    registry
        .create_task(task("task-a", "web", "alpha"))
        .unwrap();
    registry.create_task(task("task-b", "web", "beta")).unwrap();
    store.save(&registry).unwrap();

    let expected = store
        .load()
        .unwrap()
        .get_task(&TaskId::new("task-b"))
        .unwrap()
        .clone();

    let mut other_writer = store.load().unwrap();
    let other_b = other_writer.get_task_mut(&TaskId::new("task-b")).unwrap();
    other_b.add_side_flag(SideFlag::TestsFailed);
    other_b.agent_attempts.push(AgentAttempt {
        agent: AgentClient::Claude,
        launch_target: "claude".to_string(),
        started_at: SystemTime::UNIX_EPOCH + Duration::from_secs(1_900_000_000),
        finished_at: None,
        status: AgentRuntimeStatus::Running,
    });
    store.save(&other_writer).unwrap();
    let revision_after_other_writer = store.current_revision().unwrap();

    let mut web_copy = expected.clone();
    web_copy.last_activity_at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_900_000_100);
    web_copy.agent_status = AgentRuntimeStatus::Done;
    web_copy.add_side_flag(SideFlag::Dirty);

    let error = store.record_activity(&web_copy, &expected).unwrap_err();
    assert!(
        matches!(&error, RegistrySnapshotError::Decode(message) if message == "registry task activity changed on disk: task-b"),
        "unexpected error: {error:?}"
    );

    assert_eq!(
        store.current_revision().unwrap(),
        revision_after_other_writer
    );
    let after = store.load().unwrap();
    let b_after = after.get_task(&TaskId::new("task-b")).unwrap();
    assert!(b_after
        .side_flags()
        .any(|flag| flag == SideFlag::TestsFailed));
    assert!(!b_after.side_flags().any(|flag| flag == SideFlag::Dirty));
    assert_eq!(b_after.agent_attempts.len(), 1);
    assert_eq!(b_after.agent_status, expected.agent_status);
    assert_eq!(b_after.last_activity_at, expected.last_activity_at);

    std::fs::remove_file(path).unwrap();
}

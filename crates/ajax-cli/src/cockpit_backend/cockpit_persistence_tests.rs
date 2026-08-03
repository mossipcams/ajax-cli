use super::{
    refresh_cockpit_snapshot_with_paths, save_cockpit_state_to_sqlite, InteractiveCockpitHandler,
};
use crate::context::{load_context, load_tracked_context, state_file_mtime, ContextSaveState};
use crate::{CliContextPaths, CliError};
use ajax_core::{
    adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec},
    commands::CommandContext,
    config::Config,
    models::{AgentClient, LifecycleStatus, Task, TaskId},
    registry::{InMemoryRegistry, Registry as _, SqliteRegistryStore},
};
use ajax_tui::CockpitEventHandler;
use std::{thread, time::Duration};

struct EmptyTmuxRunner;

impl CommandRunner for EmptyTmuxRunner {
    fn run(&mut self, _command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        Ok(CommandOutput {
            status_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

fn sample_active_task(handle: &str) -> Task {
    let mut task = Task::new(
        TaskId::new(format!("web/{handle}")),
        "web",
        handle,
        handle,
        format!("ajax/{handle}"),
        "main",
        format!("/tmp/worktrees/web-{handle}"),
        format!("ajax-web-{handle}"),
        "task",
        AgentClient::Codex,
    );
    task.lifecycle_status = LifecycleStatus::Active;
    task
}

fn temp_state_paths(label: &str) -> CliContextPaths {
    let root = std::env::temp_dir().join(format!(
        "ajax-cockpit-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    CliContextPaths::new(root.join("config.toml"), root.join("state.db"))
}

#[test]
fn refresh_cockpit_snapshot_with_paths_reloads_sqlite_when_mtime_advances() {
    let paths = temp_state_paths("reload-on-mtime");
    let mut initial = InMemoryRegistry::default();
    initial.create_task(sample_active_task("a")).unwrap();
    SqliteRegistryStore::new(&paths.state_file)
        .save(&initial)
        .unwrap();

    let mut context = load_context(&paths).unwrap();
    let mut last_loaded_mtime = state_file_mtime(&paths);
    let mut cached_snapshot = None;
    let mut state_changed = false;
    let mut runner = EmptyTmuxRunner;

    let first = refresh_cockpit_snapshot_with_paths(
        &mut context,
        &mut runner,
        &mut state_changed,
        &mut cached_snapshot,
        Some(&paths),
        &mut last_loaded_mtime,
        None,
    )
    .unwrap();
    assert_eq!(first.cards.len(), 1);
    assert!(first
        .cards
        .iter()
        .any(|card| card.qualified_handle == "web/a"));

    thread::sleep(Duration::from_millis(50));
    let mut next = InMemoryRegistry::default();
    next.create_task(sample_active_task("a")).unwrap();
    next.create_task(sample_active_task("b")).unwrap();
    SqliteRegistryStore::new(&paths.state_file)
        .save(&next)
        .unwrap();

    let mtime_before_reload = last_loaded_mtime;
    let mut runner = EmptyTmuxRunner;
    let second = refresh_cockpit_snapshot_with_paths(
        &mut context,
        &mut runner,
        &mut state_changed,
        &mut cached_snapshot,
        Some(&paths),
        &mut last_loaded_mtime,
        None,
    )
    .unwrap();

    let handles: Vec<&str> = second
        .cards
        .iter()
        .map(|card| card.qualified_handle.as_str())
        .collect();
    assert!(
        handles.contains(&"web/a") && handles.contains(&"web/b"),
        "expected both web/a and web/b after sqlite advance, got {handles:?}"
    );
    assert_eq!(second.cards.len(), 2);
    assert_ne!(
        mtime_before_reload, last_loaded_mtime,
        "last_loaded_mtime should advance after SQLite revision changes"
    );

    let _ = std::fs::remove_dir_all(paths.state_file.parent().unwrap());
}

#[test]
fn cockpit_save_uses_reloaded_sqlite_state_as_its_concurrency_baseline() {
    let paths = temp_state_paths("reload-save-baseline");
    let mut initial = InMemoryRegistry::default();
    initial.create_task(sample_active_task("a")).unwrap();
    SqliteRegistryStore::new(&paths.state_file)
        .save(&initial)
        .unwrap();

    let mut tracked = load_tracked_context(&paths).unwrap();
    let mut last_loaded_mtime = state_file_mtime(&paths);

    thread::sleep(Duration::from_millis(50));
    let mut concurrent = initial.clone();
    concurrent
        .get_task_mut(&TaskId::new("web/a"))
        .expect("concurrent task")
        .metadata
        .insert("web".to_string(), "persisted".to_string());
    SqliteRegistryStore::new(&paths.state_file)
        .save(&concurrent)
        .unwrap();

    let mut cached_snapshot = None;
    let mut state_changed = false;
    let mut runner = EmptyTmuxRunner;
    refresh_cockpit_snapshot_with_paths(
        &mut tracked.context,
        &mut runner,
        &mut state_changed,
        &mut cached_snapshot,
        Some(&paths),
        &mut last_loaded_mtime,
        Some(&mut tracked.save_state),
    )
    .expect("reload concurrent SQLite state");

    tracked
        .context
        .registry
        .get_task_mut(&TaskId::new("web/a"))
        .expect("reloaded task")
        .metadata
        .insert("native".to_string(), "persisted".to_string());

    save_cockpit_state_to_sqlite(
        &paths,
        &tracked.context,
        &mut tracked.save_state,
        &mut last_loaded_mtime,
    )
    .expect("save after Cockpit reload");

    let reloaded = load_context(&paths).expect("reload saved state");
    let task = reloaded
        .registry
        .get_task(&TaskId::new("web/a"))
        .expect("saved task");
    assert_eq!(
        task.metadata.get("web").map(String::as_str),
        Some("persisted")
    );
    assert_eq!(
        task.metadata.get("native").map(String::as_str),
        Some("persisted")
    );

    let _ = std::fs::remove_dir_all(paths.state_file.parent().unwrap());
}

#[test]
fn cockpit_save_reloads_sqlite_even_when_mtime_stays_the_same() {
    let paths = temp_state_paths("reload-save-mtime-stall");
    let mut initial = InMemoryRegistry::default();
    initial.create_task(sample_active_task("a")).unwrap();
    SqliteRegistryStore::new(&paths.state_file)
        .save(&initial)
        .unwrap();

    let mut tracked = load_tracked_context(&paths).unwrap();
    let mut last_loaded_mtime;

    let mut concurrent = initial.clone();
    concurrent
        .get_task_mut(&TaskId::new("web/a"))
        .expect("concurrent task")
        .metadata
        .insert("web".to_string(), "persisted".to_string());
    SqliteRegistryStore::new(&paths.state_file)
        .save(&concurrent)
        .unwrap();

    // Simulate a filesystem where the timestamp cache did not advance
    // even though SQLite revision did. The reload path should still notice
    // the revision change and refresh the save baseline.
    last_loaded_mtime = state_file_mtime(&paths);

    let mut cached_snapshot = None;
    let mut state_changed = false;
    let mut runner = EmptyTmuxRunner;
    refresh_cockpit_snapshot_with_paths(
        &mut tracked.context,
        &mut runner,
        &mut state_changed,
        &mut cached_snapshot,
        Some(&paths),
        &mut last_loaded_mtime,
        Some(&mut tracked.save_state),
    )
    .expect("reload concurrent SQLite state even when mtime is unchanged");

    tracked
        .context
        .registry
        .get_task_mut(&TaskId::new("web/a"))
        .expect("reloaded task")
        .metadata
        .insert("native".to_string(), "persisted".to_string());

    save_cockpit_state_to_sqlite(
        &paths,
        &tracked.context,
        &mut tracked.save_state,
        &mut last_loaded_mtime,
    )
    .expect("save after Cockpit reload with stale mtime");

    let reloaded = load_context(&paths).expect("reload saved state");
    let task = reloaded
        .registry
        .get_task(&TaskId::new("web/a"))
        .expect("saved task");
    assert_eq!(
        task.metadata.get("web").map(String::as_str),
        Some("persisted")
    );
    assert_eq!(
        task.metadata.get("native").map(String::as_str),
        Some("persisted")
    );

    let _ = std::fs::remove_dir_all(paths.state_file.parent().unwrap());
}

#[test]
fn interactive_cockpit_handler_on_refresh_reloads_sqlite_via_paths() {
    let paths = temp_state_paths("handler-on-refresh");
    let mut initial = InMemoryRegistry::default();
    initial.create_task(sample_active_task("a")).unwrap();
    SqliteRegistryStore::new(&paths.state_file)
        .save(&initial)
        .unwrap();

    let mut context = load_context(&paths).unwrap();
    let mut last_loaded_mtime = state_file_mtime(&paths);
    let mut cached_snapshot = None;
    let mut state_changed = false;
    let mut runner = EmptyTmuxRunner;

    let first = {
        let mut retained_repair_plan = None;
        let mut handler = InteractiveCockpitHandler {
            context: &mut context,
            runner: &mut runner,
            state_changed: &mut state_changed,
            cached_snapshot: &mut cached_snapshot,
            paths: Some(&paths),
            last_loaded_mtime: &mut last_loaded_mtime,
            save_state: None,
            retained_repair_plan: &mut retained_repair_plan,
        };
        handler.on_refresh().unwrap().expect("first snapshot")
    };
    assert_eq!(first.cards.len(), 1);

    thread::sleep(Duration::from_millis(50));
    let mut next = InMemoryRegistry::default();
    next.create_task(sample_active_task("a")).unwrap();
    next.create_task(sample_active_task("b")).unwrap();
    SqliteRegistryStore::new(&paths.state_file)
        .save(&next)
        .unwrap();

    let mut runner = EmptyTmuxRunner;
    let second = {
        let mut retained_repair_plan = None;
        let mut handler = InteractiveCockpitHandler {
            context: &mut context,
            runner: &mut runner,
            state_changed: &mut state_changed,
            cached_snapshot: &mut cached_snapshot,
            paths: Some(&paths),
            last_loaded_mtime: &mut last_loaded_mtime,
            save_state: None,
            retained_repair_plan: &mut retained_repair_plan,
        };
        handler.on_refresh().unwrap().expect("second snapshot")
    };

    let handles: Vec<&str> = second
        .cards
        .iter()
        .map(|card| card.qualified_handle.as_str())
        .collect();
    assert!(
        handles.contains(&"web/a") && handles.contains(&"web/b"),
        "expected handler.on_refresh to pick up SQLite advance, got {handles:?}"
    );

    let _ = std::fs::remove_dir_all(paths.state_file.parent().unwrap());
}

#[test]
fn save_cockpit_state_to_sqlite_persists_in_memory_mutations() {
    let paths = temp_state_paths("save-during-loop");
    let mut initial = InMemoryRegistry::default();
    initial.create_task(sample_active_task("a")).unwrap();
    SqliteRegistryStore::new(&paths.state_file)
        .save(&initial)
        .unwrap();

    let mut tracked = load_tracked_context(&paths).unwrap();
    let mut last_loaded_mtime = state_file_mtime(&paths);

    tracked
        .context
        .registry
        .get_task_mut(&TaskId::new("web/a"))
        .expect("seeded task")
        .title = "Renamed by native cockpit".to_string();

    save_cockpit_state_to_sqlite(
        &paths,
        &tracked.context,
        &mut tracked.save_state,
        &mut last_loaded_mtime,
    )
    .expect("save during interactive cockpit loop");

    let on_disk = SqliteRegistryStore::new(&paths.state_file)
        .load_tasks_only()
        .expect("reload SQLite after cockpit save");
    let task = on_disk
        .get_task(&TaskId::new("web/a"))
        .expect("persisted task")
        .clone();
    assert_eq!(
        task.title, "Renamed by native cockpit",
        "cockpit save should persist in-memory task mutations during the interactive loop"
    );
    assert!(last_loaded_mtime.is_some(), "mtime should be tracked");

    let _ = std::fs::remove_dir_all(paths.state_file.parent().unwrap());
}

#[test]
fn save_cockpit_state_to_sqlite_rejects_empty_save_over_non_empty_disk() {
    let paths = temp_state_paths("empty-save-guard");
    let mut initial = InMemoryRegistry::default();
    initial.create_task(sample_active_task("a")).unwrap();
    let store = SqliteRegistryStore::new(&paths.state_file);
    store.save(&initial).unwrap();
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let mut save_state = ContextSaveState {
        loaded_registry: InMemoryRegistry::default(),
        loaded_revision: store.current_revision().unwrap(),
        allow_empty_registry_once: false,
    };
    let mut last_loaded_mtime = state_file_mtime(&paths);

    let error =
        save_cockpit_state_to_sqlite(&paths, &context, &mut save_state, &mut last_loaded_mtime)
            .unwrap_err();

    assert!(error
        .to_string()
        .contains("refusing to save empty registry"));
    let on_disk = SqliteRegistryStore::new(&paths.state_file)
        .load_tasks_only()
        .expect("reload SQLite after rejected cockpit save");
    assert!(on_disk.get_task(&TaskId::new("web/a")).is_some());

    let _ = std::fs::remove_dir_all(paths.state_file.parent().unwrap());
}

#[test]
fn cockpit_save_guard_error_returns_to_cockpit() {
    let paths = temp_state_paths("save-guard-recovery");
    let mut initial = InMemoryRegistry::default();
    initial.create_task(sample_active_task("a")).unwrap();
    let store = SqliteRegistryStore::new(&paths.state_file);
    store.save(&initial).unwrap();

    let mut tracked = load_tracked_context(&paths).unwrap();
    let mut last_loaded_mtime = state_file_mtime(&paths);

    // Simulate the Ctrl-Q post-session path attempting to persist an
    // in-memory registry that the empty-over-non-empty guard must reject.
    tracked.context.registry = InMemoryRegistry::default();

    let error = save_cockpit_state_to_sqlite(
        &paths,
        &tracked.context,
        &mut tracked.save_state,
        &mut last_loaded_mtime,
    )
    .unwrap_err();
    assert!(
        matches!(error, CliError::ContextSave(_)),
        "guard failure should be a ContextSave error, got {error:?}"
    );

    let flash = super::recover_cockpit_save_error(
        &paths,
        &mut tracked.context,
        &mut tracked.save_state,
        &mut last_loaded_mtime,
        error,
    )
    .expect("recoverable save error should not re-raise")
    .expect("recovered save error should produce a flash message");

    assert!(
        flash.contains("refusing to save empty registry"),
        "flash should preserve the original save error text, got: {flash}"
    );

    let reloaded = tracked
        .context
        .registry
        .get_task(&TaskId::new("web/a"))
        .expect("recovery should reload the persisted task into the cockpit context");
    assert_eq!(reloaded.lifecycle_status, LifecycleStatus::Active);

    assert_eq!(
        tracked.save_state.loaded_revision,
        store.current_revision().unwrap(),
        "recovery should reset the tracked save state revision to disk"
    );
    assert!(
        tracked
            .save_state
            .loaded_registry
            .get_task(&TaskId::new("web/a"))
            .is_some(),
        "recovery should reset the tracked save state baseline to disk"
    );

    let on_disk = SqliteRegistryStore::new(&paths.state_file)
        .load_tasks_only()
        .expect("reload SQLite after recovery");
    assert!(on_disk.get_task(&TaskId::new("web/a")).is_some());

    let _ = std::fs::remove_dir_all(paths.state_file.parent().unwrap());
}

use super::{
    context_paths_from_matches_and_env, load_context, load_tracked_context,
    save_context_with_state, save_tracked_context, CliContextPaths, ContextSaveState,
};
use crate::build_cli;
use ajax_core::{
    commands::CommandContext,
    config::{Config, RuntimePathRequest, WorktreePlacement},
    models::{AgentClient, LifecycleStatus, Task, TaskId},
    registry::{InMemoryRegistry, Registry, RegistryEventKind, SqliteRegistryStore},
};
use std::{path::Path, thread, time::Duration};

fn sample_task(id: &str, handle: &str, title: &str) -> Task {
    Task::new(
        TaskId::new(id),
        "web",
        handle,
        title,
        format!("ajax/{handle}"),
        "main",
        format!("/tmp/worktrees/web-{handle}"),
        format!("ajax-web-{handle}"),
        "task",
        AgentClient::Codex,
    )
}

#[test]
fn ordinary_context_load_skips_registry_event_history() {
    let root = std::env::temp_dir().join(format!("ajax-context-events-{}", std::process::id()));
    let paths = CliContextPaths::new(root.join("config.toml"), root.join("state.db"));
    let mut registry = InMemoryRegistry::default();
    registry
        .create_task(Task::new(
            TaskId::new("task-1"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            "/tmp/worktrees/web-fix-login",
            "ajax-web-fix-login",
            "task",
            AgentClient::Codex,
        ))
        .unwrap();
    registry
        .record_event(TaskId::new("task-1"), RegistryEventKind::UserNote, "ready")
        .unwrap();
    SqliteRegistryStore::new(&paths.state_file)
        .save(&registry)
        .unwrap();

    let context = load_context(&paths).unwrap();

    assert_eq!(context.registry.list_tasks().len(), 1);
    assert!(context.registry.list_events().is_empty());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn load_context_preserves_resolved_runtime_paths() {
    let root =
        std::env::temp_dir().join(format!("ajax-context-runtime-paths-{}", std::process::id()));
    let runtime_paths = RuntimePathRequest::new(&root)
        .with_cli_profile("dev")
        .resolve();
    let paths = CliContextPaths::from_runtime_paths(runtime_paths.clone());

    let context = load_context(&paths).unwrap();

    assert_eq!(paths.config_file, runtime_paths.config_file);
    assert_eq!(paths.state_file, runtime_paths.state_db);
    assert_eq!(context.runtime_paths, runtime_paths);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn ajax_profile_env_selects_dev_runtime_paths() {
    let matches = build_cli()
        .try_get_matches_from(["ajax-cli", "status"])
        .unwrap();
    let paths = context_paths_from_matches_and_env(
        &matches,
        RuntimePathRequest::new("/Users/matt").with_env_profile("dev"),
    )
    .unwrap();

    assert_eq!(paths.runtime_paths.profile, "dev");
    assert_eq!(
        paths.runtime_paths.state_db,
        Path::new("/Users/matt/.ajax-dev/ajax.db")
    );
}

#[test]
fn dev_alias_selects_dev_runtime_paths() {
    let matches = build_cli()
        .try_get_matches_from(["ajax-cli", "dev"])
        .unwrap();
    let paths =
        context_paths_from_matches_and_env(&matches, RuntimePathRequest::new("/Users/matt"))
            .unwrap();

    assert_eq!(paths.runtime_paths.profile, "dev");
    assert_eq!(
        paths.runtime_paths.state_db,
        Path::new("/Users/matt/.ajax-dev/ajax.db")
    );
}

#[test]
fn ajax_home_env_derives_self_contained_runtime() {
    let matches = build_cli()
        .try_get_matches_from(["ajax-cli", "runtime"])
        .unwrap();
    let paths = context_paths_from_matches_and_env(
        &matches,
        RuntimePathRequest::new("/Users/matt").with_env_home("/tmp/ajax-home"),
    )
    .unwrap();

    assert_eq!(
        paths.runtime_paths.config_file,
        Path::new("/tmp/ajax-home/config.toml")
    );
    assert_eq!(
        paths.runtime_paths.state_db,
        Path::new("/tmp/ajax-home/ajax.db")
    );
}

#[test]
fn ajax_config_state_and_worktree_root_env_override_profile_paths() {
    let matches = build_cli()
        .try_get_matches_from(["ajax-cli", "--profile", "dev", "runtime"])
        .unwrap();
    let paths = context_paths_from_matches_and_env(
        &matches,
        RuntimePathRequest::new("/Users/matt")
            .with_env_config("/tmp/config.toml")
            .with_env_state("/tmp/state.db")
            .with_env_worktree_root("/tmp/worktrees"),
    )
    .unwrap();

    assert_eq!(paths.runtime_paths.profile, "dev");
    assert_eq!(
        paths.runtime_paths.config_file,
        Path::new("/tmp/config.toml")
    );
    assert_eq!(paths.runtime_paths.state_db, Path::new("/tmp/state.db"));
    assert_eq!(
        paths.runtime_paths.worktree_placement,
        WorktreePlacement::Root(Path::new("/tmp/worktrees").to_path_buf())
    );
}

#[test]
fn save_context_merges_web_companion_task_additions() {
    let root = std::env::temp_dir().join(format!(
        "ajax-context-merge-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let paths = CliContextPaths::new(root.join("config.toml"), root.join("state.db"));
    let mut baseline = InMemoryRegistry::default();
    baseline
        .create_task(sample_task("web/fix-login", "fix-login", "Fix login"))
        .unwrap();
    SqliteRegistryStore::new(&paths.state_file)
        .save(&baseline)
        .unwrap();

    let mut tracked = load_tracked_context(&paths).unwrap();
    tracked
        .context
        .registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .expect("native task")
        .title = "Updated by native".to_string();

    let mut web_registry = baseline.clone();
    web_registry
        .create_task(sample_task("web/fix-sidebar", "fix-sidebar", "Fix sidebar"))
        .unwrap();
    SqliteRegistryStore::new(&paths.state_file)
        .save(&web_registry)
        .unwrap();
    thread::sleep(Duration::from_millis(20));

    save_tracked_context(&paths, &mut tracked).expect("merge save");
    let reloaded = load_context(&paths).expect("reload");

    assert_eq!(reloaded.registry.list_tasks().len(), 2);
    assert_eq!(
        reloaded
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .expect("native task")
            .title,
        "Updated by native"
    );
    assert!(reloaded
        .registry
        .get_task(&TaskId::new("web/fix-sidebar"))
        .is_some());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn save_context_accepts_concurrent_task_deletion_without_conflict() {
    let root = std::env::temp_dir().join(format!(
        "ajax-context-deletion-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let paths = CliContextPaths::new(root.join("config.toml"), root.join("state.db"));
    let mut baseline = InMemoryRegistry::default();
    baseline
        .create_task(sample_task("web/fix-login", "fix-login", "Fix login"))
        .unwrap();
    baseline
        .create_task(sample_task("web/fix-sidebar", "fix-sidebar", "Fix sidebar"))
        .unwrap();
    SqliteRegistryStore::new(&paths.state_file)
        .save(&baseline)
        .unwrap();

    let mut tracked = load_tracked_context(&paths).unwrap();
    tracked
        .context
        .registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .expect("refreshed task")
        .title = "Refreshed by web".to_string();

    // Another writer drops fix-sidebar from disk before this writer saves.
    let mut concurrent = baseline.clone();
    concurrent
        .delete_task(&TaskId::new("web/fix-sidebar"))
        .unwrap();
    SqliteRegistryStore::new(&paths.state_file)
        .save(&concurrent)
        .unwrap();
    thread::sleep(Duration::from_millis(20));

    save_tracked_context(&paths, &mut tracked).expect("deletion merges cleanly");
    let reloaded = load_context(&paths).expect("reload");

    assert!(reloaded
        .registry
        .get_task(&TaskId::new("web/fix-sidebar"))
        .is_none());
    assert_eq!(
        reloaded
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .expect("surviving task")
            .title,
        "Refreshed by web"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn save_context_keeps_disk_task_when_memory_task_is_unchanged_from_baseline() {
    let root = std::env::temp_dir().join(format!(
        "ajax-context-unchanged-memory-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let paths = CliContextPaths::new(root.join("config.toml"), root.join("state.db"));
    let mut baseline = InMemoryRegistry::default();
    baseline
        .create_task(sample_task("web/fix-login", "fix-login", "Fix login"))
        .unwrap();
    SqliteRegistryStore::new(&paths.state_file)
        .save(&baseline)
        .unwrap();

    let mut tracked = load_tracked_context(&paths).unwrap();

    let mut web_registry = baseline.clone();
    let web_task = web_registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .expect("web task");
    web_task.live_status = Some(ajax_core::models::LiveObservation::new(
        ajax_core::models::LiveStatusKind::AgentRunning,
        "agent running",
    ));
    web_task.live_status_observed_at = Some(std::time::UNIX_EPOCH + Duration::from_secs(10));
    SqliteRegistryStore::new(&paths.state_file)
        .save(&web_registry)
        .unwrap();
    thread::sleep(Duration::from_millis(20));

    save_tracked_context(&paths, &mut tracked).expect("unchanged task keeps disk facts");
    let reloaded = load_context(&paths).expect("reload");
    assert_eq!(
        reloaded
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .expect("reloaded task")
            .live_status
            .as_ref()
            .map(|status| status.kind),
        Some(ajax_core::models::LiveStatusKind::AgentRunning)
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn save_context_merges_independent_same_task_fact_updates() {
    let root = std::env::temp_dir().join(format!(
        "ajax-context-same-task-merge-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let paths = CliContextPaths::new(root.join("config.toml"), root.join("state.db"));
    let mut baseline = InMemoryRegistry::default();
    baseline
        .create_task(sample_task("web/fix-login", "fix-login", "Fix login"))
        .unwrap();
    SqliteRegistryStore::new(&paths.state_file)
        .save(&baseline)
        .unwrap();

    let mut tracked = load_tracked_context(&paths).unwrap();
    tracked
        .context
        .registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .expect("native task")
        .title = "Updated by native".to_string();

    let observed_at = std::time::UNIX_EPOCH + Duration::from_secs(10);
    let mut web_registry = baseline.clone();
    let web_task = web_registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .expect("web task");
    web_task.live_status = Some(ajax_core::models::LiveObservation::new(
        ajax_core::models::LiveStatusKind::AgentRunning,
        "agent running",
    ));
    web_task.live_status_observed_at = Some(observed_at);
    SqliteRegistryStore::new(&paths.state_file)
        .save(&web_registry)
        .unwrap();
    thread::sleep(Duration::from_millis(20));

    save_tracked_context(&paths, &mut tracked).expect("independent updates merge");
    let reloaded = load_context(&paths).expect("reload");
    let reloaded_task = reloaded
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .expect("reloaded task");
    assert_eq!(reloaded_task.title, "Updated by native");
    assert_eq!(
        reloaded_task.live_status.as_ref().map(|status| status.kind),
        Some(ajax_core::models::LiveStatusKind::AgentRunning)
    );
    assert_eq!(reloaded_task.live_status_observed_at, Some(observed_at));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn save_context_rejects_empty_registry_that_never_loaded_disk_tasks() {
    let root = std::env::temp_dir().join(format!(
        "ajax-context-empty-overwrite-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let paths = CliContextPaths::new(root.join("config.toml"), root.join("state.db"));
    let store = SqliteRegistryStore::new(&paths.state_file);
    let mut disk_registry = InMemoryRegistry::default();
    disk_registry
        .create_task(sample_task("web/fix-login", "fix-login", "Fix login"))
        .unwrap();
    store.save(&disk_registry).unwrap();

    let empty_context = CommandContext::with_runtime_paths(
        Config::default(),
        InMemoryRegistry::default(),
        paths.runtime_paths.clone(),
    );
    let mut save_state = ContextSaveState {
        loaded_registry: InMemoryRegistry::default(),
        loaded_revision: store.current_revision().unwrap(),
        allow_empty_registry_once: false,
    };

    let error = save_context_with_state(&paths, &empty_context, &mut save_state).unwrap_err();

    assert!(error
        .to_string()
        .contains("refusing to save empty registry"));
    let reloaded = load_context(&paths).expect("reload");
    assert!(reloaded
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .is_some());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn save_context_rejects_accidental_empty_after_non_empty_load() {
    let root = std::env::temp_dir().join(format!(
        "ajax-context-empty-after-load-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let paths = CliContextPaths::new(root.join("config.toml"), root.join("state.db"));
    let mut registry = InMemoryRegistry::default();
    registry
        .create_task(sample_task("web/fix-login", "fix-login", "Fix login"))
        .unwrap();
    SqliteRegistryStore::new(&paths.state_file)
        .save(&registry)
        .unwrap();
    let mut tracked = load_tracked_context(&paths).unwrap();

    tracked.context.registry = InMemoryRegistry::default();
    let error = save_tracked_context(&paths, &mut tracked).unwrap_err();

    assert!(error
        .to_string()
        .contains("refusing to save empty registry"));
    let reloaded = load_context(&paths).expect("reload");
    assert!(reloaded
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .is_some());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn save_context_allows_empty_registry_when_disk_was_empty_at_load() {
    let root = std::env::temp_dir().join(format!(
        "ajax-context-empty-init-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let paths = CliContextPaths::new(root.join("config.toml"), root.join("state.db"));
    let empty_context = CommandContext::with_runtime_paths(
        Config::default(),
        InMemoryRegistry::default(),
        paths.runtime_paths.clone(),
    );
    let mut save_state = ContextSaveState::default();

    save_context_with_state(&paths, &empty_context, &mut save_state)
        .expect("empty registry initializes state");

    let reloaded = load_context(&paths).expect("reload");
    assert!(reloaded.registry.list_tasks().is_empty());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn save_context_allows_intentional_all_task_deletion_from_loaded_baseline() {
    let root = std::env::temp_dir().join(format!(
        "ajax-context-intentional-delete-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let paths = CliContextPaths::new(root.join("config.toml"), root.join("state.db"));
    let mut registry = InMemoryRegistry::default();
    registry
        .create_task(sample_task("web/fix-login", "fix-login", "Fix login"))
        .unwrap();
    SqliteRegistryStore::new(&paths.state_file)
        .save(&registry)
        .unwrap();
    let mut tracked = load_tracked_context(&paths).unwrap();

    tracked
        .context
        .registry
        .delete_task(&TaskId::new("web/fix-login"))
        .unwrap();
    tracked.save_state.allow_empty_registry_once();
    save_tracked_context(&paths, &mut tracked).expect("intentional deletion persists");

    let reloaded = load_context(&paths).expect("reload");
    assert!(reloaded.registry.list_tasks().is_empty());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn save_context_surfaces_conflict_when_same_task_diverges() {
    let root = std::env::temp_dir().join(format!(
        "ajax-context-conflict-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let paths = CliContextPaths::new(root.join("config.toml"), root.join("state.db"));
    let mut baseline = InMemoryRegistry::default();
    let mut native_task = sample_task("web/fix-login", "fix-login", "Fix login");
    native_task.lifecycle_status = LifecycleStatus::Reviewable;
    baseline.create_task(native_task).unwrap();
    SqliteRegistryStore::new(&paths.state_file)
        .save(&baseline)
        .unwrap();

    let mut tracked = load_tracked_context(&paths).unwrap();
    let mut web_registry = baseline.clone();
    let web_task = web_registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .expect("web task");
    web_task.lifecycle_status = LifecycleStatus::Merged;
    SqliteRegistryStore::new(&paths.state_file)
        .save(&web_registry)
        .unwrap();
    thread::sleep(Duration::from_millis(20));

    let error = save_tracked_context(&paths, &mut tracked).unwrap_err();
    assert!(error.to_string().contains("state conflict"));
    assert!(error.to_string().contains("web/fix-login"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn save_context_merges_concurrent_ack_and_live_status_change() {
    let root = std::env::temp_dir().join(format!(
        "ajax-context-ack-merge-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let paths = CliContextPaths::new(root.join("config.toml"), root.join("state.db"));
    let mut baseline = InMemoryRegistry::default();
    let mut native_task = sample_task("web/fix-login", "fix-login", "Fix login");
    native_task.lifecycle_status = LifecycleStatus::Active;
    baseline.create_task(native_task).unwrap();
    SqliteRegistryStore::new(&paths.state_file)
        .save(&baseline)
        .unwrap();

    // Native writer changes the same task's live status.
    let mut tracked = load_tracked_context(&paths).unwrap();
    let observed_at = std::time::UNIX_EPOCH + Duration::from_secs(1_700_000_800);
    {
        let native_task = tracked
            .context
            .registry
            .get_task_mut(&TaskId::new("web/fix-login"))
            .expect("native task");
        native_task.live_status = Some(ajax_core::models::LiveObservation::new(
            ajax_core::models::LiveStatusKind::AgentRunning,
            "agent running",
        ));
        native_task.live_status_observed_at = Some(observed_at);
    }

    // Concurrent writer records an acknowledgment and persists first.
    let acknowledged_at = std::time::UNIX_EPOCH + Duration::from_secs(1_700_000_900);
    let mut web_registry = baseline.clone();
    web_registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .expect("web task")
        .record_attention_acknowledgment(acknowledged_at);
    SqliteRegistryStore::new(&paths.state_file)
        .save(&web_registry)
        .unwrap();
    thread::sleep(Duration::from_millis(20));

    save_tracked_context(&paths, &mut tracked).expect("concurrent ack and live status merge");

    let reloaded = load_context(&paths).expect("reload");
    let reloaded_task = reloaded
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .expect("reloaded task");
    assert_eq!(
        reloaded_task.attention_acknowledged_at,
        Some(acknowledged_at)
    );
    assert_eq!(
        reloaded_task.live_status.as_ref().map(|status| status.kind),
        Some(ajax_core::models::LiveStatusKind::AgentRunning)
    );

    let _ = std::fs::remove_dir_all(root);
}

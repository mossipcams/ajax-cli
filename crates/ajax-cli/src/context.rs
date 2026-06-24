use ajax_core::{
    commands::CommandContext,
    config::{Config, RuntimePathRequest, RuntimePaths},
    ghost_task::is_registry_ghost_task,
    models::LifecycleStatus,
    registry::{
        InMemoryRegistry, Registry, RegistrySnapshotError, RegistryStore, SqliteRegistryStore,
    },
};
use clap::ArgMatches;
use std::{path::PathBuf, time::SystemTime};

use crate::CliError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CliContextPaths {
    pub config_file: PathBuf,
    pub state_file: PathBuf,
    pub runtime_paths: RuntimePaths,
}

impl CliContextPaths {
    pub fn new(config_file: impl Into<PathBuf>, state_file: impl Into<PathBuf>) -> Self {
        let config_file = config_file.into();
        let state_file = state_file.into();
        let runtime_paths = RuntimePathRequest::new("")
            .with_cli_config(config_file.clone())
            .with_cli_state(state_file.clone())
            .resolve();
        Self {
            config_file,
            state_file,
            runtime_paths,
        }
    }

    pub fn from_runtime_paths(runtime_paths: RuntimePaths) -> Self {
        Self {
            config_file: runtime_paths.config_file.clone(),
            state_file: runtime_paths.state_db.clone(),
            runtime_paths,
        }
    }
}

pub(crate) fn context_paths_from_matches(
    matches: &ArgMatches,
) -> Result<CliContextPaths, CliError> {
    context_paths_from_matches_and_env(matches, runtime_path_request_from_env()?)
}

pub(crate) fn default_context_paths() -> Result<CliContextPaths, CliError> {
    let matches = crate::build_cli()
        .try_get_matches_from(["ajax-cli"])
        .map_err(|error| CliError::CommandFailed(error.to_string()))?;
    context_paths_from_matches(&matches)
}

/// A CLI flag name paired with the `RuntimePathRequest` setter it feeds.
type CliFlagOverride = (
    &'static str,
    fn(RuntimePathRequest, &str) -> RuntimePathRequest,
);

pub(crate) fn context_paths_from_matches_and_env(
    matches: &ArgMatches,
    mut request: RuntimePathRequest,
) -> Result<CliContextPaths, CliError> {
    // The `dev`/`stable` aliases are sugar for `--profile`; an explicit
    // `--profile` flag still wins because it is applied last below.
    if let Some((name @ ("dev" | "stable"), _)) = matches.subcommand() {
        request = request.with_cli_profile(name);
    }

    let cli_overrides: [CliFlagOverride; 5] = [
        ("profile", |request, value| request.with_cli_profile(value)),
        ("home", |request, value| request.with_cli_home(value)),
        ("config", |request, value| request.with_cli_config(value)),
        ("state", |request, value| request.with_cli_state(value)),
        ("worktree-root", |request, value| {
            request.with_cli_worktree_root(value)
        }),
    ];
    for (flag, apply) in cli_overrides {
        if let Some(value) = matches.get_one::<String>(flag) {
            request = apply(request, value);
        }
    }

    Ok(CliContextPaths::from_runtime_paths(request.resolve()))
}

/// Seed a [`RuntimePathRequest`] from the process environment: `$HOME` plus the
/// optional `AJAX_*` overrides. CLI flags are layered on top later, so these are
/// recorded as env-sourced.
fn runtime_path_request_from_env() -> Result<RuntimePathRequest, CliError> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| CliError::ContextLoad("HOME is not set".to_string()))?;
    let mut request = RuntimePathRequest::new(home);
    if let Some(profile) = std::env::var_os("AJAX_PROFILE") {
        request = request.with_env_profile(profile.to_string_lossy().into_owned());
    }
    if let Some(home) = std::env::var_os("AJAX_HOME") {
        request = request.with_env_home(home);
    }
    if let Some(config) = std::env::var_os("AJAX_CONFIG") {
        request = request.with_env_config(config);
    }
    if let Some(state) = std::env::var_os("AJAX_STATE") {
        request = request.with_env_state(state);
    }
    if let Some(root) = std::env::var_os("AJAX_WORKTREE_ROOT") {
        request = request.with_env_worktree_root(root);
    }

    Ok(request)
}

pub(crate) fn load_context(
    paths: &CliContextPaths,
) -> Result<CommandContext<InMemoryRegistry>, CliError> {
    load_context_with_loader(paths, SqliteRegistryStore::load_tasks_only)
}

pub(crate) fn load_context_with_events(
    paths: &CliContextPaths,
) -> Result<CommandContext<InMemoryRegistry>, CliError> {
    load_context_with_loader(paths, SqliteRegistryStore::load)
}

fn load_context_with_loader(
    paths: &CliContextPaths,
    load_registry: fn(&SqliteRegistryStore) -> Result<InMemoryRegistry, RegistrySnapshotError>,
) -> Result<CommandContext<InMemoryRegistry>, CliError> {
    let config = if paths.config_file.exists() {
        let contents = std::fs::read_to_string(&paths.config_file)
            .map_err(|error| CliError::ContextLoad(error.to_string()))?;
        Config::from_toml_str(&contents)
            .map_err(|error| CliError::ContextLoad(format!("config parse failed: {error}")))?
    } else {
        Config::default()
    };
    let store = SqliteRegistryStore::new(&paths.state_file);
    let registry = if paths.state_file.exists() {
        reject_legacy_json_state(&paths.state_file)?;
        load_registry(&store)
            .map_err(|error| CliError::ContextLoad(format!("state load failed: {error}")))?
    } else {
        InMemoryRegistry::default()
    };

    Ok(CommandContext::with_runtime_paths(
        config,
        registry,
        paths.runtime_paths.clone(),
    ))
}

fn reject_legacy_json_state(path: &std::path::Path) -> Result<(), CliError> {
    let bytes = std::fs::read(path).map_err(|error| CliError::ContextLoad(error.to_string()))?;
    let Some(first) = bytes
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
    else {
        return Ok(());
    };

    if matches!(first, b'{' | b'[') {
        return Err(CliError::ContextLoad(format!(
            "legacy JSON state is unsupported after the SQLite rewrite; remove {} to start with fresh state",
            path.display()
        )));
    }

    Ok(())
}

#[derive(Clone)]
pub(crate) struct TrackedContext {
    pub context: CommandContext<InMemoryRegistry>,
    pub save_state: ContextSaveState,
}

pub(crate) fn load_tracked_context(paths: &CliContextPaths) -> Result<TrackedContext, CliError> {
    let context = load_context(paths)?;
    let save_state = tracked_save_state(paths, &context.registry)?;
    Ok(TrackedContext {
        save_state,
        context,
    })
}

pub(crate) fn tracked_save_state(
    paths: &CliContextPaths,
    registry: &InMemoryRegistry,
) -> Result<ContextSaveState, CliError> {
    let mut save_state = context_save_state_from_registry(registry);
    save_state.loaded_revision = if paths.state_file.exists() {
        SqliteRegistryStore::new(&paths.state_file)
            .current_revision()
            .map_err(|error| CliError::ContextLoad(format!("state revision failed: {error}")))?
    } else {
        0
    };
    Ok(save_state)
}

pub(crate) fn save_tracked_context(
    paths: &CliContextPaths,
    tracked: &mut TrackedContext,
) -> Result<(), CliError> {
    save_context_with_state(paths, &tracked.context, &mut tracked.save_state)
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ContextSaveState {
    pub loaded_registry: InMemoryRegistry,
    pub loaded_revision: u64,
}

pub(crate) fn state_file_mtime(paths: &CliContextPaths) -> Option<SystemTime> {
    if !paths.state_file.exists() {
        return None;
    }
    std::fs::metadata(&paths.state_file)
        .ok()
        .and_then(|meta| meta.modified().ok())
}

pub(crate) fn save_context_with_state(
    paths: &CliContextPaths,
    context: &CommandContext<InMemoryRegistry>,
    save_state: &mut ContextSaveState,
) -> Result<(), CliError> {
    if let Some(parent) = paths.state_file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| CliError::ContextSave(error.to_string()))?;
    }

    let store = SqliteRegistryStore::new(&paths.state_file);
    let disk_revision = if paths.state_file.exists() {
        store
            .current_revision()
            .map_err(|error| CliError::ContextLoad(format!("state revision failed: {error}")))?
    } else {
        0
    };
    let registry = if disk_revision != save_state.loaded_revision {
        let disk_context = load_context(paths)?;
        merge_registries(
            disk_context.registry,
            &context.registry,
            &save_state.loaded_registry,
        )?
    } else {
        context.registry.clone()
    };
    prevent_accidental_empty_overwrite(paths, &registry, save_state, disk_revision)?;

    let next_revision = store
        .save_if_revision(&registry, disk_revision)
        .map_err(|error| CliError::ContextSave(format!("state save failed: {error}")))?;
    save_state.loaded_registry = registry;
    save_state.loaded_revision = next_revision;
    Ok(())
}

pub(crate) fn context_save_state_from_registry(registry: &InMemoryRegistry) -> ContextSaveState {
    ContextSaveState {
        loaded_registry: registry.clone(),
        loaded_revision: 0,
    }
}

fn merge_registries(
    disk: InMemoryRegistry,
    in_memory: &InMemoryRegistry,
    baseline: &InMemoryRegistry,
) -> Result<InMemoryRegistry, CliError> {
    let mut merged = disk.clone();
    for memory_task in in_memory.list_tasks() {
        let disk_task = disk.get_task(&memory_task.id);
        let baseline_task = baseline.get_task(&memory_task.id);
        if disk_task.is_some_and(|disk_task| {
            disk_task.lifecycle_status != memory_task.lifecycle_status
                && disk_task.lifecycle_status != LifecycleStatus::Removed
                && memory_task.lifecycle_status != LifecycleStatus::Removed
        }) {
            return Err(CliError::ContextSave(format!(
                "state conflict for {}: disk and in-memory lifecycle diverged",
                memory_task.qualified_handle()
            )));
        }
        match (disk_task, baseline_task) {
            (Some(disk_task), Some(baseline_task)) if disk_task == baseline_task => {
                *merged.get_task_mut(&memory_task.id).expect("disk task") = memory_task.clone();
            }
            (Some(_), Some(baseline_task)) if memory_task == baseline_task => {}
            (Some(disk_task), _) if disk_task == memory_task => {}
            // The task was on disk when this writer loaded but another writer
            // has deleted it since: the deletion wins over any in-memory edits,
            // otherwise every later save fails with a permanent conflict.
            (None, Some(_)) => {}
            (None, None) => {
                merged.create_task(memory_task.clone()).map_err(|error| {
                    CliError::ContextSave(format!("state merge failed: {error}"))
                })?;
            }
            _ => {
                return Err(CliError::ContextSave(format!(
                    "state conflict for {}: disk and in-memory task facts diverged",
                    memory_task.qualified_handle()
                )));
            }
        }
    }

    for event in in_memory.list_events() {
        if merged.get_task(&event.task_id).is_none() {
            continue;
        }
        if merged
            .events_for_task(&event.task_id)
            .iter()
            .any(|existing| existing.message == event.message && existing.kind == event.kind)
        {
            continue;
        }
        merged
            .record_event(event.task_id.clone(), event.kind, &event.message)
            .map_err(|error| CliError::ContextSave(format!("state merge failed: {error}")))?;
    }
    for task in in_memory.list_tasks() {
        if merged.get_task(&task.id).is_none() {
            continue;
        }
        for receipt in in_memory.step_receipts_for_task(&task.id) {
            merged
                .record_step_receipt(receipt.clone())
                .map_err(|error| CliError::ContextSave(format!("state merge failed: {error}")))?;
        }
    }

    Ok(merged)
}

fn prevent_accidental_empty_overwrite(
    paths: &CliContextPaths,
    proposed: &InMemoryRegistry,
    save_state: &ContextSaveState,
    disk_revision: u64,
) -> Result<(), CliError> {
    if has_persistable_tasks(proposed) || has_persistable_tasks(&save_state.loaded_registry) {
        return Ok(());
    }
    if disk_revision == 0 && !paths.state_file.exists() {
        return Ok(());
    }

    let disk_context = load_context(paths)?;
    if has_persistable_tasks(&disk_context.registry) {
        return Err(CliError::ContextSave(
            "refusing to save empty registry over non-empty disk state; reload state before saving"
                .to_string(),
        ));
    }
    Ok(())
}

fn has_persistable_tasks(registry: &InMemoryRegistry) -> bool {
    registry
        .list_tasks()
        .into_iter()
        .any(|task| !is_registry_ghost_task(task))
}

#[cfg(test)]
mod tests {
    use super::{
        context_paths_from_matches_and_env, load_context, load_tracked_context,
        save_context_with_state, save_tracked_context, CliContextPaths, ContextSaveState,
    };
    use crate::build_cli;
    use ajax_core::{
        commands::CommandContext,
        config::{Config, RuntimePathRequest, WorktreePlacement},
        models::{AgentClient, LifecycleStatus, Task, TaskId},
        registry::{
            InMemoryRegistry, Registry, RegistryEventKind, RegistryStore, SqliteRegistryStore,
        },
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
            "worktrunk",
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
                "worktrunk",
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
        let runtime_paths = RuntimePathRequest::new("/Users/matt")
            .with_cli_profile("dev")
            .resolve();
        let paths = CliContextPaths::from_runtime_paths(runtime_paths.clone());

        let context = load_context(&paths).unwrap();

        assert_eq!(paths.config_file, runtime_paths.config_file);
        assert_eq!(paths.state_file, runtime_paths.state_db);
        assert_eq!(context.runtime_paths, runtime_paths);
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
    fn save_context_reports_conflict_for_concurrent_ack_and_live_status_change() {
        let root = std::env::temp_dir().join(format!(
            "ajax-context-ack-conflict-{}-{}",
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
        tracked
            .context
            .registry
            .get_task_mut(&TaskId::new("web/fix-login"))
            .expect("native task")
            .live_status = Some(ajax_core::models::LiveObservation::new(
            ajax_core::models::LiveStatusKind::AgentRunning,
            "agent running",
        ));

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

        let error = save_tracked_context(&paths, &mut tracked).unwrap_err();
        assert!(error.to_string().contains("state conflict"));
        assert!(error.to_string().contains("web/fix-login"));

        // The first writer's acknowledgment is preserved; no last-writer-wins.
        let reloaded = load_context(&paths).expect("reload");
        let reloaded_task = reloaded
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .expect("reloaded task");
        assert_eq!(
            reloaded_task.attention_acknowledged_at,
            Some(acknowledged_at)
        );
        assert_ne!(
            reloaded_task.live_status.as_ref().map(|status| status.kind),
            Some(ajax_core::models::LiveStatusKind::AgentRunning)
        );

        let _ = std::fs::remove_dir_all(root);
    }
}

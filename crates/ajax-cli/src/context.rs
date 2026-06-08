use ajax_core::{
    commands::CommandContext,
    config::{Config, RuntimePathRequest, RuntimePaths},
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
    context_paths_from_matches_and_env(matches, RuntimeEnv::from_process()?)
}

pub(crate) fn default_context_paths() -> Result<CliContextPaths, CliError> {
    let matches = crate::build_cli()
        .try_get_matches_from(["ajax-cli"])
        .map_err(|error| CliError::CommandFailed(error.to_string()))?;
    context_paths_from_matches(&matches)
}

pub(crate) fn context_paths_from_matches_and_env(
    matches: &ArgMatches,
    env: RuntimeEnv,
) -> Result<CliContextPaths, CliError> {
    let mut request = env.into_runtime_path_request();

    if matches.subcommand().is_some_and(|(name, _)| name == "dev") {
        request = request.with_cli_profile("dev");
    }
    if matches
        .subcommand()
        .is_some_and(|(name, _)| name == "stable")
    {
        request = request.with_cli_profile("stable");
    }
    if let Some(profile) = matches.get_one::<String>("profile") {
        request = request.with_cli_profile(profile);
    }
    if let Some(home) = matches.get_one::<String>("home") {
        request = request.with_cli_home(home);
    }
    if let Some(config) = matches.get_one::<String>("config") {
        request = request.with_cli_config(config);
    }
    if let Some(state) = matches.get_one::<String>("state") {
        request = request.with_cli_state(state);
    }
    if let Some(root) = matches.get_one::<String>("worktree-root") {
        request = request.with_cli_worktree_root(root);
    }

    Ok(CliContextPaths::from_runtime_paths(request.resolve()))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RuntimeEnv {
    home: PathBuf,
    ajax_profile: Option<String>,
    ajax_home: Option<PathBuf>,
    ajax_config: Option<PathBuf>,
    ajax_state: Option<PathBuf>,
    ajax_worktree_root: Option<PathBuf>,
}

impl RuntimeEnv {
    fn from_process() -> Result<Self, CliError> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| CliError::ContextLoad("HOME is not set".to_string()))?;
        let mut env = Self::for_home(home);
        if let Some(profile) = std::env::var_os("AJAX_PROFILE") {
            env = env.with_ajax_profile(profile.to_string_lossy());
        }
        if let Some(home) = std::env::var_os("AJAX_HOME") {
            env = env.with_ajax_home(home);
        }
        if let Some(config) = std::env::var_os("AJAX_CONFIG") {
            env = env.with_ajax_config(config);
        }
        if let Some(state) = std::env::var_os("AJAX_STATE") {
            env = env.with_ajax_state(state);
        }
        if let Some(root) = std::env::var_os("AJAX_WORKTREE_ROOT") {
            env = env.with_ajax_worktree_root(root);
        }

        Ok(env)
    }

    fn for_home(home: impl Into<PathBuf>) -> Self {
        Self {
            home: home.into(),
            ajax_profile: None,
            ajax_home: None,
            ajax_config: None,
            ajax_state: None,
            ajax_worktree_root: None,
        }
    }

    fn with_ajax_profile(mut self, profile: impl Into<String>) -> Self {
        self.ajax_profile = Some(profile.into());
        self
    }

    fn with_ajax_home(mut self, home: impl Into<PathBuf>) -> Self {
        self.ajax_home = Some(home.into());
        self
    }

    fn with_ajax_config(mut self, config: impl Into<PathBuf>) -> Self {
        self.ajax_config = Some(config.into());
        self
    }

    fn with_ajax_state(mut self, state: impl Into<PathBuf>) -> Self {
        self.ajax_state = Some(state.into());
        self
    }

    fn with_ajax_worktree_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.ajax_worktree_root = Some(root.into());
        self
    }

    fn into_runtime_path_request(self) -> RuntimePathRequest {
        let mut request = RuntimePathRequest::new(self.home);
        if let Some(profile) = self.ajax_profile {
            request = request.with_env_profile(profile);
        }
        if let Some(home) = self.ajax_home {
            request = request.with_env_home(home);
        }
        if let Some(config) = self.ajax_config {
            request = request.with_env_config(config);
        }
        if let Some(state) = self.ajax_state {
            request = request.with_env_state(state);
        }
        if let Some(root) = self.ajax_worktree_root {
            request = request.with_env_worktree_root(root);
        }

        request
    }
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
    let mut save_state = context_save_state_from_registry(&context.registry);
    save_state.loaded_revision = if paths.state_file.exists() {
        SqliteRegistryStore::new(&paths.state_file)
            .current_revision()
            .map_err(|error| CliError::ContextLoad(format!("state revision failed: {error}")))?
    } else {
        0
    };
    Ok(TrackedContext {
        save_state,
        context,
    })
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
        for receipt in in_memory.step_receipts_for_task(&task.id) {
            merged
                .record_step_receipt(receipt.clone())
                .map_err(|error| CliError::ContextSave(format!("state merge failed: {error}")))?;
        }
    }

    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::{
        context_paths_from_matches_and_env, load_context, load_tracked_context,
        save_tracked_context, CliContextPaths, RuntimeEnv,
    };
    use crate::build_cli;
    use ajax_core::{
        config::{RuntimePathRequest, WorktreePlacement},
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
    fn context_load_uses_store_loader_without_event_mode() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/context.rs"),
        )
        .unwrap();
        let event_load_mode = ["Event", "LoadMode"].concat();

        assert!(!source.contains(&event_load_mode));
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
            RuntimeEnv::for_home("/Users/matt").with_ajax_profile("dev"),
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
            context_paths_from_matches_and_env(&matches, RuntimeEnv::for_home("/Users/matt"))
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
            RuntimeEnv::for_home("/Users/matt").with_ajax_home("/tmp/ajax-home"),
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
            RuntimeEnv::for_home("/Users/matt")
                .with_ajax_config("/tmp/config.toml")
                .with_ajax_state("/tmp/state.db")
                .with_ajax_worktree_root("/tmp/worktrees"),
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
}

use ajax_core::{
    commands::CommandContext,
    config::{Config, RuntimePathRequest, RuntimePaths},
    ghost_task::is_registry_ghost_task,
    models::{LifecycleStatus, Task},
    registry::{InMemoryRegistry, Registry, RegistrySnapshotError, SqliteRegistryStore},
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
    pub(crate) allow_empty_registry_once: bool,
}

impl ContextSaveState {
    pub(crate) fn allow_empty_registry_once(&mut self) {
        self.allow_empty_registry_once = true;
    }
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
    let allow_empty_registry = std::mem::take(&mut save_state.allow_empty_registry_once);
    prevent_accidental_empty_overwrite(
        paths,
        &registry,
        save_state,
        disk_revision,
        allow_empty_registry,
    )?;

    let save_result = if allow_empty_registry {
        store.save_if_revision_allowing_empty_rewrite(&registry, disk_revision)
    } else {
        store.save_if_revision(&registry, disk_revision)
    };
    let next_revision = save_result
        .map_err(|error| CliError::ContextSave(format!("state save failed: {error}")))?;
    save_state.loaded_registry = registry;
    save_state.loaded_revision = next_revision;
    Ok(())
}

pub(crate) fn context_save_state_from_registry(registry: &InMemoryRegistry) -> ContextSaveState {
    ContextSaveState {
        loaded_registry: registry.clone(),
        loaded_revision: 0,
        allow_empty_registry_once: false,
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
            (Some(disk_task), Some(baseline_task)) => {
                let Some(merged_task) =
                    merge_same_task_facts(disk_task, memory_task, baseline_task)?
                else {
                    return Err(CliError::ContextSave(format!(
                        "state conflict for {}: disk and in-memory task facts diverged",
                        memory_task.qualified_handle()
                    )));
                };
                *merged.get_task_mut(&memory_task.id).expect("disk task") = merged_task;
            }
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

fn merge_same_task_facts(
    disk: &Task,
    memory: &Task,
    baseline: &Task,
) -> Result<Option<Task>, CliError> {
    let disk_value = serde_json::to_value(disk)
        .map_err(|error| CliError::ContextSave(format!("state merge failed: {error}")))?;
    let memory_value = serde_json::to_value(memory)
        .map_err(|error| CliError::ContextSave(format!("state merge failed: {error}")))?;
    let baseline_value = serde_json::to_value(baseline)
        .map_err(|error| CliError::ContextSave(format!("state merge failed: {error}")))?;

    let (Some(disk_fields), Some(memory_fields), Some(baseline_fields)) = (
        disk_value.as_object(),
        memory_value.as_object(),
        baseline_value.as_object(),
    ) else {
        return Ok(None);
    };

    let mut merged_fields = disk_fields.clone();
    for (field, memory_value) in memory_fields {
        let baseline_value = baseline_fields.get(field);
        if Some(memory_value) == baseline_value {
            continue;
        }

        let disk_value = disk_fields.get(field);
        if disk_value != baseline_value && disk_value != Some(memory_value) {
            return Ok(None);
        }
        merged_fields.insert(field.clone(), memory_value.clone());
    }

    serde_json::from_value(serde_json::Value::Object(merged_fields))
        .map(Some)
        .map_err(|error| CliError::ContextSave(format!("state merge failed: {error}")))
}

fn prevent_accidental_empty_overwrite(
    paths: &CliContextPaths,
    proposed: &InMemoryRegistry,
    save_state: &ContextSaveState,
    disk_revision: u64,
    allow_empty_registry: bool,
) -> Result<(), CliError> {
    if has_persistable_tasks(proposed) {
        return Ok(());
    }
    if disk_revision == 0 && !paths.state_file.exists() {
        return Ok(());
    }
    if allow_empty_registry {
        return Ok(());
    }
    if has_persistable_tasks(&save_state.loaded_registry) {
        return Err(CliError::ContextSave(
            "refusing to save empty registry over non-empty loaded state; authorize delete-all before saving"
                .to_string(),
        ));
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
mod tests;

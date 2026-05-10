use ajax_core::{
    commands::CommandContext,
    config::{Config, ConfigPaths},
    registry::{InMemoryRegistry, RegistryStore, SqliteRegistryStore},
};
use std::path::PathBuf;

use crate::CliError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CliContextPaths {
    pub config_file: PathBuf,
    pub state_file: PathBuf,
}

impl CliContextPaths {
    pub fn new(config_file: impl Into<PathBuf>, state_file: impl Into<PathBuf>) -> Self {
        Self {
            config_file: config_file.into(),
            state_file: state_file.into(),
        }
    }
}

pub(crate) fn default_context_paths() -> Result<CliContextPaths, CliError> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| CliError::ContextLoad("HOME is not set".to_string()))?;
    let defaults = ConfigPaths::for_home(home);
    let config_file = std::env::var_os("AJAX_CONFIG")
        .map(PathBuf::from)
        .unwrap_or(defaults.config_file);
    let state_file = std::env::var_os("AJAX_STATE")
        .map(PathBuf::from)
        .unwrap_or(defaults.state_db);

    Ok(CliContextPaths {
        config_file,
        state_file,
    })
}

pub(crate) fn load_context(
    paths: &CliContextPaths,
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
        store
            .load()
            .map_err(|error| CliError::ContextLoad(format!("state load failed: {error}")))?
    } else {
        InMemoryRegistry::default()
    };

    Ok(CommandContext::new(config, registry))
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

pub(crate) fn save_context(
    paths: &CliContextPaths,
    context: &CommandContext<InMemoryRegistry>,
) -> Result<(), CliError> {
    if let Some(parent) = paths.state_file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| CliError::ContextSave(error.to_string()))?;
    }
    SqliteRegistryStore::new(&paths.state_file)
        .save(&context.registry)
        .map_err(|error| CliError::ContextSave(format!("state save failed: {error}")))
}

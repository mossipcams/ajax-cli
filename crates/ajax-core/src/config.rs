use std::{error::Error, fmt, path::PathBuf};

use serde::{Deserialize, Serialize};

mod watcher;
pub use watcher::{LayaCommand, WatcherConfig};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorktreePlacement {
    LegacySibling,
    Root(PathBuf),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimePaths {
    pub profile: String,
    pub config_file: PathBuf,
    pub state_db: PathBuf,
    pub logs_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub worktree_placement: WorktreePlacement,
    pub overrides: Vec<RuntimePathOverride>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimePathOverride {
    pub field: RuntimePathField,
    pub source: RuntimePathSource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimePathField {
    ConfigFile,
    StateDb,
    WorktreeRoot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimePathSource {
    Cli,
    Env,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RuntimePathRequest {
    home: PathBuf,
    cli_profile: Option<String>,
    env_profile: Option<String>,
    cli_home: Option<PathBuf>,
    env_home: Option<PathBuf>,
    cli_config: Option<PathBuf>,
    env_config: Option<PathBuf>,
    cli_state: Option<PathBuf>,
    env_state: Option<PathBuf>,
    cli_worktree_root: Option<PathBuf>,
    env_worktree_root: Option<PathBuf>,
}

impl RuntimePathRequest {
    pub fn new(home: impl Into<PathBuf>) -> Self {
        Self {
            home: home.into(),
            ..Self::default()
        }
    }

    pub fn with_cli_profile(mut self, profile: impl Into<String>) -> Self {
        self.cli_profile = Some(profile.into());
        self
    }

    pub fn with_env_profile(mut self, profile: impl Into<String>) -> Self {
        self.env_profile = Some(profile.into());
        self
    }

    pub fn with_cli_home(mut self, home: impl Into<PathBuf>) -> Self {
        self.cli_home = Some(home.into());
        self
    }

    pub fn with_env_home(mut self, home: impl Into<PathBuf>) -> Self {
        self.env_home = Some(home.into());
        self
    }

    pub fn with_cli_config(mut self, config: impl Into<PathBuf>) -> Self {
        self.cli_config = Some(config.into());
        self
    }

    pub fn with_env_config(mut self, config: impl Into<PathBuf>) -> Self {
        self.env_config = Some(config.into());
        self
    }

    pub fn with_cli_state(mut self, state: impl Into<PathBuf>) -> Self {
        self.cli_state = Some(state.into());
        self
    }

    pub fn with_env_state(mut self, state: impl Into<PathBuf>) -> Self {
        self.env_state = Some(state.into());
        self
    }

    pub fn with_cli_worktree_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.cli_worktree_root = Some(root.into());
        self
    }

    pub fn with_env_worktree_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.env_worktree_root = Some(root.into());
        self
    }

    pub fn resolve(self) -> RuntimePaths {
        let profile = self
            .cli_profile
            .or(self.env_profile)
            .unwrap_or_else(|| "stable".to_string());
        let runtime_home = self.cli_home.or(self.env_home);
        let mut paths = match runtime_home {
            Some(home) => self_contained_runtime_paths(profile, home),
            None if profile == "dev" => {
                self_contained_runtime_paths(profile, self.home.join(".ajax-dev"))
            }
            None => stable_runtime_paths(self.home, profile),
        };

        if let Some((config_file, source)) = pick(self.cli_config, self.env_config) {
            paths.config_file = config_file;
            paths.record_override(RuntimePathField::ConfigFile, source);
        }
        if let Some((state_db, source)) = pick(self.cli_state, self.env_state) {
            paths.state_db = state_db;
            paths.record_override(RuntimePathField::StateDb, source);
        }
        if let Some((root, source)) = pick(self.cli_worktree_root, self.env_worktree_root) {
            paths.worktree_placement = WorktreePlacement::Root(root);
            paths.record_override(RuntimePathField::WorktreeRoot, source);
        }

        paths
    }
}

/// Resolve a single tunable: a CLI value wins over an env value, and the winner
/// reports which source it came from for `ajax runtime` to surface.
fn pick<T>(cli: Option<T>, env: Option<T>) -> Option<(T, RuntimePathSource)> {
    cli.map(|value| (value, RuntimePathSource::Cli))
        .or_else(|| env.map(|value| (value, RuntimePathSource::Env)))
}

impl RuntimePaths {
    fn record_override(&mut self, field: RuntimePathField, source: RuntimePathSource) {
        self.overrides.push(RuntimePathOverride { field, source });
    }
}

fn stable_runtime_paths(home: PathBuf, profile: String) -> RuntimePaths {
    let defaults = ConfigPaths::for_home(home);
    RuntimePaths {
        profile,
        config_file: defaults.config_file,
        state_db: defaults.state_db,
        logs_dir: defaults.logs_dir,
        cache_dir: defaults.cache_dir,
        worktree_placement: WorktreePlacement::LegacySibling,
        overrides: Vec::new(),
    }
}

fn self_contained_runtime_paths(profile: String, home: PathBuf) -> RuntimePaths {
    RuntimePaths {
        profile,
        config_file: home.join("config.toml"),
        state_db: home.join("ajax.db"),
        logs_dir: home.join("logs"),
        cache_dir: home.join("cache"),
        worktree_placement: WorktreePlacement::Root(home.join("worktrees")),
        overrides: Vec::new(),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigPaths {
    pub config_file: PathBuf,
    pub state_db: PathBuf,
    pub logs_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl ConfigPaths {
    pub fn for_home(home: impl Into<PathBuf>) -> Self {
        let home = home.into();

        Self {
            config_file: home.join(".config/ajax/config.toml"),
            state_db: home.join(".local/state/ajax/ajax.db"),
            logs_dir: home.join(".local/state/ajax/logs"),
            cache_dir: home.join(".cache/ajax"),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub repos: Vec<ManagedRepo>,
    #[serde(default)]
    pub test_commands: Vec<TestCommand>,
    #[serde(default)]
    pub stt: SttConfig,
    #[serde(default)]
    pub watcher: WatcherConfig,
}

impl Config {
    pub fn from_toml_str(input: &str) -> Result<Self, ConfigParseError> {
        toml::from_str(input).map_err(|error| {
            let message = error.to_string();
            if input.contains("[notify]")
                || message.contains("`notify`")
                || message.contains("'notify'")
            {
                ConfigParseError::Toml(
                    "unknown field `notify`: remove the [notify] webhook block; \
                     enable push notifications in Web Cockpit Settings instead"
                        .to_string(),
                )
            } else {
                ConfigParseError::Toml(message)
            }
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfigParseError {
    Toml(String),
}

impl fmt::Display for ConfigParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Toml(message) => write!(formatter, "toml parse error: {message}"),
        }
    }
}

impl Error for ConfigParseError {}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ManagedRepo {
    pub name: String,
    pub path: PathBuf,
    pub default_branch: String,
    #[serde(default)]
    pub bootstrap: Option<String>,
}

impl ManagedRepo {
    pub fn new(
        name: impl Into<String>,
        path: impl Into<PathBuf>,
        default_branch: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            default_branch: default_branch.into(),
            bootstrap: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SttConfig {
    #[serde(default)]
    pub provider_command: Option<String>,
    #[serde(default = "default_phrase_end_silence_ms")]
    pub phrase_end_silence_ms: u64,
    #[serde(default = "default_pause_grace_period_ms")]
    pub pause_grace_period_ms: u64,
    #[serde(default = "default_stt_language")]
    pub language: String,
    #[serde(default = "default_max_buffered_audio_ms")]
    pub max_buffered_audio_ms: u64,
    #[serde(default = "default_finalization_timeout_ms")]
    pub finalization_timeout_ms: u64,
}

fn default_phrase_end_silence_ms() -> u64 {
    700
}

fn default_pause_grace_period_ms() -> u64 {
    9_000
}

fn default_stt_language() -> String {
    "en-US".to_string()
}

fn default_max_buffered_audio_ms() -> u64 {
    2_000
}

fn default_finalization_timeout_ms() -> u64 {
    5_000
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            provider_command: None,
            phrase_end_silence_ms: default_phrase_end_silence_ms(),
            pause_grace_period_ms: default_pause_grace_period_ms(),
            language: default_stt_language(),
            max_buffered_audio_ms: default_max_buffered_audio_ms(),
            finalization_timeout_ms: default_finalization_timeout_ms(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct TestCommand {
    pub repo: String,
    pub command: String,
}

impl TestCommand {
    pub fn new(repo: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            repo: repo.into(),
            command: command.into(),
        }
    }
}

#[cfg(test)]
mod tests;

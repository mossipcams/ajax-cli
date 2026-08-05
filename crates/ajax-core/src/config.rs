use std::{error::Error, fmt, path::PathBuf};

use serde::{Deserialize, Serialize};

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
mod tests {
    use super::{
        Config, ConfigParseError, ConfigPaths, ManagedRepo, RuntimePathField, RuntimePathRequest,
        RuntimePathSource, SttConfig, TestCommand, WorktreePlacement,
    };
    use proptest::prelude::*;
    use std::path::Path;

    #[test]
    fn default_paths_live_outside_source_repo() {
        let source_repo = Path::new("/Users/matt/projects/ajax-cli");
        let paths = ConfigPaths::for_home("/Users/matt");

        assert_eq!(
            paths.config_file,
            Path::new("/Users/matt/.config/ajax/config.toml")
        );
        assert_eq!(
            paths.state_db,
            Path::new("/Users/matt/.local/state/ajax/ajax.db")
        );
        assert_eq!(
            paths.logs_dir,
            Path::new("/Users/matt/.local/state/ajax/logs")
        );
        assert_eq!(paths.cache_dir, Path::new("/Users/matt/.cache/ajax"));
        assert!(!paths.config_file.starts_with(source_repo));
        assert!(!paths.state_db.starts_with(source_repo));
        assert!(!paths.logs_dir.starts_with(source_repo));
        assert!(!paths.cache_dir.starts_with(source_repo));
    }

    #[test]
    fn runtime_paths_default_to_stable_profile_and_existing_paths() {
        let paths = RuntimePathRequest::new("/Users/matt").resolve();

        assert_eq!(paths.profile, "stable");
        assert_eq!(
            paths.config_file,
            Path::new("/Users/matt/.config/ajax/config.toml")
        );
        assert_eq!(
            paths.state_db,
            Path::new("/Users/matt/.local/state/ajax/ajax.db")
        );
        assert_eq!(
            paths.logs_dir,
            Path::new("/Users/matt/.local/state/ajax/logs")
        );
        assert_eq!(paths.cache_dir, Path::new("/Users/matt/.cache/ajax"));
        assert_eq!(paths.worktree_placement, WorktreePlacement::LegacySibling);
        assert!(paths.overrides.is_empty());
    }

    #[test]
    fn runtime_paths_dev_profile_uses_isolated_home_layout() {
        let paths = RuntimePathRequest::new("/Users/matt")
            .with_cli_profile("dev")
            .resolve();

        assert_eq!(paths.profile, "dev");
        assert_eq!(
            paths.config_file,
            Path::new("/Users/matt/.ajax-dev/config.toml")
        );
        assert_eq!(paths.state_db, Path::new("/Users/matt/.ajax-dev/ajax.db"));
        assert_eq!(paths.logs_dir, Path::new("/Users/matt/.ajax-dev/logs"));
        assert_eq!(paths.cache_dir, Path::new("/Users/matt/.ajax-dev/cache"));
        assert_eq!(
            paths.worktree_placement,
            WorktreePlacement::Root(Path::new("/Users/matt/.ajax-dev/worktrees").to_path_buf())
        );
    }

    #[test]
    fn runtime_paths_env_dev_profile_uses_isolated_paths() {
        let paths = RuntimePathRequest::new("/Users/matt")
            .with_env_profile("dev")
            .resolve();

        assert_eq!(paths.profile, "dev");
        assert_eq!(paths.state_db, Path::new("/Users/matt/.ajax-dev/ajax.db"));
    }

    #[test]
    fn runtime_paths_custom_home_derives_self_contained_layout() {
        let paths = RuntimePathRequest::new("/Users/matt")
            .with_cli_home("/tmp/ajax-dev")
            .resolve();

        assert_eq!(paths.profile, "stable");
        assert_eq!(paths.config_file, Path::new("/tmp/ajax-dev/config.toml"));
        assert_eq!(paths.state_db, Path::new("/tmp/ajax-dev/ajax.db"));
        assert_eq!(paths.logs_dir, Path::new("/tmp/ajax-dev/logs"));
        assert_eq!(paths.cache_dir, Path::new("/tmp/ajax-dev/cache"));
        assert_eq!(
            paths.worktree_placement,
            WorktreePlacement::Root(Path::new("/tmp/ajax-dev/worktrees").to_path_buf())
        );
    }

    #[test]
    fn runtime_paths_env_home_derives_self_contained_layout() {
        let paths = RuntimePathRequest::new("/Users/matt")
            .with_env_home("/tmp/ajax-env")
            .resolve();

        assert_eq!(paths.config_file, Path::new("/tmp/ajax-env/config.toml"));
        assert_eq!(paths.state_db, Path::new("/tmp/ajax-env/ajax.db"));
        assert_eq!(
            paths.worktree_placement,
            WorktreePlacement::Root(Path::new("/tmp/ajax-env/worktrees").to_path_buf())
        );
    }

    #[test]
    fn runtime_path_direct_overrides_win_and_report_source() {
        let paths = RuntimePathRequest::new("/Users/matt")
            .with_cli_profile("dev")
            .with_env_config("/tmp/env-config.toml")
            .with_cli_state("/tmp/cli-state.db")
            .with_env_worktree_root("/tmp/env-worktrees")
            .resolve();

        assert_eq!(paths.profile, "dev");
        assert_eq!(paths.config_file, Path::new("/tmp/env-config.toml"));
        assert_eq!(paths.state_db, Path::new("/tmp/cli-state.db"));
        assert_eq!(
            paths.worktree_placement,
            WorktreePlacement::Root(Path::new("/tmp/env-worktrees").to_path_buf())
        );
        assert!(paths
            .overrides
            .iter()
            .any(
                |override_info| override_info.field == RuntimePathField::ConfigFile
                    && override_info.source == RuntimePathSource::Env
            ));
        assert!(paths
            .overrides
            .iter()
            .any(
                |override_info| override_info.field == RuntimePathField::StateDb
                    && override_info.source == RuntimePathSource::Cli
            ));
        assert!(paths
            .overrides
            .iter()
            .any(
                |override_info| override_info.field == RuntimePathField::WorktreeRoot
                    && override_info.source == RuntimePathSource::Env
            ));
    }

    #[test]
    fn runtime_paths_stable_and_dev_do_not_collide() {
        let stable = RuntimePathRequest::new("/Users/matt")
            .with_cli_profile("stable")
            .resolve();
        let dev = RuntimePathRequest::new("/Users/matt")
            .with_cli_profile("dev")
            .resolve();

        assert_ne!(stable.state_db, dev.state_db);
        assert_ne!(stable.worktree_placement, dev.worktree_placement);
    }

    #[test]
    fn config_tracks_repos_and_tests() {
        let config = Config {
            repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
            test_commands: vec![TestCommand::new("web", "cargo test")],
            stt: SttConfig::default(),
        };

        assert_eq!(config.repos[0].name, "web");
        assert_eq!(config.test_commands[0].command, "cargo test");
    }

    #[test]
    fn stt_defaults_are_centralized_for_continuous_input() {
        let config = Config::default();

        assert_eq!(config.stt.provider_command, None);
        assert_eq!(config.stt.phrase_end_silence_ms, 700);
        assert_eq!(config.stt.pause_grace_period_ms, 9_000);
        assert_eq!(config.stt.language, "en-US");
        assert_eq!(config.stt.max_buffered_audio_ms, 2_000);
        assert_eq!(config.stt.finalization_timeout_ms, 5_000);
    }

    #[test]
    fn stt_configuration_loads_from_documented_toml_shape() {
        let config = Config::from_toml_str(
            r#"
            [stt]
            provider_command = "python3 -m ajax_stt"
            phrase_end_silence_ms = 900
            pause_grace_period_ms = 10000
            language = "en-GB"
            max_buffered_audio_ms = 3000
            finalization_timeout_ms = 7000
            "#,
        )
        .unwrap();

        assert_eq!(
            config.stt,
            SttConfig {
                provider_command: Some("python3 -m ajax_stt".to_string()),
                phrase_end_silence_ms: 900,
                pause_grace_period_ms: 10_000,
                language: "en-GB".to_string(),
                max_buffered_audio_ms: 3_000,
                finalization_timeout_ms: 7_000,
            }
        );
    }

    #[test]
    fn stt_language_from_config_reaches_provider_session_shape() {
        let config = Config::from_toml_str(
            r#"
            [stt]
            language = "en-GB"
            "#,
        )
        .unwrap();

        assert_eq!(config.stt.language, "en-GB");
    }

    proptest! {
        #[test]
        fn constructors_preserve_input_values(
            repo_name in "\\PC*",
            repo_path in "\\PC*",
            default_branch in "\\PC*",
            test_repo in "\\PC*",
            test_command in "\\PC*",
        ) {
            let repo = ManagedRepo::new(&repo_name, &repo_path, &default_branch);
            prop_assert_eq!(repo.name, repo_name);
            prop_assert_eq!(repo.path, Path::new(&repo_path));
            prop_assert_eq!(repo.default_branch, default_branch);

            let test_command_value = TestCommand::new(&test_repo, &test_command);
            prop_assert_eq!(test_command_value.repo, test_repo);
            prop_assert_eq!(test_command_value.command, test_command);
        }
    }

    #[test]
    fn leftover_notify_block_is_rejected_with_push_guidance() {
        let error = Config::from_toml_str(
            r#"
            [notify]
            webhook_url = "https://example.invalid/topic"
            "#,
        )
        .unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("notify") && message.contains("Settings"),
            "expected push migration guidance, got {message}"
        );
    }

    #[test]
    fn unknown_config_tables_are_rejected() {
        let error = Config::from_toml_str(
            r#"
            [not_a_real_table]
            value = 1
            "#,
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("unknown field"),
            "expected unknown field rejection, got {error}"
        );
    }

    #[test]
    fn config_loads_from_documented_toml_shape() {
        let config = Config::from_toml_str(
            r#"
            [[repos]]
            name = "web"
            path = "/Users/matt/projects/web"
            default_branch = "main"

            [[test_commands]]
            repo = "web"
            command = "cargo test"
            "#,
        )
        .unwrap();

        assert_eq!(config.repos[0].name, "web");
        assert_eq!(config.test_commands[0].repo, "web");
    }

    #[test]
    fn config_loads_repo_bootstrap_command() {
        let config = Config::from_toml_str(
            r#"
            [[repos]]
            name = "web"
            path = "/Users/matt/projects/web"
            default_branch = "main"
            bootstrap = "npm ci"
            "#,
        )
        .unwrap();

        assert_eq!(config.repos[0].bootstrap.as_deref(), Some("npm ci"));
    }

    #[test]
    fn config_rejects_undocumented_launcher_sections() {
        let error = Config::from_toml_str(
            r#"
            [[repos]]
            name = "web"
            path = "/Users/matt/projects/web"
            default_branch = "main"

            [[launchers]]
            name = "codex"
            command = "codex"
            "#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("unknown field `launchers`"));
    }

    #[test]
    fn config_rejects_undocumented_cleanup_sections() {
        let error = Config::from_toml_str(
            r#"
            [[repos]]
            name = "web"
            path = "/Users/matt/projects/web"
            default_branch = "main"

            [cleanup]
            require_clean_worktree = true
            require_merged_branch = true
            require_no_unpushed_commits = true
            "#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("unknown field `cleanup`"));
    }

    #[test]
    fn config_parse_errors_have_operator_facing_display() {
        assert_eq!(
            ConfigParseError::Toml("missing field".to_string()).to_string(),
            "toml parse error: missing field"
        );
    }
}

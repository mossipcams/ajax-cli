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
}

impl Config {
    pub fn from_toml_str(input: &str) -> Result<Self, ConfigParseError> {
        toml::from_str(input).map_err(|error| ConfigParseError::Toml(error.to_string()))
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
    #[serde(default)]
    pub graphify_update: Option<String>,
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
            graphify_update: None,
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
        RuntimePathSource, TestCommand, WorktreePlacement,
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
        };

        assert_eq!(config.repos[0].name, "web");
        assert_eq!(config.test_commands[0].command, "cargo test");
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
    fn config_loads_repo_graphify_update_command() {
        let config = Config::from_toml_str(
            r#"
            [[repos]]
            name = "web"
            path = "/Users/matt/projects/web"
            default_branch = "main"
            graphify_update = "graphify extract --update"
            "#,
        )
        .unwrap();

        assert_eq!(
            config.repos[0].graphify_update.as_deref(),
            Some("graphify extract --update")
        );
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

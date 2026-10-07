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
        watcher: super::WatcherConfig::default(),
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

//! Web Cockpit process lifecycle (restart via re-exec or an external script).

#[cfg(not(test))]
use std::{process::Command, thread, time::Duration};

#[cfg(not(test))]
const RESTART_DELAY: Duration = Duration::from_millis(400);

pub const RESTART_SCRIPT_ENV: &str = "AJAX_WEB_RESTART_SCRIPT";
pub const RESTART_PROFILE_ENV: &str = "AJAX_WEB_RESTART_PROFILE";
pub const RESTART_PORT_ENV: &str = "AJAX_WEB_RESTART_PORT";
pub const AJAX_PROFILE_ENV: &str = "AJAX_PROFILE";
pub const DEV_PROFILE: &str = "dev";
pub const STABLE_PROFILE: &str = "stable";
pub const DEFAULT_STABLE_PORT: &str = "8787";
const TEST_IN_STABLE_SCRIPT: &str = "test-in-stable.sh";
const DEV_WEB_RESTART_SCRIPT: &str = "dev-web-restart.sh";
const SCRIPTS_DIR: &str = "scripts";
const WORKTREES_DIR: &str = "ajax-cli__worktrees";
const MAIN_CHECKOUT_DIR: &str = "ajax-cli";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestartLaunch {
    Respawn,
    Script { path: String, args: Vec<String> },
}

pub fn restart_launch_from_env(
    script_env: Option<&str>,
    profile_env: Option<&str>,
    port_env: Option<&str>,
) -> RestartLaunch {
    if let Some(script) = script_env.filter(|value| !value.is_empty()) {
        let mut args = Vec::new();
        if let Some(profile) = profile_env.filter(|value| !value.is_empty()) {
            args.push("--profile".to_string());
            args.push(profile.to_string());
        }
        if let Some(port) = port_env.filter(|value| !value.is_empty()) {
            args.push("--port".to_string());
            args.push(port.to_string());
        }
        return RestartLaunch::Script {
            path: script.to_string(),
            args,
        };
    }
    RestartLaunch::Respawn
}

#[cfg(not(test))]
fn restart_launch() -> RestartLaunch {
    restart_launch_from_env(
        std::env::var(RESTART_SCRIPT_ENV).ok().as_deref(),
        std::env::var(RESTART_PROFILE_ENV).ok().as_deref(),
        std::env::var(RESTART_PORT_ENV).ok().as_deref(),
    )
}

fn should_exit_after_launch(result: Result<(), String>) -> bool {
    result.is_ok()
}

/// Re-exec the current process or spawn a configured restart script after a short
/// delay, then exit only when the successor spawn succeeded.
///
/// Under `cfg(test)` this is a no-op so integration tests do not terminate the runner.
pub fn schedule_process_restart() {
    #[cfg(not(test))]
    {
        thread::spawn(|| {
            thread::sleep(RESTART_DELAY);
            let result = launch_restart(restart_launch());
            if let Err(ref error) = result {
                eprintln!("Ajax web restart failed: {error}");
            }
            if should_exit_after_launch(result) {
                std::process::exit(0);
            }
        });
    }
}

#[cfg(not(test))]
fn launch_restart(plan: RestartLaunch) -> Result<(), String> {
    match plan {
        RestartLaunch::Respawn => respawn_current_process(),
        RestartLaunch::Script { path, args } => spawn_restart_script(&path, &args),
    }
}

#[cfg(not(test))]
fn spawn_restart_script(script: &str, args: &[String]) -> Result<(), String> {
    let mut command = Command::new(script);
    command.args(args).envs(std::env::vars());
    command
        .spawn()
        .map_err(|error| format!("could not spawn restart script {script}: {error}"))?;
    Ok(())
}

#[cfg(not(test))]
fn respawn_current_process() -> Result<(), String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("could not resolve executable: {error}"))?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    Command::new(&executable)
        .args(args)
        .envs(std::env::vars())
        .spawn()
        .map_err(|error| format!("could not spawn replacement process: {error}"))?;
    Ok(())
}

pub fn flag_value_from_args<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2).find_map(|window| {
        (window[0] == flag && !window[1].is_empty()).then_some(window[1].as_str())
    })
}

pub fn web_profile_from_sources<'a>(
    restart_profile: Option<&'a str>,
    cli_profile: Option<&'a str>,
    ajax_profile: Option<&'a str>,
) -> Option<&'a str> {
    restart_profile
        .filter(|value| !value.is_empty())
        .or(cli_profile.filter(|value| !value.is_empty()))
        .or_else(|| ajax_profile.filter(|value| !value.is_empty()))
}

pub fn web_profile_from_env<'a>(
    restart_profile: Option<&'a str>,
    ajax_profile: Option<&'a str>,
) -> Option<&'a str> {
    web_profile_from_sources(restart_profile, None, ajax_profile)
}

pub fn test_in_stable_enabled(profile: Option<&str>, script: Option<&str>) -> bool {
    script.is_some_and(|value| !value.is_empty())
        && matches!(profile, Some(STABLE_PROFILE) | Some(DEV_PROFILE))
}

/// Test in Stable runs through a sibling of the restart script, not the restart
/// script itself: `dev-web-restart.sh` kills the tmux session it was spawned
/// from, so a direct child of the web server dies on SIGPIPE mid-restart. The
/// wrapper re-launches the restart in its own detached tmux session.
pub fn test_in_stable_script(restart_script: &str) -> String {
    std::path::Path::new(restart_script)
        .with_file_name(TEST_IN_STABLE_SCRIPT)
        .to_string_lossy()
        .into_owned()
}

pub fn test_in_stable_script_args(port: &str) -> Vec<String> {
    vec![
        "--profile".to_string(),
        STABLE_PROFILE.to_string(),
        "--port".to_string(),
        port.to_string(),
    ]
}

fn restart_script_with_wrapper_exists(script: &str) -> bool {
    let path = std::path::Path::new(script);
    path.is_file() && std::path::Path::new(&test_in_stable_script(script)).is_file()
}

fn discover_dev_web_restart_script(cwd: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut dir = cwd.to_path_buf();
    loop {
        let candidate = dir.join(SCRIPTS_DIR).join(DEV_WEB_RESTART_SCRIPT);
        if candidate.is_file() {
            return Some(candidate);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

/// When cwd lives under `ajax-cli__worktrees`, the main checkout is the sibling
/// `ajax-cli` directory next to that worktrees folder (e.g. trashed worktree cwd).
fn infer_main_ajax_cli_checkout_from_worktree_path(
    path: &std::path::Path,
) -> Option<std::path::PathBuf> {
    for ancestor in path.ancestors() {
        if ancestor.file_name().and_then(|name| name.to_str()) == Some(WORKTREES_DIR) {
            let parent = ancestor.parent()?;
            return Some(parent.join(MAIN_CHECKOUT_DIR));
        }
    }
    None
}

fn resolve_discovered_restart_script(root: &std::path::Path) -> Option<String> {
    discover_dev_web_restart_script(root).and_then(|path| {
        let script = path.to_string_lossy().into_owned();
        restart_script_with_wrapper_exists(&script).then_some(script)
    })
}

pub fn resolve_restart_script(
    script_env: Option<&str>,
    cwd: Option<&std::path::Path>,
) -> Option<String> {
    if let Some(script) = script_env.filter(|value| !value.is_empty()) {
        return restart_script_with_wrapper_exists(script).then(|| script.to_string());
    }
    let cwd = cwd?;
    resolve_discovered_restart_script(cwd).or_else(|| {
        infer_main_ajax_cli_checkout_from_worktree_path(cwd)
            .and_then(|main_checkout| resolve_discovered_restart_script(&main_checkout))
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestInStableConfig {
    pub script: String,
    pub port: String,
    /// True when Settings should wait for cutover (stable instance only).
    /// Independent of process exit; the live listener stays up until the
    /// detached wrapper replaces it after build/install.
    pub exits_current_process: bool,
}

pub struct TestInStableResolveInput<'a> {
    pub restart_profile: Option<&'a str>,
    pub cli_args: &'a [String],
    pub ajax_profile: Option<&'a str>,
    pub restart_script_env: Option<&'a str>,
    pub restart_port_env: Option<&'a str>,
    pub cwd: Option<&'a std::path::Path>,
}

pub fn resolve_test_in_stable_config(
    input: TestInStableResolveInput<'_>,
) -> Option<TestInStableConfig> {
    let cli_profile = flag_value_from_args(input.cli_args, "--profile");
    let profile = web_profile_from_sources(input.restart_profile, cli_profile, input.ajax_profile);
    let script = resolve_restart_script(input.restart_script_env, input.cwd)?;
    if !test_in_stable_enabled(profile, Some(script.as_str())) {
        return None;
    }
    let port = if profile == Some(STABLE_PROFILE) {
        input
            .restart_port_env
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| flag_value_from_args(input.cli_args, "--port").map(str::to_string))
            .unwrap_or_else(|| DEFAULT_STABLE_PORT.to_string())
    } else {
        DEFAULT_STABLE_PORT.to_string()
    };
    Some(TestInStableConfig {
        script,
        port,
        exits_current_process: profile == Some(STABLE_PROFILE),
    })
}

fn process_test_in_stable_config() -> Option<TestInStableConfig> {
    let cli_args: Vec<String> = std::env::args().skip(1).collect();
    let cwd = std::env::current_dir().ok();
    resolve_test_in_stable_config(TestInStableResolveInput {
        restart_profile: std::env::var(RESTART_PROFILE_ENV).ok().as_deref(),
        cli_args: &cli_args,
        ajax_profile: std::env::var(AJAX_PROFILE_ENV).ok().as_deref(),
        restart_script_env: std::env::var(RESTART_SCRIPT_ENV).ok().as_deref(),
        restart_port_env: std::env::var(RESTART_PORT_ENV).ok().as_deref(),
        cwd: cwd.as_deref(),
    })
}

pub fn test_in_stable_enabled_from_env() -> bool {
    process_test_in_stable_config().is_some()
}

pub fn resolved_web_profile_from_env() -> Option<String> {
    let cli_args: Vec<String> = std::env::args().skip(1).collect();
    web_profile_from_sources(
        std::env::var(RESTART_PROFILE_ENV).ok().as_deref(),
        flag_value_from_args(&cli_args, "--profile"),
        std::env::var(AJAX_PROFILE_ENV).ok().as_deref(),
    )
    .map(str::to_string)
}

pub fn test_in_stable_restarts_current_instance() -> bool {
    process_test_in_stable_config().is_some_and(|config| config.exits_current_process)
}

/// Spawn the detached Test in Stable wrapper with stable profile args.
///
/// The live stable listener must not exit here; the wrapper rebuilds in
/// `ajax-test-in-stable` and cuts over only after the new binary is healthy.
///
/// Under `cfg(test)` this is a no-op so integration tests do not terminate the runner.
pub fn schedule_test_in_stable() {
    #[cfg(not(test))]
    {
        thread::spawn(|| {
            thread::sleep(RESTART_DELAY);
            let Some(config) = process_test_in_stable_config() else {
                return;
            };
            let args = test_in_stable_script_args(&config.port);
            if let Err(error) = spawn_restart_script(&test_in_stable_script(&config.script), &args)
            {
                eprintln!("Ajax web test-in-stable failed: {error}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{restart_launch_from_env, schedule_process_restart, RestartLaunch};

    #[test]
    fn schedule_process_restart_is_no_op_in_tests() {
        schedule_process_restart();
    }

    #[test]
    fn schedule_test_in_stable_is_no_op_in_tests() {
        super::schedule_test_in_stable();
    }

    #[test]
    fn should_exit_after_launch_only_on_success() {
        assert!(super::should_exit_after_launch(Ok(())));
        assert!(!super::should_exit_after_launch(Err(
            "spawn failed".to_string()
        )));
    }

    #[test]
    fn restart_launch_defaults_to_respawn_without_script_env() {
        assert_eq!(
            restart_launch_from_env(None, None, None),
            RestartLaunch::Respawn
        );
        assert_eq!(
            restart_launch_from_env(Some(""), None, None),
            RestartLaunch::Respawn
        );
    }

    #[test]
    fn restart_launch_uses_script_env_with_profile_and_port() {
        assert_eq!(
            restart_launch_from_env(
                Some("/repo/scripts/dev-web-restart.sh"),
                Some("dev"),
                Some("8788"),
            ),
            RestartLaunch::Script {
                path: "/repo/scripts/dev-web-restart.sh".to_string(),
                args: vec![
                    "--profile".to_string(),
                    "dev".to_string(),
                    "--port".to_string(),
                    "8788".to_string(),
                ],
            }
        );
    }

    #[test]
    fn restart_env_constant_names_match_launcher_contract() {
        assert_eq!(super::RESTART_SCRIPT_ENV, "AJAX_WEB_RESTART_SCRIPT");
        assert_eq!(super::RESTART_PROFILE_ENV, "AJAX_WEB_RESTART_PROFILE");
        assert_eq!(super::RESTART_PORT_ENV, "AJAX_WEB_RESTART_PORT");
    }

    #[test]
    fn web_profile_from_env_prefers_restart_profile_over_ajax_profile() {
        assert_eq!(
            super::web_profile_from_env(Some("stable"), Some("dev")),
            Some("stable")
        );
        assert_eq!(super::web_profile_from_env(None, Some("dev")), Some("dev"));
        assert_eq!(
            super::web_profile_from_env(Some(""), Some("dev")),
            Some("dev")
        );
        assert_eq!(super::web_profile_from_env(None, None), None);
    }

    #[test]
    fn web_profile_from_sources_prefers_cli_profile_over_ajax_profile() {
        assert_eq!(
            super::web_profile_from_sources(None, Some("stable"), Some("dev")),
            Some("stable")
        );
        assert_eq!(
            super::web_profile_from_sources(Some("dev"), Some("stable"), Some("dev")),
            Some("dev")
        );
    }

    fn write_test_in_stable_scripts(root: &std::path::Path, include_wrapper: bool) -> String {
        let scripts = root.join("scripts");
        std::fs::create_dir_all(&scripts).expect("create scripts dir");
        let restart = scripts.join(super::DEV_WEB_RESTART_SCRIPT);
        std::fs::write(&restart, "#!/bin/sh\n").expect("write restart script");
        if include_wrapper {
            std::fs::write(scripts.join(super::TEST_IN_STABLE_SCRIPT), "#!/bin/sh\n")
                .expect("write wrapper script");
        }
        restart.to_string_lossy().into_owned()
    }

    #[test]
    fn resolve_restart_script_discovers_scripts_in_ancestor_tree() {
        let root = std::env::temp_dir().join(format!(
            "ajax-test-in-stable-discover-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let restart = write_test_in_stable_scripts(&root, true);
        let nested = root.join("nested").join("deep");
        std::fs::create_dir_all(&nested).expect("create nested cwd");

        assert_eq!(
            super::resolve_restart_script(None, Some(&nested)),
            Some(restart)
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn infer_main_ajax_cli_checkout_from_worktree_path_finds_sibling() {
        assert_eq!(
            super::infer_main_ajax_cli_checkout_from_worktree_path(std::path::Path::new(
                "/Users/matt/Desktop/Projects/ajax-cli__worktrees/.ajax-trash/dead"
            )),
            Some(std::path::PathBuf::from(
                "/Users/matt/Desktop/Projects/ajax-cli"
            ))
        );
        assert_eq!(
            super::infer_main_ajax_cli_checkout_from_worktree_path(std::path::Path::new(
                "/tmp/other/repo/nested"
            )),
            None
        );
    }

    #[test]
    fn resolve_restart_script_falls_back_to_main_checkout_under_worktrees() {
        let layout_root = std::env::temp_dir().join(format!(
            "ajax-test-in-stable-worktree-fallback-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&layout_root);
        let main_checkout = layout_root.join("ajax-cli");
        let worktree_cwd = layout_root
            .join("ajax-cli__worktrees")
            .join(".ajax-trash")
            .join("dead");
        std::fs::create_dir_all(&worktree_cwd).expect("create worktree cwd");
        let restart = write_test_in_stable_scripts(&main_checkout, true);

        assert_eq!(
            super::resolve_restart_script(None, Some(&worktree_cwd)),
            Some(restart)
        );

        let _ = std::fs::remove_dir_all(&layout_root);
    }

    #[test]
    fn resolve_restart_script_missing_wrapper_disables() {
        let root = std::env::temp_dir().join(format!(
            "ajax-test-in-stable-missing-wrapper-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let restart = write_test_in_stable_scripts(&root, false);

        assert_eq!(
            super::resolve_restart_script(Some(restart.as_str()), Some(&root)),
            None
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_test_in_stable_config_uses_cli_profile_over_ajax_profile() {
        let root = std::env::temp_dir().join(format!(
            "ajax-test-in-stable-cli-profile-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let restart = write_test_in_stable_scripts(&root, true);
        let cli_args = vec![
            "web".to_string(),
            "--profile".to_string(),
            super::STABLE_PROFILE.to_string(),
            "--port".to_string(),
            "8788".to_string(),
        ];

        assert_eq!(
            super::resolve_test_in_stable_config(super::TestInStableResolveInput {
                restart_profile: None,
                cli_args: &cli_args,
                ajax_profile: Some("dev"),
                restart_script_env: Some(restart.as_str()),
                restart_port_env: None,
                cwd: Some(&root),
            }),
            Some(super::TestInStableConfig {
                script: restart,
                port: "8788".to_string(),
                exits_current_process: true,
            })
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_test_in_stable_config_enables_stable_profile_with_restart_script() {
        assert_eq!(
            super::resolve_test_in_stable_config(super::TestInStableResolveInput {
                restart_profile: Some(super::STABLE_PROFILE),
                cli_args: &[],
                ajax_profile: Some("dev"),
                restart_script_env: None,
                restart_port_env: None,
                cwd: None,
            }),
            None
        );

        let root = std::env::temp_dir().join(format!(
            "ajax-test-in-stable-stable-script-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let restart = write_test_in_stable_scripts(&root, true);

        assert_eq!(
            super::resolve_test_in_stable_config(super::TestInStableResolveInput {
                restart_profile: Some(super::STABLE_PROFILE),
                cli_args: &[],
                ajax_profile: Some("dev"),
                restart_script_env: Some(restart.as_str()),
                restart_port_env: Some("8788"),
                cwd: Some(&root),
            }),
            Some(super::TestInStableConfig {
                script: restart,
                port: "8788".to_string(),
                exits_current_process: true,
            })
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_in_stable_enabled_requires_dev_or_stable_profile_and_script() {
        assert!(super::test_in_stable_enabled(
            Some(super::STABLE_PROFILE),
            Some("/x")
        ));
        assert!(super::test_in_stable_enabled(
            Some(super::DEV_PROFILE),
            Some("/x")
        ));
        assert!(!super::test_in_stable_enabled(Some("prod"), Some("/x")));
        assert!(!super::test_in_stable_enabled(
            Some(super::STABLE_PROFILE),
            Some("")
        ));
        assert!(!super::test_in_stable_enabled(
            Some(super::STABLE_PROFILE),
            None
        ));
    }

    #[test]
    fn resolve_test_in_stable_config_dev_profile_targets_stable_port() {
        let root = std::env::temp_dir().join(format!(
            "ajax-test-in-stable-dev-port-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let restart = write_test_in_stable_scripts(&root, true);
        let cli_args = vec![
            "web".to_string(),
            "--profile".to_string(),
            super::DEV_PROFILE.to_string(),
            "--port".to_string(),
            "8788".to_string(),
        ];

        assert_eq!(
            super::resolve_test_in_stable_config(super::TestInStableResolveInput {
                restart_profile: None,
                cli_args: &cli_args,
                ajax_profile: None,
                restart_script_env: Some(restart.as_str()),
                restart_port_env: Some("8788"),
                cwd: Some(&root),
            }),
            Some(super::TestInStableConfig {
                script: restart,
                port: super::DEFAULT_STABLE_PORT.to_string(),
                exits_current_process: false,
            })
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_test_in_stable_config_stable_signals_cutover_without_process_exit() {
        let root = std::env::temp_dir().join(format!(
            "ajax-test-in-stable-no-exit-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let restart = write_test_in_stable_scripts(&root, true);

        let config = super::resolve_test_in_stable_config(super::TestInStableResolveInput {
            restart_profile: Some(super::STABLE_PROFILE),
            cli_args: &[],
            ajax_profile: None,
            restart_script_env: Some(restart.as_str()),
            restart_port_env: Some("8787"),
            cwd: Some(&root),
        })
        .expect("stable config");

        assert!(config.exits_current_process);
        // schedule_test_in_stable is cfg(test) no-op; stable must not exit the
        // live listener — cutover is owned by the detached wrapper script.
        super::schedule_test_in_stable();

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_in_stable_uses_detached_wrapper_beside_the_restart_script() {
        assert_eq!(
            super::test_in_stable_script("/repo/scripts/dev-web-restart.sh"),
            "/repo/scripts/test-in-stable.sh"
        );
    }

    #[test]
    fn test_in_stable_launch_args() {
        assert_eq!(
            super::test_in_stable_script_args("8788"),
            vec![
                "--profile".to_string(),
                "stable".to_string(),
                "--port".to_string(),
                "8788".to_string(),
            ]
        );
        assert_eq!(
            super::test_in_stable_script_args(super::DEFAULT_STABLE_PORT),
            vec![
                "--profile".to_string(),
                "stable".to_string(),
                "--port".to_string(),
                "8787".to_string(),
            ]
        );
    }
}

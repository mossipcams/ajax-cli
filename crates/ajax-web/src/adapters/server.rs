//! Web Cockpit process lifecycle (restart via re-exec or an external script).

#[cfg(not(test))]
use std::{process::Command, thread, time::Duration};

#[cfg(not(test))]
const RESTART_DELAY: Duration = Duration::from_millis(400);

const RESTART_SCRIPT_ENV: &str = "AJAX_WEB_RESTART_SCRIPT";
const RESTART_PROFILE_ENV: &str = "AJAX_WEB_RESTART_PROFILE";
const RESTART_PORT_ENV: &str = "AJAX_WEB_RESTART_PORT";
pub const AJAX_PROFILE_ENV: &str = "AJAX_PROFILE";
pub const STABLE_PROFILE: &str = "stable";
pub const DEFAULT_STABLE_PORT: &str = "8787";
const TEST_IN_STABLE_SCRIPT: &str = "test-in-stable.sh";
const DEV_WEB_RESTART_SCRIPT: &str = "dev-web-restart.sh";
const SCRIPTS_DIR: &str = "scripts";

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
    profile == Some(STABLE_PROFILE) && script.is_some_and(|value| !value.is_empty())
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

pub fn resolve_restart_script(
    script_env: Option<&str>,
    cwd: Option<&std::path::Path>,
) -> Option<String> {
    if let Some(script) = script_env.filter(|value| !value.is_empty()) {
        return restart_script_with_wrapper_exists(script).then(|| script.to_string());
    }
    let cwd = cwd?;
    discover_dev_web_restart_script(cwd).and_then(|path| {
        let script = path.to_string_lossy().into_owned();
        restart_script_with_wrapper_exists(&script).then_some(script)
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestInStableConfig {
    pub script: String,
    pub port: String,
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
    let port = input
        .restart_port_env
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| flag_value_from_args(input.cli_args, "--port").map(str::to_string))
        .unwrap_or_else(|| DEFAULT_STABLE_PORT.to_string());
    Some(TestInStableConfig { script, port })
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

/// Spawn the detached Test in Stable wrapper with stable profile args, then exit
/// only when the wrapper spawn succeeded.
///
/// Under `cfg(test)` this is a no-op so integration tests do not terminate the runner.
pub fn schedule_test_in_stable() {
    #[cfg(not(test))]
    {
        thread::spawn(|| {
            thread::sleep(RESTART_DELAY);
            let result = process_test_in_stable_config().map(|config| {
                let args = test_in_stable_script_args(&config.port);
                spawn_restart_script(&test_in_stable_script(&config.script), &args)
            });
            let exit = result
                .map(|launch| {
                    if let Err(ref error) = launch {
                        eprintln!("Ajax web test-in-stable failed: {error}");
                    }
                    should_exit_after_launch(launch)
                })
                .unwrap_or(false);
            if exit {
                std::process::exit(0);
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
            })
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_in_stable_enabled_requires_stable_profile_and_script() {
        assert!(super::test_in_stable_enabled(
            Some(super::STABLE_PROFILE),
            Some("/x")
        ));
        assert!(!super::test_in_stable_enabled(Some("dev"), Some("/x")));
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

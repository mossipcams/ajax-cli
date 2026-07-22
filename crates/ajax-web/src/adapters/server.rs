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

/// Re-exec the current process or spawn a configured restart script after a short
/// delay, then exit.
///
/// Under `cfg(test)` this is a no-op so integration tests do not terminate the runner.
pub fn schedule_process_restart() {
    #[cfg(not(test))]
    {
        thread::spawn(|| {
            thread::sleep(RESTART_DELAY);
            if let Err(error) = launch_restart(restart_launch()) {
                eprintln!("Ajax web restart failed: {error}");
            }
            std::process::exit(0);
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

pub fn web_profile_from_env<'a>(
    restart_profile: Option<&'a str>,
    ajax_profile: Option<&'a str>,
) -> Option<&'a str> {
    restart_profile
        .filter(|value| !value.is_empty())
        .or_else(|| ajax_profile.filter(|value| !value.is_empty()))
}

pub fn test_in_stable_enabled(profile: Option<&str>, script: Option<&str>) -> bool {
    profile == Some(STABLE_PROFILE) && script.is_some_and(|value| !value.is_empty())
}

pub fn test_in_stable_script_args(port: &str) -> Vec<String> {
    vec![
        "--profile".to_string(),
        STABLE_PROFILE.to_string(),
        "--port".to_string(),
        port.to_string(),
    ]
}

fn restart_script_path_exists(script: &str) -> bool {
    #[cfg(test)]
    {
        !script.is_empty()
    }
    #[cfg(not(test))]
    {
        std::path::Path::new(script).is_file()
    }
}

pub fn test_in_stable_enabled_from_env() -> bool {
    let script = std::env::var(RESTART_SCRIPT_ENV)
        .ok()
        .filter(|value| !value.is_empty());
    let restart_profile = std::env::var(RESTART_PROFILE_ENV).ok();
    let ajax_profile = std::env::var(AJAX_PROFILE_ENV).ok();
    let profile = web_profile_from_env(restart_profile.as_deref(), ajax_profile.as_deref());
    match script {
        Some(path) if restart_script_path_exists(&path) => {
            test_in_stable_enabled(profile, Some(path.as_str()))
        }
        _ => false,
    }
}

/// Spawn the restart script with stable profile args, then exit.
///
/// Under `cfg(test)` this is a no-op so integration tests do not terminate the runner.
pub fn schedule_test_in_stable() {
    #[cfg(not(test))]
    {
        thread::spawn(|| {
            thread::sleep(RESTART_DELAY);
            if let Some(script) = std::env::var(RESTART_SCRIPT_ENV)
                .ok()
                .filter(|value| !value.is_empty())
            {
                let port = std::env::var(RESTART_PORT_ENV)
                    .ok()
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| DEFAULT_STABLE_PORT.to_string());
                let args = test_in_stable_script_args(&port);
                if let Err(error) = spawn_restart_script(&script, &args) {
                    eprintln!("Ajax web test-in-stable failed: {error}");
                }
            }
            std::process::exit(0);
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

//! Web Cockpit process lifecycle (restart via re-exec or an external script).

#[cfg(not(test))]
use std::{process::Command, thread, time::Duration};

#[cfg(not(test))]
const RESTART_DELAY: Duration = Duration::from_millis(400);

const RESTART_SCRIPT_ENV: &str = "AJAX_WEB_RESTART_SCRIPT";
const RESTART_PROFILE_ENV: &str = "AJAX_WEB_RESTART_PROFILE";
const RESTART_PORT_ENV: &str = "AJAX_WEB_RESTART_PORT";

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
}

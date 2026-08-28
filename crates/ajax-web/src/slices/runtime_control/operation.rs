//! Admission and launch for runtime restart/update operations.

use crate::adapters::{http::Response, server};
use std::{path::Path, process::Command, thread, time::Duration};

use super::store::{
    append_log_line, log_file_path, queue_operation, read_state, status_file_path, OperationKind,
    RUNTIME_LOG_ENV, RUNTIME_STATUS_ENV,
};

const LAUNCH_DELAY: Duration = Duration::from_millis(400);
const RUNTIME_UPDATE_SCRIPT: &str = "runtime-control.sh";
const UPDATE_SESSION: &str = "ajax-runtime-update";
const TEST_IN_STABLE_SESSION: &str = "ajax-test-in-stable";

fn json_error_response(status_code: u16, error: &str) -> Response {
    let body = serde_json::json!({ "ok": false, "error": error });
    Response {
        status_code,
        content_type: "application/json; charset=utf-8",
        body: serde_json::to_vec(&body)
            .unwrap_or_else(|_| br#"{"ok":false,"error":"serialization failed"}"#.to_vec()),
    }
}

fn json_accepted_response(restarting: bool) -> Response {
    let body = serde_json::json!({ "ok": true, "restarting": restarting });
    Response {
        status_code: 202,
        content_type: "application/json; charset=utf-8",
        body: serde_json::to_vec(&body)
            .unwrap_or_else(|_| br#"{"ok":true,"restarting":true}"#.to_vec()),
    }
}

pub fn handle_server_restart() -> Response {
    #[cfg(test)]
    {
        if let Some(dir) = super::store::resolve_runtime_dir(
            std::env::var(server::RESTART_SCRIPT_ENV).ok().as_deref(),
            std::env::current_dir().ok().as_deref(),
        ) {
            let _ = queue_operation(&dir, OperationKind::Restart);
        }
        Response {
            status_code: 200,
            content_type: "application/json; charset=utf-8",
            body: br#"{"ok":true,"restarting":true}"#.to_vec(),
        }
    }

    #[cfg(not(test))]
    match schedule_runtime_restart() {
        Ok(()) => Response {
            status_code: 200,
            content_type: "application/json; charset=utf-8",
            body: br#"{"ok":true,"restarting":true}"#.to_vec(),
        },
        Err(error) => json_error_response(409, &error),
    }
}

pub fn handle_server_update() -> Response {
    #[cfg(test)]
    {
        if !server::test_in_stable_enabled_from_env() {
            return json_error_response(404, "runtime update is not available");
        }
        if let Some(dir) = super::store::resolve_runtime_dir(
            std::env::var(server::RESTART_SCRIPT_ENV).ok().as_deref(),
            std::env::current_dir().ok().as_deref(),
        ) {
            let _ = queue_operation(&dir, OperationKind::Update);
        }
        let restarting = server::test_in_stable_restarts_current_instance();
        json_accepted_response(restarting)
    }

    #[cfg(not(test))]
    match schedule_runtime_update() {
        Ok(()) => {
            let restarting = server::test_in_stable_restarts_current_instance();
            json_accepted_response(restarting)
        }
        Err(error) => {
            let status = if error.contains("already in progress") {
                409
            } else if error.contains("not available") {
                404
            } else {
                500
            };
            json_error_response(status, &error)
        }
    }
}

#[cfg(not(test))]
pub fn schedule_runtime_restart() -> Result<(), String> {
    const RUNTIME_RESTART_SCRIPT: &str = "runtime-restart.sh";
    let restart_script = resolve_restart_script_path()?;
    let runtime_dir = host_runtime_dir(&restart_script)?;
    refuse_if_active(&runtime_dir)?;
    queue_operation(&runtime_dir, OperationKind::Restart)?;
    let profile =
        server::resolved_web_profile_from_env().unwrap_or_else(|| server::DEV_PROFILE.to_string());
    let port = resolve_listen_port(&profile)?;
    let wrapper = sibling_script(&restart_script, RUNTIME_RESTART_SCRIPT);
    let args = vec!["--profile".to_string(), profile, "--port".to_string(), port];
    spawn_detached_wrapper(&wrapper, &args, &runtime_dir, "runtime restart")?;
    Ok(())
}

pub fn schedule_runtime_update() -> Result<(), String> {
    let restart_script = resolve_restart_script_path()?;
    if !server::test_in_stable_enabled_from_env() {
        return Err("runtime update is not available".to_string());
    }
    refuse_concurrent_update_sessions()?;
    let runtime_dir = host_runtime_dir(&restart_script)?;
    refuse_if_active(&runtime_dir)?;
    queue_operation(&runtime_dir, OperationKind::Update)?;
    let port = server::DEFAULT_STABLE_PORT.to_string();
    let wrapper = sibling_script(&restart_script, RUNTIME_UPDATE_SCRIPT);
    let args = server::test_in_stable_script_args(&port);
    spawn_detached_wrapper(&wrapper, &args, &runtime_dir, "runtime update")?;
    Ok(())
}

fn resolve_restart_script_path() -> Result<String, String> {
    server::resolve_restart_script(
        std::env::var(server::RESTART_SCRIPT_ENV).ok().as_deref(),
        std::env::current_dir().ok().as_deref(),
    )
    .ok_or_else(|| "restart script is not configured".to_string())
}

fn host_runtime_dir(restart_script: &str) -> Result<std::path::PathBuf, String> {
    super::store::host_runtime_dir_from_restart_script(restart_script)
        .ok_or_else(|| "runtime directory unresolved".to_string())
}

fn sibling_script(restart_script: &str, name: &str) -> String {
    std::path::Path::new(restart_script)
        .with_file_name(name)
        .to_string_lossy()
        .into_owned()
}

fn resolve_listen_port(profile: &str) -> Result<String, String> {
    if profile == server::STABLE_PROFILE {
        Ok(std::env::var(server::RESTART_PORT_ENV)
            .ok()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| server::DEFAULT_STABLE_PORT.to_string()))
    } else {
        Ok(std::env::var(server::RESTART_PORT_ENV)
            .ok()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "8788".to_string()))
    }
}

fn refuse_if_active(runtime_dir: &Path) -> Result<(), String> {
    let state = read_state(runtime_dir);
    if super::store::operation_is_active(&state) {
        return Err("a runtime operation is already in progress".to_string());
    }
    Ok(())
}

fn refuse_concurrent_update_sessions() -> Result<(), String> {
    for session in [TEST_IN_STABLE_SESSION, UPDATE_SESSION] {
        if tmux_has_session(session)? {
            return Err(format!(
                "{session} is already running; refuse concurrent cargo install"
            ));
        }
    }
    Ok(())
}

fn tmux_has_session(session: &str) -> Result<bool, String> {
    let output = Command::new("tmux")
        .args(["has-session", "-t", session])
        .output()
        .map_err(|error| format!("tmux probe failed: {error}"))?;
    Ok(output.status.success())
}

fn spawn_detached_wrapper(
    script: &str,
    args: &[String],
    runtime_dir: &Path,
    label: &str,
) -> Result<(), String> {
    if !Path::new(script).is_file() {
        return Err(format!("missing wrapper script: {script}"));
    }
    let status_file = status_file_path(runtime_dir);
    let log_file = log_file_path(runtime_dir);
    let runtime_dir = runtime_dir.to_path_buf();
    let script = script.to_string();
    let args = args.to_vec();
    let label = label.to_string();
    thread::spawn(move || {
        thread::sleep(LAUNCH_DELAY);
        let mut command = Command::new(&script);
        command
            .args(&args)
            .envs(std::env::vars())
            .env(
                RUNTIME_STATUS_ENV,
                status_file.to_string_lossy().to_string(),
            )
            .env(RUNTIME_LOG_ENV, log_file.to_string_lossy().to_string());
        match command.spawn() {
            Ok(_) => {
                let _ = append_log_line(&runtime_dir, &format!("spawned {label}"));
            }
            Err(error) => {
                eprintln!("Ajax {label} failed: {error}");
                let _ = append_log_line(&runtime_dir, &format!("spawn failed: {error}"));
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::resolve_listen_port;
    use crate::adapters::server;

    #[test]
    fn stable_update_port_defaults_to_8787() {
        assert_eq!(
            resolve_listen_port(server::STABLE_PROFILE).expect("port"),
            server::DEFAULT_STABLE_PORT
        );
    }
}

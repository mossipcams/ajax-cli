//! Runtime status projection for GET /api/server/runtime.

use ajax_core::adapters::{CommandRunner, CommandSpec};
use serde_json::{json, Value};
use std::path::Path;

use super::{
    logs::recent_log_lines,
    store::{process_uptime_seconds, read_state, resolve_runtime_dir, RuntimeControlState},
};

pub struct RuntimeStatusInput<'a> {
    pub version: &'static str,
    pub profile: String,
    pub test_in_stable: bool,
    pub restart_script_env: Option<&'a str>,
    pub restart_port_env: Option<&'a str>,
    pub ajax_profile: Option<&'a str>,
    pub cwd: Option<&'a Path>,
}

pub fn runtime_status_json<C: CommandRunner>(
    runner: &mut C,
    input: RuntimeStatusInput<'_>,
) -> Value {
    let runtime_dir = resolve_runtime_dir(input.restart_script_env, input.cwd);
    let state = runtime_dir
        .as_ref()
        .map(|dir| read_state(dir))
        .unwrap_or_default();
    let logs = runtime_dir
        .as_ref()
        .map(|dir| recent_log_lines(dir.as_path()))
        .unwrap_or_default();
    let update_available = runtime_dir
        .as_ref()
        .map(|dir| check_update_available(runner, dir.as_path(), &state))
        .unwrap_or_else(|| json!({ "known": false }));
    json!({
        "ok": true,
        "version": input.version,
        "commit": state.commit,
        "profile": input.profile,
        "uptime_seconds": process_uptime_seconds(),
        "update_available": update_available,
        "operation": state.operation,
        "logs": logs,
        "test_in_stable": input.test_in_stable,
    })
}

fn check_update_available<C: CommandRunner>(
    runner: &mut C,
    runtime_dir: &Path,
    state: &RuntimeControlState,
) -> Value {
    let host_clone = runtime_dir.parent();
    let Some(host_clone) = host_clone else {
        return json!({ "known": false });
    };
    let remote = match git_ls_remote_main(runner, host_clone) {
        Ok(sha) => sha,
        Err(_) => return json!({ "known": false }),
    };
    let installed = state.commit.clone().unwrap_or_default();
    json!({
        "known": true,
        "remote_commit": remote,
        "installed_commit": if installed.is_empty() { Value::Null } else { json!(installed) },
        "available": !installed.is_empty() && installed != remote,
    })
}

fn git_ls_remote_main<C: CommandRunner>(runner: &mut C, repo: &Path) -> Result<String, String> {
    let spec = CommandSpec::new(
        "git",
        [
            "-C",
            &repo.to_string_lossy(),
            "ls-remote",
            "origin",
            "refs/heads/main",
        ],
    );
    let output = runner.run(&spec).map_err(|error| error.to_string())?;
    if output.status_code != 0 {
        return Err(output.stderr);
    }
    output
        .stdout
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().next())
        .map(str::to_string)
        .ok_or_else(|| "empty ls-remote output".to_string())
}

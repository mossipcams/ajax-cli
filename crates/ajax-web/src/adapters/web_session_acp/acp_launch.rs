#[cfg(test)]
use ajax_core::adapters::acp_spawn_model_for_argv;
use ajax_core::{
    adapters::{acp_launch_for_agent, AcpLaunch},
    models::AgentClient,
};
use std::{
    collections::HashMap,
    path::Path,
    process::{Child, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};

#[cfg(test)]
use super::client::{
    TEST_ACP_COMMAND, TEST_ACP_EXTRA_ARGS, TEST_ACP_EXTRA_ARGS_GLOBAL, TEST_ACP_PROGRAM,
};
#[cfg(test)]
use std::path::PathBuf;

use super::client::acp_args_for_program;
use crate::adapters::session_shim::shim_spawn;

fn acp_candidates(launch: AcpLaunch) -> Vec<(String, Vec<String>)> {
    let mut candidates: Vec<(String, Vec<String>)> = Vec::new();
    if let Some(program) = launch.native_program {
        if native_acp_advertised(program) {
            candidates.push((program.to_string(), vec!["acp".to_string()]));
        }
    }
    for (program, base_args) in launch.candidates {
        candidates.push((
            (*program).to_string(),
            base_args.iter().map(|arg| (*arg).to_string()).collect(),
        ));
    }
    if let Some(package) = launch.acp_package {
        candidates.push((
            "npx".to_string(),
            vec!["-y".to_string(), package.to_string()],
        ));
    }
    candidates
}

fn native_acp_advertised(program: &str) -> bool {
    static CACHE: Mutex<Option<HashMap<String, (Instant, bool)>>> = Mutex::new(None);
    const TTL: Duration = Duration::from_secs(300);

    if let Ok(guard) = CACHE.lock() {
        if let Some((checked_at, advertised)) =
            guard.as_ref().and_then(|entries| entries.get(program))
        {
            if checked_at.elapsed() < TTL {
                return *advertised;
            }
        }
    }

    let Some(mut probe) = crate::adapters::program::harness_command(program) else {
        return false;
    };
    #[allow(clippy::let_and_return)]
    let advertised = probe
        .arg("--help")
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .is_some_and(|output| {
            let text = String::from_utf8_lossy(&output.stdout);
            text.lines().any(|line| {
                let line = line.trim_start();
                line.starts_with("acp ") || line == "acp"
            })
        });

    if let Ok(mut guard) = CACHE.lock() {
        guard
            .get_or_insert_with(HashMap::new)
            .insert(program.to_string(), (Instant::now(), advertised));
    }
    advertised
}

pub(super) fn spawn_acp_process(
    agent: AgentClient,
    worktree_path: &Path,
    model: Option<&str>,
) -> Result<Child, String> {
    #[cfg(test)]
    fn fake_acp_state_dir(worktree_path: &Path) -> PathBuf {
        worktree_path.to_path_buf()
    }

    #[cfg(test)]
    if let Some(program) = TEST_ACP_PROGRAM.with(|slot| slot.borrow().clone()) {
        let extra_args = TEST_ACP_EXTRA_ARGS.with(|slot| slot.borrow().clone());
        let mut command = std::process::Command::new("node");
        command.arg(program);
        if let Some(launch) = acp_launch_for_agent(agent) {
            if launch.model_pins_at_spawn() {
                if let Some(spawn_model) = acp_spawn_model_for_argv(launch, model) {
                    command.arg("--model").arg(spawn_model);
                }
            }
        }
        command.args(extra_args);
        command
            .env(
                "FAKE_ACP_STATE_DIR",
                fake_acp_state_dir(worktree_path).as_os_str(),
            )
            .current_dir(worktree_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        return super::process_group::spawn_leader(&mut command)
            .map_err(|error| format!("failed to spawn test acp program: {error}"));
    }

    #[cfg(test)]
    {
        let override_command = TEST_ACP_COMMAND.lock().unwrap().clone();
        if let Some((program, mut args)) = override_command {
            args.extend(TEST_ACP_EXTRA_ARGS_GLOBAL.lock().unwrap().iter().cloned());
            let mut command = std::process::Command::new(program);
            command.args(args);
            if let Some(launch) = acp_launch_for_agent(agent) {
                if launch.model_pins_at_spawn() {
                    if let Some(spawn_model) = acp_spawn_model_for_argv(launch, model) {
                        command.arg("--model").arg(spawn_model);
                    }
                }
            }
            command
                .env(
                    "FAKE_ACP_STATE_DIR",
                    fake_acp_state_dir(worktree_path).as_os_str(),
                )
                .current_dir(worktree_path)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            return super::process_group::spawn_leader(&mut command)
                .map_err(|error| format!("failed to spawn test acp command: {error}"));
        }
    }

    let Some(launch) = acp_launch_for_agent(agent) else {
        return Err(format!("no ACP mapping for agent {agent:?}"));
    };
    for (program, base_args) in acp_candidates(launch) {
        let base_args: Vec<&str> = base_args.iter().map(String::as_str).collect();
        let args = acp_args_for_program(launch, &base_args, model);
        let mut command = match crate::adapters::program::sandboxed_harness_command(
            &program,
            &args,
            worktree_path,
        ) {
            Ok(command) => command,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
        };
        command
            .current_dir(worktree_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(shim) = shim_spawn(&command).map_err(|error| error.to_string())? {
            return shim
                .into_child_with_pipes()
                .map_err(|error| error.to_string());
        }
        if let Ok(child) = super::process_group::spawn_leader(&mut command) {
            return Ok(child);
        }
    }
    Err(format!(
        "{agent:?} ACP agent is not installed — {}",
        launch.install_hint
    ))
}

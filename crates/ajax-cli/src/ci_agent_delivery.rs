use crate::agent_runtime::{task_file_stem, AgentRuntimeSnapshot, AgentRuntimeState};
use ajax_core::{
    adapters::{CommandRunner, CommandSpec, TmuxAdapter},
    agent_notification::{AgentNotification, AgentNotificationDeliveryStatus},
    models::{AgentClient, Task},
};
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_WRAPPER_AGE_MILLIS: u128 = 10_000;

pub(crate) fn deliver(
    cache_dir: &Path,
    runner: &mut impl CommandRunner,
    task: &Task,
    notification: &AgentNotification,
) -> Result<AgentNotificationDeliveryStatus, String> {
    let expected = expected_process(task.selected_agent)?;
    let path = cache_dir
        .join("agent-runtime")
        .join(format!("{}.json", task_file_stem(task.id.as_str())));
    let snapshot: AgentRuntimeSnapshot = serde_json::from_str(
        &fs::read_to_string(path)
            .map_err(|_| "fresh agent wrapper evidence unavailable".to_string())?,
    )
    .map_err(|_| "agent wrapper evidence is invalid".to_string())?;
    validate_snapshot(task, &snapshot)?;
    let pid = snapshot.pid.expect("validated pid");
    let process = run_stdout(
        runner,
        &CommandSpec::new("ps", ["-p", &pid.to_string(), "-o", "comm="]),
    )?;
    if process_name(&process) != expected {
        return Err(format!(
            "expected live {expected} process, observed {}",
            process.trim()
        ));
    }
    let target = format!("{}:{}", task.tmux_session, task.task_window);
    let foreground = run_stdout(
        runner,
        &CommandSpec::new(
            "tmux",
            [
                "display-message",
                "-p",
                "-t",
                &target,
                "#{pane_current_command}",
            ],
        ),
    )?;
    if process_name(&foreground) != expected {
        return Err(format!(
            "task window foreground is {}, not {expected}; notification retained",
            foreground.trim()
        ));
    }
    let output = runner
        .run(&TmuxAdapter::new("tmux").send_agent_command(
            &task.tmux_session,
            &task.task_window,
            &notification.prompt(),
        ))
        .map_err(|error| error.to_string())?;
    if output.status_code != 0 {
        return Err(format!("tmux send-keys failed: {}", output.stderr.trim()));
    }
    Ok(AgentNotificationDeliveryStatus::Accepted)
}

fn validate_snapshot(task: &Task, snapshot: &AgentRuntimeSnapshot) -> Result<(), String> {
    if snapshot.task_id != task.id.as_str()
        || snapshot.state != AgentRuntimeState::Running
        || snapshot.pid.is_none()
    {
        return Err("agent wrapper does not confirm the expected running task".to_string());
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis();
    if now.saturating_sub(snapshot.observed_at_unix_millis) > MAX_WRAPPER_AGE_MILLIS {
        return Err("agent wrapper evidence is stale; notification retained".to_string());
    }
    Ok(())
}

fn expected_process(agent: AgentClient) -> Result<&'static str, String> {
    match agent {
        AgentClient::Claude => Ok("claude"),
        AgentClient::Codex => Ok("codex"),
        AgentClient::Cursor => Ok("cursor"),
        AgentClient::Pi => Ok("pi"),
        AgentClient::Other => Err("selected agent process cannot be validated".to_string()),
    }
}

fn process_name(value: &str) -> &str {
    Path::new(value.trim())
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
}

fn run_stdout(runner: &mut impl CommandRunner, command: &CommandSpec) -> Result<String, String> {
    let output = runner.run(command).map_err(|error| error.to_string())?;
    if output.status_code == 0 {
        Ok(output.stdout)
    } else {
        Err(format!(
            "{} failed: {}",
            command.program,
            output.stderr.trim()
        ))
    }
}

use crate::agent_runtime::{task_file_stem, AgentRuntimeSnapshot, AgentRuntimeState};
use ajax_core::{
    adapters::{CommandRunner, CommandSpec, TmuxAdapter},
    agent_notification::{AgentNotification, AgentNotificationDeliveryStatus},
    models::{AgentClient, LiveStatusKind, Task},
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
    // Refuse only explicit blockers where the operator is being asked
    // something or the agent cannot accept input. An idle composer after a
    // completed turn projects as WaitingForInput / Waiting / NeedsInput, and
    // that is exactly when a premature-stop nudge must be delivered.
    if matches!(notification, AgentNotification::WatcherNudge { .. })
        && task.live_status.as_ref().is_some_and(|live| {
            matches!(
                live.kind,
                LiveStatusKind::WaitingForApproval
                    | LiveStatusKind::AuthRequired
                    | LiveStatusKind::ContextLimit
                    | LiveStatusKind::RateLimited
            )
        })
    {
        return Err("agent is blocked by an explicit blocker; watcher nudge retained".into());
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use ajax_core::adapters::{CommandOutput, CommandRunError};
    use ajax_core::agent_watcher::{nudge_prompt, WatcherReason};
    use ajax_core::models::TaskId;
    use std::sync::atomic::{AtomicU64, Ordering};

    static DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct FakeRunner {
        process_name: String,
        foreground: String,
        ps_ok: bool,
        display_ok: bool,
        send_ok: bool,
        commands: Vec<CommandSpec>,
    }

    impl FakeRunner {
        fn healthy() -> Self {
            Self {
                process_name: "claude".to_string(),
                foreground: "claude".to_string(),
                ps_ok: true,
                display_ok: true,
                send_ok: true,
                commands: Vec::new(),
            }
        }

        fn is_send_keys(command: &CommandSpec) -> bool {
            command.program == "tmux" && command.args.first().is_some_and(|arg| arg == "send-keys")
        }
    }

    impl CommandRunner for FakeRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.commands.push(command.clone());
            let (status_code, stdout, stderr) = if command.program == "ps" {
                if self.ps_ok {
                    (0, self.process_name.clone(), String::new())
                } else {
                    (1, String::new(), "no such process".to_string())
                }
            } else if Self::is_send_keys(command) {
                if self.send_ok {
                    (0, String::new(), String::new())
                } else {
                    (1, String::new(), "send failed".to_string())
                }
            } else if command.program == "tmux"
                && command
                    .args
                    .first()
                    .is_some_and(|arg| arg == "display-message")
            {
                if self.display_ok {
                    (0, self.foreground.clone(), String::new())
                } else {
                    (1, String::new(), "can't find pane".to_string())
                }
            } else {
                (0, String::new(), String::new())
            };
            Ok(CommandOutput {
                status_code,
                stdout,
                stderr,
            })
        }
    }

    fn task() -> Task {
        Task::new(
            TaskId::new("task-1"),
            "web",
            "t1",
            "title",
            "branch",
            "main",
            "/tmp/wt",
            "web-t1",
            "task",
            AgentClient::Claude,
        )
    }

    fn nudge() -> AgentNotification {
        AgentNotification::WatcherNudge {
            id: "watcher-nudge:task-1:1".to_string(),
            task_id: TaskId::new("task-1"),
            reason: WatcherReason::Stuck,
        }
    }

    fn scratch_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ajax-cli-watcher-nudge-{}-{}",
            std::process::id(),
            DIR_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_snapshot(dir: &Path, observed_at_ms: i64) {
        let snapshot = AgentRuntimeSnapshot {
            task_id: "task-1".to_string(),
            state: AgentRuntimeState::Running,
            observed_at_unix_millis: observed_at_ms as u128,
            pid: Some(4242),
            exit_code: None,
            message: None,
        };
        let runtime_dir = dir.join("agent-runtime");
        std::fs::create_dir_all(&runtime_dir).unwrap();
        std::fs::write(
            runtime_dir.join("task-1.json"),
            serde_json::to_string(&snapshot).unwrap(),
        )
        .unwrap();
    }

    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }

    fn deliver(
        dir: &Path,
        runner: FakeRunner,
    ) -> (
        Result<AgentNotificationDeliveryStatus, String>,
        Vec<CommandSpec>,
    ) {
        let task = task();
        let mut runner = runner;
        let result = super::deliver(dir, &mut runner, &task, &nudge());
        (result, runner.commands)
    }

    fn assert_no_send_keys(commands: &[CommandSpec]) {
        assert!(
            !commands.iter().any(FakeRunner::is_send_keys),
            "send-keys must not run: {commands:?}"
        );
    }

    #[test]
    fn watcher_nudge_is_refused_when_snapshot_is_missing() {
        let dir = scratch_dir();
        let (result, commands) = deliver(&dir, FakeRunner::healthy());
        assert!(result.is_err());
        assert_no_send_keys(&commands);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn watcher_nudge_is_refused_when_snapshot_is_stale() {
        let dir = scratch_dir();
        write_snapshot(&dir, now_ms() - MAX_WRAPPER_AGE_MILLIS as i64 - 30_000);
        let (result, commands) = deliver(&dir, FakeRunner::healthy());
        assert!(result.is_err());
        assert_no_send_keys(&commands);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn watcher_nudge_is_refused_when_process_is_not_the_agent() {
        let dir = scratch_dir();
        write_snapshot(&dir, now_ms());
        let mut runner = FakeRunner::healthy();
        runner.process_name = "vim".to_string();
        let (result, commands) = deliver(&dir, runner);
        assert!(result.is_err());
        assert_no_send_keys(&commands);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn watcher_nudge_is_refused_when_pane_foreground_is_not_the_agent() {
        let dir = scratch_dir();
        write_snapshot(&dir, now_ms());
        let mut runner = FakeRunner::healthy();
        runner.foreground = "bash".to_string();
        let (result, commands) = deliver(&dir, runner);
        assert!(result.is_err());
        assert_no_send_keys(&commands);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn watcher_nudge_sends_the_nudge_prompt_when_all_validation_passes() {
        let dir = scratch_dir();
        write_snapshot(&dir, now_ms());
        let (result, commands) = deliver(&dir, FakeRunner::healthy());
        assert_eq!(
            result.as_ref().ok(),
            Some(&AgentNotificationDeliveryStatus::Accepted)
        );
        let send = commands
            .iter()
            .find(|command| FakeRunner::is_send_keys(command))
            .expect("send-keys should run");
        assert_eq!(send.args.first(), Some(&"send-keys".to_string()));
        assert_eq!(
            send.args.get(3),
            Some(&nudge_prompt(&WatcherReason::Stuck).to_string())
        );
        assert_eq!(send.args.get(4), Some(&"Enter".to_string()));
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn watcher_waits_for_operator_without_typing_or_dropping_nudge() {
        use ajax_core::agent_notification::{record_delivery_for, AgentNotificationDelivery};
        use ajax_core::agent_watcher::{
            enqueue_watcher_nudge, load_store, pending_watcher_nudge, MAX_DELIVERY_ATTEMPTS,
        };
        use ajax_core::models::{LiveObservation, LiveStatusKind};
        let dir = scratch_dir();
        write_snapshot(&dir, now_ms());
        for kind in [
            LiveStatusKind::WaitingForApproval,
            LiveStatusKind::AuthRequired,
            LiveStatusKind::ContextLimit,
            LiveStatusKind::RateLimited,
        ] {
            let mut task = task();
            task.live_status = Some(LiveObservation::new(kind, "operator input required"));
            enqueue_watcher_nudge(&mut task, nudge().id(), WatcherReason::Stuck);
            let mut runner = FakeRunner::healthy();
            let result = super::deliver(&dir, &mut runner, &task, &nudge());
            assert!(result.is_err());
            assert_no_send_keys(&runner.commands);
            // The refused attempt is recorded against the mutable task: the
            // nudge stays pending and one refused attempt cannot reach the
            // drop cap.
            assert!(record_delivery_for(
                &mut task,
                &nudge(),
                AgentNotificationDelivery {
                    notification_id: nudge().id().to_string(),
                    status: AgentNotificationDeliveryStatus::Error,
                    detail: result.err(),
                }
            ));
            assert_eq!(pending_watcher_nudge(&task), Some(nudge()));
            let attempts = load_store(&task)
                .pending_nudge
                .expect("nudge retained")
                .delivery_attempts;
            assert!(attempts < MAX_DELIVERY_ATTEMPTS);
            let ci = AgentNotification::CiFailed {
                episode_id: "ci-1".into(),
                task_id: task.id.clone(),
                pr_number: 1,
                head_sha: "abc".into(),
                failed_checks: Vec::new(),
            };
            assert_eq!(
                super::deliver(&dir, &mut runner, &task, &ci),
                Ok(AgentNotificationDeliveryStatus::Accepted)
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn idle_composer_watcher_nudge_is_delivered_despite_waiting_projection() {
        use ajax_core::models::{AgentRuntimeStatus, LiveObservation, LiveStatusKind, SideFlag};
        let dir = scratch_dir();
        write_snapshot(&dir, now_ms());
        let mut task = task();
        // The status pipeline projects an agent idle at its composer after a
        // completed turn as WaitingForInput / Waiting / NeedsInput; a
        // premature-stop nudge must still be delivered there.
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::WaitingForInput,
            "agent idle at composer",
        ));
        task.agent_status = AgentRuntimeStatus::Waiting;
        task.add_side_flag(SideFlag::NeedsInput);
        let mut runner = FakeRunner::healthy();
        let result = super::deliver(&dir, &mut runner, &task, &nudge());
        assert_eq!(
            result.as_ref().ok(),
            Some(&AgentNotificationDeliveryStatus::Accepted)
        );
        let send = runner
            .commands
            .iter()
            .find(|command| FakeRunner::is_send_keys(command))
            .expect("send-keys should run");
        assert_eq!(
            send.args.get(3),
            Some(&nudge_prompt(&WatcherReason::Stuck).to_string())
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use ajax_core::events::MonitorEvent;
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
};

use crate::{
    agent::{codex::CodexAdapter, cursor::CursorAdapter},
    event_log::EventLog,
    process_observer::supervise_process_with_cancellation,
    repo_observer, SupervisorError,
};

const GIT_SNAPSHOT_MIN_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SupervisorAgent {
    #[default]
    Codex,
    Cursor,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MonitorConfig {
    pub agent: SupervisorAgent,
    pub agent_bin: String,
    pub prompt: String,
    pub worktree_path: Option<PathBuf>,
    pub channel_capacity: usize,
    pub watch_filesystem: bool,
    pub git_snapshots: GitSnapshotPolicy,
    pub hang_after: Option<Duration>,
    pub event_log_path: Option<PathBuf>,
}

impl MonitorConfig {
    pub fn codex_exec(prompt: impl Into<String>) -> Self {
        Self {
            agent: SupervisorAgent::Codex,
            agent_bin: "codex".to_string(),
            prompt: prompt.into(),
            worktree_path: None,
            channel_capacity: 1024,
            watch_filesystem: false,
            git_snapshots: GitSnapshotPolicy::Disabled,
            hang_after: None,
            event_log_path: None,
        }
    }

    pub fn cursor_exec(prompt: impl Into<String>) -> Self {
        Self {
            agent: SupervisorAgent::Cursor,
            agent_bin: "cursor".to_string(),
            prompt: prompt.into(),
            worktree_path: None,
            channel_capacity: 1024,
            watch_filesystem: false,
            git_snapshots: GitSnapshotPolicy::Disabled,
            hang_after: None,
            event_log_path: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitSnapshotPolicy {
    Disabled,
    OnStartAndExit,
    OnFileChange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MonitorExit {
    pub status_code: Option<i32>,
}

#[derive(Debug)]
pub struct MonitorHandle {
    join: JoinHandle<Result<MonitorExit, SupervisorError>>,
    event_forwarder: JoinHandle<Result<(), SupervisorError>>,
    cancel: watch::Sender<bool>,
}

impl MonitorHandle {
    pub fn cancel(&self) {
        let _ = self.cancel.send(true);
    }

    pub async fn wait(self) -> Result<MonitorExit, SupervisorError> {
        let monitor_result = self
            .join
            .await
            .map_err(|error| SupervisorError::Process(error.to_string()))?;
        self.event_forwarder
            .await
            .map_err(|error| SupervisorError::Process(error.to_string()))??;
        monitor_result
    }
}

pub fn spawn_monitor(
    config: MonitorConfig,
) -> Result<(MonitorHandle, mpsc::Receiver<MonitorEvent>), SupervisorError> {
    let capacity = config.channel_capacity.max(1);
    let (events, receiver) = mpsc::channel(capacity);
    let (raw_events, raw_receiver) = mpsc::channel(capacity);
    let event_log = config.event_log_path.clone().map(EventLog::new);
    let (cancel, cancel_rx) = watch::channel(false);
    let event_forwarder = tokio::spawn(forward_monitor_events(
        raw_receiver,
        events,
        event_log,
        cancel_rx.clone(),
    ));
    let join = tokio::spawn(async move {
        let (watcher, watcher_forwarder) = start_watcher(&config, &raw_events)?;
        if config.git_snapshots == GitSnapshotPolicy::OnStartAndExit {
            let worktree_path = config.worktree_path.as_ref().ok_or_else(|| {
                SupervisorError::Process("worktree path is required for git snapshots".to_string())
            })?;
            repo_observer::send_git_snapshot(&raw_events, worktree_path).await?;
        }

        let codex = CodexAdapter::new(config.agent_bin.clone());
        let cursor = CursorAdapter::new(config.agent_bin);
        let result = match config.agent {
            SupervisorAgent::Codex => {
                supervise_process_with_cancellation(
                    &codex,
                    &config.prompt,
                    raw_events.clone(),
                    config.hang_after,
                    cancel_rx,
                )
                .await
            }
            SupervisorAgent::Cursor => {
                supervise_process_with_cancellation(
                    &cursor,
                    &config.prompt,
                    raw_events.clone(),
                    config.hang_after,
                    cancel_rx,
                )
                .await
            }
        };

        if config.git_snapshots == GitSnapshotPolicy::OnStartAndExit {
            let worktree_path = config.worktree_path.as_ref().ok_or_else(|| {
                SupervisorError::Process("worktree path is required for git snapshots".to_string())
            })?;
            repo_observer::send_git_snapshot(&raw_events, worktree_path).await?;
        }

        let status_code = result?;
        drop(watcher);
        if let Some(watcher_forwarder) = watcher_forwarder {
            watcher_forwarder
                .await
                .map_err(|error| SupervisorError::Process(error.to_string()))?;
        }
        Ok(MonitorExit { status_code })
    });

    Ok((
        MonitorHandle {
            join,
            event_forwarder,
            cancel,
        },
        receiver,
    ))
}

async fn forward_monitor_events(
    mut raw_receiver: mpsc::Receiver<MonitorEvent>,
    events: mpsc::Sender<MonitorEvent>,
    event_log: Option<EventLog>,
    mut cancel: watch::Receiver<bool>,
) -> Result<(), SupervisorError> {
    loop {
        if *cancel.borrow() {
            break;
        }

        tokio::select! {
            biased;

            changed = cancel.changed() => match changed {
                Ok(()) if *cancel.borrow() => break,
                Ok(()) => continue,
                Err(_) => break,
            },
            event = raw_receiver.recv() => {
                let Some(event) = event else {
                    break;
                };
                if let Some(event_log) = &event_log {
                    if let Err(error) = event_log.append(&event) {
                        eprintln!("ajax supervisor: failed to append event log: {error}");
                    }
                }
                if events.send(event).await.is_err() {
                    break;
                }
            }
        }
    }

    Ok(())
}

fn start_watcher(
    config: &MonitorConfig,
    events: &mpsc::Sender<MonitorEvent>,
) -> Result<(Option<notify::RecommendedWatcher>, Option<JoinHandle<()>>), SupervisorError> {
    if !config.watch_filesystem {
        return Ok((None, None));
    }

    let worktree_path = config.worktree_path.as_ref().ok_or_else(|| {
        SupervisorError::Process("worktree path is required to watch filesystem".to_string())
    })?;
    let (notify_sender, notify_receiver) = std::sync::mpsc::channel();
    let watcher = repo_observer::watch_repo(worktree_path, notify_sender)?;
    let repo_events = events.clone();
    let git_snapshot_policy = config.git_snapshots;
    let git_snapshot_worktree = config.worktree_path.clone();
    let runtime = tokio::runtime::Handle::current();
    let forwarder = tokio::task::spawn_blocking(move || {
        let mut last_git_snapshot = None;
        while let Ok(event) = notify_receiver.recv() {
            let Ok(event) = event else {
                continue;
            };
            let monitor_events = repo_observer::notify_event_to_monitor_events(event);
            if monitor_events.is_empty() {
                continue;
            }
            for monitor_event in monitor_events {
                if repo_events.blocking_send(monitor_event).is_err() {
                    return;
                }
            }
            if git_snapshot_policy == GitSnapshotPolicy::OnFileChange
                && should_snapshot_file_change(
                    &mut last_git_snapshot,
                    Instant::now(),
                    GIT_SNAPSHOT_MIN_INTERVAL,
                )
            {
                if let Some(worktree_path) = git_snapshot_worktree.clone() {
                    let events = repo_events.clone();
                    runtime.spawn(async move {
                        let _ = repo_observer::send_git_snapshot(&events, &worktree_path).await;
                    });
                }
            }
        }
    });

    Ok((Some(watcher), Some(forwarder)))
}

fn should_snapshot_file_change(
    last_snapshot: &mut Option<Instant>,
    now: Instant,
    min_interval: Duration,
) -> bool {
    if last_snapshot.is_some_and(|last| now.duration_since(last) < min_interval) {
        return false;
    }

    *last_snapshot = Some(now);
    true
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt, time::Duration};

    use ajax_core::events::{AgentEvent, MonitorEvent, ProcessEvent};
    use tokio::sync::{mpsc, watch};

    use super::{
        should_snapshot_file_change, spawn_monitor, GitSnapshotPolicy, MonitorConfig,
        SupervisorAgent, GIT_SNAPSHOT_MIN_INTERVAL,
    };

    #[test]
    fn monitor_config_builds_codex_exec_defaults() {
        let config = MonitorConfig::codex_exec("fix tests");

        assert_eq!(config.agent, SupervisorAgent::Codex);
        assert_eq!(config.agent_bin, "codex");
        assert_eq!(config.prompt, "fix tests");
        assert_eq!(config.worktree_path, None);
        assert_eq!(config.channel_capacity, 1024);
        assert!(!config.watch_filesystem);
        assert_eq!(config.git_snapshots, GitSnapshotPolicy::Disabled);
        assert_eq!(config.hang_after, None);
        assert_eq!(config.event_log_path, None);
    }

    #[test]
    fn monitor_config_builds_cursor_exec_defaults() {
        let config = MonitorConfig::cursor_exec("fix tests");

        assert_eq!(config.agent, SupervisorAgent::Cursor);
        assert_eq!(config.agent_bin, "cursor");
        assert_eq!(config.prompt, "fix tests");
    }

    #[test]
    fn git_snapshot_gate_coalesces_rapid_file_changes() {
        let start = std::time::Instant::now();
        let mut last_snapshot = None;

        assert!(should_snapshot_file_change(
            &mut last_snapshot,
            start,
            GIT_SNAPSHOT_MIN_INTERVAL
        ));
        assert!(!should_snapshot_file_change(
            &mut last_snapshot,
            start + GIT_SNAPSHOT_MIN_INTERVAL / 2,
            GIT_SNAPSHOT_MIN_INTERVAL
        ));
        assert!(should_snapshot_file_change(
            &mut last_snapshot,
            start + GIT_SNAPSHOT_MIN_INTERVAL,
            GIT_SNAPSHOT_MIN_INTERVAL
        ));
    }

    #[tokio::test]
    async fn runtime_spawn_monitor_streams_codex_process_events() {
        let script =
            std::env::temp_dir().join(format!("ajax-runtime-codex-{}", std::process::id()));
        fs::write(
            &script,
            "#!/bin/sh\nprintf '{\"type\":\"started\"}\\n'\nprintf '{\"type\":\"approval_request\",\"command\":\"cargo test\"}\\n'\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();

        let mut config = MonitorConfig::codex_exec("ignored");
        config.agent_bin = script.display().to_string();
        config.channel_capacity = 16;
        let (handle, mut receiver) = spawn_monitor(config).expect("monitor should spawn");

        let exit = handle.wait().await.expect("monitor should complete");
        let mut events = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            events.push(event);
        }

        assert_eq!(exit.status_code, Some(0));
        assert!(events.contains(&MonitorEvent::Agent(AgentEvent::Started {
            agent: "codex".to_string()
        })));
        assert!(
            events.contains(&MonitorEvent::Agent(AgentEvent::WaitingForApproval {
                command: Some("cargo test".to_string())
            }))
        );
        assert!(
            events.contains(&MonitorEvent::Process(ProcessEvent::Exited {
                code: Some(0)
            }))
        );

        let _ = fs::remove_file(script);
    }

    #[tokio::test]
    async fn runtime_cancel_closes_monitor_receiver() {
        let script =
            std::env::temp_dir().join(format!("ajax-runtime-cancel-{}", std::process::id()));
        fs::write(
            &script,
            "#!/bin/sh\nprintf '{\"type\":\"started\"}\\n'\nsleep 10\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();

        let mut config = MonitorConfig::codex_exec("ignored");
        config.agent_bin = script.display().to_string();
        let (handle, mut receiver) = spawn_monitor(config).expect("monitor should spawn");

        let started = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("process should start")
            .expect("monitor should emit process start");
        assert!(matches!(
            started,
            MonitorEvent::Process(ProcessEvent::Started { .. })
        ));
        handle.cancel();

        assert!(
            tokio::time::timeout(Duration::from_secs(2), receiver.recv())
                .await
                .expect("receiver should close after cancellation")
                .is_none()
        );

        let _ = handle.wait().await;
        let _ = fs::remove_file(script);
    }

    #[tokio::test]
    async fn event_forwarder_closes_output_when_monitor_is_cancelled() {
        let (raw_events, raw_receiver) = mpsc::channel(4);
        let (events, mut receiver) = mpsc::channel(4);
        let (cancel, cancel_rx) = watch::channel(false);
        let forwarder = tokio::spawn(super::forward_monitor_events(
            raw_receiver,
            events,
            None,
            cancel_rx,
        ));

        raw_events
            .send(MonitorEvent::Process(ProcessEvent::Started {
                pid: Some(123),
            }))
            .await
            .expect("raw event receiver should be open");
        assert!(matches!(
            receiver.recv().await,
            Some(MonitorEvent::Process(ProcessEvent::Started {
                pid: Some(123)
            }))
        ));

        cancel.send(true).expect("cancel receiver should be open");

        assert!(
            tokio::time::timeout(Duration::from_secs(2), receiver.recv())
                .await
                .expect("receiver should close after cancellation")
                .is_none()
        );
        forwarder
            .await
            .expect("forwarder should join")
            .expect("forwarder should not fail");
        drop(raw_events);
    }

    #[tokio::test]
    async fn runtime_continues_when_event_log_append_fails() {
        let root =
            std::env::temp_dir().join(format!("ajax-runtime-bad-log-{}", std::process::id()));
        let script = root.join("fake-codex");
        fs::create_dir_all(&root).unwrap();
        fs::write(&script, "#!/bin/sh\nprintf '{\"type\":\"started\"}\\n'\n").unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();

        let bad_log_parent = root.join("monitor");
        fs::write(&bad_log_parent, b"not-a-directory").unwrap();
        let bad_log_path = bad_log_parent.join("events.jsonl");

        let mut config = MonitorConfig::codex_exec("ignored");
        config.agent_bin = script.display().to_string();
        config.event_log_path = Some(bad_log_path);
        let (handle, mut receiver) = spawn_monitor(config).expect("monitor should spawn");

        let exit = handle
            .wait()
            .await
            .expect("monitor should complete even when event log append fails");
        assert_eq!(exit.status_code, Some(0));

        let mut streamed = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            streamed.push(event);
        }
        assert!(streamed.contains(&MonitorEvent::Agent(AgentEvent::Started {
            agent: "codex".to_string()
        })));

        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn runtime_writes_optional_event_log() {
        let root = std::env::temp_dir().join(format!("ajax-runtime-log-{}", std::process::id()));
        let script = root.join("fake-codex");
        fs::create_dir_all(&root).unwrap();
        fs::write(&script, "#!/bin/sh\nprintf '{\"type\":\"started\"}\\n'\n").unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();

        let mut config = MonitorConfig::codex_exec("ignored");
        config.agent_bin = script.display().to_string();
        config.event_log_path = Some(root.join("monitor/events.jsonl"));
        let (handle, mut receiver) = spawn_monitor(config.clone()).expect("monitor should spawn");

        handle.wait().await.expect("monitor should complete");
        let mut streamed = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            streamed.push(event);
        }
        let contents = fs::read_to_string(config.event_log_path.unwrap()).unwrap();

        assert!(streamed.contains(&MonitorEvent::Agent(AgentEvent::Started {
            agent: "codex".to_string()
        })));
        assert!(contents.contains(r#""Started":{"agent":"codex"}"#));
        assert!(contents.contains(r#""Exited":{"code":0}"#));

        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn runtime_spawn_monitor_streams_cursor_process_events() {
        let script =
            std::env::temp_dir().join(format!("ajax-runtime-cursor-{}", std::process::id()));
        fs::write(
            &script,
            "#!/bin/sh\nprintf '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"abc\"}\\n'\nprintf '{\"type\":\"tool_call\",\"subtype\":\"started\",\"call_id\":\"1\",\"tool_call\":{\"readToolCall\":{\"args\":{\"path\":\"README.md\"}}}}\\n'\nprintf '{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"Approval required to run cargo test\"}]}}\\n'\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();

        let mut config = MonitorConfig::cursor_exec("ignored");
        config.agent_bin = script.display().to_string();
        config.channel_capacity = 16;
        let (handle, mut receiver) = spawn_monitor(config).expect("monitor should spawn");

        let exit = handle.wait().await.expect("monitor should complete");
        let mut events = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            events.push(event);
        }

        assert_eq!(exit.status_code, Some(0));
        assert!(events.contains(&MonitorEvent::Agent(AgentEvent::Started {
            agent: "cursor".to_string()
        })));
        assert!(events.contains(&MonitorEvent::Agent(AgentEvent::ToolCall {
            name: "read README.md".to_string()
        })));
        assert!(
            events.contains(&MonitorEvent::Agent(AgentEvent::WaitingForApproval {
                command: None
            }))
        );
        assert!(
            events.contains(&MonitorEvent::Process(ProcessEvent::Exited {
                code: Some(0)
            }))
        );

        let _ = fs::remove_file(script);
    }

    #[tokio::test]
    async fn runtime_cursor_shell_test_commands_reduce_to_tests_running() {
        let script =
            std::env::temp_dir().join(format!("ajax-runtime-cursor-tests-{}", std::process::id()));
        fs::write(
            &script,
            "#!/bin/sh\nprintf '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"abc\"}\\n'\nprintf '{\"type\":\"tool_call\",\"call_id\":\"1\",\"name\":\"shell\",\"status\":\"running\",\"args\":{\"command\":\"cargo nextest run --all-features\"}}\\n'\nprintf '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"done\"}\\n'\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();

        let mut config = MonitorConfig::cursor_exec("ignored");
        config.agent_bin = script.display().to_string();
        config.channel_capacity = 16;
        let (handle, mut receiver) = spawn_monitor(config).expect("monitor should spawn");

        let exit = handle.wait().await.expect("monitor should complete");
        let mut machine = crate::SupervisorStatusMachine::default();
        let mut observations = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            if let Some(observation) = machine.apply(&event).cloned() {
                observations.push(observation.kind);
            }
        }

        assert_eq!(exit.status_code, Some(0));
        assert!(observations.contains(&ajax_core::models::LiveStatusKind::TestsRunning));
        assert_eq!(
            observations.last().copied(),
            Some(ajax_core::models::LiveStatusKind::Done)
        );

        let _ = fs::remove_file(script);
    }

    #[tokio::test]
    async fn runtime_cursor_fake_run_reduces_full_supervisor_lifecycle() {
        let script = std::env::temp_dir().join(format!(
            "ajax-runtime-cursor-lifecycle-{}",
            std::process::id()
        ));
        fs::write(
            &script,
            "#!/bin/sh\n\
printf '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"abc\"}\\n'\n\
printf '{\"type\":\"thinking\",\"text\":\"planning\"}\\n'\n\
printf '{\"type\":\"tool_call\",\"subtype\":\"started\",\"call_id\":\"1\",\"tool_call\":{\"readToolCall\":{\"args\":{\"path\":\"README.md\"}}}}\\n'\n\
printf '{\"type\":\"tool_call\",\"call_id\":\"2\",\"name\":\"shell\",\"status\":\"running\",\"args\":{\"command\":\"cargo nextest run --all-features\"}}\\n'\n\
printf '{\"type\":\"request\",\"request_id\":\"req-1\",\"message\":\"Approve to proceed?\"}\\n'\n\
printf '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"done\"}\\n'\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();

        let mut config = MonitorConfig::cursor_exec("ignored");
        config.agent_bin = script.display().to_string();
        config.channel_capacity = 32;
        let (handle, mut receiver) = spawn_monitor(config).expect("monitor should spawn");

        let exit = handle.wait().await.expect("monitor should complete");
        let mut machine = crate::SupervisorStatusMachine::default();
        let mut observations = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            if let Some(observation) = machine.apply(&event).cloned() {
                observations.push(observation.kind);
            }
        }

        assert_eq!(exit.status_code, Some(0));
        assert!(observations.contains(&ajax_core::models::LiveStatusKind::AgentRunning));
        assert!(observations.contains(&ajax_core::models::LiveStatusKind::TestsRunning));
        assert!(observations.contains(&ajax_core::models::LiveStatusKind::WaitingForApproval));
        assert_eq!(
            observations.last().copied(),
            Some(ajax_core::models::LiveStatusKind::Done)
        );

        let _ = fs::remove_file(script);
    }

    #[tokio::test]
    async fn runtime_cursor_hung_process_reduces_to_blocked() {
        let script =
            std::env::temp_dir().join(format!("ajax-runtime-cursor-hung-{}", std::process::id()));
        fs::write(
            &script,
            "#!/bin/sh\nprintf '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"abc\"}\\n'\nsleep 5\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();

        let mut config = MonitorConfig::cursor_exec("ignored");
        config.agent_bin = script.display().to_string();
        config.hang_after = Some(Duration::from_millis(100));
        config.channel_capacity = 32;
        let (handle, mut receiver) = spawn_monitor(config).expect("monitor should spawn");

        let mut machine = crate::SupervisorStatusMachine::default();
        let mut saw_blocked = false;
        while let Some(event) = tokio::time::timeout(Duration::from_secs(3), receiver.recv())
            .await
            .expect("cursor hung event should arrive before timeout")
        {
            if machine.apply(&event).is_some_and(|observation| {
                observation.kind == ajax_core::models::LiveStatusKind::Blocked
            }) {
                saw_blocked = true;
                break;
            }
        }

        assert!(saw_blocked);
        let _ = handle.wait().await;
        let _ = fs::remove_file(script);
    }
}

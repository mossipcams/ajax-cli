use std::{path::PathBuf, time::Duration};

use ajax_core::events::MonitorEvent;
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
};

use crate::{
    agent::codex::CodexAdapter, event_log::EventLog,
    process_observer::supervise_process_with_cancellation, repo_observer, SupervisorError,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MonitorConfig {
    pub codex_bin: String,
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
            codex_bin: "codex".to_string(),
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
        let (_watcher, watcher_forwarder) = start_watcher(&config, &raw_events)?;
        if config.git_snapshots == GitSnapshotPolicy::OnStartAndExit {
            let worktree_path = config.worktree_path.as_ref().ok_or_else(|| {
                SupervisorError::Process("worktree path is required for git snapshots".to_string())
            })?;
            repo_observer::send_git_snapshot(&raw_events, worktree_path).await?;
        }

        let adapter = CodexAdapter::new(config.codex_bin);
        let result = supervise_process_with_cancellation(
            &adapter,
            &config.prompt,
            raw_events.clone(),
            config.hang_after,
            cancel_rx,
        )
        .await;

        if config.git_snapshots == GitSnapshotPolicy::OnStartAndExit {
            let worktree_path = config.worktree_path.as_ref().ok_or_else(|| {
                SupervisorError::Process("worktree path is required for git snapshots".to_string())
            })?;
            repo_observer::send_git_snapshot(&raw_events, worktree_path).await?;
        }

        let status_code = result?;
        drop(watcher_forwarder);
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
                    event_log.append(&event)?;
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
            if git_snapshot_policy == GitSnapshotPolicy::OnFileChange {
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

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt, time::Duration};

    use ajax_core::events::{AgentEvent, MonitorEvent, ProcessEvent};
    use tokio::sync::{mpsc, watch};

    use super::{spawn_monitor, GitSnapshotPolicy, MonitorConfig};

    #[test]
    fn monitor_config_builds_codex_exec_defaults() {
        let config = MonitorConfig::codex_exec("fix tests");

        assert_eq!(config.codex_bin, "codex");
        assert_eq!(config.prompt, "fix tests");
        assert_eq!(config.worktree_path, None);
        assert_eq!(config.channel_capacity, 1024);
        assert!(!config.watch_filesystem);
        assert_eq!(config.git_snapshots, GitSnapshotPolicy::Disabled);
        assert_eq!(config.hang_after, None);
        assert_eq!(config.event_log_path, None);
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
        config.codex_bin = script.display().to_string();
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
        config.codex_bin = script.display().to_string();
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
    async fn runtime_writes_optional_event_log() {
        let root = std::env::temp_dir().join(format!("ajax-runtime-log-{}", std::process::id()));
        let script = root.join("fake-codex");
        fs::create_dir_all(&root).unwrap();
        fs::write(&script, "#!/bin/sh\nprintf '{\"type\":\"started\"}\\n'\n").unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();

        let mut config = MonitorConfig::codex_exec("ignored");
        config.codex_bin = script.display().to_string();
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
}

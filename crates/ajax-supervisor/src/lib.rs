#![deny(unsafe_op_in_unsafe_fn)]

use std::{error::Error, fmt};

pub mod agent;
pub mod event_log;
pub mod process_observer;
pub mod renderer;
pub mod repo_observer;
pub mod runtime;
mod status;

pub use ajax_core::events::{AgentEvent, MonitorEvent, ProcessEvent, RepoEvent};
pub use runtime::{spawn_monitor, GitSnapshotPolicy, MonitorConfig, MonitorExit, MonitorHandle};
pub use status::SupervisorStatusMachine;

#[derive(Debug)]
pub enum SupervisorError {
    Io(String),
    Json(String),
    Notify(String),
    Process(String),
}

impl fmt::Display for SupervisorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message) => write!(formatter, "I/O error: {message}"),
            Self::Json(message) => write!(formatter, "json error: {message}"),
            Self::Notify(message) => write!(formatter, "notify error: {message}"),
            Self::Process(message) => write!(formatter, "process error: {message}"),
        }
    }
}

impl Error for SupervisorError {}

impl From<std::io::Error> for SupervisorError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

impl From<serde_json::Error> for SupervisorError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error.to_string())
    }
}

impl From<notify::Error> for SupervisorError {
    fn from(error: notify::Error) -> Self {
        Self::Notify(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, time::Duration};

    use ajax_core::models::{GitStatus, LiveStatusKind};

    use super::{
        spawn_monitor, AgentEvent, GitSnapshotPolicy, MonitorConfig, MonitorEvent, ProcessEvent,
        RepoEvent, SupervisorError, SupervisorStatusMachine,
    };

    #[test]
    fn public_monitor_config_exposes_optional_event_log_path() {
        let config = MonitorConfig::codex_exec("fix tests");

        assert_eq!(config.event_log_path, None);
    }

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
    }

    #[tokio::test]
    async fn spawn_monitor_returns_receiver_and_join_handle() {
        let config = MonitorConfig {
            codex_bin: "/bin/echo".to_string(),
            prompt: "ignored".to_string(),
            worktree_path: None,
            channel_capacity: 8,
            watch_filesystem: false,
            git_snapshots: GitSnapshotPolicy::Disabled,
            hang_after: Some(Duration::from_secs(30)),
            event_log_path: None,
        };

        let (_handle, receiver) = spawn_monitor(config).expect("monitor should spawn");

        assert!(!receiver.is_closed());

        let _ = _handle.wait().await;
    }

    #[tokio::test]
    async fn spawn_monitor_streams_codex_jsonl_stderr_and_exit_status() {
        let script =
            std::env::temp_dir().join(format!("ajax-monitor-api-codex-{}", std::process::id()));
        fs::write(
            &script,
            "#!/bin/sh\nprintf '{\"type\":\"started\"}\\n'\nprintf '{\"type\":\"approval_request\",\"command\":\"cargo test\"}\\n'\nprintf 'auth warning\\n' >&2\n",
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
        assert!(matches!(
            events.first(),
            Some(MonitorEvent::Process(ProcessEvent::Started { .. }))
        ));
        assert!(events.contains(&MonitorEvent::Agent(AgentEvent::Started {
            agent: "codex".to_string()
        })));
        assert!(
            events.contains(&MonitorEvent::Agent(AgentEvent::WaitingForApproval {
                command: Some("cargo test".to_string())
            }))
        );
        assert!(
            events.contains(&MonitorEvent::Process(ProcessEvent::Stderr {
                line: "auth warning".to_string()
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
    async fn spawn_monitor_watches_worktree_file_changes() {
        let root = std::env::temp_dir().join(format!("ajax-monitor-watch-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let script = root.join("fake-codex");
        fs::write(
            &script,
            "#!/bin/sh\nprintf '{\"type\":\"started\"}\\n'\nsleep 3\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();

        let mut config = MonitorConfig::codex_exec("ignored");
        config.codex_bin = script.display().to_string();
        config.worktree_path = Some(root.clone());
        config.watch_filesystem = true;
        config.channel_capacity = 32;
        let (handle, mut receiver) = spawn_monitor(config).expect("monitor should spawn");

        loop {
            if let MonitorEvent::Process(ProcessEvent::Started { .. }) =
                tokio::time::timeout(Duration::from_secs(2), receiver.recv())
                    .await
                    .expect("process should start before timeout")
                    .expect("monitor should keep sending events")
            {
                break;
            }
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
        let changed_path = root.join("src.rs");
        fs::write(&changed_path, "fn main() {}\n").unwrap();

        let mut saw_file_change = false;
        while let Some(event) = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("file change should arrive before timeout")
        {
            if matches!(
                event,
                MonitorEvent::Repo(RepoEvent::FileChanged { ref path }) if path.ends_with("src.rs")
            ) {
                saw_file_change = true;
                break;
            }
        }

        assert!(saw_file_change);
        let _ = handle.wait().await;
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn watched_monitor_receiver_closes_after_process_exit_before_wait() {
        let root =
            std::env::temp_dir().join(format!("ajax-monitor-watch-close-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let script = root.join("fake-codex");
        fs::write(&script, "#!/bin/sh\nprintf '{\"type\":\"started\"}\\n'\n").unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();

        let mut config = MonitorConfig::codex_exec("ignored");
        config.codex_bin = script.display().to_string();
        config.worktree_path = Some(root.clone());
        config.watch_filesystem = true;
        config.channel_capacity = 32;
        let (handle, mut receiver) = spawn_monitor(config).expect("monitor should spawn");

        let mut events = Vec::new();
        while let Some(event) = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("receiver should close after process exit")
        {
            events.push(event);
        }

        assert!(
            events.contains(&MonitorEvent::Process(ProcessEvent::Exited {
                code: Some(0)
            }))
        );
        assert_eq!(
            handle
                .wait()
                .await
                .expect("monitor should complete")
                .status_code,
            Some(0)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn spawn_monitor_emits_start_and_exit_git_snapshots() {
        let root = std::env::temp_dir().join(format!("ajax-monitor-git-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let git_init = std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .arg("init")
            .output()
            .unwrap();
        assert!(
            git_init.status.success(),
            "git init should succeed: {}",
            String::from_utf8_lossy(&git_init.stderr)
        );
        let script = root.join("fake-codex");
        fs::write(&script, "#!/bin/sh\nprintf '{\"type\":\"started\"}\\n'\n").unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();

        let mut config = MonitorConfig::codex_exec("ignored");
        config.codex_bin = script.display().to_string();
        config.worktree_path = Some(root.clone());
        config.git_snapshots = GitSnapshotPolicy::OnStartAndExit;
        config.channel_capacity = 32;
        let (handle, mut receiver) = spawn_monitor(config).expect("monitor should spawn");

        handle.wait().await.expect("monitor should complete");
        let mut snapshot_count = 0;
        while let Ok(event) = receiver.try_recv() {
            if matches!(
                event,
                MonitorEvent::Repo(RepoEvent::GitSnapshot { ref worktree_path, .. })
                    if worktree_path == &root
            ) {
                snapshot_count += 1;
            }
        }

        assert_eq!(snapshot_count, 2);
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn spawn_monitor_snapshots_git_on_file_change() {
        let root =
            std::env::temp_dir().join(format!("ajax-monitor-git-change-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let git_init = std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .arg("init")
            .output()
            .unwrap();
        assert!(
            git_init.status.success(),
            "git init should succeed: {}",
            String::from_utf8_lossy(&git_init.stderr)
        );
        let script = root.join("fake-codex");
        fs::write(
            &script,
            "#!/bin/sh\nprintf '{\"type\":\"started\"}\\n'\nsleep 3\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();

        let mut config = MonitorConfig::codex_exec("ignored");
        config.codex_bin = script.display().to_string();
        config.worktree_path = Some(root.clone());
        config.watch_filesystem = true;
        config.git_snapshots = GitSnapshotPolicy::OnFileChange;
        config.channel_capacity = 64;
        let (handle, mut receiver) = spawn_monitor(config).expect("monitor should spawn");

        loop {
            if let MonitorEvent::Process(ProcessEvent::Started { .. }) =
                tokio::time::timeout(Duration::from_secs(2), receiver.recv())
                    .await
                    .expect("process should start before timeout")
                    .expect("monitor should keep sending events")
            {
                break;
            }
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
        fs::write(root.join("changed.rs"), "fn changed() {}\n").unwrap();

        let mut saw_snapshot = false;
        for _ in 0..64 {
            let Some(event) = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
                .await
                .expect("git snapshot should arrive before timeout")
            else {
                break;
            };
            if matches!(
                event,
                MonitorEvent::Repo(RepoEvent::GitSnapshot { ref worktree_path, .. })
                    if worktree_path == &root
            ) {
                saw_snapshot = true;
                break;
            }
        }

        assert!(saw_snapshot);
        let _ = handle.wait().await;
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn spawn_monitor_emits_hung_when_process_is_quiet() {
        let root = std::env::temp_dir().join(format!("ajax-monitor-hung-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let script = root.join("fake-codex");
        fs::write(
            &script,
            "#!/bin/sh\nprintf '{\"type\":\"started\"}\\n'\nsleep 1\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();

        let mut config = MonitorConfig::codex_exec("ignored");
        config.codex_bin = script.display().to_string();
        config.hang_after = Some(Duration::from_millis(100));
        config.channel_capacity = 32;
        let (handle, mut receiver) = spawn_monitor(config).expect("monitor should spawn");

        let mut saw_hung = false;
        while let Some(event) = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("hung event should arrive before timeout")
        {
            if matches!(event, MonitorEvent::Process(ProcessEvent::Hung { .. })) {
                saw_hung = true;
                break;
            }
        }

        assert!(saw_hung);
        let _ = handle.wait().await;
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn supervisor_errors_have_operator_facing_display() {
        assert_eq!(
            SupervisorError::Process("codex exited".to_string()).to_string(),
            "process error: codex exited"
        );
        assert_eq!(
            SupervisorError::Json("expected value".to_string()).to_string(),
            "json error: expected value"
        );
    }

    #[test]
    fn supervisor_status_machine_reduces_monitor_event_sequences() {
        let mut status = SupervisorStatusMachine::default();

        status.apply(&MonitorEvent::Agent(AgentEvent::Completed));
        status.apply(&MonitorEvent::Process(ProcessEvent::Stdout {
            line: "late stdout".to_string(),
        }));

        assert_eq!(
            status.observation().map(|observation| observation.kind),
            Some(LiveStatusKind::Done)
        );

        status.apply(&MonitorEvent::Process(ProcessEvent::Exited {
            code: Some(1),
        }));

        assert_eq!(
            status.observation().map(|observation| observation.kind),
            Some(LiveStatusKind::CommandFailed)
        );
    }

    #[test]
    fn supervisor_status_machine_preserves_conflict_over_late_output() {
        let mut status = SupervisorStatusMachine::default();

        status.apply(&MonitorEvent::Agent(AgentEvent::Thinking));
        status.apply(&MonitorEvent::Repo(RepoEvent::GitSnapshot {
            worktree_path: PathBuf::from("/tmp/worktree"),
            status: git_status_with_conflicts(),
            diff_stat: String::new(),
        }));
        status.apply(&MonitorEvent::Process(ProcessEvent::Stdout {
            line: "still streaming logs".to_string(),
        }));

        assert_eq!(
            status.observation().map(|observation| observation.kind),
            Some(LiveStatusKind::MergeConflict)
        );
    }

    #[test]
    fn supervisor_status_machine_preserves_ci_failure_over_late_output() {
        let mut status = SupervisorStatusMachine::default();

        status.apply(&MonitorEvent::Agent(AgentEvent::Thinking));
        status.apply(&MonitorEvent::Process(ProcessEvent::Stdout {
            line: "GitHub Actions failed: test.yml / build".to_string(),
        }));
        status.apply(&MonitorEvent::Process(ProcessEvent::Stdout {
            line: "late stdout".to_string(),
        }));

        assert_eq!(
            status.observation().map(|observation| observation.kind),
            Some(LiveStatusKind::CiFailed)
        );
    }

    fn git_status_with_conflicts() -> GitStatus {
        GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: true,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: true,
            last_commit: None,
        }
    }
}

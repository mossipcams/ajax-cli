#![deny(unsafe_op_in_unsafe_fn)]

use std::{error::Error, fmt};
use std::{path::PathBuf, time::Duration};

use tokio::{sync::mpsc, task::JoinHandle};

mod codex;
mod process;
pub mod renderer;
mod repo;
mod status;

pub use ajax_core::events::{AgentEvent, MonitorEvent, ProcessEvent, RepoEvent};
use codex::CodexAdapter;
pub use status::SupervisorStatusMachine;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MonitorConfig {
    pub codex_bin: String,
    pub prompt: String,
    pub worktree_path: Option<PathBuf>,
    pub channel_capacity: usize,
    pub watch_filesystem: bool,
    pub git_snapshots: GitSnapshotPolicy,
    pub hang_after: Option<Duration>,
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
}

impl MonitorHandle {
    pub async fn wait(self) -> Result<MonitorExit, SupervisorError> {
        self.join
            .await
            .map_err(|error| SupervisorError::Process(error.to_string()))?
    }
}

pub fn spawn_monitor(
    config: MonitorConfig,
) -> Result<(MonitorHandle, mpsc::Receiver<MonitorEvent>), SupervisorError> {
    let capacity = config.channel_capacity.max(1);
    let (events, receiver) = mpsc::channel(capacity);
    let join = tokio::spawn(async move {
        let (_watcher, watcher_forwarder) = start_watcher(&config, &events)?;
        if config.git_snapshots == GitSnapshotPolicy::OnStartAndExit {
            let worktree_path = config.worktree_path.as_ref().ok_or_else(|| {
                SupervisorError::Process("worktree path is required for git snapshots".to_string())
            })?;
            send_git_snapshot(&events, worktree_path).await?;
        }

        let adapter = CodexAdapter::new(config.codex_bin);
        let result = adapter
            .supervise_exec_json_with_options(&config.prompt, events.clone(), config.hang_after)
            .await;

        if config.git_snapshots == GitSnapshotPolicy::OnStartAndExit {
            let worktree_path = config.worktree_path.as_ref().ok_or_else(|| {
                SupervisorError::Process("worktree path is required for git snapshots".to_string())
            })?;
            send_git_snapshot(&events, worktree_path).await?;
        }

        let status_code = result?;
        drop(watcher_forwarder);
        Ok(MonitorExit { status_code })
    });

    Ok((MonitorHandle { join }, receiver))
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
    let watcher = repo::watch_repo(worktree_path, notify_sender)?;
    let repo_events = events.clone();
    let git_snapshot_policy = config.git_snapshots;
    let git_snapshot_worktree = config.worktree_path.clone();
    let runtime = tokio::runtime::Handle::current();
    let forwarder = tokio::task::spawn_blocking(move || {
        while let Ok(event) = notify_receiver.recv() {
            let Ok(event) = event else {
                continue;
            };
            let monitor_events = repo::notify_event_to_monitor_events(event);
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
                        let _ = send_git_snapshot(&events, &worktree_path).await;
                    });
                }
            }
        }
    });

    Ok((Some(watcher), Some(forwarder)))
}

async fn send_git_snapshot(
    events: &mpsc::Sender<MonitorEvent>,
    worktree_path: &std::path::Path,
) -> Result<(), SupervisorError> {
    events
        .send(MonitorEvent::Repo(repo::git_snapshot(worktree_path).await?))
        .await
        .map_err(|_| SupervisorError::Process("monitor event receiver closed".to_string()))
}

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

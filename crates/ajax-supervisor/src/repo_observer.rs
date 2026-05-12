use std::{ffi::OsStr, path::Path, process::Output};

use ajax_core::{
    adapters::GitAdapter,
    events::{MonitorEvent, RepoEvent},
};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use tokio::process::Command;

use crate::SupervisorError;

pub fn notify_event_to_monitor_events(event: Event) -> Vec<MonitorEvent> {
    if !matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    ) {
        return Vec::new();
    }

    event
        .paths
        .into_iter()
        .filter(|path| !is_git_internal_path(path))
        .map(|path| MonitorEvent::Repo(RepoEvent::FileChanged { path }))
        .collect()
}

pub fn watch_repo(
    path: &Path,
    sender: std::sync::mpsc::Sender<notify::Result<Event>>,
) -> Result<notify::RecommendedWatcher, SupervisorError> {
    let mut watcher = notify::recommended_watcher(move |event| {
        let _ = sender.send(event);
    })?;
    watcher.watch(path, RecursiveMode::Recursive)?;
    Ok(watcher)
}

pub async fn git_snapshot(worktree_path: impl AsRef<Path>) -> Result<RepoEvent, SupervisorError> {
    let worktree_path = worktree_path.as_ref();
    let status_output = Command::new("git")
        .args(["-C"])
        .arg(worktree_path)
        .args(["status", "--porcelain=v1", "--branch"])
        .output()
        .await?;
    ensure_git_success("git status", &status_output)?;
    let diff_output = Command::new("git")
        .args(["-C"])
        .arg(worktree_path)
        .args(["diff", "--stat"])
        .output()
        .await?;
    ensure_git_success("git diff", &diff_output)?;

    Ok(RepoEvent::GitSnapshot {
        worktree_path: worktree_path.to_path_buf(),
        status: GitAdapter::parse_status(&String::from_utf8_lossy(&status_output.stdout), false),
        diff_stat: String::from_utf8_lossy(&diff_output.stdout).to_string(),
    })
}

pub async fn send_git_snapshot(
    events: &tokio::sync::mpsc::Sender<MonitorEvent>,
    worktree_path: &Path,
) -> Result<(), SupervisorError> {
    events
        .send(MonitorEvent::Repo(git_snapshot(worktree_path).await?))
        .await
        .map_err(|_| SupervisorError::Process("monitor event receiver closed".to_string()))
}

fn is_git_internal_path(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == OsStr::new(".git"))
}

fn ensure_git_success(command: &str, output: &Output) -> Result<(), SupervisorError> {
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(SupervisorError::Process(format!(
        "{command} failed: {}",
        stderr.trim()
    )))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ajax_core::events::{MonitorEvent, RepoEvent};
    use notify::{event::ModifyKind, Event, EventKind};

    use super::{git_snapshot, notify_event_to_monitor_events};

    #[test]
    fn notify_events_map_to_repo_file_changes_and_filter_git_internal_paths() {
        let event = Event {
            kind: EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Content)),
            paths: vec![
                PathBuf::from("/tmp/repo/src/lib.rs"),
                PathBuf::from("/tmp/repo/.git/index"),
            ],
            attrs: notify::event::EventAttributes::default(),
        };

        assert_eq!(
            notify_event_to_monitor_events(event),
            vec![MonitorEvent::Repo(RepoEvent::FileChanged {
                path: PathBuf::from("/tmp/repo/src/lib.rs")
            })]
        );
    }

    #[tokio::test]
    async fn git_snapshot_reports_git_command_failure() {
        let missing_repo =
            std::env::temp_dir().join(format!("ajax-missing-repo-{}", std::process::id()));

        let error = git_snapshot(&missing_repo).await.unwrap_err();

        assert!(
            matches!(error, crate::SupervisorError::Process(message) if message.contains("git status failed"))
        );
    }
}

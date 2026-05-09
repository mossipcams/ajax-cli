use std::path::Path;

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
    let diff_output = Command::new("git")
        .args(["-C"])
        .arg(worktree_path)
        .args(["diff", "--stat"])
        .output()
        .await?;

    Ok(RepoEvent::GitSnapshot {
        worktree_path: worktree_path.to_path_buf(),
        status: GitAdapter::parse_status(&String::from_utf8_lossy(&status_output.stdout), false),
        diff_stat: String::from_utf8_lossy(&diff_output.stdout).to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ajax_core::events::{MonitorEvent, RepoEvent};
    use notify::{event::ModifyKind, Event, EventKind};

    use super::notify_event_to_monitor_events;

    #[test]
    fn notify_events_map_to_repo_file_changes() {
        let event = Event {
            kind: EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Content)),
            paths: vec![PathBuf::from("/tmp/repo/src/lib.rs")],
            attrs: notify::event::EventAttributes::default(),
        };

        assert_eq!(
            notify_event_to_monitor_events(event),
            vec![MonitorEvent::Repo(RepoEvent::FileChanged {
                path: PathBuf::from("/tmp/repo/src/lib.rs")
            })]
        );
    }
}

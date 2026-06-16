//! Authoritative ghost-task classification for registry persistence and Cockpit visibility.
//!
//! A registry ghost is a task row that should not survive SQLite save/load and should not
//! appear in Cockpit. Recoverable missing-substrate tasks remain persisted so operators
//! keep history and can repair, drop, or rediscover substrate.

use crate::models::{LifecycleStatus, Task};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistryGhostReason {
    Removed,
    Stale,
    AbandonedProvisioning,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistryPersistenceDisposition {
    Persist,
    Prune(RegistryGhostReason),
}

pub fn registry_persistence_disposition(task: &Task) -> RegistryPersistenceDisposition {
    if task.lifecycle_status == LifecycleStatus::Removed {
        return RegistryPersistenceDisposition::Prune(RegistryGhostReason::Removed);
    }
    if task.facts_with_now(std::time::SystemTime::now()).stale {
        return RegistryPersistenceDisposition::Prune(RegistryGhostReason::Stale);
    }
    if is_abandoned_provisioning_ghost(task) {
        return RegistryPersistenceDisposition::Prune(RegistryGhostReason::AbandonedProvisioning);
    }
    RegistryPersistenceDisposition::Persist
}

pub fn is_registry_ghost_task(task: &Task) -> bool {
    matches!(
        registry_persistence_disposition(task),
        RegistryPersistenceDisposition::Prune(_)
    )
}

pub fn is_cockpit_visible_task(task: &Task) -> bool {
    !matches!(
        registry_persistence_disposition(task),
        RegistryPersistenceDisposition::Prune(_)
    )
}

fn is_abandoned_provisioning_ghost(task: &Task) -> bool {
    if !matches!(
        task.lifecycle_status,
        LifecycleStatus::Created | LifecycleStatus::Provisioning
    ) {
        return false;
    }
    if !task.has_missing_substrate() {
        return false;
    }
    has_no_recoverable_git_substrate(task)
}

fn has_no_recoverable_git_substrate(task: &Task) -> bool {
    git_worktree_absent(task) && git_branch_absent(task)
}

fn git_worktree_absent(task: &Task) -> bool {
    task.facts().worktree_missing
}

fn git_branch_absent(task: &Task) -> bool {
    task.facts().branch_missing
}

#[cfg(test)]
mod tests {
    use super::{
        is_cockpit_visible_task, is_registry_ghost_task, registry_persistence_disposition,
        RegistryGhostReason, RegistryPersistenceDisposition,
    };
    use crate::models::{
        AgentClient, GitStatus, LifecycleStatus, SideFlag, Task, TaskId, TmuxStatus,
    };
    use rstest::rstest;

    fn task_with_lifecycle(status: LifecycleStatus) -> Task {
        let mut task = Task::new(
            TaskId::new("web/fix-login"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            "/tmp/worktrees/web-fix-login",
            "ajax-web-fix-login",
            "worktrunk",
            AgentClient::Codex,
        );
        task.lifecycle_status = status;
        task
    }

    #[rstest]
    #[case(LifecycleStatus::Active)]
    #[case(LifecycleStatus::Waiting)]
    #[case(LifecycleStatus::Reviewable)]
    #[case(LifecycleStatus::Mergeable)]
    #[case(LifecycleStatus::Error)]
    #[case(LifecycleStatus::Orphaned)]
    #[case(LifecycleStatus::Cleanable)]
    #[case(LifecycleStatus::TeardownIncomplete)]
    fn operational_lifecycles_with_missing_substrate_persist(#[case] status: LifecycleStatus) {
        let mut task = task_with_lifecycle(status);
        task.add_side_flag(SideFlag::TmuxMissing);

        assert_eq!(
            registry_persistence_disposition(&task),
            RegistryPersistenceDisposition::Persist
        );
        assert!(!is_registry_ghost_task(&task));
        assert!(is_cockpit_visible_task(&task));
    }

    #[test]
    fn removed_and_stale_tasks_are_registry_ghosts() {
        let mut removed = task_with_lifecycle(LifecycleStatus::Removed);
        removed.add_side_flag(SideFlag::TmuxMissing);
        assert_eq!(
            registry_persistence_disposition(&removed),
            RegistryPersistenceDisposition::Prune(RegistryGhostReason::Removed)
        );
        assert!(!is_cockpit_visible_task(&removed));

        let mut stale = task_with_lifecycle(LifecycleStatus::Active);
        stale.last_activity_at = std::time::SystemTime::UNIX_EPOCH;
        stale.add_side_flag(SideFlag::WorktreeMissing);
        assert_eq!(
            registry_persistence_disposition(&stale),
            RegistryPersistenceDisposition::Prune(RegistryGhostReason::Stale)
        );
        assert!(!is_cockpit_visible_task(&stale));
    }

    #[test]
    fn ghost_classification_uses_lifecycle_and_age_without_stale_flag() {
        let stale_after = std::time::Duration::from_secs(7 * 24 * 60 * 60);
        let mut task = task_with_lifecycle(LifecycleStatus::Active);
        task.last_activity_at = std::time::SystemTime::UNIX_EPOCH;

        let disposition = if task
            .facts_with_now(
                std::time::SystemTime::UNIX_EPOCH + stale_after + std::time::Duration::from_secs(1),
            )
            .stale
        {
            RegistryPersistenceDisposition::Prune(RegistryGhostReason::Stale)
        } else {
            RegistryPersistenceDisposition::Persist
        };

        assert_eq!(
            disposition,
            RegistryPersistenceDisposition::Prune(RegistryGhostReason::Stale)
        );
        assert!(!task.has_side_flag(SideFlag::Stale));
    }

    #[test]
    fn abandoned_provisioning_without_git_substrate_is_a_ghost() {
        let mut task = task_with_lifecycle(LifecycleStatus::Provisioning);
        task.add_side_flag(SideFlag::WorktreeMissing);
        task.add_side_flag(SideFlag::BranchMissing);
        task.add_side_flag(SideFlag::TmuxMissing);

        assert_eq!(
            registry_persistence_disposition(&task),
            RegistryPersistenceDisposition::Prune(RegistryGhostReason::AbandonedProvisioning)
        );
        assert!(!is_cockpit_visible_task(&task));
    }

    #[test]
    fn provisioning_with_recoverable_git_substrate_persists() {
        let mut task = task_with_lifecycle(LifecycleStatus::Provisioning);
        task.git_status = Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        });
        task.add_side_flag(SideFlag::TmuxMissing);

        assert_eq!(
            registry_persistence_disposition(&task),
            RegistryPersistenceDisposition::Persist
        );
    }

    #[test]
    fn teardown_incomplete_with_worktree_persists_for_cleanup_retry() {
        let mut task = task_with_lifecycle(LifecycleStatus::TeardownIncomplete);
        task.tmux_status = Some(TmuxStatus {
            exists: false,
            session_name: "ajax-web-fix-login".to_string(),
        });
        task.git_status = Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        });

        assert!(!is_registry_ghost_task(&task));
        assert!(is_cockpit_visible_task(&task));
    }
}

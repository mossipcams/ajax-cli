//! Authoritative ghost-task classification for registry persistence and Cockpit visibility.
//!
//! A registry ghost is a task row that should not survive SQLite save/load and should not
//! appear in Cockpit. Recoverable missing-substrate tasks remain persisted so operators
//! keep history and can repair, drop, or rediscover substrate.

use crate::models::{LifecycleStatus, SideFlag, Task};

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
    if task.has_side_flag(SideFlag::Stale) {
        if has_no_recoverable_git_substrate(task) {
            return RegistryPersistenceDisposition::Prune(RegistryGhostReason::Stale);
        }
        // Keep stale tasks that still show recoverable git substrate so drop can
        // finish teardown. Pure Stale with no substrate evidence stays pruned so
        // Cockpit can hide long-inactive tasks.
        if task
            .git_status
            .as_ref()
            .is_some_and(|status| status.worktree_exists || status.branch_exists)
            || task.has_side_flag(SideFlag::WorktreeMissing)
            || task.has_side_flag(SideFlag::BranchMissing)
        {
            return RegistryPersistenceDisposition::Persist;
        }
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
    task.has_side_flag(SideFlag::WorktreeMissing)
        || task
            .git_status
            .as_ref()
            .is_some_and(|status| !status.worktree_exists)
}

fn git_branch_absent(task: &Task) -> bool {
    task.has_side_flag(SideFlag::BranchMissing)
        || task
            .git_status
            .as_ref()
            .is_some_and(|status| !status.branch_exists)
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
            "task",
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
        stale.add_side_flag(SideFlag::Stale);
        assert_eq!(
            registry_persistence_disposition(&stale),
            RegistryPersistenceDisposition::Prune(RegistryGhostReason::Stale)
        );
        assert!(!is_cockpit_visible_task(&stale));
    }

    #[test]
    fn stale_task_with_existing_branch_is_not_a_registry_ghost() {
        let mut task = task_with_lifecycle(LifecycleStatus::Active);
        task.add_side_flag(SideFlag::Stale);
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

        assert_eq!(
            registry_persistence_disposition(&task),
            RegistryPersistenceDisposition::Persist
        );
        assert!(is_cockpit_visible_task(&task));
    }

    #[test]
    fn stale_task_with_partial_missing_substrate_is_not_a_registry_ghost() {
        let mut task = task_with_lifecycle(LifecycleStatus::Active);
        task.add_side_flag(SideFlag::Stale);
        task.add_side_flag(SideFlag::WorktreeMissing);

        assert_eq!(
            registry_persistence_disposition(&task),
            RegistryPersistenceDisposition::Persist
        );
        assert!(is_cockpit_visible_task(&task));
    }

    #[test]
    fn stale_task_without_recoverable_git_substrate_is_a_registry_ghost() {
        let mut task = task_with_lifecycle(LifecycleStatus::Active);
        task.add_side_flag(SideFlag::Stale);
        task.add_side_flag(SideFlag::WorktreeMissing);
        task.add_side_flag(SideFlag::BranchMissing);

        assert_eq!(
            registry_persistence_disposition(&task),
            RegistryPersistenceDisposition::Prune(RegistryGhostReason::Stale)
        );
        assert!(!is_cockpit_visible_task(&task));
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

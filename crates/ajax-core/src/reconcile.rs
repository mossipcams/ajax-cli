use crate::{
    adapters::TmuxAdapter,
    attention::derive_attention_items,
    models::{
        AgentRuntimeStatus, AttentionItem, GitStatus, SideFlag, Task, TmuxStatus, WorktrunkStatus,
    },
};
use std::path::Path;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReconciliationInput {
    pub git_status: Option<GitStatus>,
    pub tmux_status: Option<TmuxStatus>,
    pub worktrunk_status: Option<WorktrunkStatus>,
}

pub fn reconcile_task(task: &mut Task, input: ReconciliationInput) {
    if let Some(git_status) = input.git_status {
        apply_git_flags(task, &git_status);
        task.git_status = Some(git_status);
    }

    if let Some(tmux_status) = input.tmux_status {
        if !tmux_status.exists {
            mark_resource_missing(task, SideFlag::TmuxMissing);
        } else {
            task.remove_side_flag(SideFlag::TmuxMissing);
        }
        task.tmux_status = Some(tmux_status);
    }

    if let Some(worktrunk_status) = input.worktrunk_status {
        if !worktrunk_status.exists || !worktrunk_status.points_at_expected_path {
            mark_resource_missing(task, SideFlag::WorktrunkMissing);
        } else {
            task.remove_side_flag(SideFlag::WorktrunkMissing);
        }
        task.worktrunk_status = Some(worktrunk_status);
    }
}

pub fn reconcile_task_from_tmux_output(
    task: &mut Task,
    list_sessions_output: &str,
    list_windows_output: &str,
) {
    reconcile_task(
        task,
        ReconciliationInput {
            git_status: None,
            tmux_status: Some(TmuxAdapter::parse_session_status(
                &task.tmux_session,
                list_sessions_output,
            )),
            worktrunk_status: Some(TmuxAdapter::parse_worktrunk_status(
                &task.worktrunk_window,
                &task.worktree_path.display().to_string(),
                list_windows_output,
            )),
        },
    );
}

pub fn reconcile_task_filesystem(task: &mut Task) {
    if !Path::new(&task.worktree_path).exists() {
        mark_resource_missing(task, SideFlag::WorktreeMissing);
    } else {
        task.remove_side_flag(SideFlag::WorktreeMissing);
    }
}

pub fn attention_items(tasks: &[Task]) -> Vec<AttentionItem> {
    derive_attention_items(tasks)
}

fn apply_git_flags(task: &mut Task, git_status: &GitStatus) {
    if !git_status.worktree_exists {
        mark_resource_missing(task, SideFlag::WorktreeMissing);
    } else {
        task.remove_side_flag(SideFlag::WorktreeMissing);
    }

    let on_expected_branch = git_status
        .current_branch
        .as_deref()
        .is_some_and(|current_branch| current_branch == task.branch);

    if !git_status.branch_exists || !on_expected_branch {
        mark_resource_missing(task, SideFlag::BranchMissing);
    } else {
        task.remove_side_flag(SideFlag::BranchMissing);
    }

    if git_status.dirty || git_status.untracked_files > 0 {
        task.add_side_flag(SideFlag::Dirty);
    } else {
        task.remove_side_flag(SideFlag::Dirty);
    }

    if git_status.conflicted {
        task.add_side_flag(SideFlag::Conflicted);
    } else {
        task.remove_side_flag(SideFlag::Conflicted);
    }

    if git_status.has_unpushed_work() {
        task.add_side_flag(SideFlag::Unpushed);
    } else {
        task.remove_side_flag(SideFlag::Unpushed);
    }
}

fn mark_resource_missing(task: &mut Task, flag: SideFlag) {
    task.agent_status = AgentRuntimeStatus::Unknown;
    task.add_side_flag(flag);
    task.remove_side_flag(SideFlag::AgentRunning);
}

#[cfg(test)]
mod tests {
    use super::{
        attention_items, reconcile_task, reconcile_task_from_tmux_output, ReconciliationInput,
    };
    use crate::models::{
        AgentClient, AgentRuntimeStatus, AttentionItem, GitStatus, SideFlag, Task, TaskId,
        TmuxStatus, WorktrunkStatus,
    };

    fn base_task() -> Task {
        Task::new(
            TaskId::new("task-1"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            "/tmp/worktrees/web-fix-login",
            "ajax-web-fix-login",
            "worktrunk",
            AgentClient::Codex,
        )
    }

    #[test]
    fn reconciliation_marks_missing_external_resources() {
        let mut task = base_task();
        reconcile_task(
            &mut task,
            ReconciliationInput {
                git_status: Some(GitStatus {
                    worktree_exists: false,
                    branch_exists: false,
                    current_branch: None,
                    dirty: false,
                    ahead: 0,
                    behind: 0,
                    merged: false,
                    untracked_files: 0,
                    unpushed_commits: 0,
                    conflicted: false,
                    last_commit: None,
                }),
                tmux_status: Some(TmuxStatus {
                    exists: false,
                    session_name: "ajax-web-fix-login".to_string(),
                }),
                worktrunk_status: Some(WorktrunkStatus {
                    exists: false,
                    window_name: "worktrunk".to_string(),
                    current_path: "/tmp/wrong".into(),
                    points_at_expected_path: false,
                }),
            },
        );

        assert!(task.has_side_flag(SideFlag::WorktreeMissing));
        assert!(task.has_side_flag(SideFlag::BranchMissing));
        assert!(task.has_side_flag(SideFlag::TmuxMissing));
        assert!(task.has_side_flag(SideFlag::WorktrunkMissing));
    }

    #[test]
    fn reconciliation_clears_running_state_for_missing_resources() {
        let mut task = base_task();
        task.agent_status = AgentRuntimeStatus::Running;
        task.add_side_flag(SideFlag::AgentRunning);

        reconcile_task(
            &mut task,
            ReconciliationInput {
                git_status: Some(GitStatus {
                    worktree_exists: false,
                    branch_exists: false,
                    current_branch: None,
                    dirty: false,
                    ahead: 0,
                    behind: 0,
                    merged: false,
                    untracked_files: 0,
                    unpushed_commits: 0,
                    conflicted: false,
                    last_commit: None,
                }),
                tmux_status: Some(TmuxStatus {
                    exists: false,
                    session_name: "ajax-web-fix-login".to_string(),
                }),
                worktrunk_status: Some(WorktrunkStatus {
                    exists: false,
                    window_name: "worktrunk".to_string(),
                    current_path: "/tmp/wrong".into(),
                    points_at_expected_path: false,
                }),
            },
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Unknown);
        assert!(!task.has_side_flag(SideFlag::AgentRunning));
    }

    #[test]
    fn filesystem_reconciliation_clears_running_state_for_missing_worktree() {
        let mut task = base_task();
        task.agent_status = AgentRuntimeStatus::Running;
        task.add_side_flag(SideFlag::AgentRunning);
        task.worktree_path = format!("/tmp/ajax-missing-worktree-{}", std::process::id()).into();

        super::reconcile_task_filesystem(&mut task);

        assert_eq!(task.agent_status, AgentRuntimeStatus::Unknown);
        assert!(!task.has_side_flag(SideFlag::AgentRunning));
        assert!(task.has_side_flag(SideFlag::WorktreeMissing));
    }

    #[test]
    fn reconciliation_marks_dirty_conflicted_and_unpushed_work() {
        let mut task = base_task();
        reconcile_task(
            &mut task,
            ReconciliationInput {
                git_status: Some(GitStatus {
                    worktree_exists: true,
                    branch_exists: true,
                    current_branch: Some("ajax/fix-login".to_string()),
                    dirty: true,
                    ahead: 1,
                    behind: 0,
                    merged: false,
                    untracked_files: 2,
                    unpushed_commits: 1,
                    conflicted: true,
                    last_commit: None,
                }),
                tmux_status: None,
                worktrunk_status: None,
            },
        );

        assert!(task.has_side_flag(SideFlag::Dirty));
        assert!(task.has_side_flag(SideFlag::Conflicted));
        assert!(task.has_side_flag(SideFlag::Unpushed));
    }

    #[test]
    fn reconciliation_marks_branch_missing_when_git_is_on_wrong_branch() {
        let mut task = base_task();
        reconcile_task(
            &mut task,
            ReconciliationInput {
                git_status: Some(GitStatus {
                    worktree_exists: true,
                    branch_exists: true,
                    current_branch: Some("main".to_string()),
                    dirty: false,
                    ahead: 0,
                    behind: 0,
                    merged: false,
                    untracked_files: 0,
                    unpushed_commits: 0,
                    conflicted: false,
                    last_commit: None,
                }),
                tmux_status: None,
                worktrunk_status: None,
            },
        );

        assert!(task.has_side_flag(SideFlag::BranchMissing));
    }

    #[test]
    fn attention_items_are_derived_from_task_flags() {
        let mut task = base_task();
        task.add_side_flag(SideFlag::NeedsInput);
        task.add_side_flag(SideFlag::TestsFailed);

        let items = attention_items(&[task]);

        assert_eq!(
            items,
            vec![
                AttentionItem {
                    task_id: TaskId::new("task-1"),
                    task_handle: "web/fix-login".to_string(),
                    reason: "agent needs input".to_string(),
                    priority: 10,
                    recommended_action: "open task".to_string(),
                },
                AttentionItem {
                    task_id: TaskId::new("task-1"),
                    task_handle: "web/fix-login".to_string(),
                    reason: "tests failed".to_string(),
                    priority: 15,
                    recommended_action: "inspect test output".to_string(),
                },
            ]
        );
    }

    #[test]
    fn reconciliation_can_use_tmux_discovery_output() {
        let mut task = base_task();

        reconcile_task_from_tmux_output(
            &mut task,
            "ajax-web-fix-login\n",
            "worktrunk\t/tmp/worktrees/web-fix-login\n",
        );

        assert!(!task.has_side_flag(SideFlag::TmuxMissing));
        assert!(!task.has_side_flag(SideFlag::WorktrunkMissing));
        assert!(task.tmux_status.as_ref().unwrap().exists);
        assert!(
            task.worktrunk_status
                .as_ref()
                .unwrap()
                .points_at_expected_path
        );
    }

    #[test]
    fn reconciliation_marks_missing_tmux_discovery_output() {
        let mut task = base_task();

        reconcile_task_from_tmux_output(&mut task, "other-session\n", "agent\t/tmp/worktree\n");

        assert!(task.has_side_flag(SideFlag::TmuxMissing));
        assert!(task.has_side_flag(SideFlag::WorktrunkMissing));
    }

    #[test]
    fn reconciliation_marks_missing_worktree_from_filesystem() {
        let mut task = base_task();
        task.worktree_path = format!("/tmp/ajax-missing-worktree-{}", std::process::id()).into();

        super::reconcile_task_filesystem(&mut task);

        assert!(task.has_side_flag(SideFlag::WorktreeMissing));
    }

    #[test]
    fn reconciliation_clears_recovered_external_flags() {
        let mut task = base_task();
        task.add_side_flag(SideFlag::Dirty);
        task.add_side_flag(SideFlag::Unpushed);
        task.add_side_flag(SideFlag::Conflicted);
        task.add_side_flag(SideFlag::WorktreeMissing);
        task.add_side_flag(SideFlag::BranchMissing);
        task.add_side_flag(SideFlag::TmuxMissing);
        task.add_side_flag(SideFlag::WorktrunkMissing);

        reconcile_task(
            &mut task,
            ReconciliationInput {
                git_status: Some(GitStatus {
                    worktree_exists: true,
                    branch_exists: true,
                    current_branch: Some("ajax/fix-login".to_string()),
                    dirty: false,
                    ahead: 0,
                    behind: 0,
                    merged: true,
                    untracked_files: 0,
                    unpushed_commits: 0,
                    conflicted: false,
                    last_commit: None,
                }),
                tmux_status: Some(TmuxStatus {
                    exists: true,
                    session_name: "ajax-web-fix-login".to_string(),
                }),
                worktrunk_status: Some(WorktrunkStatus {
                    exists: true,
                    window_name: "worktrunk".to_string(),
                    current_path: "/tmp/worktrees/web-fix-login".into(),
                    points_at_expected_path: true,
                }),
            },
        );

        for flag in [
            SideFlag::Dirty,
            SideFlag::Unpushed,
            SideFlag::Conflicted,
            SideFlag::WorktreeMissing,
            SideFlag::BranchMissing,
            SideFlag::TmuxMissing,
            SideFlag::WorktrunkMissing,
        ] {
            assert!(!task.has_side_flag(flag), "{flag:?} should be cleared");
        }
    }
}

use crate::models::Task;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskValidityIssue {
    MissingTmuxSession,
    MissingWorktree,
    MissingBranch,
    MissingWorktrunk,
    WorktrunkWrongPath,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskValidity {
    pub issues: Vec<TaskValidityIssue>,
}

impl TaskValidity {
    pub fn is_valid(&self) -> bool {
        self.issues.is_empty()
    }
}

pub fn task_validity(task: &Task) -> TaskValidity {
    let mut issues = Vec::new();

    if task
        .tmux_status
        .as_ref()
        .is_none_or(|status| !status.exists)
    {
        issues.push(TaskValidityIssue::MissingTmuxSession);
    }

    if task
        .git_status
        .as_ref()
        .is_none_or(|status| !status.worktree_exists)
    {
        issues.push(TaskValidityIssue::MissingWorktree);
    }

    if task
        .git_status
        .as_ref()
        .is_none_or(|status| !status.branch_exists)
    {
        issues.push(TaskValidityIssue::MissingBranch);
    }

    match task.worktrunk_status.as_ref() {
        Some(status) if status.exists && status.points_at_expected_path => {}
        Some(status) if status.exists => issues.push(TaskValidityIssue::WorktrunkWrongPath),
        _ => issues.push(TaskValidityIssue::MissingWorktrunk),
    }

    TaskValidity { issues }
}

#[cfg(test)]
mod tests {
    use super::{task_validity, TaskValidityIssue};
    use crate::models::{AgentClient, GitStatus, Task, TaskId, TmuxStatus, WorktrunkStatus};

    fn valid_task() -> Task {
        let mut task = Task::new(
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
        );
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
        task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
        task.worktrunk_status = Some(WorktrunkStatus::present(
            "worktrunk",
            "/tmp/worktrees/web-fix-login",
        ));
        task
    }

    #[test]
    fn task_is_valid_when_sql_tmux_worktree_branch_and_worktrunk_align() {
        let validity = task_validity(&valid_task());

        assert!(validity.is_valid());
        assert!(validity.issues.is_empty());
    }

    #[test]
    fn task_validity_reports_each_missing_piece() {
        let mut task = valid_task();
        task.tmux_status = Some(TmuxStatus {
            exists: false,
            session_name: "ajax-web-fix-login".to_string(),
        });
        task.git_status.as_mut().unwrap().worktree_exists = false;
        task.git_status.as_mut().unwrap().branch_exists = false;
        task.worktrunk_status = Some(WorktrunkStatus {
            exists: false,
            window_name: "worktrunk".to_string(),
            current_path: "/tmp/worktrees/web-fix-login".into(),
            points_at_expected_path: false,
        });

        let validity = task_validity(&task);

        assert!(!validity.is_valid());
        assert_eq!(
            validity.issues,
            vec![
                TaskValidityIssue::MissingTmuxSession,
                TaskValidityIssue::MissingWorktree,
                TaskValidityIssue::MissingBranch,
                TaskValidityIssue::MissingWorktrunk,
            ]
        );
    }

    #[test]
    fn task_validity_reports_worktrunk_wrong_path() {
        let mut task = valid_task();
        task.worktrunk_status = Some(WorktrunkStatus {
            exists: true,
            window_name: "worktrunk".to_string(),
            current_path: "/tmp/wrong".into(),
            points_at_expected_path: false,
        });

        let validity = task_validity(&task);

        assert_eq!(validity.issues, vec![TaskValidityIssue::WorktrunkWrongPath]);
    }
}

use crate::models::{SafetyClassification, SafetyReport, SideFlag, Task};

pub fn cleanup_safety(task: &Task) -> SafetyReport {
    let mut classification = SafetyClassification::Safe;
    let mut reasons = Vec::new();

    let Some(git_status) = task.git_status.as_ref() else {
        return SafetyReport {
            classification: SafetyClassification::Blocked,
            reasons: vec!["git status is unknown".to_string()],
        };
    };

    if !git_status.worktree_exists || task.has_side_flag(SideFlag::WorktreeMissing) {
        mark(
            &mut classification,
            SafetyClassification::Blocked,
            &mut reasons,
            "worktree is missing",
        );
    }

    if !git_status.branch_exists || task.has_side_flag(SideFlag::BranchMissing) {
        mark(
            &mut classification,
            SafetyClassification::Blocked,
            &mut reasons,
            "branch is missing",
        );
    }

    if git_status.conflicted || task.has_side_flag(SideFlag::Conflicted) {
        mark(
            &mut classification,
            SafetyClassification::Dangerous,
            &mut reasons,
            "working tree has conflicts",
        );
    }

    if git_status.dirty || git_status.untracked_files > 0 || task.has_side_flag(SideFlag::Dirty) {
        mark(
            &mut classification,
            SafetyClassification::NeedsConfirmation,
            &mut reasons,
            "working tree has local changes",
        );
    }

    if !git_status.merged {
        mark(
            &mut classification,
            SafetyClassification::NeedsConfirmation,
            &mut reasons,
            "branch is not merged",
        );
    } else {
        reasons.push("branch is merged".to_string());
    }

    if git_status.has_unpushed_work() || task.has_side_flag(SideFlag::Unpushed) {
        mark(
            &mut classification,
            SafetyClassification::NeedsConfirmation,
            &mut reasons,
            "branch has unpushed commits",
        );
    }

    SafetyReport {
        classification,
        reasons,
    }
}

fn mark(
    current: &mut SafetyClassification,
    candidate: SafetyClassification,
    reasons: &mut Vec<String>,
    reason: &str,
) {
    if severity(candidate) > severity(*current) {
        *current = candidate;
    }

    reasons.push(reason.to_string());
}

fn severity(classification: SafetyClassification) -> u8 {
    match classification {
        SafetyClassification::Safe => 0,
        SafetyClassification::NeedsConfirmation => 1,
        SafetyClassification::Dangerous => 2,
        SafetyClassification::Blocked => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::cleanup_safety;
    use crate::models::{
        AgentClient, GitStatus, SafetyClassification, SideFlag, Task, TaskId, TmuxStatus,
    };

    fn clean_merged_task() -> Task {
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
            merged: true,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: Some("abc123 Fix login".to_string()),
        });
        task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
        task
    }

    #[test]
    fn merged_clean_task_is_safe_to_clean() {
        let report = cleanup_safety(&clean_merged_task());

        assert_eq!(report.classification, SafetyClassification::Safe);
        assert!(report
            .reasons
            .iter()
            .any(|reason| reason == "branch is merged"));
    }

    #[test]
    fn dirty_worktree_requires_confirmation() {
        let mut task = clean_merged_task();
        task.git_status.as_mut().unwrap().dirty = true;
        task.add_side_flag(SideFlag::Dirty);

        let report = cleanup_safety(&task);

        assert_eq!(
            report.classification,
            SafetyClassification::NeedsConfirmation
        );
        assert!(report
            .reasons
            .iter()
            .any(|reason| reason == "working tree has local changes"));
    }

    #[test]
    fn unmerged_branch_with_unpushed_commits_requires_confirmation() {
        let mut task = clean_merged_task();
        let git = task.git_status.as_mut().unwrap();
        git.merged = false;
        git.ahead = 1;
        git.unpushed_commits = 1;

        let report = cleanup_safety(&task);

        assert_eq!(
            report.classification,
            SafetyClassification::NeedsConfirmation
        );
        assert!(report
            .reasons
            .iter()
            .any(|reason| reason == "branch is not merged"));
        assert!(report
            .reasons
            .iter()
            .any(|reason| reason == "branch has unpushed commits"));
    }

    #[test]
    fn conflicted_task_is_dangerous_to_clean() {
        let mut task = clean_merged_task();
        task.git_status.as_mut().unwrap().conflicted = true;
        task.add_side_flag(SideFlag::Conflicted);

        let report = cleanup_safety(&task);

        assert_eq!(report.classification, SafetyClassification::Dangerous);
        assert!(report
            .reasons
            .iter()
            .any(|reason| reason == "working tree has conflicts"));
    }

    #[test]
    fn missing_worktree_blocks_cleanup() {
        let mut task = clean_merged_task();
        task.git_status.as_mut().unwrap().worktree_exists = false;
        task.add_side_flag(SideFlag::WorktreeMissing);

        let report = cleanup_safety(&task);

        assert_eq!(report.classification, SafetyClassification::Blocked);
        assert!(report
            .reasons
            .iter()
            .any(|reason| reason == "worktree is missing"));
    }
}

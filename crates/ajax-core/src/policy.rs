use crate::models::{LifecycleStatus, SafetyClassification, SafetyReport, SideFlag, Task};

pub fn merge_safety(task: &Task) -> SafetyReport {
    let mut classification = SafetyClassification::Safe;
    let mut reasons = Vec::new();

    if !matches!(
        task.lifecycle_status,
        LifecycleStatus::Reviewable | LifecycleStatus::Mergeable
    ) {
        return SafetyReport {
            classification: SafetyClassification::Blocked,
            reasons: vec!["lifecycle is not reviewable or mergeable".to_string()],
        };
    }

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
    use proptest::prelude::*;
    use rstest::rstest;

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

    #[derive(Clone, Copy, Debug)]
    struct CleanupScenario {
        git_worktree_exists: bool,
        flag_worktree_missing: bool,
        git_branch_exists: bool,
        flag_branch_missing: bool,
        git_conflicted: bool,
        flag_conflicted: bool,
        git_dirty: bool,
        untracked_files: u32,
        flag_dirty: bool,
        merged: bool,
        ahead: u32,
        unpushed_commits: u32,
        flag_unpushed: bool,
    }

    impl CleanupScenario {
        fn apply_to(self, task: &mut Task) {
            let git = task.git_status.as_mut().unwrap();
            git.worktree_exists = self.git_worktree_exists;
            git.branch_exists = self.git_branch_exists;
            git.conflicted = self.git_conflicted;
            git.dirty = self.git_dirty;
            git.untracked_files = self.untracked_files;
            git.merged = self.merged;
            git.ahead = self.ahead;
            git.unpushed_commits = self.unpushed_commits;

            if self.flag_worktree_missing {
                task.add_side_flag(SideFlag::WorktreeMissing);
            }
            if self.flag_branch_missing {
                task.add_side_flag(SideFlag::BranchMissing);
            }
            if self.flag_conflicted {
                task.add_side_flag(SideFlag::Conflicted);
            }
            if self.flag_dirty {
                task.add_side_flag(SideFlag::Dirty);
            }
            if self.flag_unpushed {
                task.add_side_flag(SideFlag::Unpushed);
            }
        }

        fn expected_classification(self) -> SafetyClassification {
            if !self.git_worktree_exists
                || self.flag_worktree_missing
                || !self.git_branch_exists
                || self.flag_branch_missing
            {
                SafetyClassification::Blocked
            } else if self.git_conflicted || self.flag_conflicted {
                SafetyClassification::Dangerous
            } else if self.git_dirty
                || self.untracked_files > 0
                || self.flag_dirty
                || !self.merged
                || self.ahead > 0
                || self.unpushed_commits > 0
                || self.flag_unpushed
            {
                SafetyClassification::NeedsConfirmation
            } else {
                SafetyClassification::Safe
            }
        }
    }

    fn cleanup_scenario_strategy() -> impl Strategy<Value = CleanupScenario> {
        (0_u16..1024, 0..4_u32, 0..4_u32, 0..4_u32).prop_map(
            |(mask, untracked_files, ahead, unpushed_commits)| CleanupScenario {
                git_worktree_exists: mask & (1 << 0) != 0,
                flag_worktree_missing: mask & (1 << 1) != 0,
                git_branch_exists: mask & (1 << 2) != 0,
                flag_branch_missing: mask & (1 << 3) != 0,
                git_conflicted: mask & (1 << 4) != 0,
                flag_conflicted: mask & (1 << 5) != 0,
                git_dirty: mask & (1 << 6) != 0,
                untracked_files,
                flag_dirty: mask & (1 << 7) != 0,
                merged: mask & (1 << 8) != 0,
                ahead,
                unpushed_commits,
                flag_unpushed: mask & (1 << 9) != 0,
            },
        )
    }

    proptest! {
        #[test]
        fn cleanup_safety_uses_highest_risk_classification(
            scenario in cleanup_scenario_strategy(),
        ) {
            let mut task = clean_merged_task();
            scenario.apply_to(&mut task);

            let report = cleanup_safety(&task);

            prop_assert_eq!(report.classification, scenario.expected_classification());
        }
    }

    #[derive(Clone, Copy)]
    enum SafetyScenario {
        GitWorktreeMissing,
        FlagWorktreeMissing,
        GitBranchMissing,
        FlagBranchMissing,
        GitConflicted,
        FlagConflicted,
        GitDirty,
        GitUntracked,
        FlagDirty,
        GitUnmerged,
        GitAhead,
        GitUnpushedCommits,
        FlagUnpushed,
    }

    fn apply_safety_scenario(task: &mut Task, scenario: SafetyScenario) {
        match scenario {
            SafetyScenario::GitWorktreeMissing => {
                task.git_status.as_mut().unwrap().worktree_exists = false;
            }
            SafetyScenario::FlagWorktreeMissing => {
                task.add_side_flag(SideFlag::WorktreeMissing);
            }
            SafetyScenario::GitBranchMissing => {
                task.git_status.as_mut().unwrap().branch_exists = false;
            }
            SafetyScenario::FlagBranchMissing => {
                task.add_side_flag(SideFlag::BranchMissing);
            }
            SafetyScenario::GitConflicted => {
                task.git_status.as_mut().unwrap().conflicted = true;
            }
            SafetyScenario::FlagConflicted => {
                task.add_side_flag(SideFlag::Conflicted);
            }
            SafetyScenario::GitDirty => {
                task.git_status.as_mut().unwrap().dirty = true;
            }
            SafetyScenario::GitUntracked => {
                task.git_status.as_mut().unwrap().untracked_files = 1;
            }
            SafetyScenario::FlagDirty => {
                task.add_side_flag(SideFlag::Dirty);
            }
            SafetyScenario::GitUnmerged => {
                task.git_status.as_mut().unwrap().merged = false;
            }
            SafetyScenario::GitAhead => {
                task.git_status.as_mut().unwrap().ahead = 1;
            }
            SafetyScenario::GitUnpushedCommits => {
                task.git_status.as_mut().unwrap().unpushed_commits = 1;
            }
            SafetyScenario::FlagUnpushed => {
                task.add_side_flag(SideFlag::Unpushed);
            }
        }
    }

    #[rstest]
    #[case::git_worktree_missing(
        SafetyScenario::GitWorktreeMissing,
        SafetyClassification::Blocked,
        "worktree is missing"
    )]
    #[case::flag_worktree_missing(
        SafetyScenario::FlagWorktreeMissing,
        SafetyClassification::Blocked,
        "worktree is missing"
    )]
    #[case::git_branch_missing(
        SafetyScenario::GitBranchMissing,
        SafetyClassification::Blocked,
        "branch is missing"
    )]
    #[case::flag_branch_missing(
        SafetyScenario::FlagBranchMissing,
        SafetyClassification::Blocked,
        "branch is missing"
    )]
    #[case::git_conflicted(
        SafetyScenario::GitConflicted,
        SafetyClassification::Dangerous,
        "working tree has conflicts"
    )]
    #[case::flag_conflicted(
        SafetyScenario::FlagConflicted,
        SafetyClassification::Dangerous,
        "working tree has conflicts"
    )]
    #[case::git_dirty(
        SafetyScenario::GitDirty,
        SafetyClassification::NeedsConfirmation,
        "working tree has local changes"
    )]
    #[case::git_untracked(
        SafetyScenario::GitUntracked,
        SafetyClassification::NeedsConfirmation,
        "working tree has local changes"
    )]
    #[case::flag_dirty(
        SafetyScenario::FlagDirty,
        SafetyClassification::NeedsConfirmation,
        "working tree has local changes"
    )]
    #[case::git_unmerged(
        SafetyScenario::GitUnmerged,
        SafetyClassification::NeedsConfirmation,
        "branch is not merged"
    )]
    #[case::git_ahead(
        SafetyScenario::GitAhead,
        SafetyClassification::NeedsConfirmation,
        "branch has unpushed commits"
    )]
    #[case::git_unpushed_commits(
        SafetyScenario::GitUnpushedCommits,
        SafetyClassification::NeedsConfirmation,
        "branch has unpushed commits"
    )]
    #[case::flag_unpushed(
        SafetyScenario::FlagUnpushed,
        SafetyClassification::NeedsConfirmation,
        "branch has unpushed commits"
    )]
    fn cleanup_safety_classifies_each_risk_signal_independently(
        #[case] scenario: SafetyScenario,
        #[case] expected: SafetyClassification,
        #[case] expected_reason: &str,
    ) {
        let mut task = clean_merged_task();
        apply_safety_scenario(&mut task, scenario);

        let report = cleanup_safety(&task);

        assert_eq!(report.classification, expected);
        assert!(
            report
                .reasons
                .iter()
                .any(|reason| reason == expected_reason),
            "missing reason {expected_reason:?} in {:?}",
            report.reasons
        );
    }

    use super::merge_safety;
    use crate::lifecycle::{mark_active, mark_mergeable, mark_merged, mark_reviewable};

    fn reviewable_clean_task() -> Task {
        let mut task = clean_merged_task();
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();
        let git = task.git_status.as_mut().unwrap();
        git.merged = false;
        git.ahead = 0;
        git.unpushed_commits = 0;
        task
    }

    #[test]
    fn reviewable_clean_task_is_safe_to_merge() {
        let report = merge_safety(&reviewable_clean_task());

        assert_eq!(report.classification, SafetyClassification::Safe);
    }

    #[test]
    fn mergeable_clean_task_is_safe_to_merge() {
        let mut task = reviewable_clean_task();
        mark_mergeable(&mut task).unwrap();

        let report = merge_safety(&task);

        assert_eq!(report.classification, SafetyClassification::Safe);
    }

    #[test]
    fn merge_safety_blocks_non_review_lifecycle() {
        let mut task = reviewable_clean_task();
        mark_mergeable(&mut task).unwrap();
        mark_merged(&mut task).unwrap();

        let report = merge_safety(&task);

        assert_eq!(report.classification, SafetyClassification::Blocked);
        assert!(report
            .reasons
            .iter()
            .any(|reason| reason == "lifecycle is not reviewable or mergeable"));
    }

    #[test]
    fn merge_safety_flags_conflicted_worktree_as_dangerous() {
        let mut task = reviewable_clean_task();
        task.git_status.as_mut().unwrap().conflicted = true;
        task.add_side_flag(SideFlag::Conflicted);

        let report = merge_safety(&task);

        assert_eq!(report.classification, SafetyClassification::Dangerous);
    }

    #[test]
    fn merge_safety_flags_dirty_worktree_as_needs_confirmation() {
        let mut task = reviewable_clean_task();
        task.git_status.as_mut().unwrap().dirty = true;
        task.add_side_flag(SideFlag::Dirty);

        let report = merge_safety(&task);

        assert_eq!(
            report.classification,
            SafetyClassification::NeedsConfirmation
        );
    }

    #[test]
    fn merge_safety_flags_unpushed_branch_as_needs_confirmation() {
        let mut task = reviewable_clean_task();
        task.git_status.as_mut().unwrap().ahead = 2;
        task.git_status.as_mut().unwrap().unpushed_commits = 2;

        let report = merge_safety(&task);

        assert_eq!(
            report.classification,
            SafetyClassification::NeedsConfirmation
        );
    }

    #[test]
    fn merge_safety_blocks_missing_worktree() {
        let mut task = reviewable_clean_task();
        task.git_status.as_mut().unwrap().worktree_exists = false;
        task.add_side_flag(SideFlag::WorktreeMissing);

        let report = merge_safety(&task);

        assert_eq!(report.classification, SafetyClassification::Blocked);
    }

    #[test]
    fn merge_safety_blocks_when_git_status_unknown() {
        let mut task = reviewable_clean_task();
        task.git_status = None;

        let report = merge_safety(&task);

        assert_eq!(report.classification, SafetyClassification::Blocked);
        assert!(report
            .reasons
            .iter()
            .any(|reason| reason == "git status is unknown"));
    }
}

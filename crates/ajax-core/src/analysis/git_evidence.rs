use crate::{adapters::GitAdapter, models::GitStatus};

pub fn interpret_git_status(
    porcelain_branch_output: &str,
    previous: Option<&GitStatus>,
    merged: bool,
) -> Option<GitStatus> {
    let has_branch_evidence = porcelain_branch_output
        .lines()
        .any(|line| line.starts_with("## "));
    let mut git_status = GitAdapter::parse_status(porcelain_branch_output, merged);

    if !has_branch_evidence && porcelain_branch_output.trim().is_empty() {
        return previous.cloned();
    }

    if !has_branch_evidence {
        if let Some(previous) = previous {
            git_status.branch_exists = previous.branch_exists;
            git_status
                .current_branch
                .clone_from(&previous.current_branch);
        }
    }

    Some(git_status)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::{
        analysis::git_evidence::interpret_git_status,
        models::{GitStatus, SideFlag, Task},
    };

    fn previous_status() -> GitStatus {
        GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: true,
            ahead: 1,
            behind: 0,
            merged: false,
            untracked_files: 1,
            unpushed_commits: 1,
            conflicted: true,
            last_commit: Some("abc123".to_string()),
        }
    }

    struct GitEvidenceCase {
        porcelain_status: &'static str,
        previous: Option<GitStatus>,
        merged: bool,
        branch_exists: bool,
        current_branch: Option<&'static str>,
        dirty: bool,
        untracked_files: u32,
        conflicted: bool,
        unpushed_commits: u32,
        flags: &'static [SideFlag],
    }

    #[rstest]
    #[case::clean(GitEvidenceCase {
        porcelain_status: "## ajax/fix-login...origin/ajax/fix-login\n",
        previous: None,
        merged: false,
        branch_exists: true,
        current_branch: Some("ajax/fix-login"),
        dirty: false,
        untracked_files: 0,
        conflicted: false,
        unpushed_commits: 0,
        flags: &[],
    })]
    #[case::dirty(GitEvidenceCase {
        porcelain_status: "## ajax/fix-login...origin/ajax/fix-login\n M src/lib.rs\n?? notes.md\n",
        previous: None,
        merged: false,
        branch_exists: true,
        current_branch: Some("ajax/fix-login"),
        dirty: true,
        untracked_files: 1,
        conflicted: false,
        unpushed_commits: 0,
        flags: &[SideFlag::Dirty],
    })]
    #[case::conflicted(GitEvidenceCase {
        porcelain_status: "## ajax/fix-login...origin/ajax/fix-login\nUU src/lib.rs\n",
        previous: None,
        merged: false,
        branch_exists: true,
        current_branch: Some("ajax/fix-login"),
        dirty: true,
        untracked_files: 0,
        conflicted: true,
        unpushed_commits: 0,
        flags: &[SideFlag::Dirty, SideFlag::Conflicted],
    })]
    #[case::unpushed(GitEvidenceCase {
        porcelain_status: "## ajax/fix-login...origin/ajax/fix-login [ahead 2]\n",
        previous: None,
        merged: false,
        branch_exists: true,
        current_branch: Some("ajax/fix-login"),
        dirty: false,
        untracked_files: 0,
        conflicted: false,
        unpushed_commits: 2,
        flags: &[SideFlag::Unpushed],
    })]
    #[case::missing_branch(GitEvidenceCase {
        porcelain_status: "## HEAD (no branch)\n",
        previous: None,
        merged: false,
        branch_exists: false,
        current_branch: None,
        dirty: false,
        untracked_files: 0,
        conflicted: false,
        unpushed_commits: 0,
        flags: &[SideFlag::BranchMissing],
    })]
    #[case::partial_status_output(GitEvidenceCase {
        porcelain_status: " M src/lib.rs\n",
        previous: Some(previous_status()),
        merged: false,
        branch_exists: true,
        current_branch: Some("ajax/fix-login"),
        dirty: true,
        untracked_files: 0,
        conflicted: false,
        unpushed_commits: 0,
        flags: &[SideFlag::Dirty],
    })]
    fn interpreted_git_status_covers_status_and_side_flags(#[case] case: GitEvidenceCase) {
        let interpreted =
            interpret_git_status(case.porcelain_status, case.previous.as_ref(), case.merged)
                .expect("git status should produce evidence");
        let mut task = Task::new(
            crate::models::TaskId::new("task-1"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            "/tmp/worktrees/web-fix-login",
            "ajax-web-fix-login",
            "worktrunk",
            crate::models::AgentClient::Codex,
        );

        task.apply_git_status(interpreted);

        let status = task.git_status.as_ref().unwrap();
        assert!(status.worktree_exists);
        assert_eq!(status.branch_exists, case.branch_exists);
        assert_eq!(status.current_branch.as_deref(), case.current_branch);
        assert_eq!(status.dirty, case.dirty);
        assert_eq!(status.untracked_files, case.untracked_files);
        assert_eq!(status.conflicted, case.conflicted);
        assert_eq!(status.unpushed_commits, case.unpushed_commits);
        for flag in [
            SideFlag::BranchMissing,
            SideFlag::Dirty,
            SideFlag::Conflicted,
            SideFlag::Unpushed,
        ] {
            assert_eq!(
                task.has_side_flag(flag),
                case.flags.contains(&flag),
                "{flag:?}"
            );
        }
    }

    #[test]
    fn empty_partial_output_preserves_existing_status() {
        let previous = previous_status();

        let interpreted = interpret_git_status("", Some(&previous), false)
            .expect("empty status output should preserve previous evidence");

        assert_eq!(interpreted, previous);
    }

    #[test]
    fn empty_status_without_previous_evidence_returns_none() {
        let interpreted = interpret_git_status("", None, false);

        assert_eq!(interpreted, None);
    }
}

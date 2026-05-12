use super::command::{CommandMode, CommandSpec};
use crate::models::GitStatus;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitAdapter {
    program: String,
}

impl GitAdapter {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
        }
    }

    pub fn status(&self, worktree_path: &str) -> CommandSpec {
        CommandSpec::new(
            &self.program,
            ["-C", worktree_path, "status", "--porcelain=v1", "--branch"],
        )
    }

    pub fn add_worktree(
        &self,
        repo_path: &str,
        worktree_path: &str,
        branch: &str,
        start_point: &str,
    ) -> CommandSpec {
        CommandSpec {
            program: self.program.clone(),
            args: vec![
                "-C".to_string(),
                repo_path.to_string(),
                "worktree".to_string(),
                "add".to_string(),
                "-b".to_string(),
                branch.to_string(),
                worktree_path.to_string(),
                start_point.to_string(),
            ],
            cwd: None,
            mode: CommandMode::Capture,
        }
    }

    pub fn remove_worktree(&self, repo_path: &str, worktree_path: &str) -> CommandSpec {
        CommandSpec {
            program: self.program.clone(),
            args: vec![
                "-C".to_string(),
                repo_path.to_string(),
                "worktree".to_string(),
                "remove".to_string(),
                worktree_path.to_string(),
            ],
            cwd: None,
            mode: CommandMode::Capture,
        }
    }

    pub fn force_remove_worktree(&self, repo_path: &str, worktree_path: &str) -> CommandSpec {
        CommandSpec {
            program: self.program.clone(),
            args: vec![
                "-C".to_string(),
                repo_path.to_string(),
                "worktree".to_string(),
                "remove".to_string(),
                "--force".to_string(),
                worktree_path.to_string(),
            ],
            cwd: None,
            mode: CommandMode::Capture,
        }
    }

    pub fn delete_branch(&self, repo_path: &str, branch: &str) -> CommandSpec {
        CommandSpec {
            program: self.program.clone(),
            args: vec![
                "-C".to_string(),
                repo_path.to_string(),
                "branch".to_string(),
                "-d".to_string(),
                branch.to_string(),
            ],
            cwd: None,
            mode: CommandMode::Capture,
        }
    }

    pub fn force_delete_branch(&self, repo_path: &str, branch: &str) -> CommandSpec {
        CommandSpec {
            program: self.program.clone(),
            args: vec![
                "-C".to_string(),
                repo_path.to_string(),
                "branch".to_string(),
                "-D".to_string(),
                branch.to_string(),
            ],
            cwd: None,
            mode: CommandMode::Capture,
        }
    }

    pub fn switch_branch(&self, repo_path: &str, branch: &str) -> CommandSpec {
        CommandSpec::new(&self.program, ["-C", repo_path, "switch", branch])
    }

    pub fn merge_branch(&self, repo_path: &str, branch: &str) -> CommandSpec {
        CommandSpec::new(
            &self.program,
            ["-C", repo_path, "merge", "--ff-only", branch],
        )
    }

    pub fn merge_base_is_ancestor(
        &self,
        worktree_path: &str,
        ancestor: &str,
        descendant: &str,
    ) -> CommandSpec {
        CommandSpec::new(
            &self.program,
            [
                "-C",
                worktree_path,
                "merge-base",
                "--is-ancestor",
                ancestor,
                descendant,
            ],
        )
    }

    pub fn parse_status(porcelain_branch_output: &str, merged: bool) -> GitStatus {
        let mut status = GitStatus {
            worktree_exists: true,
            branch_exists: false,
            current_branch: None,
            dirty: false,
            ahead: 0,
            behind: 0,
            merged,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        };

        for line in porcelain_branch_output.lines() {
            if let Some(branch_line) = line.strip_prefix("## ") {
                status.current_branch = parse_current_branch(branch_line);
                status.branch_exists =
                    !branch_line.starts_with("No commits yet") && status.current_branch.is_some();
                apply_branch_divergence(&mut status, branch_line);
                continue;
            }

            if line.starts_with("??") {
                status.dirty = true;
                status.untracked_files += 1;
                continue;
            }

            if line.len() >= 2 {
                status.dirty = true;
                let code = &line[..2];
                if matches!(code, "DD" | "AU" | "UD" | "UA" | "DU" | "AA" | "UU") {
                    status.conflicted = true;
                }
            }
        }

        status.unpushed_commits = status.ahead;
        status
    }
}

fn parse_current_branch(branch_line: &str) -> Option<String> {
    if branch_line.starts_with("No commits yet") || branch_line.starts_with("HEAD ") {
        return None;
    }

    let branch = branch_line
        .split_once("...")
        .map_or(branch_line, |(branch, _)| branch);
    let branch = branch.split_once(' ').map_or(branch, |(branch, _)| branch);

    (!branch.is_empty()).then(|| branch.to_string())
}

fn apply_branch_divergence(status: &mut GitStatus, branch_line: &str) {
    let Some(open_bracket) = branch_line.find('[') else {
        return;
    };
    let Some(close_bracket) = branch_line[open_bracket..].find(']') else {
        return;
    };
    let divergence = &branch_line[open_bracket + 1..open_bracket + close_bracket];

    for part in divergence.split(',').map(str::trim) {
        if let Some(ahead) = part.strip_prefix("ahead ") {
            if let Ok(value) = ahead.parse::<u32>() {
                status.ahead = value;
            }
        }
        if let Some(behind) = part.strip_prefix("behind ") {
            if let Ok(value) = behind.parse::<u32>() {
                status.behind = value;
            }
        }
    }
}

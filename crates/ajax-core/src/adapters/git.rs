use super::command::CommandSpec;
use crate::models::GitStatus;
use std::time::Duration;

const GIT_FETCH_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitAdapter {
    program: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitWorktree {
    pub path: String,
    pub branch: Option<String>,
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

    pub fn list_worktrees(&self, repo_path: &str) -> CommandSpec {
        CommandSpec::new(
            &self.program,
            ["-C", repo_path, "worktree", "list", "--porcelain"],
        )
    }

    pub fn list_branches(&self, repo_path: &str) -> CommandSpec {
        CommandSpec::new(
            &self.program,
            ["-C", repo_path, "branch", "--format=%(refname:short)"],
        )
    }

    pub fn fetch_origin_branch(&self, repo_path: &str, branch: &str) -> CommandSpec {
        CommandSpec::new(&self.program, ["-C", repo_path, "fetch", "origin", branch])
            .with_timeout(GIT_FETCH_TIMEOUT)
    }

    pub fn add_worktree(
        &self,
        repo_path: &str,
        worktree_path: &str,
        branch: &str,
        start_point: &str,
    ) -> CommandSpec {
        CommandSpec::new(
            &self.program,
            [
                "-C",
                repo_path,
                "worktree",
                "add",
                "-b",
                branch,
                worktree_path,
                start_point,
            ],
        )
    }

    pub fn add_worktree_existing_branch(
        &self,
        repo_path: &str,
        worktree_path: &str,
        branch: &str,
    ) -> CommandSpec {
        CommandSpec::new(
            &self.program,
            ["-C", repo_path, "worktree", "add", worktree_path, branch],
        )
    }

    pub fn remove_worktree(&self, repo_path: &str, worktree_path: &str) -> CommandSpec {
        CommandSpec::new(
            &self.program,
            ["-C", repo_path, "worktree", "remove", worktree_path],
        )
    }

    pub fn force_remove_worktree(&self, repo_path: &str, worktree_path: &str) -> CommandSpec {
        CommandSpec::new(
            &self.program,
            [
                "-C",
                repo_path,
                "worktree",
                "remove",
                "--force",
                worktree_path,
            ],
        )
    }

    pub fn delete_branch(&self, repo_path: &str, branch: &str) -> CommandSpec {
        CommandSpec::new(&self.program, ["-C", repo_path, "branch", "-d", branch])
    }

    pub fn force_delete_branch(&self, repo_path: &str, branch: &str) -> CommandSpec {
        CommandSpec::new(&self.program, ["-C", repo_path, "branch", "-D", branch])
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

    pub fn parse_worktrees(porcelain_output: &str) -> Vec<GitWorktree> {
        porcelain_output
            .split("\n\n")
            .filter_map(parse_worktree_entry)
            .collect()
    }

    pub fn parse_branches(branch_output: &str) -> Vec<String> {
        branch_output
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect()
    }
}

fn parse_worktree_entry(entry: &str) -> Option<GitWorktree> {
    let mut path = None;
    let mut branch = None;

    for line in entry.lines() {
        if let Some(value) = line.strip_prefix("worktree ") {
            path = Some(value.to_string());
            continue;
        }
        if let Some(value) = line.strip_prefix("branch refs/heads/") {
            branch = Some(value.to_string());
        }
    }

    path.zip(branch).map(|(path, branch)| GitWorktree {
        path,
        branch: Some(branch),
    })
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

#[cfg(test)]
mod tests {
    use super::{CommandSpec, GitAdapter, GIT_FETCH_TIMEOUT};

    #[test]
    fn sync_default_branch_fetches_origin_ref_before_fast_forwarding_local_branch() {
        let adapter = GitAdapter::new("git");

        assert_eq!(
            adapter.fetch_origin_branch("/repos/web", "main"),
            CommandSpec::new("git", ["-C", "/repos/web", "fetch", "origin", "main"])
                .with_timeout(GIT_FETCH_TIMEOUT)
        );
    }

    #[test]
    fn list_worktrees_uses_porcelain_for_repo_root() {
        let adapter = GitAdapter::new("git");

        assert_eq!(
            adapter.list_worktrees("/repos/ajax-cli"),
            CommandSpec::new(
                "git",
                ["-C", "/repos/ajax-cli", "worktree", "list", "--porcelain"]
            )
        );
    }

    #[test]
    fn add_worktree_existing_branch_attaches_without_creating_branch() {
        let adapter = GitAdapter::new("git");

        assert_eq!(
            adapter.add_worktree_existing_branch(
                "/Users/matt/projects/web",
                "/tmp/worktrees/web-fix-login",
                "ajax/fix-login",
            ),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "worktree",
                    "add",
                    "/tmp/worktrees/web-fix-login",
                    "ajax/fix-login",
                ],
            )
        );
    }

    #[test]
    fn parse_worktrees_keeps_paths_and_branches() {
        let output = "\
worktree /repos/ajax-cli
HEAD abc123
branch refs/heads/main

worktree /repos/ajax-cli__worktrees/ajax-code
HEAD def456
branch refs/heads/ajax/code

worktree /repos/ajax-cli__worktrees/manual
HEAD fedcba
detached

";

        let worktrees = GitAdapter::parse_worktrees(output);

        assert_eq!(worktrees.len(), 2);
        assert_eq!(worktrees[0].path, "/repos/ajax-cli");
        assert_eq!(worktrees[0].branch.as_deref(), Some("main"));
        assert_eq!(worktrees[1].path, "/repos/ajax-cli__worktrees/ajax-code");
        assert_eq!(worktrees[1].branch.as_deref(), Some("ajax/code"));
    }
}

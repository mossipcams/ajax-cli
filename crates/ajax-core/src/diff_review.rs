use crate::adapters::{CommandRunner, GithubChecksAdapter};
use crate::models::Task;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const AJAX_PULL_REQUESTS_KEY: &str = "ajax_pull_requests";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PullRequestState {
    Open,
    Merged,
    Closed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PullRequestRef {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: PullRequestState,
    pub head_ref: String,
    pub head_sha: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffHunk {
    pub header: String,
    pub lines: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffFile {
    pub path: String,
    pub status: String,
    pub additions: u32,
    pub deletions: u32,
    pub hunks: Vec<DiffHunk>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiffSource {
    Pr { number: u64 },
    Local,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskDiffProjection {
    pub source: DiffSource,
    pub files: Vec<DiffFile>,
    pub pr: Option<PullRequestRef>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiffReviewError {
    TaskNotFound,
    Unobservable(String),
    PrNotFound(u64),
}

pub fn stored_pull_requests(task: &Task) -> Vec<PullRequestRef> {
    task.metadata
        .get(AJAX_PULL_REQUESTS_KEY)
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default()
}

pub fn remember_pull_requests(task: &mut Task, refs: &[PullRequestRef]) {
    let stored = stored_pull_requests(task);
    let merged = merge_pull_request_lists(&stored, refs);
    if let Ok(json) = serde_json::to_string(&merged) {
        task.metadata
            .insert(AJAX_PULL_REQUESTS_KEY.to_string(), json);
    }
}

pub fn merge_pull_request_lists(
    stored: &[PullRequestRef],
    live: &[PullRequestRef],
) -> Vec<PullRequestRef> {
    let mut by_number: HashMap<u64, PullRequestRef> = HashMap::new();
    for pr in stored {
        by_number.insert(pr.number, pr.clone());
    }
    for pr in live {
        by_number.insert(pr.number, pr.clone());
    }

    let mut merged: Vec<_> = by_number.into_values().collect();
    merged.sort_by(|left, right| {
        let left_open = matches!(left.state, PullRequestState::Open);
        let right_open = matches!(right.state, PullRequestState::Open);
        match (left_open, right_open) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => right.number.cmp(&left.number),
        }
    });
    merged
}

pub fn select_default_pr(prs: &[PullRequestRef]) -> Option<&PullRequestRef> {
    let open: Vec<_> = prs
        .iter()
        .filter(|pr| matches!(pr.state, PullRequestState::Open))
        .collect();
    if !open.is_empty() {
        return open.into_iter().max_by_key(|pr| pr.number);
    }
    prs.iter().max_by_key(|pr| pr.number)
}

pub fn parse_unified_diff(patch: &str) -> Vec<DiffFile> {
    let trimmed = patch.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut starts = trimmed
        .match_indices("\ndiff --git ")
        .map(|(index, _)| index + 1)
        .collect::<Vec<_>>();
    if trimmed.starts_with("diff --git ") {
        starts.insert(0, 0);
    }

    if starts.is_empty() {
        return Vec::new();
    }

    let mut files = Vec::new();
    for (index, &start) in starts.iter().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(trimmed.len());
        let section = &trimmed[start..end];
        if let Some(file) = parse_diff_section(section) {
            files.push(file);
        }
    }
    files
}

fn parse_diff_section(section: &str) -> Option<DiffFile> {
    let mut lines = section.lines();
    let first = lines.next()?;
    let path_line = first.strip_prefix("diff --git ").unwrap_or(first);
    let path = extract_path(path_line)?;

    let rest: Vec<&str> = lines.collect();
    let body = rest.join("\n");
    let status = if body.contains("new file mode") {
        "added"
    } else if body.contains("deleted file mode") {
        "deleted"
    } else if body.contains("rename from") {
        "renamed"
    } else {
        "modified"
    };

    let mut hunks = Vec::new();
    let mut additions = 0u32;
    let mut deletions = 0u32;

    let mut index = 0;
    while index < rest.len() {
        if rest[index].starts_with("@@ ") {
            let header = rest[index].to_string();
            let mut hunk_lines = Vec::new();
            index += 1;
            while index < rest.len()
                && !rest[index].starts_with("@@ ")
                && !rest[index].starts_with("diff --git ")
            {
                let line = rest[index];
                if line.starts_with('+') && !line.starts_with("+++") {
                    additions += 1;
                    hunk_lines.push(line.to_string());
                } else if line.starts_with('-') && !line.starts_with("---") {
                    deletions += 1;
                    hunk_lines.push(line.to_string());
                } else if line.starts_with(' ') {
                    hunk_lines.push(line.to_string());
                }
                index += 1;
            }
            hunks.push(DiffHunk {
                header,
                lines: hunk_lines,
            });
        } else {
            index += 1;
        }
    }

    Some(DiffFile {
        path,
        status: status.to_string(),
        additions,
        deletions,
        hunks,
    })
}

fn extract_path(path_line: &str) -> Option<String> {
    let parts: Vec<&str> = path_line.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }
    let path = parts[1];
    Some(path.strip_prefix("b/").unwrap_or(path).to_string())
}

/// Observe live GitHub PRs for the task branch, merge with stored refs, and
/// persist the merged list back onto the task metadata when live observation
/// succeeds. When `gh` is unobservable, return stored refs (possibly empty)
/// instead of failing — Diff Review can still fall back to a local diff.
pub fn observe_task_pull_requests(
    task: &mut Task,
    runner: &mut impl CommandRunner,
    github: &GithubChecksAdapter,
) -> Result<Vec<PullRequestRef>, DiffReviewError> {
    let worktree = task.worktree_path.display().to_string();
    let command = github.pr_list(&worktree, &task.branch);
    let live = match GithubChecksAdapter::parse_pr_list(&runner.run(&command)) {
        Ok(prs) => prs,
        Err(_reason) => return Ok(stored_pull_requests(task)),
    };
    remember_pull_requests(task, &live);
    Ok(stored_pull_requests(task))
}

/// Build a structured diff for a selected PR, or the local base...HEAD fallback.
pub fn project_task_diff(
    task: &Task,
    runner: &mut impl CommandRunner,
    github: &GithubChecksAdapter,
    pr_number: Option<u64>,
    force_local: bool,
) -> Result<TaskDiffProjection, DiffReviewError> {
    let worktree = task.worktree_path.display().to_string();
    let prs = stored_pull_requests(task);

    if force_local || pr_number.is_none() && prs.is_empty() {
        let command = github.local_diff(&worktree, &task.base_branch);
        let output = runner
            .run(&command)
            .map_err(|error| DiffReviewError::Unobservable(error.to_string()))?;
        if output.status_code != 0 {
            return Err(DiffReviewError::Unobservable(if output.stderr.is_empty() {
                format!("git diff failed with status {}", output.status_code)
            } else {
                output.stderr
            }));
        }
        return Ok(TaskDiffProjection {
            source: DiffSource::Local,
            files: parse_unified_diff(&output.stdout),
            pr: None,
        });
    }

    let number = match pr_number {
        Some(number) => number,
        None => select_default_pr(&prs)
            .map(|pr| pr.number)
            .ok_or(DiffReviewError::TaskNotFound)?,
    };
    let pr = prs
        .iter()
        .find(|pr| pr.number == number)
        .cloned()
        .or_else(|| {
            Some(PullRequestRef {
                number,
                title: format!("#{number}"),
                url: String::new(),
                state: PullRequestState::Open,
                head_ref: task.branch.clone(),
                head_sha: None,
            })
        });

    let command = github.pr_diff(&worktree, number);
    let output = runner.run(&command).map_err(|error| {
        if error.to_string().contains("could not find") {
            DiffReviewError::PrNotFound(number)
        } else {
            DiffReviewError::Unobservable(error.to_string())
        }
    });
    let output = match output {
        Ok(output) => output,
        Err(DiffReviewError::Unobservable(_)) if !force_local => {
            // Hybrid fallback: PR patch unavailable → local base...HEAD.
            return project_task_diff(task, runner, github, None, true);
        }
        Err(error) => return Err(error),
    };
    if output.status_code != 0 {
        let stderr = output.stderr.to_ascii_lowercase();
        if stderr.contains("could not find") || stderr.contains("no pull requests") {
            return Err(DiffReviewError::PrNotFound(number));
        }
        if !force_local {
            return project_task_diff(task, runner, github, None, true);
        }
        return Err(DiffReviewError::Unobservable(if output.stderr.is_empty() {
            format!("gh pr diff failed with status {}", output.status_code)
        } else {
            output.stderr
        }));
    }

    Ok(TaskDiffProjection {
        source: DiffSource::Pr { number },
        files: parse_unified_diff(&output.stdout),
        pr,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        merge_pull_request_lists, parse_unified_diff, remember_pull_requests, select_default_pr,
        stored_pull_requests, PullRequestRef, PullRequestState,
    };
    use crate::models::{AgentClient, Task, TaskId};

    fn sample_pr(number: u64, state: PullRequestState, title: &str) -> PullRequestRef {
        PullRequestRef {
            number,
            title: title.to_string(),
            url: format!("https://github.com/org/repo/pull/{number}"),
            state,
            head_ref: "feature".to_string(),
            head_sha: Some(format!("sha{number}")),
        }
    }

    fn sample_task() -> Task {
        Task::new(
            TaskId::new("task-1"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            "/tmp/worktrees/web-fix-login",
            "ajax-web-fix-login",
            "task",
            AgentClient::Codex,
        )
    }

    #[test]
    fn merge_pull_request_lists_keeps_merged_and_open_for_same_branch_history() {
        let stored = vec![
            sample_pr(10, PullRequestState::Merged, "First attempt"),
            sample_pr(8, PullRequestState::Merged, "Earlier"),
        ];
        let live = vec![sample_pr(12, PullRequestState::Open, "Retry")];

        let merged = merge_pull_request_lists(&stored, &live);

        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].number, 12);
        assert_eq!(merged[0].state, PullRequestState::Open);
        assert!(merged.iter().any(|pr| pr.number == 10));
        assert!(merged.iter().any(|pr| pr.number == 8));
    }

    #[test]
    fn merge_pull_request_lists_prefers_live_fields_on_conflict() {
        let stored = vec![PullRequestRef {
            number: 5,
            title: "stale title".to_string(),
            url: "https://example.com/old".to_string(),
            state: PullRequestState::Open,
            head_ref: "old-branch".to_string(),
            head_sha: Some("old-sha".to_string()),
        }];
        let live = vec![PullRequestRef {
            number: 5,
            title: "fresh title".to_string(),
            url: "https://example.com/new".to_string(),
            state: PullRequestState::Merged,
            head_ref: "new-branch".to_string(),
            head_sha: Some("new-sha".to_string()),
        }];

        let merged = merge_pull_request_lists(&stored, &live);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].title, "fresh title");
        assert_eq!(merged[0].state, PullRequestState::Merged);
        assert_eq!(merged[0].head_sha.as_deref(), Some("new-sha"));
    }

    #[test]
    fn select_default_pr_prefers_open_over_higher_numbered_merged() {
        let prs = vec![
            sample_pr(20, PullRequestState::Merged, "merged newer"),
            sample_pr(15, PullRequestState::Open, "open retry"),
            sample_pr(10, PullRequestState::Merged, "merged older"),
        ];

        let selected = select_default_pr(&prs).expect("expected a default pr");

        assert_eq!(selected.number, 15);
        assert_eq!(selected.state, PullRequestState::Open);
    }

    #[test]
    fn select_default_pr_picks_highest_number_when_none_open() {
        let prs = vec![
            sample_pr(10, PullRequestState::Merged, "older"),
            sample_pr(20, PullRequestState::Closed, "newer closed"),
        ];

        let selected = select_default_pr(&prs).expect("expected a default pr");

        assert_eq!(selected.number, 20);
    }

    #[test]
    fn parse_unified_diff_parses_multi_file_patch_with_hunks_and_counts() {
        let patch = "\
diff --git a/src/foo.rs b/src/foo.rs
index 1111111..2222222 100644
--- a/src/foo.rs
+++ b/src/foo.rs
@@ -1,3 +1,4 @@
 fn foo() {
     let x = 1;
+    let y = 2;
     let z = 3;
 }
diff --git a/src/bar.rs b/src/bar.rs
new file mode 100644
index 0000000..3333333
--- /dev/null
+++ b/src/bar.rs
@@ -0,0 +1,2 @@
+alpha
+beta
";

        let files = parse_unified_diff(patch);

        assert_eq!(files.len(), 2);

        let foo = &files[0];
        assert_eq!(foo.path, "src/foo.rs");
        assert_eq!(foo.status, "modified");
        assert_eq!(foo.additions, 1);
        assert_eq!(foo.deletions, 0);
        assert_eq!(foo.hunks.len(), 1);
        assert_eq!(foo.hunks[0].header, "@@ -1,3 +1,4 @@");
        assert!(foo.hunks[0]
            .lines
            .iter()
            .any(|line| line == "+    let y = 2;"));

        let bar = &files[1];
        assert_eq!(bar.path, "src/bar.rs");
        assert_eq!(bar.status, "added");
        assert_eq!(bar.additions, 2);
        assert_eq!(bar.deletions, 0);
        assert_eq!(bar.hunks.len(), 1);
    }

    #[test]
    fn remember_pull_requests_round_trips_through_task_metadata() {
        let mut task = sample_task();
        let refs = vec![
            sample_pr(7, PullRequestState::Open, "Open PR"),
            sample_pr(3, PullRequestState::Merged, "Merged PR"),
        ];

        remember_pull_requests(&mut task, &refs);

        let stored = stored_pull_requests(&task);
        assert_eq!(stored.len(), 2);
        assert_eq!(stored[0].number, 7);
        assert_eq!(stored[1].number, 3);
    }

    #[test]
    fn empty_stored_pull_requests_returns_empty_vec() {
        let task = sample_task();

        assert!(stored_pull_requests(&task).is_empty());
    }
}

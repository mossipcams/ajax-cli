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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffFileRole {
    Signal,
    Noise,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffFile {
    pub path: String,
    pub status: String,
    pub additions: u32,
    pub deletions: u32,
    pub role: DiffFileRole,
    pub hunks: Vec<DiffHunk>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DiffTotals {
    pub files: u32,
    pub signal: u32,
    pub noise: u32,
    pub additions: u32,
    pub deletions: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffFlagKind {
    UnexpectedPath,
    DeletedTest,
    SecretPattern,
    PermissionWiden,
    DependencyManifest,
    DeletedCheckPath,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffFlagSeverity {
    Info,
    Warn,
    Critical,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DiffFlag {
    pub kind: DiffFlagKind,
    pub severity: DiffFlagSeverity,
    pub path: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DiffJudgment {
    pub totals: DiffTotals,
    pub reading_order: Vec<String>,
    pub flags: Vec<DiffFlag>,
}

/// Deterministic path heuristics for vibe review: lockfiles, build output, and
/// minified/generated artifacts are noise; everything else is signal.
pub fn classify_diff_path(path: &str) -> DiffFileRole {
    let basename = path.rsplit('/').next().unwrap_or(path);

    if matches!(
        basename,
        "Cargo.lock" | "package-lock.json" | "yarn.lock" | "pnpm-lock.yaml"
    ) {
        return DiffFileRole::Noise;
    }

    if basename.ends_with(".lock") {
        return DiffFileRole::Noise;
    }

    if basename.ends_with(".min.js") || basename.ends_with(".map") {
        return DiffFileRole::Noise;
    }

    for segment in ["/dist/", "/target/", "/.next/"] {
        if path.contains(segment) {
            return DiffFileRole::Noise;
        }
    }

    if path.starts_with("dist/") || path.starts_with("target/") || path.starts_with(".next/") {
        return DiffFileRole::Noise;
    }

    DiffFileRole::Signal
}

const EXPECTED_PATH_PREFIXES: &[&str] = &[
    "crates/",
    "scripts/",
    ".github/",
    "docs/",
    ".planning/",
    "web/",
];

fn path_is_expected(path: &str) -> bool {
    EXPECTED_PATH_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(prefix))
}

fn is_test_path(path: &str) -> bool {
    let basename = path.rsplit('/').next().unwrap_or(path);
    basename.ends_with("_test.rs")
        || path.contains(".test.")
        || path.contains(".spec.")
        || path.contains("/tests/")
        || path.starts_with("tests/")
}

fn is_deleted_check_path(path: &str) -> bool {
    if path.starts_with(".github/workflows/") {
        return true;
    }
    let basename = path.rsplit('/').next().unwrap_or(path);
    path.starts_with("scripts/") && basename.contains("verify")
}

fn is_dependency_manifest(path: &str) -> bool {
    matches!(
        path.rsplit('/').next().unwrap_or(path),
        "Cargo.toml" | "package.json" | "Cargo.lock"
    )
}

fn secret_severity(line: &str) -> Option<DiffFlagSeverity> {
    let lower = line.to_ascii_lowercase();
    if line.contains("-----BEGIN") || line.contains("ghp_") || line.contains("AKIA") {
        return Some(DiffFlagSeverity::Critical);
    }
    if line.contains("sk-") || lower.contains("api_key=") {
        return Some(DiffFlagSeverity::Warn);
    }
    None
}

fn is_added_hunk_line(line: &str) -> bool {
    line.starts_with('+') && !line.starts_with("+++")
}

fn scan_added_lines(file: &DiffFile) -> (Option<DiffFlagSeverity>, bool) {
    let mut secret: Option<DiffFlagSeverity> = None;
    let mut permission_widen = false;
    for hunk in &file.hunks {
        for line in &hunk.lines {
            if !is_added_hunk_line(line) {
                continue;
            }
            if let Some(severity) = secret_severity(line) {
                secret = Some(match secret {
                    Some(existing) => existing.max(severity),
                    None => severity,
                });
            }
            if line.contains("chmod 777") || line.contains("0o777") {
                permission_widen = true;
            }
        }
    }
    (secret, permission_widen)
}

fn flag(kind: DiffFlagKind, severity: DiffFlagSeverity, path: &str) -> DiffFlag {
    DiffFlag {
        kind,
        severity,
        path: path.to_string(),
    }
}

/// Deterministic vibe-judgment projection over already-parsed diff files.
pub fn assess_diff_judgment(files: &[DiffFile]) -> DiffJudgment {
    let mut signal = 0u32;
    let mut noise = 0u32;
    let mut additions = 0u32;
    let mut deletions = 0u32;
    let mut flags = Vec::new();

    for file in files {
        match file.role {
            DiffFileRole::Signal => signal += 1,
            DiffFileRole::Noise => noise += 1,
        }
        additions += file.additions;
        deletions += file.deletions;

        if is_dependency_manifest(&file.path) {
            flags.push(flag(
                DiffFlagKind::DependencyManifest,
                DiffFlagSeverity::Info,
                &file.path,
            ));
        }

        if file.status == "deleted" && is_test_path(&file.path) {
            flags.push(flag(
                DiffFlagKind::DeletedTest,
                DiffFlagSeverity::Warn,
                &file.path,
            ));
        }

        if file.status == "deleted" && is_deleted_check_path(&file.path) {
            flags.push(flag(
                DiffFlagKind::DeletedCheckPath,
                DiffFlagSeverity::Warn,
                &file.path,
            ));
        }

        let (secret, permission_widen) = scan_added_lines(file);
        if let Some(severity) = secret {
            flags.push(flag(DiffFlagKind::SecretPattern, severity, &file.path));
        }
        if permission_widen {
            flags.push(flag(
                DiffFlagKind::PermissionWiden,
                DiffFlagSeverity::Warn,
                &file.path,
            ));
        }

        if file.role == DiffFileRole::Signal && !path_is_expected(&file.path) {
            flags.push(flag(
                DiffFlagKind::UnexpectedPath,
                DiffFlagSeverity::Info,
                &file.path,
            ));
        }
    }

    let mut signal_files: Vec<&DiffFile> = files
        .iter()
        .filter(|file| file.role == DiffFileRole::Signal)
        .collect();
    signal_files.sort_by(|left, right| {
        let churn_cmp = (right.additions + right.deletions).cmp(&(left.additions + left.deletions));
        if churn_cmp != std::cmp::Ordering::Equal {
            return churn_cmp;
        }
        left.path.cmp(&right.path)
    });
    let reading_order = signal_files
        .into_iter()
        .map(|file| file.path.clone())
        .collect();

    DiffJudgment {
        totals: DiffTotals {
            files: files.len() as u32,
            signal,
            noise,
            additions,
            deletions,
        },
        reading_order,
        flags,
    }
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
    /// PR number requested before hybrid fallback to local base...HEAD.
    pub fell_back_from_pr: Option<u64>,
    pub judgment: DiffJudgment,
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
        path: path.clone(),
        status: status.to_string(),
        additions,
        deletions,
        role: classify_diff_path(&path),
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
        let files = parse_unified_diff(&output.stdout);
        return Ok(TaskDiffProjection {
            source: DiffSource::Local,
            judgment: assess_diff_judgment(&files),
            files,
            pr: None,
            fell_back_from_pr: None,
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
            return local_diff_with_pr_fallback(task, runner, github, number);
        }
        Err(error) => return Err(error),
    };
    if output.status_code != 0 {
        let stderr = output.stderr.to_ascii_lowercase();
        if stderr.contains("could not find") || stderr.contains("no pull requests") {
            return Err(DiffReviewError::PrNotFound(number));
        }
        if !force_local {
            return local_diff_with_pr_fallback(task, runner, github, number);
        }
        return Err(DiffReviewError::Unobservable(if output.stderr.is_empty() {
            format!("gh pr diff failed with status {}", output.status_code)
        } else {
            output.stderr
        }));
    }

    let files = parse_unified_diff(&output.stdout);
    Ok(TaskDiffProjection {
        source: DiffSource::Pr { number },
        judgment: assess_diff_judgment(&files),
        files,
        pr,
        fell_back_from_pr: None,
    })
}

fn local_diff_with_pr_fallback(
    task: &Task,
    runner: &mut impl CommandRunner,
    github: &GithubChecksAdapter,
    pr_number: u64,
) -> Result<TaskDiffProjection, DiffReviewError> {
    let projection = project_task_diff(task, runner, github, None, true)?;
    Ok(TaskDiffProjection {
        fell_back_from_pr: Some(pr_number),
        ..projection
    })
}

#[cfg(test)]
mod tests {
    use super::{
        assess_diff_judgment, classify_diff_path, merge_pull_request_lists, parse_unified_diff,
        project_task_diff, remember_pull_requests, select_default_pr, stored_pull_requests,
        DiffFile, DiffFileRole, DiffFlagKind, DiffFlagSeverity, DiffHunk, PullRequestRef,
        PullRequestState,
    };
    use crate::adapters::{
        CommandOutput, CommandRunError, CommandRunner, CommandSpec, GithubChecksAdapter,
    };
    use crate::models::{AgentClient, Task, TaskId};
    use std::collections::VecDeque;

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
        assert_eq!(foo.role, DiffFileRole::Signal);

        let bar = &files[1];
        assert_eq!(bar.path, "src/bar.rs");
        assert_eq!(bar.status, "added");
        assert_eq!(bar.additions, 2);
        assert_eq!(bar.deletions, 0);
        assert_eq!(bar.hunks.len(), 1);
        assert_eq!(bar.role, DiffFileRole::Signal);
    }

    #[test]
    fn classify_diff_path_marks_lockfiles_and_generated_paths_as_noise() {
        assert_eq!(classify_diff_path("Cargo.lock"), DiffFileRole::Noise);
        assert_eq!(classify_diff_path("package-lock.json"), DiffFileRole::Noise);
        assert_eq!(classify_diff_path("yarn.lock"), DiffFileRole::Noise);
        assert_eq!(classify_diff_path("pnpm-lock.yaml"), DiffFileRole::Noise);
        assert_eq!(classify_diff_path("foo.lock"), DiffFileRole::Noise);
        assert_eq!(
            classify_diff_path("crates/app/target/debug/app"),
            DiffFileRole::Noise
        );
        assert_eq!(classify_diff_path("dist/bundle.js"), DiffFileRole::Noise);
        assert_eq!(
            classify_diff_path(".next/static/chunk.js"),
            DiffFileRole::Noise
        );
        assert_eq!(classify_diff_path("web/app.min.js"), DiffFileRole::Noise);
        assert_eq!(classify_diff_path("web/app.js.map"), DiffFileRole::Noise);
        assert_eq!(classify_diff_path("src/lib.rs"), DiffFileRole::Signal);
    }

    #[test]
    fn parse_unified_diff_preserves_file_order_while_annotating_roles() {
        let patch = "\
diff --git a/Cargo.lock b/Cargo.lock
--- a/Cargo.lock
+++ b/Cargo.lock
@@ -1 +1,2 @@
 keep
+new
diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -1 +1,2 @@
 keep
+code
";

        let files = parse_unified_diff(patch);

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "Cargo.lock");
        assert_eq!(files[0].role, DiffFileRole::Noise);
        assert_eq!(files[1].path, "src/main.rs");
        assert_eq!(files[1].role, DiffFileRole::Signal);
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

    struct QueuedRunner {
        outputs: VecDeque<Result<CommandOutput, CommandRunError>>,
    }

    impl QueuedRunner {
        fn new(outputs: Vec<Result<CommandOutput, CommandRunError>>) -> Self {
            Self {
                outputs: outputs.into(),
            }
        }
    }

    impl CommandRunner for QueuedRunner {
        fn run(&mut self, _command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.outputs
                .pop_front()
                .unwrap_or(Err(CommandRunError::SpawnFailed(
                    "queued runner exhausted".into(),
                )))
        }
    }

    fn ok(stdout: &str) -> Result<CommandOutput, CommandRunError> {
        Ok(CommandOutput {
            status_code: 0,
            stdout: stdout.to_string(),
            stderr: String::new(),
        })
    }

    #[test]
    fn project_task_diff_hybrid_fallback_sets_fell_back_from_pr() {
        let mut task = sample_task();
        remember_pull_requests(&mut task, &[sample_pr(12, PullRequestState::Open, "Retry")]);
        let patch = "\
diff --git a/src/a.rs b/src/a.rs
--- a/src/a.rs
+++ b/src/a.rs
@@ -1 +1,2 @@
 keep
+new
";
        let mut runner = QueuedRunner::new(vec![
            Err(CommandRunError::SpawnFailed(
                "gh pr diff unavailable".into(),
            )),
            ok(patch),
        ]);
        let github = GithubChecksAdapter::new("gh");

        let projection = project_task_diff(&task, &mut runner, &github, Some(12), false)
            .expect("hybrid fallback");

        assert_eq!(projection.source, super::DiffSource::Local);
        assert_eq!(projection.fell_back_from_pr, Some(12));
        assert_eq!(projection.files.len(), 1);
    }

    #[test]
    fn project_task_diff_force_local_does_not_set_fell_back_from_pr() {
        let task = sample_task();
        let patch = "\
diff --git a/src/a.rs b/src/a.rs
--- a/src/a.rs
+++ b/src/a.rs
@@ -1 +1,2 @@
 keep
+new
";
        let mut runner = QueuedRunner::new(vec![ok(patch)]);
        let github = GithubChecksAdapter::new("gh");

        let projection =
            project_task_diff(&task, &mut runner, &github, None, true).expect("local diff");

        assert_eq!(projection.source, super::DiffSource::Local);
        assert_eq!(projection.fell_back_from_pr, None);
    }

    fn file(
        path: &str,
        status: &str,
        additions: u32,
        deletions: u32,
        role: DiffFileRole,
        hunks: Vec<DiffHunk>,
    ) -> DiffFile {
        DiffFile {
            path: path.to_string(),
            status: status.to_string(),
            additions,
            deletions,
            role,
            hunks,
        }
    }

    #[test]
    fn assess_diff_judgment_empty_files_is_zeroed() {
        let judgment = assess_diff_judgment(&[]);
        assert_eq!(judgment.totals.files, 0);
        assert_eq!(judgment.totals.signal, 0);
        assert_eq!(judgment.totals.noise, 0);
        assert_eq!(judgment.totals.additions, 0);
        assert_eq!(judgment.totals.deletions, 0);
        assert!(judgment.reading_order.is_empty());
        assert!(judgment.flags.is_empty());
    }

    #[test]
    fn assess_diff_judgment_reading_order_sorts_signal_by_churn_then_path() {
        let files = vec![
            file(
                "crates/b.rs",
                "modified",
                1,
                0,
                DiffFileRole::Signal,
                vec![],
            ),
            file("Cargo.lock", "modified", 50, 0, DiffFileRole::Noise, vec![]),
            file(
                "crates/a.rs",
                "modified",
                5,
                5,
                DiffFileRole::Signal,
                vec![],
            ),
            file(
                "crates/c.rs",
                "modified",
                5,
                5,
                DiffFileRole::Signal,
                vec![],
            ),
        ];

        let judgment = assess_diff_judgment(&files);

        assert_eq!(
            judgment.reading_order,
            vec![
                "crates/a.rs".to_string(),
                "crates/c.rs".to_string(),
                "crates/b.rs".to_string()
            ]
        );
        assert_eq!(judgment.totals.files, 4);
        assert_eq!(judgment.totals.signal, 3);
        assert_eq!(judgment.totals.noise, 1);
        assert_eq!(judgment.totals.additions, 61);
    }

    #[test]
    fn assess_diff_judgment_flags_deleted_test_and_manifest_and_unexpected() {
        let files = vec![
            file(
                "crates/ajax-core/src/foo_test.rs",
                "deleted",
                0,
                10,
                DiffFileRole::Signal,
                vec![],
            ),
            file("Cargo.toml", "modified", 1, 0, DiffFileRole::Signal, vec![]),
            file(
                "tmp/scratch.rs",
                "added",
                2,
                0,
                DiffFileRole::Signal,
                vec![],
            ),
        ];

        let judgment = assess_diff_judgment(&files);
        assert!(judgment.flags.iter().any(|flag| {
            flag.kind == DiffFlagKind::DeletedTest
                && flag.severity == DiffFlagSeverity::Warn
                && flag.path == "crates/ajax-core/src/foo_test.rs"
        }));
        assert!(judgment.flags.iter().any(|flag| {
            flag.kind == DiffFlagKind::DependencyManifest && flag.path == "Cargo.toml"
        }));
        assert!(judgment.flags.iter().any(|flag| {
            flag.kind == DiffFlagKind::UnexpectedPath && flag.path == "tmp/scratch.rs"
        }));
    }

    #[test]
    fn assess_diff_judgment_flags_critical_secret_in_added_line() {
        let files = vec![file(
            "crates/ajax-cli/src/main.rs",
            "modified",
            1,
            0,
            DiffFileRole::Signal,
            vec![DiffHunk {
                header: "@@ -1 +1,2 @@".to_string(),
                lines: vec![
                    " keep".to_string(),
                    "+token = \"ghp_abcdefghijklmnopqrstuvwxyz\"".to_string(),
                ],
            }],
        )];

        let judgment = assess_diff_judgment(&files);
        let secret = judgment
            .flags
            .iter()
            .find(|flag| flag.kind == DiffFlagKind::SecretPattern)
            .expect("secret flag");
        assert_eq!(secret.severity, DiffFlagSeverity::Critical);
    }

    #[test]
    fn assess_diff_judgment_flags_permission_widen_and_deleted_workflow() {
        let files = vec![
            file(
                "scripts/setup.sh",
                "modified",
                1,
                0,
                DiffFileRole::Signal,
                vec![DiffHunk {
                    header: "@@ -1 +1,2 @@".to_string(),
                    lines: vec!["+chmod 777 /tmp/x".to_string()],
                }],
            ),
            file(
                ".github/workflows/ci.yml",
                "deleted",
                0,
                20,
                DiffFileRole::Signal,
                vec![],
            ),
        ];

        let judgment = assess_diff_judgment(&files);
        assert!(judgment.flags.iter().any(|flag| {
            flag.kind == DiffFlagKind::PermissionWiden && flag.severity == DiffFlagSeverity::Warn
        }));
        assert!(judgment.flags.iter().any(|flag| {
            flag.kind == DiffFlagKind::DeletedCheckPath && flag.path == ".github/workflows/ci.yml"
        }));
    }
}

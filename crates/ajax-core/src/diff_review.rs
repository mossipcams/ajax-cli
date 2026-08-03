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
mod tests;

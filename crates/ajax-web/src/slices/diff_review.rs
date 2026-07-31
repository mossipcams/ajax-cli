//! Read-only Diff Review projections for the Web Cockpit.

use ajax_core::{
    adapters::{CommandRunner, GithubChecksAdapter},
    commands::CommandContext,
    diff_review::{
        observe_task_pull_requests, project_task_diff, DiffFile, DiffFileRole, DiffFlag,
        DiffFlagKind, DiffFlagSeverity, DiffHunk, DiffJudgment, DiffReviewError, DiffSource,
        DiffTotals, PullRequestRef, PullRequestState, TaskDiffProjection, AJAX_PULL_REQUESTS_KEY,
    },
    registry::Registry,
};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct PullRequestDto {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: &'static str,
    pub head_ref: String,
    pub head_sha: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DiffHunkDto {
    pub header: String,
    pub lines: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DiffFileDto {
    pub path: String,
    pub status: String,
    pub additions: u32,
    pub deletions: u32,
    pub role: &'static str,
    pub hunks: Vec<DiffHunkDto>,
}

fn role_label(role: DiffFileRole) -> &'static str {
    match role {
        DiffFileRole::Signal => "signal",
        DiffFileRole::Noise => "noise",
    }
}

fn flag_kind_label(kind: DiffFlagKind) -> &'static str {
    match kind {
        DiffFlagKind::UnexpectedPath => "unexpected_path",
        DiffFlagKind::DeletedTest => "deleted_test",
        DiffFlagKind::SecretPattern => "secret_pattern",
        DiffFlagKind::PermissionWiden => "permission_widen",
        DiffFlagKind::DependencyManifest => "dependency_manifest",
        DiffFlagKind::DeletedCheckPath => "deleted_check_path",
    }
}

fn flag_severity_label(severity: DiffFlagSeverity) -> &'static str {
    match severity {
        DiffFlagSeverity::Info => "info",
        DiffFlagSeverity::Warn => "warn",
        DiffFlagSeverity::Critical => "critical",
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct DiffTotalsDto {
    pub files: u32,
    pub signal: u32,
    pub noise: u32,
    pub additions: u32,
    pub deletions: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct DiffFlagDto {
    pub kind: &'static str,
    pub severity: &'static str,
    pub path: Option<String>,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DiffJudgmentDto {
    pub totals: DiffTotalsDto,
    pub reading_order: Vec<String>,
    pub flags: Vec<DiffFlagDto>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TaskDiffDto {
    pub source: String,
    pub pr: Option<PullRequestDto>,
    pub files: Vec<DiffFileDto>,
    pub fell_back_from_pr: Option<u64>,
    pub judgment: DiffJudgmentDto,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiffReviewRouteError {
    TaskNotFound,
    Unobservable(String),
    PrNotFound(u64),
}

impl From<DiffReviewError> for DiffReviewRouteError {
    fn from(error: DiffReviewError) -> Self {
        match error {
            DiffReviewError::TaskNotFound => Self::TaskNotFound,
            DiffReviewError::Unobservable(reason) => Self::Unobservable(reason),
            DiffReviewError::PrNotFound(number) => Self::PrNotFound(number),
        }
    }
}

fn find_task_id<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<ajax_core::models::TaskId, DiffReviewRouteError> {
    context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
        .map(|task| task.id.clone())
        .ok_or(DiffReviewRouteError::TaskNotFound)
}

fn state_label(state: PullRequestState) -> &'static str {
    match state {
        PullRequestState::Open => "OPEN",
        PullRequestState::Merged => "MERGED",
        PullRequestState::Closed => "CLOSED",
    }
}

fn pr_dto(pr: PullRequestRef) -> PullRequestDto {
    PullRequestDto {
        number: pr.number,
        title: pr.title,
        url: pr.url,
        state: state_label(pr.state),
        head_ref: pr.head_ref,
        head_sha: pr.head_sha,
    }
}

fn hunk_dto(hunk: DiffHunk) -> DiffHunkDto {
    DiffHunkDto {
        header: hunk.header,
        lines: hunk.lines,
    }
}

fn file_dto(file: DiffFile) -> DiffFileDto {
    DiffFileDto {
        path: file.path,
        status: file.status,
        additions: file.additions,
        deletions: file.deletions,
        role: role_label(file.role),
        hunks: file.hunks.into_iter().map(hunk_dto).collect(),
    }
}

fn totals_dto(totals: DiffTotals) -> DiffTotalsDto {
    DiffTotalsDto {
        files: totals.files,
        signal: totals.signal,
        noise: totals.noise,
        additions: totals.additions,
        deletions: totals.deletions,
    }
}

fn flag_dto(flag: DiffFlag) -> DiffFlagDto {
    DiffFlagDto {
        kind: flag_kind_label(flag.kind),
        severity: flag_severity_label(flag.severity),
        path: flag.path,
        detail: flag.detail,
    }
}

fn judgment_dto(judgment: DiffJudgment) -> DiffJudgmentDto {
    DiffJudgmentDto {
        totals: totals_dto(judgment.totals),
        reading_order: judgment.reading_order,
        flags: judgment.flags.into_iter().map(flag_dto).collect(),
    }
}

fn diff_dto(projection: TaskDiffProjection) -> TaskDiffDto {
    let source = match projection.source {
        DiffSource::Local => "local".to_string(),
        DiffSource::Pr { number } => format!("pr:{number}"),
    };
    TaskDiffDto {
        source,
        pr: projection.pr.map(pr_dto),
        files: projection.files.into_iter().map(file_dto).collect(),
        fell_back_from_pr: projection.fell_back_from_pr,
        judgment: judgment_dto(projection.judgment),
    }
}

#[derive(Clone, Debug)]
pub struct PullRequestListProjection {
    pub pull_requests: Vec<PullRequestDto>,
    pub metadata_changed: bool,
}

#[derive(Clone, Debug)]
pub struct TaskDiffRouteProjection {
    pub diff: TaskDiffDto,
    pub metadata_changed: bool,
}

fn pr_metadata_snapshot<R: Registry>(
    context: &CommandContext<R>,
    task_id: &ajax_core::models::TaskId,
) -> Option<String> {
    context
        .registry
        .get_task(task_id)
        .and_then(|task| task.metadata.get(AJAX_PULL_REQUESTS_KEY).cloned())
}

pub fn list_task_pull_requests<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    qualified_handle: &str,
) -> Result<PullRequestListProjection, DiffReviewRouteError> {
    let task_id = find_task_id(context, qualified_handle)?;
    let before = pr_metadata_snapshot(context, &task_id);
    let github = GithubChecksAdapter::new("gh");
    let task = context
        .registry
        .get_task_mut(&task_id)
        .ok_or(DiffReviewRouteError::TaskNotFound)?;
    let prs = observe_task_pull_requests(task, runner, &github)?;
    let after = pr_metadata_snapshot(context, &task_id);
    Ok(PullRequestListProjection {
        pull_requests: prs.into_iter().map(pr_dto).collect(),
        metadata_changed: before != after,
    })
}

pub fn task_diff_projection<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    qualified_handle: &str,
    pr_number: Option<u64>,
    force_local: bool,
) -> Result<TaskDiffRouteProjection, DiffReviewRouteError> {
    let task_id = find_task_id(context, qualified_handle)?;
    let github = GithubChecksAdapter::new("gh");
    let before = pr_metadata_snapshot(context, &task_id);

    if !force_local {
        let task = context
            .registry
            .get_task_mut(&task_id)
            .ok_or(DiffReviewRouteError::TaskNotFound)?;
        let _ = observe_task_pull_requests(task, runner, &github);
    }

    let after = pr_metadata_snapshot(context, &task_id);
    let task = context
        .registry
        .get_task(&task_id)
        .ok_or(DiffReviewRouteError::TaskNotFound)?;
    let projection = project_task_diff(task, runner, &github, pr_number, force_local)?;
    Ok(TaskDiffRouteProjection {
        diff: diff_dto(projection),
        metadata_changed: before != after,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;
    use ajax_core::{
        adapters::{CommandOutput, CommandRunError, CommandSpec},
        diff_review::{remember_pull_requests, PullRequestRef, PullRequestState},
        models::TaskId,
        registry::InMemoryRegistry,
    };
    use std::collections::VecDeque;

    struct QueuedRunner {
        outputs: VecDeque<Result<CommandOutput, CommandRunError>>,
        commands: Vec<CommandSpec>,
    }

    impl QueuedRunner {
        fn new(outputs: Vec<Result<CommandOutput, CommandRunError>>) -> Self {
            Self {
                outputs: outputs.into(),
                commands: Vec::new(),
            }
        }
    }

    impl CommandRunner for QueuedRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.commands.push(command.clone());
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

    fn context() -> CommandContext<InMemoryRegistry> {
        test_support::context_with_fix_login_task()
    }

    #[test]
    fn list_task_pull_requests_merges_live_and_persists() {
        let mut context = context();
        let mut runner = QueuedRunner::new(vec![ok(
            r#"[{"number":12,"title":"Retry","url":"https://example.com/12","state":"OPEN","headRefName":"ajax/fix-login","headRefOid":"abc"}]"#,
        )]);

        let projection = list_task_pull_requests(&mut context, &mut runner, "web/fix-login")
            .expect("list should succeed");

        assert!(projection.metadata_changed);
        assert_eq!(projection.pull_requests.len(), 1);
        assert_eq!(projection.pull_requests[0].number, 12);
        assert_eq!(projection.pull_requests[0].state, "OPEN");
        let task = context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .expect("task");
        assert!(task.metadata.contains_key("ajax_pull_requests"));
    }

    #[test]
    fn list_task_pull_requests_returns_stored_when_gh_unobservable() {
        let mut context = context();
        {
            let task = context
                .registry
                .get_task_mut(&TaskId::new("web/fix-login"))
                .unwrap();
            remember_pull_requests(
                task,
                &[PullRequestRef {
                    number: 9,
                    title: "Stored".into(),
                    url: "https://example.com/9".into(),
                    state: PullRequestState::Merged,
                    head_ref: "ajax/fix-login".into(),
                    head_sha: None,
                }],
            );
        }
        let mut runner = QueuedRunner::new(vec![Err(CommandRunError::SpawnFailed(
            "gh not found".into(),
        ))]);

        let projection = list_task_pull_requests(&mut context, &mut runner, "web/fix-login")
            .expect("stored fallback");

        assert!(!projection.metadata_changed);
        assert_eq!(projection.pull_requests.len(), 1);
        assert_eq!(projection.pull_requests[0].number, 9);
        assert_eq!(projection.pull_requests[0].state, "MERGED");
    }

    #[test]
    fn task_diff_projection_local_fallback_parses_patch() {
        let mut context = context();
        let patch = "\
diff --git a/src/a.rs b/src/a.rs
--- a/src/a.rs
+++ b/src/a.rs
@@ -1 +1,2 @@
 keep
+new
";
        let mut runner = QueuedRunner::new(vec![ok(patch)]);

        let projection =
            task_diff_projection(&mut context, &mut runner, "web/fix-login", None, true)
                .expect("local diff");

        assert!(!projection.metadata_changed);
        assert_eq!(projection.diff.source, "local");
        assert_eq!(projection.diff.fell_back_from_pr, None);
        assert_eq!(projection.diff.files.len(), 1);
        assert_eq!(projection.diff.files[0].path, "src/a.rs");
        assert_eq!(projection.diff.files[0].role, "signal");
        assert_eq!(projection.diff.files[0].additions, 1);
        assert_eq!(projection.diff.judgment.totals.files, 1);
        assert_eq!(projection.diff.judgment.totals.signal, 1);
        assert_eq!(projection.diff.judgment.reading_order, vec!["src/a.rs"]);
        assert!(projection
            .diff
            .judgment
            .flags
            .iter()
            .any(|flag| flag.kind == "unexpected_path"));
        assert!(runner
            .commands
            .iter()
            .any(|command| command.program == "git"));
    }

    #[test]
    fn task_diff_projection_local_nonzero_status_is_unobservable() {
        let mut context = context();
        let mut runner = QueuedRunner::new(vec![Ok(CommandOutput {
            status_code: 128,
            stdout: String::new(),
            stderr: "fatal: bad revision 'main'".into(),
        })]);

        let error = task_diff_projection(&mut context, &mut runner, "web/fix-login", None, true)
            .unwrap_err();

        assert!(matches!(
            error,
            DiffReviewRouteError::Unobservable(reason) if reason.contains("bad revision")
        ));
    }

    #[test]
    fn task_diff_projection_pr_fallback_sets_fell_back_from_pr() {
        let mut context = context();
        {
            let task = context
                .registry
                .get_task_mut(&TaskId::new("web/fix-login"))
                .unwrap();
            remember_pull_requests(
                task,
                &[PullRequestRef {
                    number: 12,
                    title: "Retry".into(),
                    url: "https://example.com/12".into(),
                    state: PullRequestState::Open,
                    head_ref: "ajax/fix-login".into(),
                    head_sha: Some("abc".into()),
                }],
            );
        }
        let patch = "\
diff --git a/src/a.rs b/src/a.rs
--- a/src/a.rs
+++ b/src/a.rs
@@ -1 +1,2 @@
 keep
+new
";
        let mut runner = QueuedRunner::new(vec![
            ok(r#"[]"#),
            Err(CommandRunError::SpawnFailed(
                "gh pr diff unavailable".into(),
            )),
            ok(patch),
        ]);

        let projection =
            task_diff_projection(&mut context, &mut runner, "web/fix-login", Some(12), false)
                .expect("hybrid fallback");

        assert_eq!(projection.diff.source, "local");
        assert_eq!(projection.diff.fell_back_from_pr, Some(12));
        assert_eq!(projection.diff.files.len(), 1);
    }

    #[test]
    fn unknown_handle_is_task_not_found() {
        let mut context = context();
        let mut runner = QueuedRunner::new(vec![]);

        let error = list_task_pull_requests(&mut context, &mut runner, "web/missing").unwrap_err();

        assert_eq!(error, DiffReviewRouteError::TaskNotFound);
    }
}

use super::{
    assess_diff_judgment, classify_diff_path, merge_pull_request_lists, parse_unified_diff,
    project_task_diff, remember_pull_requests, select_default_pr, stored_pull_requests, DiffFile,
    DiffFileRole, DiffFlagKind, DiffFlagSeverity, DiffHunk, PullRequestRef, PullRequestState,
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

    let projection =
        project_task_diff(&task, &mut runner, &github, Some(12), false).expect("hybrid fallback");

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

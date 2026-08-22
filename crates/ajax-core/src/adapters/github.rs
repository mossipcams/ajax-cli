use super::command::{CommandOutput, CommandRunError, CommandSpec};
use crate::agent_notification::CiFailedCheck;
use crate::diff_review::{PullRequestRef, PullRequestState};
use serde::Deserialize;
use std::time::Duration;

const GH_PR_CHECKS_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GithubChecksAdapter {
    program: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CiChecksObservation {
    /// At least one check reached a definitive terminal failure. The summary
    /// names the first failed check encountered in the payload.
    Failed { summary: String },
    /// Every check resolved to a healthy (or neutral) terminal state.
    Healthy,
    /// One or more checks are still running or queued, and none have failed.
    Pending,
    /// CI state could not be observed. The reason carries diagnostic text
    /// (a `gh`/auth/network message or the runner error's Display text).
    /// An `Unobservable` outcome must never be treated as a CI failure —
    /// callers projecting to `LiveStatusKind::CiFailed` must treat it as
    /// "no signal" rather than "failed".
    Unobservable { reason: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CiChecksState {
    Failed,
    Healthy,
    Pending,
    Unobservable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CiChecksReport {
    pub state: CiChecksState,
    pub failed_checks: Vec<CiFailedCheck>,
    pub check_identities: Vec<String>,
    pub has_pending: bool,
    pub error: Option<String>,
}

impl GithubChecksAdapter {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
        }
    }

    pub fn pr_checks(&self, worktree_path: &str, branch: &str) -> CommandSpec {
        CommandSpec::new(
            &self.program,
            ["pr", "checks", branch, "--json", "name,state,link"],
        )
        .with_cwd(worktree_path)
        .with_timeout(GH_PR_CHECKS_TIMEOUT)
    }

    pub fn pr_checks_for_pr(&self, worktree_path: &str, number: u64) -> CommandSpec {
        CommandSpec::new(
            &self.program,
            [
                "pr",
                "checks",
                &number.to_string(),
                "--json",
                "name,state,link",
            ],
        )
        .with_cwd(worktree_path)
        .with_timeout(GH_PR_CHECKS_TIMEOUT)
    }

    pub fn parse_pr_checks(result: &Result<CommandOutput, CommandRunError>) -> CiChecksObservation {
        observation_from_report(Self::parse_pr_checks_report(result))
    }

    pub fn parse_pr_checks_report(
        result: &Result<CommandOutput, CommandRunError>,
    ) -> CiChecksReport {
        match result {
            Err(error) => unobservable_report(error.to_string()),
            Ok(output) => parse_report_stdout_or_stderr(output),
        }
    }

    pub fn pr_list(&self, worktree_path: &str, branch: &str) -> CommandSpec {
        CommandSpec::new(
            &self.program,
            [
                "pr",
                "list",
                "--state",
                "all",
                "--head",
                branch,
                "--json",
                "number,title,url,state,headRefName,headRefOid",
            ],
        )
        .with_cwd(worktree_path)
        .with_timeout(GH_PR_CHECKS_TIMEOUT)
    }

    pub fn parse_pr_list(
        result: &Result<CommandOutput, CommandRunError>,
    ) -> Result<Vec<PullRequestRef>, String> {
        let output = match result {
            Err(error) => return Err(error.to_string()),
            Ok(output) => output,
        };
        if output.status_code != 0 {
            return Err(if output.stderr.is_empty() {
                format!("gh pr list failed with status {}", output.status_code)
            } else {
                output.stderr.clone()
            });
        }

        let rows: Vec<PrListRow> = serde_json::from_str(&output.stdout)
            .map_err(|error| format!("unparsable gh pr list output: {error}"))?;

        Ok(rows.into_iter().map(map_pr_row).collect())
    }

    pub fn pr_diff(&self, worktree_path: &str, number: u64) -> CommandSpec {
        CommandSpec::new(&self.program, ["pr", "diff", &number.to_string()])
            .with_cwd(worktree_path)
            .with_timeout(GH_PR_CHECKS_TIMEOUT)
    }

    pub fn local_diff(&self, worktree_path: &str, base_branch: &str) -> CommandSpec {
        let range = format!("{base_branch}...HEAD");
        CommandSpec::new("git", ["diff", &range])
            .with_cwd(worktree_path)
            .with_timeout(GH_PR_CHECKS_TIMEOUT)
    }
}

#[derive(Deserialize)]
struct CheckRow {
    name: String,
    state: String,
    #[serde(default)]
    link: Option<String>,
}

#[derive(Deserialize)]
struct PrListRow {
    number: u64,
    title: String,
    url: String,
    state: String,
    #[serde(rename = "headRefName")]
    head_ref: String,
    #[serde(rename = "headRefOid")]
    head_sha: Option<String>,
}

fn map_pr_row(row: PrListRow) -> PullRequestRef {
    let state = match row.state.to_ascii_uppercase().as_str() {
        "OPEN" => PullRequestState::Open,
        "MERGED" => PullRequestState::Merged,
        _ => PullRequestState::Closed,
    };

    PullRequestRef {
        number: row.number,
        title: row.title,
        url: row.url,
        state,
        head_ref: row.head_ref,
        head_sha: row.head_sha,
    }
}

fn parse_report_stdout_or_stderr(output: &CommandOutput) -> CiChecksReport {
    let rows = match serde_json::from_str::<Vec<CheckRow>>(&output.stdout) {
        Ok(rows) if !rows.is_empty() => rows,
        Ok(_) => return unobservable_report("no checks reported for pull request".to_string()),
        Err(_) => {
            return match classify_stderr(&output.stderr) {
                CiChecksObservation::Unobservable { reason } => unobservable_report(reason),
                _ => unreachable!("stderr classification is always unobservable"),
            };
        }
    };
    classify_report_rows(&rows)
}

fn unobservable_report(reason: String) -> CiChecksReport {
    CiChecksReport {
        state: CiChecksState::Unobservable,
        failed_checks: Vec::new(),
        check_identities: Vec::new(),
        has_pending: false,
        error: Some(reason),
    }
}

fn classify_report_rows(rows: &[CheckRow]) -> CiChecksReport {
    let mut failed_checks = rows
        .iter()
        .filter(|row| is_failure_state(&row.state))
        .map(|row| CiFailedCheck {
            name: row.name.clone(),
            link: row.link.clone().filter(|link| !link.trim().is_empty()),
            identity: row.link.as_deref().and_then(check_identity),
        })
        .collect::<Vec<_>>();
    failed_checks.sort();
    let mut check_identities = rows
        .iter()
        .filter_map(|row| row.link.as_deref().and_then(check_identity))
        .collect::<Vec<_>>();
    check_identities.sort();
    check_identities.dedup();
    finish_report(rows, failed_checks, check_identities)
}

fn finish_report(
    rows: &[CheckRow],
    failed_checks: Vec<CiFailedCheck>,
    check_identities: Vec<String>,
) -> CiChecksReport {
    let has_pending = rows.iter().any(|row| is_pending_state(&row.state))
        || rows.iter().any(|row| {
            !is_failure_state(&row.state)
                && !is_pending_state(&row.state)
                && !is_healthy_state(&row.state)
        });
    let state = if !failed_checks.is_empty() {
        CiChecksState::Failed
    } else if has_pending {
        CiChecksState::Pending
    } else {
        CiChecksState::Healthy
    };
    CiChecksReport {
        state,
        failed_checks,
        check_identities,
        has_pending,
        error: None,
    }
}

fn check_identity(link: &str) -> Option<String> {
    let (_, suffix) = link.split_once("/runs/")?;
    let run = suffix.split('/').next()?.trim();
    if run.is_empty() {
        return None;
    }
    let job = suffix
        .split_once("/job/")
        .and_then(|(_, job)| job.split('/').next())
        .filter(|job| !job.is_empty());
    Some(match job {
        Some(job) => format!("run:{run}/job:{job}"),
        None => format!("run:{run}"),
    })
}

fn observation_from_report(report: CiChecksReport) -> CiChecksObservation {
    match report.state {
        CiChecksState::Failed => CiChecksObservation::Failed {
            summary: report
                .failed_checks
                .first()
                .map(|check| check.name.clone())
                .unwrap_or_else(|| "unknown check".to_string()),
        },
        CiChecksState::Healthy => CiChecksObservation::Healthy,
        CiChecksState::Pending => CiChecksObservation::Pending,
        CiChecksState::Unobservable => CiChecksObservation::Unobservable {
            reason: report
                .error
                .unwrap_or_else(|| "CI checks unobservable".to_string()),
        },
    }
}

fn classify_stderr(stderr: &str) -> CiChecksObservation {
    let trimmed = stderr.trim();
    if trimmed.is_empty() {
        return CiChecksObservation::Unobservable {
            reason: "unparsable gh pr checks output".to_string(),
        };
    }

    let reason = if trimmed.contains("no pull requests found") {
        format!("no pull request for branch: {trimmed}")
    } else {
        trimmed.to_string()
    };

    CiChecksObservation::Unobservable { reason }
}

fn is_failure_state(state: &str) -> bool {
    matches!(
        state.to_ascii_uppercase().as_str(),
        "FAILURE" | "CANCELLED" | "TIMED_OUT" | "ERROR" | "STARTUP_FAILURE"
    )
}

fn is_healthy_state(state: &str) -> bool {
    matches!(
        state.to_ascii_uppercase().as_str(),
        "SUCCESS" | "SKIPPED" | "NEUTRAL"
    )
}

fn is_pending_state(state: &str) -> bool {
    matches!(
        state.to_ascii_uppercase().as_str(),
        "PENDING" | "QUEUED" | "IN_PROGRESS" | "WAITING"
    )
}

#[cfg(test)]
mod tests {
    use super::{
        CiChecksObservation, CommandOutput, CommandRunError, CommandSpec, GithubChecksAdapter,
        GH_PR_CHECKS_TIMEOUT,
    };
    use std::path::Path;

    fn ok_output(status_code: i32, stdout: &str, stderr: &str) -> CommandOutput {
        CommandOutput {
            status_code,
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
        }
    }

    #[test]
    fn pr_checks_plans_gh_json_in_worktree_with_timeout() {
        let adapter = GithubChecksAdapter::new("gh");

        let spec = adapter.pr_checks("/worktrees/ajax-fix-login", "feature-branch");

        assert_eq!(
            spec,
            CommandSpec::new(
                "gh",
                [
                    "pr",
                    "checks",
                    "feature-branch",
                    "--json",
                    "name,state,link"
                ]
            )
            .with_cwd("/worktrees/ajax-fix-login")
            .with_timeout(GH_PR_CHECKS_TIMEOUT)
        );
        assert_eq!(
            spec.cwd.as_deref(),
            Some(Path::new("/worktrees/ajax-fix-login"))
        );
        assert_eq!(spec.timeout, Some(GH_PR_CHECKS_TIMEOUT));
    }

    fn parse(result: &Result<CommandOutput, CommandRunError>) -> CiChecksObservation {
        GithubChecksAdapter::parse_pr_checks(result)
    }

    #[test]
    fn failure_state_produces_failure_naming_first_failed_check() {
        let stdout = r#"[{"name":"ci","state":"FAILURE","link":"https://example.com"},{"name":"lint","state":"SUCCESS","link":"https://example.com"}]"#;
        let result = Ok(ok_output(1, stdout, ""));

        assert_eq!(
            parse(&result),
            CiChecksObservation::Failed {
                summary: "ci".to_string()
            }
        );
    }

    #[test]
    fn failure_states_cancelled_timed_out_fail_case_insensitively() {
        for (label, state) in [
            ("CANCELLED", "CANCELLED"),
            ("cancelled lowercase", "cancelled"),
            ("TIMED_OUT", "TIMED_OUT"),
            ("timed_out lowercase", "timed_out"),
            ("ERROR", "ERROR"),
            ("STARTUP_FAILURE", "STARTUP_FAILURE"),
        ] {
            let stdout =
                format!(r#"[{{"name":"{label}","state":"{state}","link":"https://example.com"}}]"#);
            let result = Ok(ok_output(1, &stdout, ""));

            assert!(
                matches!(parse(&result), CiChecksObservation::Failed { .. }),
                "expected failure for state {state}"
            );
        }
    }

    #[test]
    fn healthy_success_skipped_neutral_produces_healthy() {
        let stdout = r#"[
            {"name":"ci","state":"SUCCESS","link":"x"},
            {"name":"lint","state":"SKIPPED","link":"x"},
            {"name":"audit","state":"NEUTRAL","link":"x"}
        ]"#;
        let result = Ok(ok_output(0, stdout, ""));

        assert_eq!(parse(&result), CiChecksObservation::Healthy);
    }

    #[test]
    fn pending_mixed_with_success_is_pending() {
        let stdout = r#"[
            {"name":"ci","state":"SUCCESS","link":"x"},
            {"name":"lint","state":"PENDING","link":"x"}
        ]"#;
        let result = Ok(ok_output(1, stdout, ""));

        assert_eq!(parse(&result), CiChecksObservation::Pending);
    }

    #[test]
    fn in_progress_mixed_with_success_is_pending() {
        let stdout = r#"[
            {"name":"ci","state":"SUCCESS","link":"x"},
            {"name":"lint","state":"IN_PROGRESS","link":"x"}
        ]"#;
        let result = Ok(ok_output(1, stdout, ""));

        assert_eq!(parse(&result), CiChecksObservation::Pending);
    }

    #[test]
    fn failure_mixed_with_pending_is_failure() {
        let stdout = r#"[
            {"name":"ci","state":"FAILURE","link":"x"},
            {"name":"lint","state":"PENDING","link":"x"}
        ]"#;
        let result = Ok(ok_output(1, stdout, ""));

        assert_eq!(
            parse(&result),
            CiChecksObservation::Failed {
                summary: "ci".to_string()
            }
        );
    }

    #[test]
    fn no_pull_request_for_branch_is_unobservable() {
        let result = Ok(ok_output(
            1,
            "",
            "no pull requests found for branch \"feature-x\"",
        ));

        match parse(&result) {
            CiChecksObservation::Unobservable { reason } => {
                assert!(
                    reason.contains("no pull request"),
                    "reason should mention no PR, got: {reason}"
                );
            }
            other => panic!("expected unobservable, got {other:?}"),
        }
    }

    #[test]
    fn auth_failure_is_unobservable_carrying_stderr() {
        let result = Ok(ok_output(
            1,
            "",
            "gh: To get started with GitHub CLI, please run: gh auth login",
        ));

        match parse(&result) {
            CiChecksObservation::Unobservable { reason } => {
                assert!(
                    reason.contains("gh auth login"),
                    "reason should carry stderr text, got: {reason}"
                );
            }
            other => panic!("expected unobservable, got {other:?}"),
        }
    }

    #[test]
    fn runner_errors_are_unobservable_for_every_variant() {
        let cases: Vec<(&str, CommandRunError)> = vec![
            (
                "SpawnFailed",
                CommandRunError::SpawnFailed("program not found: gh".to_string()),
            ),
            (
                "TimedOut",
                CommandRunError::TimedOut {
                    program: "gh".to_string(),
                    timeout: std::time::Duration::from_secs(30),
                },
            ),
            ("MissingStatusCode", CommandRunError::MissingStatusCode),
            (
                "NonZeroExit",
                CommandRunError::NonZeroExit {
                    program: "gh".to_string(),
                    status_code: 1,
                    stderr: "boom".to_string(),
                    cwd: None,
                },
            ),
        ];

        for (label, error) in cases {
            let result: Result<CommandOutput, CommandRunError> = Err(error);
            match parse(&result) {
                CiChecksObservation::Unobservable { reason } => {
                    assert!(!reason.is_empty(), "{label}: expected non-empty reason");
                }
                other => panic!("{label}: expected unobservable, got {other:?}"),
            }
        }
    }

    #[test]
    fn unknown_state_without_failure_is_pending() {
        let stdout = r#"[
            {"name":"ci","state":"SUCCESS","link":"x"},
            {"name":"lint","state":"SOMETHING_NEW","link":"x"}
        ]"#;
        let result = Ok(ok_output(1, stdout, ""));

        assert_eq!(parse(&result), CiChecksObservation::Pending);
    }

    #[test]
    fn unknown_state_with_failure_still_yields_failure() {
        let stdout = r#"[
            {"name":"ci","state":"FAILURE","link":"x"},
            {"name":"lint","state":"SOMETHING_NEW","link":"x"}
        ]"#;
        let result = Ok(ok_output(1, stdout, ""));

        assert_eq!(
            parse(&result),
            CiChecksObservation::Failed {
                summary: "ci".to_string()
            }
        );
    }

    #[test]
    fn non_json_stdout_on_success_is_unobservable() {
        let result = Ok(ok_output(0, "not json", ""));

        assert!(matches!(
            parse(&result),
            CiChecksObservation::Unobservable { .. }
        ));
    }

    #[test]
    fn empty_json_array_is_unobservable_not_healthy() {
        let result = Ok(ok_output(0, "[]", ""));

        match parse(&result) {
            CiChecksObservation::Unobservable { reason } => {
                assert!(
                    reason.contains("no checks"),
                    "empty array should be unobservable, got reason: {reason}"
                );
            }
            other => panic!("empty array should be unobservable, got {other:?}"),
        }
    }

    #[test]
    fn pr_list_plans_gh_json_in_worktree_with_timeout() {
        let adapter = GithubChecksAdapter::new("gh");

        let spec = adapter.pr_list("/worktrees/ajax-fix-login", "feature-branch");

        assert_eq!(
            spec,
            CommandSpec::new(
                "gh",
                [
                    "pr",
                    "list",
                    "--state",
                    "all",
                    "--head",
                    "feature-branch",
                    "--json",
                    "number,title,url,state,headRefName,headRefOid",
                ]
            )
            .with_cwd("/worktrees/ajax-fix-login")
            .with_timeout(GH_PR_CHECKS_TIMEOUT)
        );
    }

    #[test]
    fn parse_pr_list_maps_open_and_merged_states() {
        let stdout = r#"[
            {"number":12,"title":"Open PR","url":"https://example.com/12","state":"OPEN","headRefName":"feature","headRefOid":"abc"},
            {"number":10,"title":"Merged PR","url":"https://example.com/10","state":"MERGED","headRefName":"feature","headRefOid":"def"},
            {"number":8,"title":"Closed PR","url":"https://example.com/8","state":"CLOSED","headRefName":"feature","headRefOid":null}
        ]"#;
        let result = Ok(ok_output(0, stdout, ""));

        let prs = GithubChecksAdapter::parse_pr_list(&result).expect("parse should succeed");

        assert_eq!(prs.len(), 3);
        assert_eq!(prs[0].state, crate::diff_review::PullRequestState::Open);
        assert_eq!(prs[1].state, crate::diff_review::PullRequestState::Merged);
        assert_eq!(prs[2].state, crate::diff_review::PullRequestState::Closed);
        assert_eq!(prs[0].head_sha.as_deref(), Some("abc"));
        assert!(prs[2].head_sha.is_none());
    }

    #[test]
    fn parse_pr_list_nonzero_status_is_error() {
        let result = Ok(ok_output(1, "", "gh: Not logged into any GitHub hosts"));

        let error = GithubChecksAdapter::parse_pr_list(&result).expect_err("nonzero should err");

        assert!(error.contains("Not logged into"));
    }

    #[test]
    fn parse_pr_list_empty_array_is_ok_not_error() {
        let result = Ok(ok_output(0, "[]", ""));

        let prs = GithubChecksAdapter::parse_pr_list(&result).expect("empty list should parse");

        assert!(prs.is_empty());
    }

    #[test]
    fn pr_diff_plans_gh_command_in_worktree() {
        let adapter = GithubChecksAdapter::new("gh");

        let spec = adapter.pr_diff("/worktrees/ajax-fix-login", 42);

        assert_eq!(
            spec,
            CommandSpec::new("gh", ["pr", "diff", "42"])
                .with_cwd("/worktrees/ajax-fix-login")
                .with_timeout(GH_PR_CHECKS_TIMEOUT)
        );
    }

    #[test]
    fn local_diff_plans_git_range_in_worktree() {
        let adapter = GithubChecksAdapter::new("gh");

        let spec = adapter.local_diff("/worktrees/ajax-fix-login", "main");

        assert_eq!(
            spec,
            CommandSpec::new("git", ["diff", "main...HEAD"])
                .with_cwd("/worktrees/ajax-fix-login")
                .with_timeout(GH_PR_CHECKS_TIMEOUT)
        );
    }
}

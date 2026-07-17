use super::command::{CommandOutput, CommandRunError, CommandSpec};
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

    pub fn parse_pr_checks(result: &Result<CommandOutput, CommandRunError>) -> CiChecksObservation {
        match result {
            Err(error) => CiChecksObservation::Unobservable {
                reason: error.to_string(),
            },
            Ok(output) => parse_stdout_or_stderr(output),
        }
    }
}

#[derive(Deserialize)]
struct CheckRow {
    name: String,
    state: String,
}

fn parse_stdout_or_stderr(output: &CommandOutput) -> CiChecksObservation {
    match serde_json::from_str::<Vec<CheckRow>>(&output.stdout) {
        Ok(rows) => classify_rows(&rows),
        Err(_) => classify_stderr(&output.stderr),
    }
}

fn classify_rows(rows: &[CheckRow]) -> CiChecksObservation {
    if rows.is_empty() {
        return CiChecksObservation::Unobservable {
            reason: "no checks reported for branch".to_string(),
        };
    }

    for row in rows {
        if is_failure_state(&row.state) {
            return CiChecksObservation::Failed {
                summary: row.name.clone(),
            };
        }
    }

    if rows.iter().any(|row| is_pending_state(&row.state))
        || rows.iter().any(|row| !is_healthy_state(&row.state))
    {
        CiChecksObservation::Pending
    } else {
        CiChecksObservation::Healthy
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
}

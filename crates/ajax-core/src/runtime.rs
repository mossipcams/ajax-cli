use std::time::{Duration, SystemTime};

use crate::models::{
    GitStatus, RuntimeHealth, RuntimeObservationSource, RuntimeProjection, TaskWindowStatus,
    TmuxStatus,
};

pub const RUNTIME_PROJECTION_FRESH_FOR: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedTaskRuntime {
    pub git_status: Option<GitStatus>,
    pub tmux_status: Option<TmuxStatus>,
    pub task_window_status: Option<TaskWindowStatus>,
}

pub fn reconcile_runtime_for_task(
    observed: &ObservedTaskRuntime,
    expected_branch: &str,
    tmux_optional: bool,
    observed_at: SystemTime,
    source: RuntimeObservationSource,
) -> RuntimeProjection {
    RuntimeProjection::new(
        runtime_health(observed, expected_branch, tmux_optional),
        observed_at,
        source,
    )
}

fn runtime_health(
    observed: &ObservedTaskRuntime,
    expected_branch: &str,
    tmux_optional: bool,
) -> RuntimeHealth {
    let Some(git_status) = observed.git_status.as_ref() else {
        return RuntimeHealth::Unobservable;
    };
    if !git_status.worktree_exists {
        return RuntimeHealth::MissingWorktree;
    }
    if git_status.current_branch.as_deref() != Some(expected_branch) {
        return RuntimeHealth::CheckoutMismatch;
    }

    if tmux_optional {
        if observed
            .task_window_status
            .as_ref()
            .is_some_and(|w| w.exists && !w.points_at_expected_path)
        {
            return RuntimeHealth::WrongTaskWindowPath;
        }
        return RuntimeHealth::Healthy;
    }

    let Some(tmux_status) = observed.tmux_status.as_ref() else {
        return RuntimeHealth::Unobservable;
    };
    if !tmux_status.exists {
        return RuntimeHealth::MissingSession;
    }

    let Some(task_window_status) = observed.task_window_status.as_ref() else {
        return RuntimeHealth::Unobservable;
    };
    if !task_window_status.exists {
        return RuntimeHealth::MissingTaskWindow;
    }
    if !task_window_status.points_at_expected_path {
        return RuntimeHealth::WrongTaskWindowPath;
    }

    RuntimeHealth::Healthy
}

#[cfg(test)]
mod tests {
    use super::{reconcile_runtime_for_task, ObservedTaskRuntime};
    use crate::models::{
        GitStatus, RuntimeHealth, RuntimeObservationSource, TaskWindowStatus, TmuxStatus,
    };
    use std::{path::PathBuf, time::SystemTime};

    fn git_status(worktree_exists: bool) -> GitStatus {
        git_status_with_branch(worktree_exists, Some("ajax/fix-login"))
    }

    fn git_status_with_branch(worktree_exists: bool, current_branch: Option<&str>) -> GitStatus {
        GitStatus {
            worktree_exists,
            branch_exists: true,
            current_branch: current_branch.map(str::to_string),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: Some("abc123 Fix login".to_string()),
        }
    }

    fn tmux_missing() -> TmuxStatus {
        TmuxStatus {
            exists: false,
            session_name: "ajax-web-fix-login".to_string(),
        }
    }

    fn task_window_missing() -> TaskWindowStatus {
        TaskWindowStatus {
            exists: false,
            window_name: "task".to_string(),
            current_path: PathBuf::new(),
            points_at_expected_path: false,
        }
    }

    fn observed(
        git: Option<GitStatus>,
        tmux: Option<TmuxStatus>,
        task: Option<TaskWindowStatus>,
    ) -> ObservedTaskRuntime {
        ObservedTaskRuntime {
            git_status: git,
            tmux_status: tmux,
            task_window_status: task,
        }
    }

    #[test]
    fn runtime_reconciliation_distinguishes_checkout_mismatch_from_missing_or_unobserved() {
        let now = SystemTime::UNIX_EPOCH;
        let source = RuntimeObservationSource::TmuxProbe;
        let expected_branch = "ajax/fix-login";
        let healthy = observed(
            Some(git_status(true)),
            Some(TmuxStatus::present("ajax-web-fix-login")),
            Some(TaskWindowStatus::present(
                "task",
                PathBuf::from("/tmp/worktrees/web-fix-login"),
            )),
        );

        let cases = [
            (healthy, RuntimeHealth::Healthy),
            (
                observed(
                    Some(git_status_with_branch(true, Some("ajax/other"))),
                    Some(tmux_missing()),
                    Some(task_window_missing()),
                ),
                RuntimeHealth::CheckoutMismatch,
            ),
            (
                observed(
                    Some(git_status_with_branch(true, None)),
                    Some(tmux_missing()),
                    Some(task_window_missing()),
                ),
                RuntimeHealth::CheckoutMismatch,
            ),
            (
                observed(
                    Some(git_status_with_branch(false, Some("ajax/other"))),
                    Some(tmux_missing()),
                    None,
                ),
                RuntimeHealth::MissingWorktree,
            ),
            (
                observed(None, Some(tmux_missing()), None),
                RuntimeHealth::Unobservable,
            ),
        ];

        for (observed, expected_health) in cases {
            let projection =
                reconcile_runtime_for_task(&observed, expected_branch, false, now, source);

            assert_eq!(projection.health, expected_health);
            assert_eq!(projection.observed_at, now);
            assert_eq!(projection.source, source);
        }
    }

    #[test]
    fn runtime_reconciliation_collapses_substrate_evidence_into_one_health_verdict() {
        let now = SystemTime::UNIX_EPOCH;
        let source = RuntimeObservationSource::TmuxProbe;
        let healthy = observed(
            Some(git_status(true)),
            Some(TmuxStatus::present("ajax-web-fix-login")),
            Some(TaskWindowStatus::present(
                "task",
                PathBuf::from("/tmp/worktrees/web-fix-login"),
            )),
        );

        let cases = [
            (
                observed(
                    Some(git_status(false)),
                    Some(TmuxStatus {
                        exists: false,
                        session_name: "ajax-web-fix-login".to_string(),
                    }),
                    None,
                ),
                RuntimeHealth::MissingWorktree,
            ),
            (
                observed(
                    Some(git_status(true)),
                    Some(TmuxStatus {
                        exists: false,
                        session_name: "ajax-web-fix-login".to_string(),
                    }),
                    None,
                ),
                RuntimeHealth::MissingSession,
            ),
            (
                observed(
                    Some(git_status(true)),
                    Some(TmuxStatus::present("ajax-web-fix-login")),
                    Some(TaskWindowStatus {
                        exists: false,
                        window_name: "task".to_string(),
                        current_path: PathBuf::new(),
                        points_at_expected_path: false,
                    }),
                ),
                RuntimeHealth::MissingTaskWindow,
            ),
            (
                observed(
                    Some(git_status(true)),
                    Some(TmuxStatus::present("ajax-web-fix-login")),
                    Some(TaskWindowStatus {
                        exists: true,
                        window_name: "task".to_string(),
                        current_path: PathBuf::from("/tmp/other"),
                        points_at_expected_path: false,
                    }),
                ),
                RuntimeHealth::WrongTaskWindowPath,
            ),
            (healthy, RuntimeHealth::Healthy),
            (
                observed(Some(git_status(true)), None, None),
                RuntimeHealth::Unobservable,
            ),
        ];

        for (observed, expected_health) in cases {
            let projection =
                reconcile_runtime_for_task(&observed, "ajax/fix-login", false, now, source);

            assert_eq!(projection.health, expected_health);
            assert_eq!(projection.observed_at, now);
            assert_eq!(projection.source, source);
        }
    }
}

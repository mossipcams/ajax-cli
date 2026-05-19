use std::time::{Duration, SystemTime};

use crate::models::{
    GitStatus, RuntimeHealth, RuntimeObservationSource, RuntimeProjection, TmuxStatus,
    WorktrunkStatus,
};

pub const RUNTIME_PROJECTION_FRESH_FOR: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedTaskRuntime {
    pub git_status: Option<GitStatus>,
    pub tmux_status: Option<TmuxStatus>,
    pub worktrunk_status: Option<WorktrunkStatus>,
}

pub fn reconcile_runtime(
    observed: &ObservedTaskRuntime,
    observed_at: SystemTime,
    source: RuntimeObservationSource,
) -> RuntimeProjection {
    RuntimeProjection::new(runtime_health(observed), observed_at, source)
}

fn runtime_health(observed: &ObservedTaskRuntime) -> RuntimeHealth {
    let Some(git_status) = observed.git_status.as_ref() else {
        return RuntimeHealth::Unobservable;
    };
    if !git_status.worktree_exists {
        return RuntimeHealth::MissingWorktree;
    }

    let Some(tmux_status) = observed.tmux_status.as_ref() else {
        return RuntimeHealth::Unobservable;
    };
    if !tmux_status.exists {
        return RuntimeHealth::MissingSession;
    }

    let Some(worktrunk_status) = observed.worktrunk_status.as_ref() else {
        return RuntimeHealth::Unobservable;
    };
    if !worktrunk_status.exists {
        return RuntimeHealth::MissingTaskWindow;
    }
    if !worktrunk_status.points_at_expected_path {
        return RuntimeHealth::WrongTaskWindowPath;
    }

    RuntimeHealth::Healthy
}

#[cfg(test)]
mod tests {
    use super::{reconcile_runtime, ObservedTaskRuntime};
    use crate::models::{
        GitStatus, RuntimeHealth, RuntimeObservationSource, TmuxStatus, WorktrunkStatus,
    };
    use std::{path::PathBuf, time::SystemTime};

    fn git_status(worktree_exists: bool) -> GitStatus {
        GitStatus {
            worktree_exists,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
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

    fn observed(
        git: Option<GitStatus>,
        tmux: Option<TmuxStatus>,
        worktrunk: Option<WorktrunkStatus>,
    ) -> ObservedTaskRuntime {
        ObservedTaskRuntime {
            git_status: git,
            tmux_status: tmux,
            worktrunk_status: worktrunk,
        }
    }

    #[test]
    fn runtime_reconciliation_collapses_substrate_evidence_into_one_health_verdict() {
        let now = SystemTime::UNIX_EPOCH;
        let source = RuntimeObservationSource::TmuxProbe;
        let healthy = observed(
            Some(git_status(true)),
            Some(TmuxStatus::present("ajax-web-fix-login")),
            Some(WorktrunkStatus::present(
                "worktrunk",
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
                    Some(WorktrunkStatus {
                        exists: false,
                        window_name: "worktrunk".to_string(),
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
                    Some(WorktrunkStatus {
                        exists: true,
                        window_name: "worktrunk".to_string(),
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
            let projection = reconcile_runtime(&observed, now, source);

            assert_eq!(projection.health, expected_health);
            assert_eq!(projection.observed_at, now);
            assert_eq!(projection.source, source);
        }
    }
}

use std::time::{Duration, SystemTime};

use crate::models::{
    GitStatus, RuntimeHealth, RuntimeObservationSource, RuntimeProjection, TmuxStatus,
    WorktrunkStatus,
};

pub const RUNTIME_PROJECTION_FRESH_FOR: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservationConfidence {
    Authoritative,
    High,
    Low,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeEvidenceSource {
    OperationResult,
    GitProbe,
    TmuxProbe,
    AgentWrapper,
    AgentStatusCache,
    PaneClassifier,
    SupervisorEvent,
    FilesystemEvent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourcePresence {
    Present,
    Missing,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeEvidence<T> {
    Observed {
        value: T,
        observed_at: SystemTime,
        source: RuntimeEvidenceSource,
        confidence: ObservationConfidence,
    },
    NotObserved {
        reason: String,
        observed_at: SystemTime,
        source: RuntimeEvidenceSource,
    },
    ProbeFailed {
        reason: String,
        observed_at: SystemTime,
        source: RuntimeEvidenceSource,
    },
    Stale {
        value: T,
        observed_at: SystemTime,
        stale_at: SystemTime,
        source: RuntimeEvidenceSource,
        confidence: ObservationConfidence,
    },
}

impl<T> RuntimeEvidence<T> {
    pub fn observed(
        value: T,
        observed_at: SystemTime,
        source: RuntimeEvidenceSource,
        confidence: ObservationConfidence,
    ) -> Self {
        Self::Observed {
            value,
            observed_at,
            source,
            confidence,
        }
    }

    pub fn not_observed(
        reason: impl Into<String>,
        observed_at: SystemTime,
        source: RuntimeEvidenceSource,
    ) -> Self {
        Self::NotObserved {
            reason: reason.into(),
            observed_at,
            source,
        }
    }

    pub fn probe_failed(
        reason: impl Into<String>,
        observed_at: SystemTime,
        source: RuntimeEvidenceSource,
    ) -> Self {
        Self::ProbeFailed {
            reason: reason.into(),
            observed_at,
            source,
        }
    }

    pub fn stale(
        value: T,
        observed_at: SystemTime,
        stale_at: SystemTime,
        source: RuntimeEvidenceSource,
        confidence: ObservationConfidence,
    ) -> Self {
        Self::Stale {
            value,
            observed_at,
            stale_at,
            source,
            confidence,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeEvidenceStatus {
    Present,
    Missing,
    ProbeFailed,
    NotObserved,
    Stale,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReducedRuntimeEvidence {
    pub status: RuntimeEvidenceStatus,
    pub label: String,
}

pub fn reduce_resource_evidence(
    resource: &str,
    evidence: &RuntimeEvidence<ResourcePresence>,
) -> ReducedRuntimeEvidence {
    match evidence {
        RuntimeEvidence::Observed {
            value: ResourcePresence::Present,
            ..
        } => ReducedRuntimeEvidence {
            status: RuntimeEvidenceStatus::Present,
            label: format!("{resource} present"),
        },
        RuntimeEvidence::Observed {
            value: ResourcePresence::Missing,
            ..
        } => ReducedRuntimeEvidence {
            status: RuntimeEvidenceStatus::Missing,
            label: format!("{resource} missing"),
        },
        RuntimeEvidence::ProbeFailed { reason, .. } => ReducedRuntimeEvidence {
            status: RuntimeEvidenceStatus::ProbeFailed,
            label: format!("{resource} probe failed: {reason}"),
        },
        RuntimeEvidence::NotObserved { reason, .. } => ReducedRuntimeEvidence {
            status: RuntimeEvidenceStatus::NotObserved,
            label: format!("{resource} not observed: {reason}"),
        },
        RuntimeEvidence::Stale { .. } => ReducedRuntimeEvidence {
            status: RuntimeEvidenceStatus::Stale,
            label: format!("{resource} status stale"),
        },
    }
}

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
    use super::{
        reconcile_runtime, reduce_resource_evidence, ObservationConfidence, ObservedTaskRuntime,
        ResourcePresence, RuntimeEvidence, RuntimeEvidenceSource, RuntimeEvidenceStatus,
    };
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

    #[test]
    fn runtime_evidence_keeps_missing_probe_failed_not_observed_and_stale_distinct() {
        let observed_at = SystemTime::UNIX_EPOCH;
        let stale_at = observed_at + std::time::Duration::from_secs(60);
        let source = RuntimeEvidenceSource::TmuxProbe;
        let confidence = ObservationConfidence::High;
        let cases = [
            (
                RuntimeEvidence::observed(
                    ResourcePresence::Missing,
                    observed_at,
                    source,
                    confidence,
                ),
                RuntimeEvidenceStatus::Missing,
                "tmux session missing",
            ),
            (
                RuntimeEvidence::probe_failed("tmux list-sessions exited 1", observed_at, source),
                RuntimeEvidenceStatus::ProbeFailed,
                "tmux session probe failed: tmux list-sessions exited 1",
            ),
            (
                RuntimeEvidence::not_observed("tmux was not queried", observed_at, source),
                RuntimeEvidenceStatus::NotObserved,
                "tmux session not observed: tmux was not queried",
            ),
            (
                RuntimeEvidence::stale(
                    ResourcePresence::Present,
                    observed_at,
                    stale_at,
                    source,
                    confidence,
                ),
                RuntimeEvidenceStatus::Stale,
                "tmux session status stale",
            ),
        ];

        for (evidence, expected_status, expected_label) in cases {
            let reduced = reduce_resource_evidence("tmux session", &evidence);

            assert_eq!(reduced.status, expected_status);
            assert_eq!(reduced.label, expected_label);
            assert_ne!(reduced.label, "unknown");
        }
    }
}

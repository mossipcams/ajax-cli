use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::models::{
    AgentClient, AgentRuntimeStatus, GitStatus, LiveObservation, LiveStatusKind, RuntimeHealth,
    RuntimeObservationSource, RuntimeProjection, TmuxStatus, WorktrunkStatus,
};

pub const RUNTIME_PROJECTION_FRESH_FOR: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservationConfidence {
    Authoritative,
    High,
    Low,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AgentRuntimeEvidenceSource {
    RuntimeWrapper,
    Hook,
    Pane,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentRuntimeCandidate {
    pub source: AgentRuntimeEvidenceSource,
    pub value: String,
    pub observed_at: SystemTime,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentRuntimeEvidence {
    pub candidates: Vec<AgentRuntimeCandidate>,
    #[serde(default)]
    pub status_hint: Option<AgentRuntimeStatus>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReducedAgentRuntimeEvidence {
    pub agent_status: AgentRuntimeStatus,
    pub observation: Option<LiveObservation>,
    pub source: Option<AgentRuntimeEvidenceSource>,
    pub observed_at: Option<SystemTime>,
    pub acknowledged_hold: bool,
}

impl AgentRuntimeEvidence {
    pub fn record(
        &mut self,
        source: AgentRuntimeEvidenceSource,
        value: impl Into<String>,
        observed_at: SystemTime,
    ) {
        self.candidates.push(AgentRuntimeCandidate {
            source,
            value: value.into(),
            observed_at,
        });
    }

    pub fn replace_source(
        &mut self,
        source: AgentRuntimeEvidenceSource,
        value: impl Into<String>,
        observed_at: SystemTime,
    ) {
        self.candidates
            .retain(|candidate| candidate.source != source);
        self.record(source, value, observed_at);
    }

    pub fn reduce(
        &self,
        selected_agent: AgentClient,
        prior: Option<&LiveObservation>,
        acknowledged_at: Option<SystemTime>,
        now: SystemTime,
    ) -> ReducedAgentRuntimeEvidence {
        let candidates = self
            .candidates
            .iter()
            .filter_map(|candidate| {
                let source = match candidate.source {
                    AgentRuntimeEvidenceSource::RuntimeWrapper => {
                        crate::live::AgentEvidenceSource::RuntimeWrapper
                    }
                    AgentRuntimeEvidenceSource::Hook => crate::live::AgentEvidenceSource::Hook,
                    AgentRuntimeEvidenceSource::Pane => return None,
                };
                Some(crate::live::StatusCandidate::new(
                    source,
                    candidate.value.clone(),
                    candidate.observed_at,
                ))
            })
            .collect::<Vec<_>>();
        let decision = crate::live::select_status_observation(crate::live::StatusDecisionInput {
            selected_agent,
            prior,
            acknowledged_at,
            now,
            candidates: &candidates,
        });
        if decision.applied || decision.acknowledged_hold {
            let source = decision.source.map(|source| match source {
                crate::live::AgentEvidenceSource::RuntimeWrapper => {
                    AgentRuntimeEvidenceSource::RuntimeWrapper
                }
                crate::live::AgentEvidenceSource::Hook => AgentRuntimeEvidenceSource::Hook,
            });
            return reduced_agent_runtime(
                decision.observation,
                source,
                decision.observed_at,
                decision.acknowledged_hold,
            );
        }

        let pane = self
            .candidates
            .iter()
            .filter(|candidate| candidate.source == AgentRuntimeEvidenceSource::Pane)
            .max_by_key(|candidate| candidate.observed_at);
        match pane {
            Some(pane) => reduced_agent_runtime(
                Some(crate::live::classify_agent_pane(
                    selected_agent,
                    &pane.value,
                )),
                Some(AgentRuntimeEvidenceSource::Pane),
                Some(pane.observed_at),
                false,
            ),
            None => ReducedAgentRuntimeEvidence {
                agent_status: self.status_hint.unwrap_or(AgentRuntimeStatus::Unknown),
                observation: prior.cloned(),
                source: None,
                observed_at: None,
                acknowledged_hold: false,
            },
        }
    }
}

fn reduced_agent_runtime(
    observation: Option<LiveObservation>,
    source: Option<AgentRuntimeEvidenceSource>,
    observed_at: Option<SystemTime>,
    acknowledged_hold: bool,
) -> ReducedAgentRuntimeEvidence {
    let agent_status = observation
        .as_ref()
        .map_or(
            AgentRuntimeStatus::Unknown,
            |observation| match observation.kind {
                LiveStatusKind::AgentRunning
                | LiveStatusKind::CommandRunning
                | LiveStatusKind::TestsRunning => AgentRuntimeStatus::Running,
                LiveStatusKind::WaitingForApproval | LiveStatusKind::WaitingForInput => {
                    AgentRuntimeStatus::Waiting
                }
                LiveStatusKind::Done => AgentRuntimeStatus::Done,
                LiveStatusKind::ShellIdle => AgentRuntimeStatus::NotStarted,
                LiveStatusKind::Unknown => AgentRuntimeStatus::Unknown,
                _ => AgentRuntimeStatus::Blocked,
            },
        );
    ReducedAgentRuntimeEvidence {
        agent_status,
        observation,
        source,
        observed_at,
        acknowledged_hold,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubstrateObservationState {
    Present,
    Absent,
    ProbeFailed,
    NotObserved,
}

pub trait SubstratePresence {
    fn is_present(&self) -> bool;
}

impl SubstratePresence for GitStatus {
    fn is_present(&self) -> bool {
        self.worktree_exists && self.branch_exists
    }
}

impl SubstratePresence for TmuxStatus {
    fn is_present(&self) -> bool {
        self.exists
    }
}

impl SubstratePresence for WorktrunkStatus {
    fn is_present(&self) -> bool {
        self.exists && self.points_at_expected_path
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SubstrateObservation<T> {
    pub value: Option<T>,
    pub observed_at: SystemTime,
    pub source: RuntimeEvidenceSource,
    pub probe_error: Option<String>,
    pub observed: bool,
}

impl<T> Default for SubstrateObservation<T> {
    fn default() -> Self {
        Self {
            value: None,
            observed_at: SystemTime::UNIX_EPOCH,
            source: RuntimeEvidenceSource::OperationResult,
            probe_error: None,
            observed: false,
        }
    }
}

impl<T: SubstratePresence> SubstrateObservation<T> {
    pub fn observed(value: T, observed_at: SystemTime, source: RuntimeEvidenceSource) -> Self {
        Self {
            value: Some(value),
            observed_at,
            source,
            probe_error: None,
            observed: true,
        }
    }

    pub fn record_probe_failure(
        &mut self,
        error: impl Into<String>,
        observed_at: SystemTime,
        source: RuntimeEvidenceSource,
    ) {
        self.observed_at = observed_at;
        self.source = source;
        self.probe_error = Some(error.into());
    }

    pub fn state(&self) -> SubstrateObservationState {
        if self.probe_error.is_some() {
            SubstrateObservationState::ProbeFailed
        } else if !self.observed {
            SubstrateObservationState::NotObserved
        } else if self
            .value
            .as_ref()
            .is_some_and(SubstratePresence::is_present)
        {
            SubstrateObservationState::Present
        } else {
            SubstrateObservationState::Absent
        }
    }
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
        reconcile_runtime, reduce_resource_evidence, AgentRuntimeEvidence,
        AgentRuntimeEvidenceSource, ObservationConfidence, ObservedTaskRuntime, ResourcePresence,
        RuntimeEvidence, RuntimeEvidenceSource, RuntimeEvidenceStatus, SubstrateObservation,
        SubstrateObservationState,
    };
    use crate::models::{
        AgentClient, AgentRuntimeStatus, GitStatus, LiveStatusKind, RuntimeHealth,
        RuntimeObservationSource, TmuxStatus, WorktrunkStatus,
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

    #[test]
    fn git_observation_distinguishes_present_absent_probe_failed_and_not_observed() {
        let observed_at = SystemTime::UNIX_EPOCH;
        let not_observed = SubstrateObservation::<GitStatus>::default();
        assert_eq!(not_observed.state(), SubstrateObservationState::NotObserved);

        let present = SubstrateObservation::observed(
            git_status(true),
            observed_at,
            RuntimeEvidenceSource::GitProbe,
        );
        assert_eq!(present.state(), SubstrateObservationState::Present);

        let absent = SubstrateObservation::observed(
            git_status(false),
            observed_at,
            RuntimeEvidenceSource::GitProbe,
        );
        assert_eq!(absent.state(), SubstrateObservationState::Absent);

        let mut failed = present;
        failed.record_probe_failure(
            "git status failed",
            observed_at + std::time::Duration::from_secs(1),
            RuntimeEvidenceSource::GitProbe,
        );
        assert_eq!(failed.state(), SubstrateObservationState::ProbeFailed);
    }

    #[test]
    fn tmux_observation_preserves_last_credible_value_after_probe_failure() {
        let observed_at = SystemTime::UNIX_EPOCH;
        let mut observation = SubstrateObservation::observed(
            TmuxStatus::present("ajax-web-fix-login"),
            observed_at,
            RuntimeEvidenceSource::TmuxProbe,
        );

        observation.record_probe_failure(
            "tmux unavailable",
            observed_at + std::time::Duration::from_secs(1),
            RuntimeEvidenceSource::TmuxProbe,
        );

        assert!(observation.value.as_ref().is_some_and(|value| value.exists));
        assert_eq!(observation.state(), SubstrateObservationState::ProbeFailed);
    }

    #[test]
    fn task_window_observation_records_wrong_path_without_losing_observed_path() {
        let observation = SubstrateObservation::observed(
            WorktrunkStatus {
                exists: true,
                window_name: "worktrunk".to_string(),
                current_path: PathBuf::from("/tmp/wrong"),
                points_at_expected_path: false,
            },
            SystemTime::UNIX_EPOCH,
            RuntimeEvidenceSource::TmuxProbe,
        );

        assert_eq!(observation.state(), SubstrateObservationState::Absent);
        assert_eq!(
            observation.value.unwrap().current_path,
            PathBuf::from("/tmp/wrong")
        );
    }

    #[test]
    fn runtime_evidence_derives_agent_status_and_live_observation() {
        let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(500);
        let mut evidence = AgentRuntimeEvidence::default();
        evidence.record(
            AgentRuntimeEvidenceSource::Hook,
            "wait",
            now - std::time::Duration::from_secs(1),
        );

        let reduced = evidence.reduce(AgentClient::Codex, None, None, now);

        assert_eq!(reduced.agent_status, AgentRuntimeStatus::Waiting);
        assert_eq!(
            reduced
                .observation
                .as_ref()
                .map(|observation| observation.kind),
            Some(LiveStatusKind::WaitingForInput)
        );
    }

    #[test]
    fn runtime_evidence_preserves_source_and_observed_at() {
        let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(500);
        let observed_at = now - std::time::Duration::from_secs(1);
        let mut evidence = AgentRuntimeEvidence::default();
        evidence.record(
            AgentRuntimeEvidenceSource::RuntimeWrapper,
            "done",
            observed_at,
        );

        let reduced = evidence.reduce(AgentClient::Codex, None, None, now);

        assert_eq!(
            reduced.source,
            Some(AgentRuntimeEvidenceSource::RuntimeWrapper)
        );
        assert_eq!(reduced.observed_at, Some(observed_at));
        assert_eq!(reduced.agent_status, AgentRuntimeStatus::Done);
    }
}

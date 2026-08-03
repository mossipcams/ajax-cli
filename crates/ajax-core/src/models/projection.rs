use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use super::intent::{LifecycleStatus, TaskId};
use super::observations::{AgentRuntimeStatus, LiveStatusKind, SideFlag};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum RuntimeHealth {
    Healthy,
    MissingWorktree,
    CheckoutMismatch,
    MissingSession,
    MissingTaskWindow,
    WrongTaskWindowPath,
    Unobservable,
}

impl RuntimeHealth {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::MissingWorktree => "missing_worktree",
            Self::CheckoutMismatch => "checkout_mismatch",
            Self::MissingSession => "missing_session",
            Self::MissingTaskWindow => "missing_task_window",
            Self::WrongTaskWindowPath => "wrong_task_window_path",
            Self::Unobservable => "unobservable",
        }
    }

    pub fn from_label(value: &str) -> Option<Self> {
        match value {
            "healthy" => Some(Self::Healthy),
            "missing_worktree" => Some(Self::MissingWorktree),
            "checkout_mismatch" => Some(Self::CheckoutMismatch),
            "missing_session" => Some(Self::MissingSession),
            "missing_task_window" => Some(Self::MissingTaskWindow),
            "wrong_task_window_path" => Some(Self::WrongTaskWindowPath),
            "unobservable" => Some(Self::Unobservable),
            _ => None,
        }
    }

    pub const fn is_missing_substrate(self) -> bool {
        matches!(
            self,
            Self::MissingWorktree
                | Self::MissingSession
                | Self::MissingTaskWindow
                | Self::WrongTaskWindowPath
        )
    }

    pub const fn is_git_substrate_gap(self) -> bool {
        matches!(self, Self::MissingWorktree)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum RuntimeObservationSource {
    StartupScan,
    FilesystemEvent,
    TmuxProbe,
    CommandResult,
    Unknown,
}

impl RuntimeObservationSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StartupScan => "startup_scan",
            Self::FilesystemEvent => "filesystem_event",
            Self::TmuxProbe => "tmux_probe",
            Self::CommandResult => "command_result",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_label(value: &str) -> Option<Self> {
        match value {
            "startup_scan" => Some(Self::StartupScan),
            "filesystem_event" => Some(Self::FilesystemEvent),
            "tmux_probe" => Some(Self::TmuxProbe),
            "command_result" => Some(Self::CommandResult),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct RuntimeProjection {
    pub health: RuntimeHealth,
    pub observed_at: SystemTime,
    pub source: RuntimeObservationSource,
    #[serde(default)]
    pub observation_error: Option<String>,
}

impl RuntimeProjection {
    pub fn new(
        health: RuntimeHealth,
        observed_at: SystemTime,
        source: RuntimeObservationSource,
    ) -> Self {
        Self {
            health,
            observed_at,
            source,
            observation_error: None,
        }
    }

    pub fn with_observation_error(
        health: RuntimeHealth,
        observed_at: SystemTime,
        source: RuntimeObservationSource,
        observation_error: impl Into<String>,
    ) -> Self {
        Self {
            health,
            observed_at,
            source,
            observation_error: Some(observation_error.into()),
        }
    }

    pub fn requires_refresh(&self, now: SystemTime, max_age: Duration) -> bool {
        if self.source == RuntimeObservationSource::Unknown || self.observation_error.is_some() {
            return true;
        }
        if self.health == RuntimeHealth::Unobservable {
            return true;
        }

        now.duration_since(self.observed_at)
            .is_ok_and(|age| age > max_age)
    }

    pub fn is_fresh_at(&self, now: SystemTime, max_age: Duration) -> bool {
        !self.requires_refresh(now, max_age)
    }
}

impl Default for RuntimeProjection {
    fn default() -> Self {
        Self {
            health: RuntimeHealth::Unobservable,
            observed_at: SystemTime::UNIX_EPOCH,
            source: RuntimeObservationSource::Unknown,
            observation_error: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum SafetyClassification {
    Safe,
    NeedsConfirmation,
    Dangerous,
    Blocked,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct SafetyReport {
    pub classification: SafetyClassification,
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct CockpitActionItem {
    pub task_id: TaskId,
    pub task_handle: String,
    pub reason: String,
    pub priority: u32,
    pub action: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Deserialize, Serialize)]
pub enum OperatorAction {
    Start,
    Resume,
    Review,
    Ship,
    Drop,
    Repair,
}

impl OperatorAction {
    pub const fn all() -> &'static [Self] {
        &[
            Self::Start,
            Self::Resume,
            Self::Review,
            Self::Ship,
            Self::Drop,
            Self::Repair,
        ]
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Resume => "resume",
            Self::Review => "review",
            Self::Ship => "ship",
            Self::Drop => "drop",
            Self::Repair => "repair",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        Self::all()
            .iter()
            .copied()
            .find(|action| action.as_str() == label)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Deserialize, Serialize)]
pub enum AnnotationKind {
    NeedsMe,
    Broken,
    Reviewable,
    Cleanable,
}

impl AnnotationKind {
    pub const fn severity(self) -> u32 {
        match self {
            Self::NeedsMe => 1,
            Self::Broken => 2,
            Self::Reviewable => 3,
            Self::Cleanable => 4,
        }
    }

    pub const fn suggests(self) -> OperatorAction {
        match self {
            Self::NeedsMe => OperatorAction::Resume,
            Self::Broken => OperatorAction::Repair,
            Self::Reviewable => OperatorAction::Review,
            Self::Cleanable => OperatorAction::Drop,
        }
    }

    pub const fn glyph(self) -> char {
        match self {
            Self::NeedsMe => '?',
            Self::Broken => '!',
            Self::Reviewable => 'R',
            Self::Cleanable => '~',
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::NeedsMe => "needs you",
            Self::Broken => "broken",
            Self::Reviewable => "reviewable",
            Self::Cleanable => "cleanable",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Deserialize, Serialize)]
pub enum SubstrateGap {
    WorktreeMissing,
    TmuxMissing,
    TaskWindowMissing,
    BranchMissing,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum Evidence {
    LiveStatus(LiveStatusKind),
    AgentStatus(AgentRuntimeStatus),
    SideFlag(SideFlag),
    Lifecycle(LifecycleStatus),
    Substrate(SubstrateGap),
    RuntimeObservationFailed,
    CheckoutMismatch,
}

impl Evidence {
    pub const fn label(&self) -> &'static str {
        match self {
            Evidence::LiveStatus(status) => match status {
                LiveStatusKind::WaitingForApproval => "waiting for approval",
                LiveStatusKind::WaitingForInput => "waiting for input",
                LiveStatusKind::AuthRequired => "auth required",
                LiveStatusKind::RateLimited => "rate limited",
                LiveStatusKind::ContextLimit => "context limit",
                LiveStatusKind::CommandFailed => "command failed",
                LiveStatusKind::Blocked => "blocked",
                LiveStatusKind::WorktreeMissing => "worktree missing",
                LiveStatusKind::TmuxMissing => "tmux missing",
                LiveStatusKind::TaskWindowMissing => "task window missing",
                LiveStatusKind::MergeConflict => "merge conflict",
                LiveStatusKind::Done => "done",
                LiveStatusKind::ShellIdle => "shell idle",
                LiveStatusKind::CommandRunning => "command running",
                LiveStatusKind::TestsRunning => "tests running",
                LiveStatusKind::AgentRunning => "agent running",
                LiveStatusKind::CiFailed => "ci failed",
                LiveStatusKind::CiPending => "ci running",
                LiveStatusKind::Unknown => "live status",
            },
            Evidence::AgentStatus(status) => match status {
                AgentRuntimeStatus::NotStarted => "agent not started",
                AgentRuntimeStatus::Running => "agent running",
                AgentRuntimeStatus::Waiting => "agent waiting",
                AgentRuntimeStatus::Blocked => "agent blocked",
                AgentRuntimeStatus::Done => "agent done",
                AgentRuntimeStatus::Dead => "agent dead",
                AgentRuntimeStatus::Unknown => "agent status not observed",
            },
            Evidence::SideFlag(flag) => match flag {
                SideFlag::Dirty => "dirty",
                SideFlag::AgentRunning => "agent running",
                SideFlag::AgentDead => "agent dead",
                SideFlag::NeedsInput => "needs input",
                SideFlag::TestsFailed => "tests failed",
                SideFlag::TmuxMissing => "tmux missing",
                SideFlag::WorktreeMissing => "worktree missing",
                SideFlag::TaskWindowMissing => "task window missing",
                SideFlag::BranchMissing => "branch missing",
                SideFlag::Stale => "stale",
                SideFlag::Conflicted => "conflicted",
                SideFlag::Unpushed => "unpushed",
            },
            Evidence::Lifecycle(status) => match status {
                LifecycleStatus::Created => "created",
                LifecycleStatus::Provisioning => "provisioning",
                LifecycleStatus::Active => "active",
                LifecycleStatus::Waiting => "waiting",
                LifecycleStatus::Reviewable => "reviewable",
                LifecycleStatus::Mergeable => "mergeable",
                LifecycleStatus::Merged => "merged",
                LifecycleStatus::Cleanable => "cleanable",
                LifecycleStatus::Removing => "removing",
                LifecycleStatus::TeardownIncomplete => "teardown incomplete",
                LifecycleStatus::Removed => "removed",
                LifecycleStatus::Orphaned => "orphaned",
                LifecycleStatus::Error => "error",
            },
            Evidence::Substrate(gap) => match gap {
                SubstrateGap::WorktreeMissing => "worktree missing",
                SubstrateGap::TmuxMissing => "tmux missing",
                SubstrateGap::TaskWindowMissing => "task window missing",
                SubstrateGap::BranchMissing => "branch missing",
            },
            Evidence::RuntimeObservationFailed => "status unavailable",
            Evidence::CheckoutMismatch => "checkout mismatch",
        }
    }

    pub const fn attention_label(&self) -> &'static str {
        match self {
            Evidence::LiveStatus(LiveStatusKind::WaitingForInput) => "needs input",
            evidence => evidence.label(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct Annotation {
    pub kind: AnnotationKind,
    pub severity: u32,
    pub evidence: Evidence,
    pub suggests: OperatorAction,
}

impl Annotation {
    pub fn new(kind: AnnotationKind, evidence: Evidence) -> Self {
        Self {
            kind,
            severity: kind.severity(),
            evidence,
            suggests: kind.suggests(),
        }
    }

    pub fn row_label(&self) -> String {
        if self.kind == AnnotationKind::NeedsMe && is_waiting_evidence(&self.evidence) {
            return self.evidence.label().to_string();
        }
        format!("{} · {}", self.kind.label(), self.evidence.label())
    }
}

const fn is_waiting_evidence(evidence: &Evidence) -> bool {
    matches!(
        evidence,
        Evidence::LiveStatus(LiveStatusKind::WaitingForApproval | LiveStatusKind::WaitingForInput)
    )
}

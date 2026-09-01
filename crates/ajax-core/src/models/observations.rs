use std::{path::PathBuf, time::SystemTime};

use serde::{Deserialize, Serialize};

use super::intent::AgentClient;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Deserialize, Serialize)]
pub enum SideFlag {
    Dirty,
    AgentRunning,
    AgentDead,
    NeedsInput,
    TestsFailed,
    TmuxMissing,
    WorktreeMissing,
    TaskWindowMissing,
    BranchMissing,
    Stale,
    Conflicted,
    Unpushed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum LiveStatusKind {
    WorktreeMissing,
    TmuxMissing,
    TaskWindowMissing,
    ShellIdle,
    CommandRunning,
    TestsRunning,
    AgentRunning,
    WaitingForApproval,
    WaitingForInput,
    Blocked,
    RateLimited,
    AuthRequired,
    MergeConflict,
    CiFailed,
    CiPending,
    ContextLimit,
    CommandFailed,
    Done,
    Unknown,
}

/// Attention class of a live-status kind. One shared classification consumed
/// by the operator-status reducer, annotations, and the waiting-confirmation
/// gate so their memberships cannot drift apart.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveStatusClass {
    Running,
    Waiting,
    Error,
    MissingSubstrate,
    Neutral,
}

impl LiveStatusKind {
    pub const fn class(self) -> LiveStatusClass {
        match self {
            Self::AgentRunning | Self::CommandRunning | Self::TestsRunning | Self::CiPending => {
                LiveStatusClass::Running
            }
            Self::WaitingForApproval
            | Self::WaitingForInput
            | Self::AuthRequired
            | Self::RateLimited
            | Self::ContextLimit
            | Self::Done => LiveStatusClass::Waiting,
            Self::CiFailed | Self::MergeConflict | Self::CommandFailed | Self::Blocked => {
                LiveStatusClass::Error
            }
            Self::WorktreeMissing | Self::TmuxMissing | Self::TaskWindowMissing => {
                LiveStatusClass::MissingSubstrate
            }
            Self::ShellIdle | Self::Unknown => LiveStatusClass::Neutral,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct LiveObservation {
    pub kind: LiveStatusKind,
    pub summary: String,
}

impl LiveObservation {
    pub fn new(kind: LiveStatusKind, summary: impl Into<String>) -> Self {
        Self {
            kind,
            summary: summary.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum AgentRuntimeStatus {
    NotStarted,
    Running,
    Waiting,
    Blocked,
    Dead,
    Done,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct AgentAttempt {
    pub agent: AgentClient,
    pub launch_target: String,
    pub started_at: SystemTime,
    pub finished_at: Option<SystemTime>,
    pub status: AgentRuntimeStatus,
}

impl AgentAttempt {
    pub fn new(agent: AgentClient, launch_target: impl Into<String>) -> Self {
        Self {
            agent,
            launch_target: launch_target.into(),
            started_at: SystemTime::now(),
            finished_at: None,
            status: AgentRuntimeStatus::Running,
        }
    }

    pub fn is_open(&self) -> bool {
        self.finished_at.is_none()
    }

    /// Close this launch-episode row with a terminal status and timestamp.
    pub fn close(&mut self, status: AgentRuntimeStatus, at: SystemTime) {
        if self.finished_at.is_some() {
            return;
        }
        self.status = status;
        self.finished_at = Some(at);
    }
}

/// Keep open launch-episode attempts aligned with `Task.agent_status`.
///
/// Close only when the launch ended: `NotStarted` (never started / spawn-auth
/// fail, and not interactive tmux `AgentRunning`), or `Dead` / Drop. Never close
/// on `Waiting`, `Blocked`, `Done`, or `Unknown`. Reopen the last attempt when
/// `Running` follows a provisional `NotStarted` close (ACP turn start after
/// `AgentCommandSent`).
pub fn sync_open_attempts(task: &mut super::Task, at: SystemTime) {
    match task.agent_status {
        AgentRuntimeStatus::Dead => close_open_attempts(task, AgentRuntimeStatus::Dead, at),
        AgentRuntimeStatus::NotStarted if !task.has_side_flag(SideFlag::AgentRunning) => {
            close_open_attempts(task, AgentRuntimeStatus::NotStarted, at);
        }
        AgentRuntimeStatus::Running => reopen_last_launch_attempt_if_closed_before_start(task),
        _ => {}
    }
}

fn close_open_attempts(task: &mut super::Task, status: AgentRuntimeStatus, at: SystemTime) {
    for attempt in &mut task.agent_attempts {
        if attempt.is_open() {
            attempt.close(status, at);
        }
    }
}

fn reopen_last_launch_attempt_if_closed_before_start(task: &mut super::Task) {
    let Some(last) = task.agent_attempts.last_mut() else {
        return;
    };
    if last.finished_at.is_some() && last.status == AgentRuntimeStatus::NotStarted {
        last.finished_at = None;
        last.status = AgentRuntimeStatus::Running;
    } else if last.is_open() {
        last.status = AgentRuntimeStatus::Running;
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct GitStatus {
    pub worktree_exists: bool,
    pub branch_exists: bool,
    #[serde(default)]
    pub current_branch: Option<String>,
    pub dirty: bool,
    pub ahead: u32,
    pub behind: u32,
    pub merged: bool,
    pub untracked_files: u32,
    pub unpushed_commits: u32,
    pub conflicted: bool,
    pub last_commit: Option<String>,
}

impl GitStatus {
    pub fn has_unpushed_work(&self) -> bool {
        self.unpushed_commits > 0 || self.ahead > 0
    }
}

impl SideFlag {
    pub fn is_missing_substrate(self) -> bool {
        matches!(
            self,
            SideFlag::TaskWindowMissing
                | SideFlag::TmuxMissing
                | SideFlag::WorktreeMissing
                | SideFlag::BranchMissing
        )
    }
}

impl LiveStatusKind {
    pub fn is_missing_substrate(self) -> bool {
        matches!(
            self,
            LiveStatusKind::WorktreeMissing
                | LiveStatusKind::TmuxMissing
                | LiveStatusKind::TaskWindowMissing
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct TmuxStatus {
    pub exists: bool,
    pub session_name: String,
}

impl TmuxStatus {
    pub fn present(session_name: impl Into<String>) -> Self {
        Self {
            exists: true,
            session_name: session_name.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct TaskWindowStatus {
    pub exists: bool,
    pub window_name: String,
    pub current_path: PathBuf,
    pub points_at_expected_path: bool,
}

impl TaskWindowStatus {
    pub fn present(window_name: impl Into<String>, current_path: impl Into<PathBuf>) -> Self {
        Self {
            exists: true,
            window_name: window_name.into(),
            current_path: current_path.into(),
            points_at_expected_path: true,
        }
    }

    pub fn missing(window_name: impl Into<String>, expected_path: impl Into<PathBuf>) -> Self {
        Self {
            exists: false,
            window_name: window_name.into(),
            current_path: expected_path.into(),
            points_at_expected_path: false,
        }
    }
}

use crate::models::{
    AgentRuntimeStatus, LifecycleStatus, LiveStatusClass, LiveStatusKind, SideFlag, Task,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Running,
    Waiting,
    Idle,
    Error,
    Unknown,
}

/// Which of the operator's four questions this task answers. Precedence
/// mirrors `derive_task_status`: an actionable gate is checked BEFORE the
/// review boundary, or a card reading "Waiting for approval" files under
/// review. Lifecycle is read directly so an acknowledged reviewable task
/// stays in `Review` instead of sinking to `Idle`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AttentionBand {
    NeedsYou,
    Review,
    Active,
    Idle,
}

pub fn attention_band(status: &OperatorStatus, lifecycle: LifecycleStatus) -> AttentionBand {
    if status.status == TaskStatus::Error {
        return AttentionBand::NeedsYou;
    }
    if status.status == TaskStatus::Waiting && status.actionable {
        return AttentionBand::NeedsYou;
    }
    if matches!(
        lifecycle,
        LifecycleStatus::Reviewable | LifecycleStatus::Mergeable
    ) {
        return AttentionBand::Review;
    }
    if status.status == TaskStatus::Running {
        return AttentionBand::Active;
    }
    if status.status == TaskStatus::Waiting {
        return AttentionBand::Active;
    }
    AttentionBand::Idle
}

impl TaskStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Waiting => "Waiting",
            Self::Idle => "Idle",
            Self::Error => "Error",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperatorStatus {
    pub status: TaskStatus,
    pub explanation: Option<String>,
    /// True when this is an actionable operator-attention state — an `Error`, or
    /// a genuine input/approval `Waiting` — that should phone-ping. Derived from
    /// structured evidence at projection time; the notifier reads this field, it
    /// never matches the explanation string.
    pub actionable: bool,
}

pub fn derive_operator_status(task: &Task) -> OperatorStatus {
    derive_task_status(task)
}

/// Error: always actionable.
fn err(explanation: impl Into<String>) -> OperatorStatus {
    OperatorStatus {
        status: TaskStatus::Error,
        explanation: Some(explanation.into()),
        actionable: true,
    }
}

/// Running: never actionable.
fn run(explanation: impl Into<String>) -> OperatorStatus {
    OperatorStatus {
        status: TaskStatus::Running,
        explanation: Some(explanation.into()),
        actionable: false,
    }
}

/// Actionable Waiting: a real operator input/approval gate — phone-pings.
fn ping(explanation: impl Into<String>) -> OperatorStatus {
    OperatorStatus {
        status: TaskStatus::Waiting,
        explanation: Some(explanation.into()),
        actionable: true,
    }
}

/// Soft Waiting: visible as Waiting but not a personal attention gate (auth,
/// rate limit, context limit, response-ready, ready-for-review, delegated) —
/// no phone-ping.
fn soft(explanation: impl Into<String>) -> OperatorStatus {
    OperatorStatus {
        status: TaskStatus::Waiting,
        explanation: Some(explanation.into()),
        actionable: false,
    }
}

fn idle() -> OperatorStatus {
    OperatorStatus {
        status: TaskStatus::Idle,
        explanation: None,
        actionable: false,
    }
}

fn unknown() -> OperatorStatus {
    OperatorStatus {
        status: TaskStatus::Unknown,
        explanation: None,
        actionable: false,
    }
}

fn derive_task_status(task: &Task) -> OperatorStatus {
    // 0. TeardownIncomplete is always an error (requirement 11).
    if task.lifecycle_status == LifecycleStatus::TeardownIncomplete {
        return err("Teardown incomplete");
    }

    // 1. Terminal/cleanup lifecycle decides whether runtime substrate is still
    //    expected. Once merged or being cleaned up, a missing tmux session,
    //    task window, worktree, or branch is normal — not an error (req 7, 10).
    let resources_expected = !matches!(
        task.lifecycle_status,
        LifecycleStatus::Merged
            | LifecycleStatus::Cleanable
            | LifecycleStatus::Removing
            | LifecycleStatus::Removed
    );

    // 2-4. Missing required substrate, an unobservable probe, or a checkout
    //      mismatch are errors only while the lifecycle still expects those
    //      resources (requirements 8-10).
    if resources_expected {
        if let Some(explanation) = canonical_missing_substrate_explanation(task) {
            return err(explanation);
        }
        if task.runtime_projection.observation_error.is_some() {
            return err("Status unavailable");
        }
        if let Some(explanation) = canonical_checkout_mismatch_explanation(task) {
            return err(explanation);
        }
    }

    // 5. Relevant GitHub failure/conflict (and other error-class live status)
    //    overrides the native agent phase (requirement 6).
    if let Some(live) = task.live_status.as_ref() {
        if let Some(explanation) = canonical_error_explanation(live.kind) {
            return err(explanation);
        }
    }
    if task.has_side_flag(SideFlag::TestsFailed) {
        return err("Tests failed");
    }
    if task.has_side_flag(SideFlag::Conflicted) {
        return err("Merge conflict");
    }
    if task.has_side_flag(SideFlag::AgentDead) || task.agent_status == AgentRuntimeStatus::Dead {
        return err("Agent unavailable");
    }
    if task.lifecycle_status == LifecycleStatus::Error {
        return err("Task failed");
    }

    // 6/9. Running: GitHub pending (CiPending, "CI running") or a native running
    //      phase. Passing CI is not represented here — it clears the override
    //      and reveals the native phase (requirement 6).
    if let Some(live) = task.live_status.as_ref() {
        if let Some(explanation) = canonical_running_explanation(live.kind) {
            return run(explanation);
        }
    }
    if task.agent_status == AgentRuntimeStatus::Running
        || task.has_side_flag(SideFlag::AgentRunning)
    {
        return run("Agent working");
    }

    // 14. Terminal/cleanup lifecycles are idle unless running/error overrode
    //     them above (requirement 10).
    if !resources_expected {
        return idle();
    }

    let live_acknowledged = live_evidence_is_acknowledged(task);
    if !live_acknowledged {
        if let Some(live) = task.live_status.as_ref() {
            // Delegated waiting is on children, not the operator — soft.
            if let Some(explanation) =
                crate::agent_status::operator_explanation_for_summary(&live.summary)
            {
                return soft(explanation);
            }
            if let Some((explanation, actionable)) = canonical_waiting_explanation(live.kind) {
                return if actionable {
                    ping(explanation)
                } else {
                    soft(explanation)
                };
            }
        }
    }

    // Lifecycle review boundary is Waiting in the UI but not a personal ping.
    if matches!(
        task.lifecycle_status,
        LifecycleStatus::Reviewable | LifecycleStatus::Mergeable
    ) && !workflow_boundary_is_acknowledged(task)
    {
        return soft("Ready for review");
    }
    if !live_acknowledged
        && (task.has_side_flag(SideFlag::NeedsInput)
            || task.agent_status == AgentRuntimeStatus::Waiting)
    {
        return ping("Waiting for input");
    }
    if !live_acknowledged && task.agent_status == AgentRuntimeStatus::Blocked {
        return err("Agent blocked");
    }
    if !live_acknowledged && task.agent_status == AgentRuntimeStatus::Done {
        return soft("Response ready");
    }

    // 16. An operational task with no status evidence at all cannot be proven
    //     Running, Waiting, Done, or Error — report Unknown rather than pretend
    //     it is at rest (precedence step 6). Every other resting state (terminal
    //     lifecycle, acknowledged waiting, any live status) is Idle above/here.
    if matches!(
        task.lifecycle_status,
        LifecycleStatus::Active | LifecycleStatus::Waiting
    ) && has_no_status_evidence(task)
    {
        return unknown();
    }

    idle()
}

/// Metadata key stamped by runtime refresh while the launch wrapper reports a
/// fresh live process. This is precedence tier 3: it proves the process exists
/// and never asserts activity, so it can rule out `Unknown` but never produce
/// `Running`.
pub const AGENT_PROCESS_ALIVE_KEY: &str = "agent_process_alive_at";

/// True when refresh last saw a fresh launch-wrapper heartbeat for this task.
///
/// Presence alone is the signal: refresh writes the key only while the
/// heartbeat is inside `agent_status::PROCESS_LIVENESS_FRESH_FOR` and removes
/// it otherwise, which keeps this projection free of any notion of "now".
pub fn agent_process_is_alive(task: &Task) -> bool {
    task.metadata.contains_key(AGENT_PROCESS_ALIVE_KEY)
}

/// True when a task carries no agent-status evidence of any kind: no live
/// status, an unstarted agent, no running/waiting side flags, and no confirmed
/// live process.
fn has_no_status_evidence(task: &Task) -> bool {
    task.live_status.is_none()
        && task.agent_status == AgentRuntimeStatus::NotStarted
        && !task.has_side_flag(SideFlag::AgentRunning)
        && !task.has_side_flag(SideFlag::NeedsInput)
        && !agent_process_is_alive(task)
}

fn live_evidence_is_acknowledged(task: &Task) -> bool {
    let Some(live) = task.live_status.as_ref() else {
        return false;
    };
    if live.kind.class() != LiveStatusClass::Waiting {
        return false;
    }
    matches!(
        (task.live_status_observed_at, task.attention_acknowledged_at),
        (Some(observed_at), Some(acknowledged_at)) if observed_at <= acknowledged_at
    )
}

fn workflow_boundary_is_acknowledged(task: &Task) -> bool {
    task.attention_acknowledged_at
        .is_some_and(|acknowledged_at| acknowledged_at >= task.last_activity_at)
}

fn canonical_running_explanation(kind: LiveStatusKind) -> Option<&'static str> {
    match kind {
        LiveStatusKind::AgentRunning => Some("Agent working"),
        LiveStatusKind::CommandRunning => Some("Running command"),
        LiveStatusKind::TestsRunning => Some("Running tests"),
        LiveStatusKind::CiPending => Some("CI running"),
        _ => None,
    }
}

/// Waiting-class explanation and whether it is an actionable operator gate.
/// Approval/input are actionable (phone-ping); auth, rate limit, context limit,
/// and response-ready are soft — visible as Waiting but not personal attention.
fn canonical_waiting_explanation(kind: LiveStatusKind) -> Option<(&'static str, bool)> {
    match kind {
        LiveStatusKind::WaitingForApproval => Some(("Waiting for approval", true)),
        LiveStatusKind::WaitingForInput => Some(("Waiting for input", true)),
        LiveStatusKind::AuthRequired => Some(("Authentication required", false)),
        LiveStatusKind::RateLimited => Some(("Rate limited", false)),
        LiveStatusKind::ContextLimit => Some(("Context limit reached", false)),
        LiveStatusKind::Done => Some(("Response ready", false)),
        _ => None,
    }
}

fn canonical_error_explanation(kind: LiveStatusKind) -> Option<&'static str> {
    match kind {
        LiveStatusKind::CiFailed => Some("CI failed"),
        LiveStatusKind::MergeConflict => Some("Merge conflict"),
        LiveStatusKind::CommandFailed => Some("Command failed"),
        LiveStatusKind::Blocked => Some("Agent blocked"),
        _ => None,
    }
}

fn canonical_checkout_mismatch_explanation(task: &Task) -> Option<String> {
    if task.has_missing_substrate() {
        return None;
    }
    task.checkout_mismatch_explanation()
}

fn canonical_missing_substrate_explanation(task: &Task) -> Option<&'static str> {
    missing_substrate_label(task).map(|label| match label {
        "worktree missing" => "Worktree missing",
        "branch missing" => "Branch missing",
        "tmux session missing" => "Tmux session missing",
        "task window missing" => "Task window missing",
        _ => "Runtime resource missing",
    })
}

fn missing_substrate_label(task: &Task) -> Option<&'static str> {
    if task.has_side_flag(SideFlag::WorktreeMissing)
        || task.runtime_projection.health == crate::models::RuntimeHealth::MissingWorktree
    {
        return Some("worktree missing");
    }
    if task.has_side_flag(SideFlag::BranchMissing) {
        return Some("branch missing");
    }
    if task.has_side_flag(SideFlag::TmuxMissing)
        || task.runtime_projection.health == crate::models::RuntimeHealth::MissingSession
    {
        return Some("tmux session missing");
    }
    if task.has_side_flag(SideFlag::TaskWindowMissing)
        || matches!(
            task.runtime_projection.health,
            crate::models::RuntimeHealth::MissingTaskWindow
                | crate::models::RuntimeHealth::WrongTaskWindowPath
        )
    {
        return Some("task window missing");
    }
    None
}

#[cfg(test)]
mod tests;

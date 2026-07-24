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
}

pub fn derive_operator_status(task: &Task) -> OperatorStatus {
    let (status, explanation) = derive_task_status(task);
    OperatorStatus {
        status,
        explanation,
    }
}

fn derive_task_status(task: &Task) -> (TaskStatus, Option<String>) {
    // 0. TeardownIncomplete is always an error (requirement 11).
    if task.lifecycle_status == LifecycleStatus::TeardownIncomplete {
        return canonical(TaskStatus::Error, "Teardown incomplete");
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
            return canonical(TaskStatus::Error, explanation);
        }
        if task.runtime_projection.observation_error.is_some() {
            return canonical(TaskStatus::Error, "Status unavailable");
        }
        if let Some(explanation) = canonical_checkout_mismatch_explanation(task) {
            return canonical(TaskStatus::Error, explanation);
        }
    }

    // 5. Relevant GitHub failure/conflict (and other error-class live status)
    //    overrides the native agent phase (requirement 6).
    if let Some(live) = task.live_status.as_ref() {
        if let Some(explanation) = canonical_error_explanation(live.kind) {
            return canonical(TaskStatus::Error, explanation);
        }
    }
    if task.has_side_flag(SideFlag::TestsFailed) {
        return canonical(TaskStatus::Error, "Tests failed");
    }
    if task.has_side_flag(SideFlag::Conflicted) {
        return canonical(TaskStatus::Error, "Merge conflict");
    }
    if task.has_side_flag(SideFlag::AgentDead) || task.agent_status == AgentRuntimeStatus::Dead {
        return canonical(TaskStatus::Error, "Agent unavailable");
    }
    if task.lifecycle_status == LifecycleStatus::Error {
        return canonical(TaskStatus::Error, "Task failed");
    }

    // 6/9. Running: GitHub pending (CiPending, "CI running") or a native running
    //      phase. Passing CI is not represented here — it clears the override
    //      and reveals the native phase (requirement 6).
    if let Some(live) = task.live_status.as_ref() {
        if let Some(explanation) = canonical_running_explanation(live.kind) {
            return canonical(TaskStatus::Running, explanation);
        }
    }
    if task.agent_status == AgentRuntimeStatus::Running
        || task.has_side_flag(SideFlag::AgentRunning)
    {
        return canonical(TaskStatus::Running, "Agent working");
    }

    // 14. Terminal/cleanup lifecycles are idle unless running/error overrode
    //     them above (requirement 10).
    if !resources_expected {
        return (TaskStatus::Idle, None);
    }

    let live_acknowledged = live_evidence_is_acknowledged(task);
    if !live_acknowledged {
        if let Some(live) = task.live_status.as_ref() {
            if let Some(explanation) =
                crate::agent_status::operator_explanation_for_summary(&live.summary)
            {
                return canonical(TaskStatus::Waiting, explanation);
            }
            if let Some(explanation) = canonical_waiting_explanation(live.kind) {
                return canonical(TaskStatus::Waiting, explanation);
            }
        }
    }

    if matches!(
        task.lifecycle_status,
        LifecycleStatus::Reviewable | LifecycleStatus::Mergeable
    ) && !workflow_boundary_is_acknowledged(task)
    {
        return canonical(TaskStatus::Waiting, "Ready for review");
    }
    if !live_acknowledged
        && (task.has_side_flag(SideFlag::NeedsInput)
            || task.agent_status == AgentRuntimeStatus::Waiting)
    {
        return canonical(TaskStatus::Waiting, "Waiting for input");
    }
    if !live_acknowledged && task.agent_status == AgentRuntimeStatus::Blocked {
        return canonical(TaskStatus::Error, "Agent blocked");
    }
    if !live_acknowledged && task.agent_status == AgentRuntimeStatus::Done {
        return canonical(TaskStatus::Waiting, "Response ready");
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
        return (TaskStatus::Unknown, None);
    }

    (TaskStatus::Idle, None)
}

/// True when a task carries no agent-status evidence of any kind: no live
/// status, an unstarted agent, and no running/waiting side flags.
fn has_no_status_evidence(task: &Task) -> bool {
    task.live_status.is_none()
        && task.agent_status == AgentRuntimeStatus::NotStarted
        && !task.has_side_flag(SideFlag::AgentRunning)
        && !task.has_side_flag(SideFlag::NeedsInput)
}

fn canonical(status: TaskStatus, explanation: impl Into<String>) -> (TaskStatus, Option<String>) {
    (status, Some(explanation.into()))
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

fn canonical_waiting_explanation(kind: LiveStatusKind) -> Option<&'static str> {
    match kind {
        LiveStatusKind::WaitingForApproval => Some("Waiting for approval"),
        LiveStatusKind::WaitingForInput => Some("Waiting for input"),
        LiveStatusKind::AuthRequired => Some("Authentication required"),
        LiveStatusKind::RateLimited => Some("Rate limited"),
        LiveStatusKind::ContextLimit => Some("Context limit reached"),
        LiveStatusKind::Done => Some("Response ready"),
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
mod tests {
    use super::{derive_operator_status, TaskStatus};
    use crate::{
        lifecycle::{
            mark_active, mark_cleanable, mark_error, mark_mergeable, mark_merged, mark_removed,
            mark_reviewable,
        },
        models::{
            AgentClient, AgentRuntimeStatus, GitStatus, LiveObservation, LiveStatusKind,
            RuntimeObservationSource, SideFlag, Task, TaskId,
        },
    };

    fn base_task() -> Task {
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

    fn claude_active_task() -> Task {
        let mut task = base_task();
        task.selected_agent = AgentClient::Claude;
        task.lifecycle_status = crate::models::LifecycleStatus::Active;
        task
    }

    #[test]
    fn acknowledged_claude_waiting_projects_idle() {
        let mut task = claude_active_task();
        crate::live::apply_observation_at(
            &mut task,
            LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input"),
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(400),
        );
        crate::live::acknowledge_attention(
            &mut task,
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(500),
        );

        let status = super::derive_operator_status(&task);

        assert_eq!(status.status, TaskStatus::Idle);
        assert_eq!(status.explanation, None);
        assert_eq!(
            task.lifecycle_status,
            crate::models::LifecycleStatus::Active
        );
    }

    #[test]
    fn new_claude_waiting_after_acknowledgment_projects_needs_input() {
        let mut task = claude_active_task();
        crate::live::apply_observation_at(
            &mut task,
            LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input"),
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(400),
        );
        crate::live::acknowledge_attention(
            &mut task,
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(500),
        );
        // Waiting evidence newer than the acknowledgment.
        crate::live::apply_observation_at(
            &mut task,
            LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input"),
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(600),
        );

        let status = super::derive_operator_status(&task);

        assert_eq!(status.status, TaskStatus::Waiting);
        assert_eq!(status.explanation.as_deref(), Some("Waiting for input"));
    }

    #[test]
    fn acknowledgment_does_not_hide_failure_or_missing_substrate() {
        // CommandFailed surfaces as a NeedsInput attention state and TmuxMissing
        // as Failed; acknowledgment must change neither, so neither becomes Idle.
        for status in [LiveStatusKind::CommandFailed, LiveStatusKind::TmuxMissing] {
            let mut task = claude_active_task();
            crate::live::apply_observation(&mut task, LiveObservation::new(status, "evidence"));
            let before = super::derive_operator_status(&task);

            crate::live::acknowledge_attention(
                &mut task,
                std::time::UNIX_EPOCH + std::time::Duration::from_secs(500),
            );
            let after = super::derive_operator_status(&task);

            assert_eq!(after, before, "{status:?}");
            assert_ne!(after.status, TaskStatus::Idle, "{status:?}");
        }
    }

    fn clean_git_status() -> GitStatus {
        GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: true,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: Some("abc123".to_string()),
        }
    }

    #[test]
    fn removed_lifecycle_becomes_archived_regardless_of_other_signals() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();
        mark_merged(&mut task).unwrap();
        mark_removed(&mut task).unwrap();
        task.add_side_flag(SideFlag::NeedsInput);
        task.add_side_flag(SideFlag::Dirty);

        assert_eq!(derive_operator_status(&task).status, TaskStatus::Idle);
    }

    #[test]
    fn needs_input_dominates_active_lifecycle() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        task.add_side_flag(SideFlag::NeedsInput);

        assert_eq!(derive_operator_status(&task).status, TaskStatus::Waiting);
    }

    #[test]
    fn needs_input_is_distinct_from_blocked() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        task.add_side_flag(SideFlag::NeedsInput);

        assert_eq!(derive_operator_status(&task).status, TaskStatus::Waiting);
    }

    #[test]
    fn blocker_signals_outrank_review_ready_lifecycle() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();
        task.add_side_flag(SideFlag::Conflicted);

        assert_eq!(derive_operator_status(&task).status, TaskStatus::Error);
    }

    #[test]
    fn waiting_agent_status_needs_input() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        task.agent_status = AgentRuntimeStatus::Waiting;

        assert_eq!(derive_operator_status(&task).status, TaskStatus::Waiting);
    }

    #[test]
    fn merge_conflict_live_status_is_blocked() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::MergeConflict,
            "conflict",
        ));

        assert_eq!(derive_operator_status(&task).status, TaskStatus::Error);
    }

    #[test]
    fn missing_substrate_is_failed_even_with_otherwise_clean_lifecycle() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        task.mark_resource_missing(SideFlag::WorktreeMissing);

        assert_eq!(derive_operator_status(&task).status, TaskStatus::Error);
    }

    #[test]
    fn runtime_probe_failure_is_failed_without_changing_lifecycle() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        task.record_runtime_probe_failure(
            RuntimeObservationSource::TmuxProbe,
            "tmux server unavailable",
        );

        assert_eq!(derive_operator_status(&task).status, TaskStatus::Error);
        assert_eq!(
            task.lifecycle_status,
            crate::models::LifecycleStatus::Active
        );
    }

    #[test]
    fn waiting_live_statuses_need_input_instead_of_blocking() {
        for live_status in [
            LiveStatusKind::WaitingForApproval,
            LiveStatusKind::WaitingForInput,
        ] {
            let mut task = base_task();
            mark_active(&mut task).unwrap();
            task.live_status = Some(LiveObservation::new(live_status, "waiting"));

            assert_eq!(
                derive_operator_status(&task).status,
                TaskStatus::Waiting,
                "{live_status:?}"
            );
        }
    }

    #[test]
    fn failure_live_statuses_project_error_and_operator_boundaries_project_waiting() {
        for live_status in [
            LiveStatusKind::CiFailed,
            LiveStatusKind::MergeConflict,
            LiveStatusKind::CommandFailed,
            LiveStatusKind::Blocked,
        ] {
            let mut task = base_task();
            mark_active(&mut task).unwrap();
            task.live_status = Some(LiveObservation::new(live_status, "blocked"));

            assert_eq!(
                derive_operator_status(&task).status,
                TaskStatus::Error,
                "{live_status:?}"
            );
        }

        for live_status in [
            LiveStatusKind::AuthRequired,
            LiveStatusKind::RateLimited,
            LiveStatusKind::ContextLimit,
        ] {
            let mut task = base_task();
            mark_active(&mut task).unwrap();
            task.live_status = Some(LiveObservation::new(live_status, "attention"));

            assert_eq!(
                derive_operator_status(&task).status,
                TaskStatus::Waiting,
                "{live_status:?}"
            );
        }
    }

    #[test]
    fn error_lifecycle_without_blocker_is_failed() {
        let mut task = base_task();
        mark_error(&mut task).unwrap();

        assert_eq!(derive_operator_status(&task).status, TaskStatus::Error);
    }

    #[test]
    fn mergeable_lifecycle_is_safe_merge() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();
        mark_mergeable(&mut task).unwrap();

        assert_eq!(derive_operator_status(&task).status, TaskStatus::Waiting);
    }

    #[test]
    fn mergeable_lifecycle_with_blocker_is_blocked() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();
        mark_mergeable(&mut task).unwrap();
        task.add_side_flag(SideFlag::Conflicted);

        assert_eq!(derive_operator_status(&task).status, TaskStatus::Error);
    }

    #[test]
    fn cleanable_lifecycle_is_cleanable() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();
        mark_merged(&mut task).unwrap();
        mark_cleanable(&mut task).unwrap();

        assert_eq!(derive_operator_status(&task).status, TaskStatus::Idle);
    }

    #[test]
    fn merged_lifecycle_with_clean_git_is_cleanable() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();
        mark_merged(&mut task).unwrap();
        task.git_status = Some(clean_git_status());

        assert_eq!(derive_operator_status(&task).status, TaskStatus::Idle);
    }

    #[test]
    fn merged_lifecycle_with_dirty_git_falls_back_to_idle() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();
        mark_merged(&mut task).unwrap();
        let mut git = clean_git_status();
        git.dirty = true;
        task.git_status = Some(git);
        task.add_side_flag(SideFlag::Dirty);

        assert_eq!(derive_operator_status(&task).status, TaskStatus::Idle);
    }

    #[test]
    fn reviewable_lifecycle_with_safe_merge_promotes_to_safe_merge() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();
        let mut git = clean_git_status();
        git.merged = false;
        task.git_status = Some(git);

        assert_eq!(derive_operator_status(&task).status, TaskStatus::Waiting);
    }

    #[test]
    fn reviewable_lifecycle_without_blocker_is_review_ready() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();

        assert_eq!(derive_operator_status(&task).status, TaskStatus::Waiting);
    }

    #[test]
    fn running_evidence_outranks_reviewable_lifecycle() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();
        task.agent_status = AgentRuntimeStatus::Running;
        task.add_side_flag(SideFlag::AgentRunning);

        assert_eq!(derive_operator_status(&task).status, TaskStatus::Running);
    }

    #[test]
    fn active_lifecycle_with_agent_running_is_running() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        task.agent_status = AgentRuntimeStatus::Running;
        task.add_side_flag(SideFlag::AgentRunning);

        assert_eq!(derive_operator_status(&task).status, TaskStatus::Running);
    }

    #[test]
    fn active_lifecycle_with_tests_running_live_status_is_running() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        task.live_status = Some(LiveObservation::new(LiveStatusKind::TestsRunning, "tests"));

        assert_eq!(derive_operator_status(&task).status, TaskStatus::Running);
    }

    #[test]
    fn active_lifecycle_without_signals_is_unknown() {
        // An active task with no live status, an unstarted agent, and no flags
        // has no source that can prove Running/Waiting/Done/Error — it projects
        // Unknown rather than a fabricated Idle (precedence step 6).
        let mut task = base_task();
        mark_active(&mut task).unwrap();

        assert_eq!(derive_operator_status(&task).status, TaskStatus::Unknown);
    }

    #[test]
    fn active_lifecycle_with_acknowledged_waiting_is_idle_not_unknown() {
        // Positive evidence of rest (an acknowledged waiting live status) keeps
        // the task Idle; only the true no-evidence case becomes Unknown.
        let mut task = claude_active_task();
        crate::live::apply_observation_at(
            &mut task,
            LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input"),
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(400),
        );
        crate::live::acknowledge_attention(
            &mut task,
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(500),
        );

        assert_eq!(derive_operator_status(&task).status, TaskStatus::Idle);
    }

    #[rstest::rstest]
    #[case(
        LiveStatusKind::AgentRunning,
        TaskStatus::Running,
        Some("Agent working")
    )]
    #[case(
        LiveStatusKind::CommandRunning,
        TaskStatus::Running,
        Some("Running command")
    )]
    #[case(
        LiveStatusKind::TestsRunning,
        TaskStatus::Running,
        Some("Running tests")
    )]
    #[case(
        LiveStatusKind::WaitingForApproval,
        TaskStatus::Waiting,
        Some("Waiting for approval")
    )]
    #[case(
        LiveStatusKind::WaitingForInput,
        TaskStatus::Waiting,
        Some("Waiting for input")
    )]
    #[case(LiveStatusKind::Done, TaskStatus::Waiting, Some("Response ready"))]
    #[case(
        LiveStatusKind::CommandFailed,
        TaskStatus::Error,
        Some("Command failed")
    )]
    #[case(LiveStatusKind::CiFailed, TaskStatus::Error, Some("CI failed"))]
    #[case(
        LiveStatusKind::MergeConflict,
        TaskStatus::Error,
        Some("Merge conflict")
    )]
    fn canonical_status_maps_live_evidence(
        #[case] live_kind: LiveStatusKind,
        #[case] expected_status: TaskStatus,
        #[case] expected_explanation: Option<&str>,
    ) {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        crate::live::apply_observation_at(
            &mut task,
            LiveObservation::new(live_kind, "raw summary"),
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(100),
        );

        let status = super::derive_operator_status(&task);

        assert_eq!(status.status, expected_status);
        assert_eq!(status.explanation.as_deref(), expected_explanation);
    }

    #[test]
    fn acknowledged_waiting_evidence_projects_idle_without_deleting_evidence() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        let observed_at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(100);
        crate::live::apply_observation_at(
            &mut task,
            LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting"),
            observed_at,
        );
        crate::live::acknowledge_attention(
            &mut task,
            observed_at + std::time::Duration::from_secs(1),
        );

        let status = super::derive_operator_status(&task);

        assert_eq!(status.status, TaskStatus::Idle);
        assert_eq!(status.explanation, None);
        assert_eq!(
            task.live_status.as_ref().map(|live| live.kind),
            Some(LiveStatusKind::WaitingForInput)
        );
    }

    #[test]
    fn reviewable_lifecycle_is_waiting_until_acknowledged() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();

        let before = super::derive_operator_status(&task);
        assert_eq!(before.status, TaskStatus::Waiting);
        assert_eq!(before.explanation.as_deref(), Some("Ready for review"));

        let acknowledged_at = task.last_activity_at + std::time::Duration::from_secs(1);
        crate::live::acknowledge_attention(&mut task, acknowledged_at);
        let after = super::derive_operator_status(&task);
        assert_eq!(after.status, TaskStatus::Idle);
        assert_eq!(after.explanation, None);
    }

    #[test]
    fn live_status_class_matches_canonical_explanations() {
        use crate::models::{LiveStatusClass, LiveStatusKind};
        let all = [
            LiveStatusKind::WorktreeMissing,
            LiveStatusKind::TmuxMissing,
            LiveStatusKind::TaskWindowMissing,
            LiveStatusKind::ShellIdle,
            LiveStatusKind::CommandRunning,
            LiveStatusKind::TestsRunning,
            LiveStatusKind::AgentRunning,
            LiveStatusKind::WaitingForApproval,
            LiveStatusKind::WaitingForInput,
            LiveStatusKind::Blocked,
            LiveStatusKind::RateLimited,
            LiveStatusKind::AuthRequired,
            LiveStatusKind::MergeConflict,
            LiveStatusKind::CiFailed,
            LiveStatusKind::ContextLimit,
            LiveStatusKind::CommandFailed,
            LiveStatusKind::Done,
            LiveStatusKind::Unknown,
        ];
        for kind in all {
            assert_eq!(
                super::canonical_waiting_explanation(kind).is_some(),
                kind.class() == LiveStatusClass::Waiting,
                "waiting membership diverged for {kind:?}"
            );
            assert_eq!(
                super::canonical_error_explanation(kind).is_some(),
                kind.class() == LiveStatusClass::Error,
                "error membership diverged for {kind:?}"
            );
            assert_eq!(
                super::canonical_running_explanation(kind).is_some(),
                kind.class() == LiveStatusClass::Running,
                "running membership diverged for {kind:?}"
            );
        }
    }

    #[test]
    fn stale_checkout_mismatch_health_defers_to_missing_worktree_status() {
        use crate::lifecycle::mark_active;
        use crate::models::RuntimeHealth;

        let mut task = base_task();
        mark_active(&mut task).unwrap();
        let mut git = clean_git_status();
        git.worktree_exists = false;
        git.current_branch = Some("fix/pane-stuck".to_string());
        task.git_status = Some(git);
        task.mark_resource_missing(SideFlag::WorktreeMissing);
        task.runtime_projection.health = RuntimeHealth::CheckoutMismatch;

        let status = derive_operator_status(&task);

        assert_eq!(status.status, TaskStatus::Error);
        assert_eq!(status.explanation.as_deref(), Some("Worktree missing"));
        assert!(!status
            .explanation
            .as_deref()
            .is_some_and(|explanation| explanation.contains("expected")));
        assert!(task.has_missing_substrate());
    }

    #[test]
    fn checkout_mismatch_status_names_observed_and_expected_checkout() {
        use crate::lifecycle::mark_active;
        use crate::models::RuntimeHealth;

        let mut named_branch = base_task();
        mark_active(&mut named_branch).unwrap();
        let mut git = clean_git_status();
        git.current_branch = Some("fix/pane-stuck".to_string());
        named_branch.git_status = Some(git);
        named_branch.runtime_projection.health = RuntimeHealth::CheckoutMismatch;

        let named_status = derive_operator_status(&named_branch);
        assert_eq!(named_status.status, TaskStatus::Error);
        assert_eq!(
            named_status.explanation.as_deref(),
            Some("Worktree on fix/pane-stuck; expected ajax/fix-login")
        );
        assert!(!named_status
            .explanation
            .as_deref()
            .is_some_and(|explanation| explanation.contains("missing")));
        assert!(!named_branch.has_missing_substrate());

        let mut detached = base_task();
        mark_active(&mut detached).unwrap();
        let mut detached_git = clean_git_status();
        detached_git.current_branch = None;
        detached.git_status = Some(detached_git);

        let detached_status = derive_operator_status(&detached);
        assert_eq!(detached_status.status, TaskStatus::Error);
        assert_eq!(
            detached_status.explanation.as_deref(),
            Some("Worktree detached; expected ajax/fix-login")
        );
        assert!(!detached_status
            .explanation
            .as_deref()
            .is_some_and(|explanation| explanation.contains("missing")));
        assert!(!detached.has_missing_substrate());
    }

    #[test]
    fn canonical_status_labels_are_stable_and_unique() {
        let labels = [
            TaskStatus::Running,
            TaskStatus::Waiting,
            TaskStatus::Idle,
            TaskStatus::Error,
        ]
        .map(TaskStatus::as_str);

        let mut sorted = labels.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), labels.len());
    }
}

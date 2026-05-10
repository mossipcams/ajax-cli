use std::time::SystemTime;

use crate::models::{AgentRuntimeStatus, LifecycleStatus, SideFlag, Task};

pub use crate::models::{LiveObservation, LiveStatusKind};

pub fn classify_pane(pane: &str) -> LiveObservation {
    let trimmed = pane.trim();
    if trimmed.is_empty() {
        return LiveObservation::new(LiveStatusKind::Unknown, "pane is empty");
    }

    let lower = trimmed.to_ascii_lowercase();

    if contains_any(
        &lower,
        &[
            "test result: ok",
            "tests passed",
            "all pre-pr checks passed",
            "successfully completed",
            "task complete",
            "all done",
            "done",
        ],
    ) {
        return LiveObservation::new(LiveStatusKind::Done, "done");
    }

    if looks_like_shell_prompt(trimmed) {
        return LiveObservation::new(LiveStatusKind::ShellIdle, "shell idle");
    }

    if contains_any(
        &lower,
        &[
            "do you want to proceed",
            "allow command",
            "approve",
            "approval",
            "y/n",
            "[y/n]",
        ],
    ) {
        return LiveObservation::new(LiveStatusKind::WaitingForApproval, "waiting for approval");
    }

    if contains_any(
        &lower,
        &[
            "please login",
            "please log in",
            "log in to",
            "login to continue",
            "authenticate",
            "auth required",
        ],
    ) {
        return LiveObservation::new(LiveStatusKind::AuthRequired, "authentication required");
    }

    if contains_any(
        &lower,
        &["rate limit", "too many requests", "try again later"],
    ) {
        return LiveObservation::new(LiveStatusKind::RateLimited, "rate limited");
    }

    if contains_any(&lower, &["context limit", "token limit", "context length"]) {
        return LiveObservation::new(LiveStatusKind::ContextLimit, "context limit reached");
    }

    if contains_any(
        &lower,
        &["blocked", "cannot continue", "manual intervention required"],
    ) {
        return LiveObservation::new(LiveStatusKind::Blocked, "blocked");
    }

    if contains_any(
        &lower,
        &[
            "merge conflict",
            "conflict (",
            "automatic merge failed",
            "fix conflicts",
        ],
    ) {
        return LiveObservation::new(
            LiveStatusKind::MergeConflict,
            "merge conflict needs attention",
        );
    }

    if contains_any(
        &lower,
        &[
            "ci failed",
            "github actions failed",
            "check run failed",
            "workflow failed",
            "failing checks",
        ],
    ) {
        return LiveObservation::new(LiveStatusKind::CiFailed, "ci failed");
    }

    if contains_any(
        &lower,
        &[
            "waiting for input",
            "press enter",
            "continue?",
            "enter your choice",
            "select an option",
        ],
    ) {
        return LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input");
    }

    if contains_any(
        &lower,
        &[
            "test result: failed",
            "command failed",
            "exit code",
            "nonzeroexit",
            "failed with",
        ],
    ) {
        return LiveObservation::new(LiveStatusKind::CommandFailed, "command failed");
    }

    if contains_any(
        &lower,
        &["running command", "executing command", "$ cargo", "$ npm"],
    ) {
        return LiveObservation::new(LiveStatusKind::CommandRunning, "command running");
    }

    if contains_any(
        &lower,
        &["cargo test", "running test", "running 0 tests", "running "],
    ) {
        return LiveObservation::new(LiveStatusKind::TestsRunning, "tests running");
    }

    if contains_any(
        &lower,
        &[
            "codex is working",
            "claude is working",
            "thinking",
            "working on your task",
        ],
    ) {
        return LiveObservation::new(LiveStatusKind::AgentRunning, "agent running");
    }

    LiveObservation::new(LiveStatusKind::Unknown, "unknown terminal state")
}

pub fn apply_observation(task: &mut Task, observation: LiveObservation) {
    let observation = reduce_task_live_observation(task, observation);
    let refresh_activity = refreshes_activity(observation.kind);
    let has_missing_substrate_flag = has_missing_substrate_flag(task);

    match observation.kind {
        LiveStatusKind::WorktreeMissing => {
            task.mark_resource_missing(SideFlag::WorktreeMissing);
        }
        LiveStatusKind::TmuxMissing => {
            task.mark_resource_missing(SideFlag::TmuxMissing);
        }
        LiveStatusKind::WorktrunkMissing => {
            task.mark_resource_missing(SideFlag::WorktrunkMissing);
        }
        LiveStatusKind::AgentRunning
        | LiveStatusKind::CommandRunning
        | LiveStatusKind::TestsRunning => {
            if has_missing_substrate_flag {
                task.agent_status = AgentRuntimeStatus::Unknown;
                task.remove_side_flag(SideFlag::AgentRunning);
            } else {
                task.agent_status = AgentRuntimeStatus::Running;
                task.add_side_flag(SideFlag::AgentRunning);
                update_live_lifecycle(task, LifecycleStatus::Active);
            }
            task.remove_side_flag(SideFlag::NeedsInput);
            task.remove_side_flag(SideFlag::AgentDead);
        }
        LiveStatusKind::WaitingForApproval | LiveStatusKind::WaitingForInput => {
            task.agent_status = AgentRuntimeStatus::Waiting;
            update_live_lifecycle(task, LifecycleStatus::Waiting);
            task.add_side_flag(SideFlag::NeedsInput);
            task.remove_side_flag(SideFlag::AgentRunning);
        }
        LiveStatusKind::AuthRequired
        | LiveStatusKind::RateLimited
        | LiveStatusKind::ContextLimit
        | LiveStatusKind::CiFailed
        | LiveStatusKind::CommandFailed
        | LiveStatusKind::Blocked => {
            task.agent_status = AgentRuntimeStatus::Blocked;
            update_live_lifecycle(task, LifecycleStatus::Error);
            if observation.kind == LiveStatusKind::CiFailed {
                task.add_side_flag(SideFlag::TestsFailed);
            }
            task.add_side_flag(SideFlag::NeedsInput);
            task.remove_side_flag(SideFlag::AgentRunning);
        }
        LiveStatusKind::MergeConflict => {
            task.agent_status = AgentRuntimeStatus::Blocked;
            update_live_lifecycle(task, LifecycleStatus::Error);
            task.add_side_flag(SideFlag::Conflicted);
            task.add_side_flag(SideFlag::NeedsInput);
            task.remove_side_flag(SideFlag::AgentRunning);
        }
        LiveStatusKind::ShellIdle => {
            task.agent_status = AgentRuntimeStatus::Unknown;
            task.remove_side_flag(SideFlag::AgentRunning);
        }
        LiveStatusKind::Done => {
            task.agent_status = AgentRuntimeStatus::Done;
            update_live_lifecycle(task, LifecycleStatus::Reviewable);
            task.remove_side_flag(SideFlag::AgentRunning);
            task.remove_side_flag(SideFlag::NeedsInput);
        }
        LiveStatusKind::Unknown => {
            task.agent_status = AgentRuntimeStatus::Unknown;
            task.remove_side_flag(SideFlag::AgentRunning);
        }
    }

    task.live_status = Some(observation);
    if refresh_activity {
        task.last_activity_at = SystemTime::now();
        task.remove_side_flag(SideFlag::Stale);
    }
}

fn reduce_task_live_observation(task: &Task, next: LiveObservation) -> LiveObservation {
    if recovered_from_missing_substrate(task, next.kind) {
        return next;
    }

    reduce_live_observation(task.live_status.as_ref(), next)
}

fn recovered_from_missing_substrate(task: &Task, next: LiveStatusKind) -> bool {
    task.live_status
        .as_ref()
        .is_some_and(|status| status.kind.is_missing_substrate())
        && !next.is_missing_substrate()
        && !has_missing_substrate_flag(task)
}

fn has_missing_substrate_flag(task: &Task) -> bool {
    task.side_flags().any(SideFlag::is_missing_substrate)
}

pub fn reduce_live_observation(
    current: Option<&LiveObservation>,
    next: LiveObservation,
) -> LiveObservation {
    let Some(current) = current else {
        return next;
    };

    if next.kind.is_missing_substrate() {
        return next;
    }

    if current.kind.is_missing_substrate() {
        return current.clone();
    }

    if should_keep_current_status(current.kind, next.kind) {
        return current.clone();
    }

    next
}

fn should_keep_current_status(current: LiveStatusKind, next: LiveStatusKind) -> bool {
    if current == LiveStatusKind::Done {
        return is_incidental_observation(next);
    }

    if is_waiting_status(current) {
        return is_incidental_observation(next);
    }

    if is_failure_status(current) {
        return is_incidental_observation(next);
    }

    false
}

fn is_waiting_status(kind: LiveStatusKind) -> bool {
    matches!(
        kind,
        LiveStatusKind::WaitingForApproval | LiveStatusKind::WaitingForInput
    )
}

fn is_failure_status(kind: LiveStatusKind) -> bool {
    matches!(
        kind,
        LiveStatusKind::AuthRequired
            | LiveStatusKind::RateLimited
            | LiveStatusKind::ContextLimit
            | LiveStatusKind::CiFailed
            | LiveStatusKind::CommandFailed
            | LiveStatusKind::Blocked
            | LiveStatusKind::MergeConflict
    )
}

fn is_incidental_observation(kind: LiveStatusKind) -> bool {
    matches!(
        kind,
        LiveStatusKind::ShellIdle
            | LiveStatusKind::Unknown
            | LiveStatusKind::AgentRunning
            | LiveStatusKind::CommandRunning
            | LiveStatusKind::TestsRunning
    )
}

fn refreshes_activity(kind: LiveStatusKind) -> bool {
    matches!(
        kind,
        LiveStatusKind::ShellIdle
            | LiveStatusKind::CommandRunning
            | LiveStatusKind::TestsRunning
            | LiveStatusKind::AgentRunning
            | LiveStatusKind::WaitingForApproval
            | LiveStatusKind::WaitingForInput
            | LiveStatusKind::Blocked
            | LiveStatusKind::RateLimited
            | LiveStatusKind::AuthRequired
            | LiveStatusKind::MergeConflict
            | LiveStatusKind::CiFailed
            | LiveStatusKind::ContextLimit
            | LiveStatusKind::CommandFailed
            | LiveStatusKind::Done
    )
}

fn update_live_lifecycle(task: &mut Task, status: LifecycleStatus) {
    if matches!(
        task.lifecycle_status,
        LifecycleStatus::Merged | LifecycleStatus::Cleanable | LifecycleStatus::Removed
    ) {
        return;
    }

    task.lifecycle_status = status;
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn looks_like_shell_prompt(text: &str) -> bool {
    text.lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .is_some_and(|line| {
            let line = line.trim_end();
            line.ends_with('%') || line.ends_with('$') || line.ends_with('#')
        })
}

#[cfg(test)]
mod tests {
    use crate::models::{
        AgentClient, AgentRuntimeStatus, LiveObservation, LiveStatusKind, SideFlag, Task, TaskId,
    };

    use super::{apply_observation, classify_pane};

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
            "worktrunk",
            AgentClient::Codex,
        )
    }

    #[test]
    fn pane_classifier_detects_agent_attention_states() {
        for (pane, expected) in [
            (
                "Do you want to proceed? y/n",
                LiveStatusKind::WaitingForApproval,
            ),
            (
                "Waiting for input. Press Enter to continue.",
                LiveStatusKind::WaitingForInput,
            ),
            ("Please login to continue", LiveStatusKind::AuthRequired),
            (
                "rate limit exceeded; try again later",
                LiveStatusKind::RateLimited,
            ),
            ("context limit reached", LiveStatusKind::ContextLimit),
            (
                "CONFLICT (content): merge conflict in src/lib.rs",
                LiveStatusKind::MergeConflict,
            ),
        ] {
            let observation = classify_pane(pane);

            assert_eq!(observation.kind, expected, "{pane}");
        }
    }

    #[test]
    fn pane_classifier_detects_conflict_and_ci_failure_evidence() {
        for (pane, expected) in [
            (
                "Automatic merge failed; fix conflicts and then commit the result.",
                LiveStatusKind::MergeConflict,
            ),
            (
                "CONFLICT (modify/delete): src/lib.rs deleted in HEAD and modified in feature",
                LiveStatusKind::MergeConflict,
            ),
            ("CI failed for this branch", LiveStatusKind::CiFailed),
            (
                "GitHub Actions failed: test.yml / build",
                LiveStatusKind::CiFailed,
            ),
            ("check run failed: cargo test", LiveStatusKind::CiFailed),
            ("workflow failed after 3m", LiveStatusKind::CiFailed),
            (
                "There are failing checks on the PR",
                LiveStatusKind::CiFailed,
            ),
        ] {
            let observation = classify_pane(pane);

            assert_eq!(observation.kind, expected, "{pane}");
        }
    }

    #[test]
    fn pane_classifier_detects_runtime_states() {
        for (pane, expected) in [
            (
                "cargo test --all-features\nrunning 37 tests",
                LiveStatusKind::TestsRunning,
            ),
            ("running command: npm test", LiveStatusKind::CommandRunning),
            ("test result: ok. 37 passed", LiveStatusKind::Done),
            (
                "codex is working on your task",
                LiveStatusKind::AgentRunning,
            ),
            (
                "Command failed with exit code 101",
                LiveStatusKind::CommandFailed,
            ),
            ("✓ Successfully completed task", LiveStatusKind::Done),
            ("matt@host project % ", LiveStatusKind::ShellIdle),
            ("", LiveStatusKind::Unknown),
        ] {
            let observation = classify_pane(pane);

            assert_eq!(observation.kind, expected, "{pane}");
        }
    }

    #[test]
    fn pane_classifier_uses_final_prompt_over_stale_running_history() {
        let pane = "\
The targeted checks pass. I’m continuing the cherry-pick now.
The rebased commit is created. I’m running the full pre-PR parity script now.
All pre-PR checks passed.
matt@Matts-MacBook-Pro ajax-tech-debt %";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::Done);
    }

    #[test]
    fn pane_classifier_uses_later_success_over_stale_failure_history() {
        let pane = "\
Earlier command failed with exit code 101.
I fixed the issue and reran the full suite.
All pre-PR checks passed.
matt@Matts-MacBook-Pro ajax-tech-debt %";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::Done);
    }

    #[test]
    fn pane_classifier_uses_final_prompt_over_stale_approval_history() {
        let pane = "\
Do you want to proceed? y/n
Approved and continued.
No more work is running.
matt@Matts-MacBook-Pro ajax-tech-debt %";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::ShellIdle);
    }

    #[test]
    fn pane_classifier_does_not_treat_login_task_text_as_auth_required() {
        let pane = "\
Task: Fix login form alignment
Review the button spacing.
matt@Matts-MacBook-Pro ajax-fix-login %";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::ShellIdle);
    }

    #[test]
    fn missing_resource_observations_clear_agent_running() {
        for status in [
            LiveStatusKind::WorktreeMissing,
            LiveStatusKind::TmuxMissing,
            LiveStatusKind::WorktrunkMissing,
        ] {
            let mut task = base_task();
            task.agent_status = AgentRuntimeStatus::Running;
            task.add_side_flag(SideFlag::AgentRunning);

            apply_observation(&mut task, LiveObservation::new(status, "resource missing"));

            assert_eq!(task.agent_status, AgentRuntimeStatus::Unknown, "{status:?}");
            assert!(!task.has_side_flag(SideFlag::AgentRunning), "{status:?}");
        }
    }

    #[test]
    fn running_observation_does_not_override_missing_resources() {
        let mut task = base_task();
        task.add_side_flag(SideFlag::WorktreeMissing);

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Unknown);
        assert!(!task.has_side_flag(SideFlag::AgentRunning));
        assert!(task.has_side_flag(SideFlag::WorktreeMissing));
    }

    #[test]
    fn recovered_missing_resource_can_accept_new_live_status() {
        let mut task = base_task();

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::WorktreeMissing, "worktree missing"),
        );
        task.remove_side_flag(SideFlag::WorktreeMissing);
        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Running);
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::AgentRunning)
        );
        assert!(!task.has_side_flag(SideFlag::WorktreeMissing));
    }

    #[test]
    fn done_observation_is_not_downgraded_by_shell_idle() {
        let mut task = base_task();

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::Done, "done"),
        );
        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::ShellIdle, "shell idle"),
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Done);
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::Done)
        );
    }

    #[test]
    fn waiting_observation_is_not_downgraded_by_incidental_activity() {
        let mut task = base_task();

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::WaitingForApproval, "waiting for approval"),
        );
        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting);
        assert!(task.has_side_flag(SideFlag::NeedsInput));
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::WaitingForApproval)
        );
    }

    #[test]
    fn failed_observation_is_not_downgraded_by_later_output() {
        let mut task = base_task();

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::CommandFailed, "command failed"),
        );
        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::CommandRunning, "command running"),
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Blocked);
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::CommandFailed)
        );
    }

    #[test]
    fn ci_failed_observation_marks_task_blocked_and_tests_failed() {
        let mut task = base_task();

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::CiFailed, "ci failed"),
        );
        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Blocked);
        assert!(task.has_side_flag(SideFlag::NeedsInput));
        assert!(task.has_side_flag(SideFlag::TestsFailed));
        assert!(!task.has_side_flag(SideFlag::AgentRunning));
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::CiFailed)
        );
    }

    #[test]
    fn active_live_observation_refreshes_activity_and_clears_stale() {
        let mut task = base_task();
        task.last_activity_at = std::time::SystemTime::UNIX_EPOCH;
        task.add_side_flag(SideFlag::Stale);

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert!(!task.has_side_flag(SideFlag::Stale));
        assert!(task.last_activity_at > std::time::SystemTime::UNIX_EPOCH);
    }
}

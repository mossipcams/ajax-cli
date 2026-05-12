pub use crate::live_application::apply_observation;
pub use crate::models::{LiveObservation, LiveStatusKind};

pub fn classify_pane(pane: &str) -> LiveObservation {
    let trimmed = pane.trim();
    if trimmed.is_empty() {
        return LiveObservation::new(LiveStatusKind::Unknown, "pane is empty");
    }

    let lines = meaningful_lines(trimmed);
    if lines
        .last()
        .is_some_and(|line| looks_like_shell_prompt(line))
    {
        if let Some(observation) = lines
            .iter()
            .rev()
            .nth(1)
            .and_then(|line| classify_pane_line(line))
        {
            return observation;
        }

        return LiveObservation::new(LiveStatusKind::ShellIdle, "shell idle");
    }

    lines
        .iter()
        .rev()
        .find_map(|line| classify_pane_line(line))
        .unwrap_or_else(|| LiveObservation::new(LiveStatusKind::Unknown, "unknown terminal state"))
}

fn meaningful_lines(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect()
}

fn classify_pane_line(line: &str) -> Option<LiveObservation> {
    let lower = line.to_ascii_lowercase();

    if is_completion_line(&lower) {
        return Some(LiveObservation::new(LiveStatusKind::Done, "done"));
    }

    if contains_any(
        &lower,
        &[
            "do you want to proceed",
            "approve to proceed",
            "allow command",
            "approval request",
            "y/n",
            "[y/n]",
        ],
    ) {
        return Some(LiveObservation::new(
            LiveStatusKind::WaitingForApproval,
            "waiting for approval",
        ));
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
        return Some(LiveObservation::new(
            LiveStatusKind::AuthRequired,
            "authentication required",
        ));
    }

    if contains_any(
        &lower,
        &["rate limit", "too many requests", "try again later"],
    ) {
        return Some(LiveObservation::new(
            LiveStatusKind::RateLimited,
            "rate limited",
        ));
    }

    if contains_any(&lower, &["context limit", "token limit", "context length"]) {
        return Some(LiveObservation::new(
            LiveStatusKind::ContextLimit,
            "context limit reached",
        ));
    }

    if contains_any(
        &lower,
        &["blocked", "cannot continue", "manual intervention required"],
    ) {
        return Some(LiveObservation::new(LiveStatusKind::Blocked, "blocked"));
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
        return Some(LiveObservation::new(
            LiveStatusKind::MergeConflict,
            "merge conflict needs attention",
        ));
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
        return Some(LiveObservation::new(LiveStatusKind::CiFailed, "ci failed"));
    }

    if contains_any(
        &lower,
        &[
            "waiting for input",
            "what kind of ",
            "what do you want me to",
            "what you want me to do",
            "send me the problem",
            "did you mean",
            "specific task",
            "press enter",
            "continue?",
            "enter your choice",
            "select an option",
        ],
    ) {
        return Some(LiveObservation::new(
            LiveStatusKind::WaitingForInput,
            "waiting for input",
        ));
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
        return Some(LiveObservation::new(
            LiveStatusKind::CommandFailed,
            "command failed",
        ));
    }

    if contains_any(
        &lower,
        &["running command", "executing command", "$ cargo", "$ npm"],
    ) {
        return Some(LiveObservation::new(
            LiveStatusKind::CommandRunning,
            "command running",
        ));
    }

    if contains_any(
        &lower,
        &["cargo test", "running test", "running 0 tests", "running "],
    ) {
        return Some(LiveObservation::new(
            LiveStatusKind::TestsRunning,
            "tests running",
        ));
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
        return Some(LiveObservation::new(
            LiveStatusKind::AgentRunning,
            "agent running",
        ));
    }

    None
}

fn is_completion_line(lower: &str) -> bool {
    contains_any(
        lower,
        &[
            "test result: ok",
            "tests passed",
            "all pre-pr checks passed",
            "successfully completed",
            "task complete",
            "all done",
        ],
    ) || lower.trim_matches(|character: char| !character.is_ascii_alphanumeric()) == "done"
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
    fn pane_classifier_detects_codex_clarification_prompts_as_waiting_for_input() {
        for pane in [
            "\
› Math

⚠ Heads up, you have less than 25% of your weekly limit left.

• What kind of math do you want to work on? Send me the problem, equation, or
  topic.

› Use /skills to list available skills",
            "\
› trst

⚠ Heads up, you have less than 25% of your weekly limit left.

• I’m not sure what you want me to do with “trst”. Did you mean “test”, or is
  there a specific task in this repo you want me to handle?

› Use /skills to list available skills",
        ] {
            let observation = classify_pane(pane);

            assert_eq!(observation.kind, LiveStatusKind::WaitingForInput, "{pane}");
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
    fn pane_classifier_treats_plan_approval_prompt_as_waiting_for_approval() {
        let pane = "\
Task 1: Badge accessibility + duplication cleanup

- Test to write: add failing Vitest coverage.
- Code to implement: extract a small internal badge-rendering helper.
- Verify: run rtk npm test -- badges.test.ts.

Plan ready. Approve to proceed.";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::WaitingForApproval);
    }

    #[test]
    fn pane_classifier_does_not_treat_negative_done_phrasing_as_complete() {
        let pane = "The task is not done yet; running cargo test now";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::TestsRunning);
    }

    #[test]
    fn pane_classifier_uses_current_failure_over_stale_success_history() {
        let pane = "\
All pre-PR checks passed.
Later validation found a regression.
Command failed with exit code 101
matt@Matts-MacBook-Pro ajax-tech-debt %";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::CommandFailed);
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
    fn live_lifecycle_updates_ignore_invalid_transition_edges() {
        let mut task = base_task();
        task.lifecycle_status = crate::models::LifecycleStatus::Error;

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::WaitingForApproval, "waiting for approval"),
        );

        assert_eq!(task.lifecycle_status, crate::models::LifecycleStatus::Error);
        assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting);
        assert!(task.has_side_flag(SideFlag::NeedsInput));
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

    #[test]
    fn live_projection_functions_do_not_mutate_lifecycle_or_substrate() {
        let task = base_task();
        let lifecycle_before = task.lifecycle_status;
        let git_before = task.git_status.clone();
        let tmux_before = task.tmux_status.clone();
        let worktrunk_before = task.worktrunk_status.clone();

        let classified = classify_pane("Do you want to proceed? y/n\n");
        let reduced = super::reduce_live_observation(
            task.live_status.as_ref(),
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert_eq!(classified.kind, LiveStatusKind::WaitingForApproval);
        assert_eq!(reduced.kind, LiveStatusKind::AgentRunning);
        assert_eq!(task.lifecycle_status, lifecycle_before);
        assert_eq!(task.git_status, git_before);
        assert_eq!(task.tmux_status, tmux_before);
        assert_eq!(task.worktrunk_status, worktrunk_before);
    }

    #[test]
    fn live_projection_module_does_not_own_lifecycle_mutation() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/live.rs"),
        )
        .unwrap();

        let transition_call = ["transition", "_lifecycle("].concat();
        let transition_reason = ["Lifecycle", "TransitionReason"].concat();

        assert!(!source.contains(&transition_call));
        assert!(!source.contains(&transition_reason));
    }
}

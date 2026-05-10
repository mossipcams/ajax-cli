use crate::models::{AgentRuntimeStatus, SideFlag, Task};

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
        &["login", "log in", "authenticate", "auth required"],
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
        &["merge conflict", "conflict (content)", "fix conflicts"],
    ) {
        return LiveObservation::new(
            LiveStatusKind::MergeConflict,
            "merge conflict needs attention",
        );
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
            "error:",
            "failed with",
        ],
    ) {
        return LiveObservation::new(LiveStatusKind::CommandFailed, "command failed");
    }

    if contains_any(&lower, &["test result: ok", "tests passed"]) {
        return LiveObservation::new(LiveStatusKind::Done, "done");
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

    if contains_any(
        &lower,
        &[
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

    LiveObservation::new(LiveStatusKind::Unknown, "unknown terminal state")
}

pub fn apply_observation(task: &mut Task, observation: LiveObservation) {
    match observation.kind {
        LiveStatusKind::WorktreeMissing => {
            mark_resource_missing(task, SideFlag::WorktreeMissing);
        }
        LiveStatusKind::TmuxMissing => {
            mark_resource_missing(task, SideFlag::TmuxMissing);
        }
        LiveStatusKind::WorktrunkMissing => {
            mark_resource_missing(task, SideFlag::WorktrunkMissing);
        }
        LiveStatusKind::AgentRunning
        | LiveStatusKind::CommandRunning
        | LiveStatusKind::TestsRunning => {
            if has_missing_resource(task) {
                task.agent_status = AgentRuntimeStatus::Unknown;
                task.remove_side_flag(SideFlag::AgentRunning);
            } else {
                task.agent_status = AgentRuntimeStatus::Running;
                task.add_side_flag(SideFlag::AgentRunning);
            }
            task.remove_side_flag(SideFlag::NeedsInput);
            task.remove_side_flag(SideFlag::AgentDead);
        }
        LiveStatusKind::WaitingForApproval | LiveStatusKind::WaitingForInput => {
            task.agent_status = AgentRuntimeStatus::Waiting;
            task.add_side_flag(SideFlag::NeedsInput);
            task.remove_side_flag(SideFlag::AgentRunning);
        }
        LiveStatusKind::AuthRequired
        | LiveStatusKind::RateLimited
        | LiveStatusKind::ContextLimit
        | LiveStatusKind::CommandFailed
        | LiveStatusKind::Blocked => {
            task.agent_status = AgentRuntimeStatus::Blocked;
            task.add_side_flag(SideFlag::NeedsInput);
            task.remove_side_flag(SideFlag::AgentRunning);
        }
        LiveStatusKind::MergeConflict => {
            task.agent_status = AgentRuntimeStatus::Blocked;
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
            task.remove_side_flag(SideFlag::AgentRunning);
            task.remove_side_flag(SideFlag::NeedsInput);
        }
        LiveStatusKind::Unknown => {
            task.agent_status = AgentRuntimeStatus::Unknown;
            task.remove_side_flag(SideFlag::AgentRunning);
        }
    }

    task.live_status = Some(observation);
}

fn mark_resource_missing(task: &mut Task, flag: SideFlag) {
    task.agent_status = AgentRuntimeStatus::Unknown;
    task.add_side_flag(flag);
    task.remove_side_flag(SideFlag::AgentRunning);
}

fn has_missing_resource(task: &Task) -> bool {
    [
        SideFlag::WorktreeMissing,
        SideFlag::TmuxMissing,
        SideFlag::WorktrunkMissing,
        SideFlag::BranchMissing,
    ]
    .into_iter()
    .any(|flag| task.has_side_flag(flag))
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
}

use crate::models::{LifecycleStatus, Task};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleTransitionReason {
    Generic,
    Recovery,
    OperationResult,
    ForceRemove,
    Restore,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LifecycleTransitionError {
    pub from: LifecycleStatus,
    pub to: LifecycleStatus,
    pub reason: LifecycleTransitionReason,
}

pub fn validate_lifecycle_transition(
    from: LifecycleStatus,
    to: LifecycleStatus,
    reason: LifecycleTransitionReason,
) -> Result<(), LifecycleTransitionError> {
    if from == to
        || generic_transition_allowed(from, to)
        || named_transition_allowed(from, to, reason)
    {
        return Ok(());
    }

    Err(LifecycleTransitionError { from, to, reason })
}

pub fn transition_lifecycle(
    task: &mut Task,
    status: LifecycleStatus,
    reason: LifecycleTransitionReason,
) -> Result<(), LifecycleTransitionError> {
    let from = task.lifecycle_status;
    if let Err(error) = validate_lifecycle_transition(from, status, reason) {
        tracing::debug!(
            target: "ajax_core",
            from = ?error.from,
            to = ?error.to,
            reason = ?error.reason,
            "lifecycle"
        );
        return Err(error);
    }
    task.lifecycle_status = status;
    if from != status {
        tracing::info!(
            target: "ajax_core",
            from = ?from,
            to = ?status,
            reason = ?reason,
            "lifecycle"
        );
    }
    Ok(())
}

pub fn hydrate_lifecycle_status(task: &mut Task, status: LifecycleStatus) {
    task.lifecycle_status = status;
}

pub fn mark_provisioning(task: &mut Task) -> Result<(), LifecycleTransitionError> {
    transition_lifecycle(
        task,
        LifecycleStatus::Provisioning,
        LifecycleTransitionReason::Generic,
    )
}

pub fn mark_active(task: &mut Task) -> Result<(), LifecycleTransitionError> {
    transition_lifecycle(
        task,
        LifecycleStatus::Active,
        LifecycleTransitionReason::Generic,
    )
}

pub fn restore_active(task: &mut Task) -> Result<(), LifecycleTransitionError> {
    transition_lifecycle(
        task,
        LifecycleStatus::Active,
        LifecycleTransitionReason::Restore,
    )
}

pub fn mark_reviewable(task: &mut Task) -> Result<(), LifecycleTransitionError> {
    transition_lifecycle(
        task,
        LifecycleStatus::Reviewable,
        LifecycleTransitionReason::Generic,
    )
}

pub fn mark_mergeable(task: &mut Task) -> Result<(), LifecycleTransitionError> {
    transition_lifecycle(
        task,
        LifecycleStatus::Mergeable,
        LifecycleTransitionReason::Generic,
    )
}

pub fn mark_merged(task: &mut Task) -> Result<(), LifecycleTransitionError> {
    transition_lifecycle(
        task,
        LifecycleStatus::Merged,
        LifecycleTransitionReason::Generic,
    )
}

pub fn mark_cleanable(task: &mut Task) -> Result<(), LifecycleTransitionError> {
    transition_lifecycle(
        task,
        LifecycleStatus::Cleanable,
        LifecycleTransitionReason::Generic,
    )
}

pub fn mark_removed(task: &mut Task) -> Result<(), LifecycleTransitionError> {
    transition_lifecycle(
        task,
        LifecycleStatus::Removed,
        LifecycleTransitionReason::Generic,
    )
}

pub fn force_mark_removed(task: &mut Task) -> Result<(), LifecycleTransitionError> {
    transition_lifecycle(
        task,
        LifecycleStatus::Removed,
        LifecycleTransitionReason::ForceRemove,
    )
}

pub fn mark_error(task: &mut Task) -> Result<(), LifecycleTransitionError> {
    transition_lifecycle(
        task,
        LifecycleStatus::Error,
        LifecycleTransitionReason::Generic,
    )
}

fn named_transition_allowed(
    from: LifecycleStatus,
    to: LifecycleStatus,
    reason: LifecycleTransitionReason,
) -> bool {
    matches!(
        (reason, from, to),
        (
            LifecycleTransitionReason::Recovery | LifecycleTransitionReason::OperationResult,
            LifecycleStatus::Error,
            LifecycleStatus::Active | LifecycleStatus::Reviewable
        ) | (
            LifecycleTransitionReason::ForceRemove,
            LifecycleStatus::Created
                | LifecycleStatus::Provisioning
                | LifecycleStatus::Active
                | LifecycleStatus::Waiting
                | LifecycleStatus::Reviewable
                | LifecycleStatus::Mergeable
                | LifecycleStatus::Merged
                | LifecycleStatus::Cleanable
                | LifecycleStatus::Orphaned
                | LifecycleStatus::Error,
            LifecycleStatus::Removed
        ) | (
            LifecycleTransitionReason::Restore,
            LifecycleStatus::Removed,
            LifecycleStatus::Active
        )
    )
}

fn generic_transition_allowed(from: LifecycleStatus, to: LifecycleStatus) -> bool {
    matches!(
        (from, to),
        (LifecycleStatus::Created, LifecycleStatus::Provisioning)
            | (LifecycleStatus::Created, LifecycleStatus::Active)
            | (LifecycleStatus::Created, LifecycleStatus::Reviewable)
            | (LifecycleStatus::Created, LifecycleStatus::Error)
            | (LifecycleStatus::Provisioning, LifecycleStatus::Active)
            | (LifecycleStatus::Provisioning, LifecycleStatus::Error)
            | (LifecycleStatus::Active, LifecycleStatus::Waiting)
            | (LifecycleStatus::Active, LifecycleStatus::Reviewable)
            | (LifecycleStatus::Active, LifecycleStatus::Error)
            | (LifecycleStatus::Waiting, LifecycleStatus::Active)
            | (LifecycleStatus::Waiting, LifecycleStatus::Reviewable)
            | (LifecycleStatus::Waiting, LifecycleStatus::Error)
            | (LifecycleStatus::Reviewable, LifecycleStatus::Mergeable)
            | (LifecycleStatus::Reviewable, LifecycleStatus::Merged)
            | (LifecycleStatus::Reviewable, LifecycleStatus::Error)
            | (LifecycleStatus::Mergeable, LifecycleStatus::Merged)
            | (LifecycleStatus::Mergeable, LifecycleStatus::Error)
            | (LifecycleStatus::Created, LifecycleStatus::Removing)
            | (LifecycleStatus::Provisioning, LifecycleStatus::Removing)
            | (LifecycleStatus::Active, LifecycleStatus::Removing)
            | (LifecycleStatus::Waiting, LifecycleStatus::Removing)
            | (LifecycleStatus::Reviewable, LifecycleStatus::Removing)
            | (LifecycleStatus::Mergeable, LifecycleStatus::Removing)
            | (LifecycleStatus::Merged, LifecycleStatus::Removing)
            | (LifecycleStatus::Merged, LifecycleStatus::Cleanable)
            | (LifecycleStatus::Cleanable, LifecycleStatus::Removing)
            | (
                LifecycleStatus::Removing,
                LifecycleStatus::TeardownIncomplete
            )
            | (
                LifecycleStatus::TeardownIncomplete,
                LifecycleStatus::Removing
            )
            | (LifecycleStatus::Removing, LifecycleStatus::Removed)
            | (
                LifecycleStatus::TeardownIncomplete,
                LifecycleStatus::Removed
            )
            | (LifecycleStatus::Merged, LifecycleStatus::Removed)
            | (LifecycleStatus::Merged, LifecycleStatus::Error)
            | (LifecycleStatus::Cleanable, LifecycleStatus::Removed)
            | (LifecycleStatus::Cleanable, LifecycleStatus::Error)
            | (LifecycleStatus::Error, LifecycleStatus::Removing)
            | (LifecycleStatus::Orphaned, LifecycleStatus::Removing)
            | (LifecycleStatus::Orphaned, LifecycleStatus::Error)
    )
}

#[cfg(test)]
mod tests {
    use super::{
        mark_active, mark_cleanable, mark_error, mark_merged, mark_provisioning, mark_removed,
        mark_reviewable, restore_active, transition_lifecycle, validate_lifecycle_transition,
        LifecycleTransitionReason,
    };
    use crate::models::{AgentClient, LifecycleStatus, Task, TaskId};
    use rstest::rstest;

    #[rstest]
    #[case(LifecycleStatus::Created, LifecycleStatus::Provisioning)]
    #[case(LifecycleStatus::Provisioning, LifecycleStatus::Active)]
    #[case(LifecycleStatus::Provisioning, LifecycleStatus::Error)]
    #[case(LifecycleStatus::Active, LifecycleStatus::Waiting)]
    #[case(LifecycleStatus::Active, LifecycleStatus::Reviewable)]
    #[case(LifecycleStatus::Waiting, LifecycleStatus::Active)]
    #[case(LifecycleStatus::Waiting, LifecycleStatus::Reviewable)]
    #[case(LifecycleStatus::Reviewable, LifecycleStatus::Mergeable)]
    #[case(LifecycleStatus::Reviewable, LifecycleStatus::Merged)]
    #[case(LifecycleStatus::Mergeable, LifecycleStatus::Merged)]
    #[case(LifecycleStatus::Active, LifecycleStatus::Removing)]
    #[case(LifecycleStatus::Reviewable, LifecycleStatus::Removing)]
    #[case(LifecycleStatus::Merged, LifecycleStatus::Removing)]
    #[case(LifecycleStatus::Merged, LifecycleStatus::Cleanable)]
    #[case(LifecycleStatus::Cleanable, LifecycleStatus::Removing)]
    #[case(LifecycleStatus::Error, LifecycleStatus::Removing)]
    #[case(LifecycleStatus::Orphaned, LifecycleStatus::Removing)]
    #[case(LifecycleStatus::Removing, LifecycleStatus::TeardownIncomplete)]
    #[case(LifecycleStatus::TeardownIncomplete, LifecycleStatus::Removing)]
    #[case(LifecycleStatus::Removing, LifecycleStatus::Removed)]
    #[case(LifecycleStatus::TeardownIncomplete, LifecycleStatus::Removed)]
    #[case(LifecycleStatus::Merged, LifecycleStatus::Removed)]
    #[case(LifecycleStatus::Cleanable, LifecycleStatus::Removed)]
    fn generic_lifecycle_transition_matrix_allows_valid_edges(
        #[case] from: LifecycleStatus,
        #[case] to: LifecycleStatus,
    ) {
        assert!(
            validate_lifecycle_transition(from, to, LifecycleTransitionReason::Generic).is_ok(),
            "{from:?} -> {to:?}"
        );
    }

    #[rstest]
    #[case(LifecycleStatus::Created, LifecycleStatus::Merged)]
    #[case(LifecycleStatus::Active, LifecycleStatus::Merged)]
    #[case(LifecycleStatus::Reviewable, LifecycleStatus::Removed)]
    #[case(LifecycleStatus::Mergeable, LifecycleStatus::Cleanable)]
    #[case(LifecycleStatus::Merged, LifecycleStatus::Active)]
    #[case(LifecycleStatus::Cleanable, LifecycleStatus::Active)]
    #[case(LifecycleStatus::Removed, LifecycleStatus::Active)]
    #[case(LifecycleStatus::Error, LifecycleStatus::Active)]
    fn generic_lifecycle_transition_matrix_blocks_invalid_edges(
        #[case] from: LifecycleStatus,
        #[case] to: LifecycleStatus,
    ) {
        assert!(
            validate_lifecycle_transition(from, to, LifecycleTransitionReason::Generic).is_err(),
            "{from:?} -> {to:?}"
        );
    }

    #[rstest]
    #[case(LifecycleStatus::Active, LifecycleTransitionReason::Recovery)]
    #[case(LifecycleStatus::Reviewable, LifecycleTransitionReason::Recovery)]
    #[case(LifecycleStatus::Active, LifecycleTransitionReason::OperationResult)]
    #[case(
        LifecycleStatus::Reviewable,
        LifecycleTransitionReason::OperationResult
    )]
    fn error_lifecycle_recovery_requires_named_transition_reason(
        #[case] to: LifecycleStatus,
        #[case] reason: LifecycleTransitionReason,
    ) {
        assert!(
            validate_lifecycle_transition(
                LifecycleStatus::Error,
                to,
                LifecycleTransitionReason::Generic
            )
            .is_err(),
            "generic Error -> {to:?} should stay blocked"
        );
        assert!(
            validate_lifecycle_transition(LifecycleStatus::Error, to, reason).is_ok(),
            "{reason:?} Error -> {to:?}"
        );
    }

    fn task() -> Task {
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

    fn capture_tracing_output<F: FnOnce()>(level: tracing::Level, f: F) -> String {
        use std::io::Write;
        use std::sync::{Arc, Mutex};

        struct CapturingWriter(Arc<Mutex<Vec<u8>>>);

        impl Write for CapturingWriter {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(buf);
                Ok(buf.len())
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let buffer = Arc::new(Mutex::new(Vec::new()));
        let writer = {
            let buffer = Arc::clone(&buffer);
            move || CapturingWriter(Arc::clone(&buffer))
        };
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(level)
            .with_writer(writer)
            .with_ansi(false)
            .with_target(false)
            .finish();

        tracing::subscriber::with_default(subscriber, f);
        let bytes = buffer.lock().unwrap().clone();
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn transition_lifecycle_logs_successful_transition_at_info() {
        let output = capture_tracing_output(tracing::Level::INFO, || {
            let mut task = task();
            mark_active(&mut task).unwrap();
            mark_reviewable(&mut task).unwrap();
        });

        assert!(
            output.contains("lifecycle")
                && output.contains("from=")
                && output.contains("to=")
                && output.contains("reason="),
            "expected lifecycle info log with from/to/reason, got: {output}"
        );
        assert!(
            output.contains("Reviewable"),
            "expected Reviewable in lifecycle log, got: {output}"
        );
    }

    #[test]
    fn rejected_transition_logs_debug_with_from_to_and_reason() {
        let output = capture_tracing_output(tracing::Level::DEBUG, || {
            let mut task = task();
            let _ = transition_lifecycle(
                &mut task,
                LifecycleStatus::Merged,
                LifecycleTransitionReason::Generic,
            );
        });

        assert!(
            output.contains("lifecycle")
                && output.contains("from=")
                && output.contains("to=")
                && output.contains("reason="),
            "expected lifecycle debug log with from/to/reason, got: {output}"
        );
        assert!(
            output.contains("Merged"),
            "expected blocked target status in debug log, got: {output}"
        );
    }

    #[test]
    fn noop_same_status_transition_does_not_log_info() {
        let output = capture_tracing_output(tracing::Level::INFO, || {
            let mut task = task();
            mark_active(&mut task).unwrap();
            transition_lifecycle(
                &mut task,
                LifecycleStatus::Active,
                LifecycleTransitionReason::Generic,
            )
            .unwrap();
        });

        let lifecycle_lines: Vec<_> = output
            .lines()
            .filter(|line| line.contains("lifecycle"))
            .collect();
        assert_eq!(
            lifecycle_lines.len(),
            1,
            "expected one lifecycle info line for Created -> Active only, got: {output}"
        );
    }

    #[test]
    fn transition_lifecycle_mutates_only_after_validating_edge() {
        let mut task = task();

        transition_lifecycle(
            &mut task,
            LifecycleStatus::Provisioning,
            LifecycleTransitionReason::Generic,
        )
        .unwrap();

        assert_eq!(task.lifecycle_status, LifecycleStatus::Provisioning);
    }

    #[test]
    fn rejected_transition_leaves_lifecycle_unchanged() {
        let mut task = task();

        let result = transition_lifecycle(
            &mut task,
            LifecycleStatus::Merged,
            LifecycleTransitionReason::Generic,
        );

        assert!(result.is_err());
        assert_eq!(task.lifecycle_status, LifecycleStatus::Created);
    }

    #[test]
    fn removed_task_cannot_be_reactivated_without_explicit_restore_transition() {
        let mut task = task();
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();
        mark_merged(&mut task).unwrap();
        mark_removed(&mut task).unwrap();

        let result = mark_active(&mut task);

        assert!(result.is_err());
        assert_eq!(task.lifecycle_status, LifecycleStatus::Removed);

        restore_active(&mut task).unwrap();

        assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
    }

    #[test]
    fn named_helpers_apply_expected_lifecycle_transitions() {
        let mut task = task();

        mark_provisioning(&mut task).unwrap();
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();
        mark_merged(&mut task).unwrap();
        mark_cleanable(&mut task).unwrap();
        mark_removed(&mut task).unwrap();

        assert_eq!(task.lifecycle_status, LifecycleStatus::Removed);
    }

    #[test]
    fn evidence_driven_operation_result_can_recover_error_to_reviewable() {
        let mut task = task();
        mark_error(&mut task).unwrap();

        transition_lifecycle(
            &mut task,
            LifecycleStatus::Reviewable,
            LifecycleTransitionReason::OperationResult,
        )
        .unwrap();

        assert_eq!(task.lifecycle_status, LifecycleStatus::Reviewable);
    }

    #[test]
    fn lifecycle_status_assignments_are_not_in_production_submodules() {
        let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut violations = Vec::new();

        visit_rs_files(&src_dir, &mut |path| {
            if path.file_name().and_then(|name| name.to_str()) == Some("lifecycle.rs") {
                return;
            }
            let source = std::fs::read_to_string(path).unwrap();
            let production_source = source
                .split("\n#[cfg(test)]")
                .next()
                .unwrap_or(source.as_str());

            for (index, line) in production_source.lines().enumerate() {
                let trimmed = line.trim_start();
                let is_assignment = trimmed.starts_with("lifecycle_status =")
                    || line.contains(".lifecycle_status =");
                let is_equality = trimmed.starts_with("lifecycle_status ==")
                    || line.contains(".lifecycle_status ==");
                let looks_like_code =
                    !line.contains('"') && !line.contains('\'') && !line.contains("excluded.");
                if is_assignment && looks_like_code && !is_equality {
                    violations.push(format!(
                        "{}:{}:{line}",
                        path.strip_prefix(&src_dir).unwrap().display(),
                        index + 1
                    ));
                }
            }
        });

        assert!(
            violations.is_empty(),
            "lifecycle writes must go through ajax_core::lifecycle:\n{}",
            violations.join("\n")
        );
    }

    fn visit_rs_files(dir: &std::path::Path, visit: &mut impl FnMut(&std::path::Path)) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                visit_rs_files(&path, visit);
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
                visit(&path);
            }
        }
    }
}

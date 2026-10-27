use super::*;

// #1229: Repair used to discard a failed Git refresh and act on stale evidence.
#[test]
fn issue_1229_repair_stops_when_git_evidence_cannot_be_refreshed() {
    let mut context = context_with_named_checkout_mismatch();
    let before = context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .unwrap()
        .clone();
    let mut runner = QueuedRefreshRunner::new(vec![CommandOutput {
        status_code: 128,
        stdout: String::new(),
        stderr: "fatal: cannot change to '/repo/web': No such file or directory".to_string(),
    }]);

    let error = operate(
        &mut context,
        &mut runner,
        OperateRequest {
            task_handle: "web/fix-login".to_string(),
            action: "review".to_string(),
            confirmed: true,
            branch_adoption: None,
        },
    )
    .unwrap_err();

    assert!(
        matches!(
            error,
            OperateError::Command(CommandError::CommandRun(_), false)
        ),
        "expected the Git observation failure, got {error:?}"
    );
    assert_eq!(
        runner.commands.len(),
        1,
        "nothing may run after the failed observation"
    );
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap(),
        &before
    );
}

// Review and Repair act on Git evidence. A task whose repository is no longer
// configured cannot have its checkout observed, so cached evidence must not be
// used to adopt a branch.
#[test]
fn repair_adoption_is_refused_when_task_repository_is_not_configured() {
    let mut context = context_with_renamed_branch();
    context.config.repos.clear();
    let before = context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .unwrap()
        .clone();
    let mut runner = RecordingCommandRunner::default();

    let error = operate(
        &mut context,
        &mut runner,
        OperateRequest {
            task_handle: "web/fix-login".to_string(),
            action: "repair".to_string(),
            confirmed: true,
            branch_adoption: Some(ajax_core::commands::BranchAdoptionPlan {
                expected_branch: "ajax/fix-login".to_string(),
                observed_branch: "fix/web/login".to_string(),
            }),
        },
    )
    .unwrap_err();

    assert!(
        matches!(
            error,
            OperateError::Command(CommandError::PlanBlocked(_), false)
        ),
        "expected the unconfigured repository to block repair, got {error:?}"
    );
    assert!(runner.commands().is_empty());
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap(),
        &before
    );
}

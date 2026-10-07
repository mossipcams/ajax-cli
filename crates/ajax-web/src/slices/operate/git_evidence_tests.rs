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
            action: "repair".to_string(),
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

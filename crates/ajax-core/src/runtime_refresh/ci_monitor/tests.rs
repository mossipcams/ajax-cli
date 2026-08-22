use super::{load_state, pending_notification, reduce_report, refresh_ci_monitor, CiAttemptStatus};
use crate::runtime_refresh::tests::{context_with_active_task, task_fixture};
use crate::{
    adapters::{
        CiChecksReport, CiChecksState, CommandOutput, CommandRunError, CommandRunner, CommandSpec,
    },
    agent_notification::{
        record_delivery, AgentNotificationDelivery, AgentNotificationDeliveryStatus,
    },
    diff_review::{PullRequestRef, PullRequestState},
    models::Task,
    registry::Registry,
};

struct MonitorRunner {
    prs: String,
    checks: String,
    commands: Vec<CommandSpec>,
}

impl CommandRunner for MonitorRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        self.commands.push(command.clone());
        let stdout = if command.args.get(1).is_some_and(|arg| arg == "list") {
            self.prs.clone()
        } else {
            self.checks.clone()
        };
        Ok(CommandOutput {
            status_code: 0,
            stdout,
            stderr: String::new(),
        })
    }
}

fn open_pr(number: u64, head: &str) -> PullRequestRef {
    PullRequestRef {
        number,
        title: "Fix login".to_string(),
        url: format!("https://github.test/pull/{number}"),
        state: PullRequestState::Open,
        head_ref: "ajax/fix-login".to_string(),
        head_sha: Some(head.to_string()),
    }
}

fn failed_report(ids: &[&str], pending: bool) -> CiChecksReport {
    CiChecksReport {
        state: CiChecksState::Failed,
        failed_checks: ids
            .iter()
            .enumerate()
            .map(|(i, id)| crate::agent_notification::CiFailedCheck {
                name: format!("check-{i}"),
                link: Some(format!("https://github.test/{id}")),
                identity: Some((*id).to_string()),
            })
            .collect(),
        check_identities: ids.iter().map(|id| (*id).to_string()).collect(),
        has_pending: pending,
        error: None,
    }
}

fn accept(task: &mut Task) {
    let notification = pending_notification(task).expect("notification");
    record_delivery(
        task,
        AgentNotificationDelivery {
            notification_id: notification.id().to_string(),
            status: AgentNotificationDeliveryStatus::Accepted,
            detail: None,
        },
    );
}

fn snapshots<R: Registry>(registry: &R) -> Vec<Task> {
    registry.list_tasks().into_iter().cloned().collect()
}

fn task0<R: Registry>(registry: &R) -> Task {
    registry.list_tasks()[0].clone()
}

fn reset_monitor<R: Registry>(ctx: &mut crate::commands::CommandContext<R>) {
    ctx.registry
        .get_task_mut(&crate::models::TaskId::new("task-1"))
        .unwrap()
        .metadata
        .remove(crate::agent_notification::CI_MONITOR_STATE_KEY);
}

#[test]
fn failure_episode_rules_cover_dedupe_rerun_and_incremental_completion() {
    let mut task = task_fixture();
    let first = failed_report(&["run-1"], false);
    assert!(reduce_report(
        &mut task,
        &open_pr(42, "aaa"),
        first.clone(),
        100
    ));
    accept(&mut task);
    reduce_report(&mut task, &open_pr(42, "aaa"), first, 101);
    assert!(pending_notification(&task).is_none());
    assert!(reduce_report(
        &mut task,
        &open_pr(42, "bbb"),
        failed_report(&["run-1"], false),
        102,
    ));
    assert_eq!(load_state(&task).head_sha.as_deref(), Some("bbb"));
    reduce_report(
        &mut task,
        &open_pr(42, "bbb"),
        CiChecksReport {
            state: CiChecksState::Pending,
            failed_checks: vec![],
            check_identities: vec!["run-2".into()],
            has_pending: true,
            error: None,
        },
        103,
    );
    reduce_report(
        &mut task,
        &open_pr(42, "bbb"),
        failed_report(&["run-2"], false),
        104,
    );
    assert!(pending_notification(&task).is_some());
}

fn tick(
    ctx: &mut crate::commands::CommandContext<crate::registry::InMemoryRegistry>,
    runner: &mut MonitorRunner,
    now: u64,
    changed: &mut bool,
) {
    refresh_ci_monitor(ctx, runner, now, &snapshots(&ctx.registry), changed);
}

#[test]
fn refresh_polls_active_checks_and_records_pass() {
    let open = r#"[{"number":42,"title":"Fix","url":"https://github.test/pull/42","state":"OPEN","headRefName":"ajax/fix-login","headRefOid":"aaa"}]"#;
    let in_progress =
        r#"[{"name":"CI","state":"IN_PROGRESS","link":"https://github.test/actions/runs/1"}]"#;
    let success =
        r#"[{"name":"CI","state":"SUCCESS","link":"https://github.test/actions/runs/1"}]"#;
    let mut changed = false;
    let mut ctx = context_with_active_task();
    reset_monitor(&mut ctx);
    let mut runner = MonitorRunner {
        prs: open.into(),
        checks: in_progress.into(),
        commands: vec![],
    };
    tick(&mut ctx, &mut runner, 1000, &mut changed);
    tick(&mut ctx, &mut runner, 1029, &mut changed);
    assert_eq!(
        runner.commands.iter().filter(|c| c.program == "gh").count(),
        2
    );
    assert_eq!(
        load_state(&task0(&ctx.registry)).status,
        CiAttemptStatus::Pending
    );
    runner.checks = success.into();
    for now in [1000, 1300] {
        tick(&mut ctx, &mut runner, now, &mut changed);
    }
    assert_eq!(
        load_state(&task0(&ctx.registry)).status,
        CiAttemptStatus::Passed
    );
}

#[test]
fn ci_monitor_live_probe_forces_ci_failure() {
    // Temporary live probe: intentionally fails so GitHub CI reports red for ajax_ci_monitor polling.
    panic!("ci_monitor live probe: force CI failure");
}

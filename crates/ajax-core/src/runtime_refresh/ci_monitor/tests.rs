use super::{load_state, pending_notification, reduce_report, refresh_ci_monitor, CiAttemptStatus};
use crate::runtime_refresh::tests::{context_with_active_task, task_fixture, TASK_BRANCH};
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
fn terminal_failure_with_pending_siblings_starts_episode_and_notification() {
    let mut task = task_fixture();
    let lint_failed_while_matrix_runs = failed_report(&["lint-run-1"], true);
    assert!(reduce_report(
        &mut task,
        &open_pr(42, "aaa"),
        lint_failed_while_matrix_runs.clone(),
        100,
    ));
    let state = load_state(&task);
    assert!(state.has_pending);
    assert!(state.episode_id.is_some());
    assert!(pending_notification(&task).is_some());

    reduce_report(
        &mut task,
        &open_pr(42, "aaa"),
        failed_report(&["lint-run-1", "codeql-run-2"], true),
        101,
    );
    assert_eq!(load_state(&task).episode_id, state.episode_id);
    assert!(pending_notification(&task).is_some());
    accept(&mut task);
    reduce_report(
        &mut task,
        &open_pr(42, "aaa"),
        failed_report(&["lint-run-1", "codeql-run-2"], true),
        102,
    );
    assert!(pending_notification(&task).is_none());
}

#[test]
fn pending_status_suppresses_agent_notification() {
    let mut task = task_fixture();
    assert!(reduce_report(
        &mut task,
        &open_pr(42, "aaa"),
        failed_report(&["run-1"], false),
        100,
    ));
    reduce_report(
        &mut task,
        &open_pr(42, "aaa"),
        CiChecksReport {
            state: CiChecksState::Pending,
            failed_checks: vec![],
            check_identities: vec!["run-2".into()],
            has_pending: true,
            error: None,
        },
        101,
    );
    assert!(super::checks_in_flight(&load_state(&task)));
    assert!(pending_notification(&task).is_none());
}

#[test]
fn active_check_min_gap_is_ten_seconds() {
    let state = super::CiMonitorState {
        status: CiAttemptStatus::Failed,
        last_check_probe_at: Some(100),
        ..Default::default()
    };
    assert!(super::checks_due(&state, 110));
    assert!(!super::checks_due(&state, 109));
}

#[test]
fn rerun_in_progress_suppresses_agent_notification_until_settled() {
    let mut task = task_fixture();
    assert!(reduce_report(
        &mut task,
        &open_pr(42, "aaa"),
        failed_report(&["run-1"], false),
        100,
    ));
    reduce_report(
        &mut task,
        &open_pr(42, "aaa"),
        CiChecksReport {
            state: CiChecksState::Pending,
            failed_checks: vec![],
            check_identities: vec!["run-2".into()],
            has_pending: true,
            error: None,
        },
        101,
    );
    let state = load_state(&task);
    assert!(state.saw_pending_after_failure);
    assert!(super::rerun_in_progress(&state));

    reduce_report(
        &mut task,
        &open_pr(42, "aaa"),
        failed_report(&["run-1"], true),
        102,
    );
    assert!(super::rerun_in_progress(&load_state(&task)));
    assert!(pending_notification(&task).is_none());

    reduce_report(
        &mut task,
        &open_pr(42, "aaa"),
        failed_report(&["run-2"], false),
        103,
    );
    assert!(!super::rerun_in_progress(&load_state(&task)));
    assert!(pending_notification(&task).is_some());
}

#[test]
fn busy_agent_does_not_suppress_pending_notification() {
    let mut task = task_fixture();
    task.agent_status = crate::models::AgentRuntimeStatus::Running;
    task.add_side_flag(crate::models::SideFlag::AgentRunning);
    assert!(reduce_report(
        &mut task,
        &open_pr(42, "aaa"),
        failed_report(&["run-1"], false),
        100,
    ));
    assert!(pending_notification(&task).is_some());
}

#[test]
fn distinct_rerun_identities_while_pending_still_suppresses_notification() {
    let mut task = task_fixture();
    assert!(reduce_report(
        &mut task,
        &open_pr(42, "aaa"),
        failed_report(&["run-1"], false),
        100,
    ));
    reduce_report(
        &mut task,
        &open_pr(42, "aaa"),
        CiChecksReport {
            state: CiChecksState::Pending,
            failed_checks: vec![],
            check_identities: vec!["run-2".into()],
            has_pending: true,
            error: None,
        },
        101,
    );
    let episode_before = load_state(&task).episode_id.clone();
    reduce_report(
        &mut task,
        &open_pr(42, "aaa"),
        failed_report(&["run-1", "run-2"], true),
        102,
    );
    let state = load_state(&task);
    assert!(state.saw_pending_after_failure);
    assert!(super::rerun_in_progress(&state));
    assert_eq!(state.episode_id, episode_before);
    assert!(pending_notification(&task).is_none());

    reduce_report(
        &mut task,
        &open_pr(42, "aaa"),
        failed_report(&["run-2"], false),
        103,
    );
    assert!(!super::rerun_in_progress(&load_state(&task)));
    assert!(pending_notification(&task).is_some());
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

fn gh_command_count(runner: &MonitorRunner) -> usize {
    runner.commands.iter().filter(|c| c.program == "gh").count()
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
    let after_first = gh_command_count(&runner);
    tick(&mut ctx, &mut runner, 1009, &mut changed);
    assert_eq!(
        gh_command_count(&runner),
        after_first,
        "9s after probe must not re-probe"
    );
    tick(&mut ctx, &mut runner, 1010, &mut changed);
    assert_eq!(
        gh_command_count(&runner),
        after_first + 1,
        "10s active interval should probe again at 10s"
    );
    assert_eq!(
        load_state(&task0(&ctx.registry)).status,
        CiAttemptStatus::Pending
    );
    runner.checks = success.into();
    tick(&mut ctx, &mut runner, 1020, &mut changed);
    assert_eq!(
        load_state(&task0(&ctx.registry)).status,
        CiAttemptStatus::Passed
    );
}

#[test]
fn red_ci_demotes_mergeable_task_so_ship_is_not_offered() {
    let open = r#"[{"number":42,"title":"Fix","url":"https://github.test/pull/42","state":"OPEN","headRefName":"ajax/fix-login","headRefOid":"aaa"}]"#;
    let success =
        r#"[{"name":"CI","state":"SUCCESS","link":"https://github.test/actions/runs/1"}]"#;
    let failure =
        r#"[{"name":"CI","state":"FAILURE","link":"https://github.test/actions/runs/1"}]"#;
    let mut changed = false;
    let mut ctx = context_with_active_task();
    reset_monitor(&mut ctx);
    ctx.registry
        .get_task_mut(&crate::models::TaskId::new("task-1"))
        .unwrap()
        .lifecycle_status = crate::models::LifecycleStatus::Reviewable;
    let mut runner = MonitorRunner {
        prs: open.into(),
        checks: success.into(),
        commands: vec![],
    };
    tick(&mut ctx, &mut runner, 1000, &mut changed);
    assert_eq!(
        task0(&ctx.registry).lifecycle_status,
        crate::models::LifecycleStatus::Mergeable
    );

    runner.checks = failure.into();
    tick(&mut ctx, &mut runner, 1300, &mut changed);
    let task = task0(&ctx.registry);
    assert_eq!(
        load_state(&task).status,
        CiAttemptStatus::Failed,
        "red CI must be observed"
    );
    assert_eq!(
        task.lifecycle_status,
        crate::models::LifecycleStatus::Reviewable
    );
    assert!(
        !crate::recommended::available_operator_actions(&task)
            .contains(&crate::models::OperatorAction::Ship),
        "red CI must not offer Ship"
    );
}

fn pr_list_head(runner: &MonitorRunner) -> Option<String> {
    runner
        .commands
        .iter()
        .filter(|c| c.program == "gh" && c.args.get(1).is_some_and(|arg| arg == "list"))
        .find_map(|c| {
            let head = c.args.iter().position(|arg| arg == "--head")?;
            c.args.get(head + 1).cloned()
        })
}

#[test]
fn pr_discovery_uses_observed_branch_when_worktree_is_switched() {
    let open = r#"[{"number":42,"title":"Fix","url":"https://github.test/pull/42","state":"OPEN","headRefName":"fix/web/login","headRefOid":"aaa"}]"#;
    let success =
        r#"[{"name":"CI","state":"SUCCESS","link":"https://github.test/actions/runs/1"}]"#;
    let mut changed = false;
    let mut ctx = context_with_active_task();
    reset_monitor(&mut ctx);
    ctx.registry
        .get_task_mut(&crate::models::TaskId::new("task-1"))
        .unwrap()
        .git_status
        .as_mut()
        .unwrap()
        .current_branch = Some("fix/web/login".into());
    let mut runner = MonitorRunner {
        prs: open.into(),
        checks: success.into(),
        commands: vec![],
    };
    tick(&mut ctx, &mut runner, 1000, &mut changed);
    assert_eq!(pr_list_head(&runner).as_deref(), Some("fix/web/login"));
}

#[test]
fn pr_discovery_uses_task_branch_when_worktree_is_on_it_or_unknown() {
    let success =
        r#"[{"name":"CI","state":"SUCCESS","link":"https://github.test/actions/runs/1"}]"#;
    for current_branch in [Some(TASK_BRANCH.to_string()), None] {
        let mut changed = false;
        let mut ctx = context_with_active_task();
        reset_monitor(&mut ctx);
        ctx.registry
            .get_task_mut(&crate::models::TaskId::new("task-1"))
            .unwrap()
            .git_status
            .as_mut()
            .unwrap()
            .current_branch = current_branch;
        let mut runner = MonitorRunner {
            prs: "[]".into(),
            checks: success.into(),
            commands: vec![],
        };
        tick(&mut ctx, &mut runner, 1000, &mut changed);
        assert_eq!(pr_list_head(&runner).as_deref(), Some("ajax/fix-login"));
    }
}

#[test]
fn pr_discovery_never_queries_the_base_branch_when_worktree_is_on_it() {
    let success =
        r#"[{"name":"CI","state":"SUCCESS","link":"https://github.test/actions/runs/1"}]"#;
    let mut changed = false;
    let mut ctx = context_with_active_task();
    reset_monitor(&mut ctx);
    ctx.registry
        .get_task_mut(&crate::models::TaskId::new("task-1"))
        .unwrap()
        .git_status
        .as_mut()
        .unwrap()
        .current_branch = Some("main".into());
    let mut runner = MonitorRunner {
        prs: "[]".into(),
        checks: success.into(),
        commands: vec![],
    };
    tick(&mut ctx, &mut runner, 1000, &mut changed);
    assert_eq!(pr_list_head(&runner).as_deref(), Some("ajax/fix-login"));
}

#[test]
fn retiring_a_task_without_ci_state_leaves_it_unchanged() {
    let mut changed = false;
    let mut ctx = context_with_active_task();
    reset_monitor(&mut ctx);
    ctx.registry
        .get_task_mut(&crate::models::TaskId::new("task-1"))
        .unwrap()
        .lifecycle_status = crate::models::LifecycleStatus::Cleanable;
    let before = task0(&ctx.registry);
    let mut runner = MonitorRunner {
        prs: "[]".into(),
        checks: "[]".into(),
        commands: vec![],
    };
    tick(&mut ctx, &mut runner, 1000, &mut changed);
    assert!(
        !changed,
        "retiring a task with no CI state must not report a change"
    );
    assert_eq!(task0(&ctx.registry), before);
    assert!(!task0(&ctx.registry)
        .metadata
        .contains_key(crate::agent_notification::CI_MONITOR_STATE_KEY));
}

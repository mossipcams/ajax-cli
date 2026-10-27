use super::test_support::{fake_acp_fixture, scratch_dir, BlockingSessionDirectory};
use super::{record_session_activity, SessionActivity};
use crate::adapters::web_session_acp::{with_test_acp_extra_args, with_test_acp_program};
use ajax_core::commands::CommandContext;
use ajax_core::models::AgentClient;
use ajax_core::registry::{InMemoryRegistry, Registry};
use ajax_core::ui_state::{derive_operator_status, TaskStatus};
use std::{
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime},
};

type SharedContext = Arc<Mutex<CommandContext<InMemoryRegistry>>>;

const DEADLINE: Duration = Duration::from_secs(15);
const AFTER_TURN_DEADLINE: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(50);

fn provisioned_handle_context() -> (String, SharedContext) {
    let mut task = crate::test_support::fix_login_task();
    task.set_skip_interactive_agent(true);
    let handle = task.qualified_handle();
    let context = Arc::new(Mutex::new(crate::test_support::context_with_tasks(
        &["web"],
        vec![task],
    )));
    (handle, context)
}

fn record_reports(
    directory: &BlockingSessionDirectory,
    context: &SharedContext,
) -> Arc<Mutex<Vec<SessionActivity>>> {
    let reported = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&reported);
    let ctx = Arc::clone(context);
    directory
        .inner()
        .set_report_session_activity(Arc::new(move |qualified_handle, activity| {
            let recorded = record_session_activity(
                &mut ctx.lock().expect("context lock"),
                qualified_handle,
                activity,
                SystemTime::now(),
            )
            .is_ok();
            sink.lock().expect("reported lock").push(activity);
            recorded
        }));
    reported
}

fn wait_for(
    reported: &Mutex<Vec<SessionActivity>>,
    found: impl Fn(&[SessionActivity]) -> bool,
) -> Vec<SessionActivity> {
    wait_for_within(reported, DEADLINE, found)
}

fn wait_for_within(
    reported: &Mutex<Vec<SessionActivity>>,
    within: Duration,
    found: impl Fn(&[SessionActivity]) -> bool,
) -> Vec<SessionActivity> {
    let deadline = Instant::now() + within;
    loop {
        let snapshot = reported.lock().expect("reported lock").clone();
        if found(&snapshot) {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting on background-work activity; reported: {snapshot:?}"
        );
        thread::sleep(POLL);
    }
}

fn turn_ended_after_background(activity: &[SessionActivity]) -> bool {
    activity
        .iter()
        .position(|reported| *reported == SessionActivity::BackgroundRunning)
        .is_some_and(|background| activity[background..].contains(&SessionActivity::TurnEnded))
}

fn task_status(context: &SharedContext, handle: &str) -> TaskStatus {
    let context = context.lock().expect("context lock");
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == handle)
        .expect("task");
    derive_operator_status(task).status
}

#[test]
fn background_shell_reports_background_running_then_turn_ended() {
    let (handle, context) = provisioned_handle_context();
    let dir = scratch_dir("background-live-held");
    let directory = BlockingSessionDirectory::new(dir.clone());
    let reported = record_reports(&directory, &context);

    with_test_acp_program(&fake_acp_fixture(), || {
        with_test_acp_extra_args(&["--background-shell"], || {
            directory
                .acquire(&handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt(&handle, "start background work".to_string())
                .expect("submit prompt");
        });

        let seen = wait_for(&reported, |activity| {
            activity.contains(&SessionActivity::BackgroundRunning)
        });
        let background = seen
            .iter()
            .position(|activity| *activity == SessionActivity::BackgroundRunning)
            .expect("background position");
        assert!(
            !seen[..background].contains(&SessionActivity::TurnEnded),
            "TurnEnded must not be reported before BackgroundRunning: {seen:?}"
        );
        assert_eq!(
            task_status(&context, &handle),
            TaskStatus::Running,
            "background work must read as the task still running"
        );

        let seen = wait_for(&reported, turn_ended_after_background);
        assert!(
            turn_ended_after_background(&seen),
            "TurnEnded must follow BackgroundRunning: {seen:?}"
        );
        assert_ne!(
            task_status(&context, &handle),
            TaskStatus::Running,
            "turn end after background work must retract the running status"
        );
    });

    drop(directory);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn turn_end_without_background_shell_never_reports_background_running() {
    let (handle, context) = provisioned_handle_context();
    let dir = scratch_dir("background-live-plain");
    let directory = BlockingSessionDirectory::new(dir.clone());
    let reported = record_reports(&directory, &context);

    with_test_acp_program(&fake_acp_fixture(), || {
        with_test_acp_extra_args(&[], || {
            directory
                .acquire(&handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt(&handle, "plain turn".to_string())
                .expect("submit prompt");
        });

        let seen = wait_for(&reported, |activity| {
            activity.contains(&SessionActivity::TurnEnded)
        });
        assert!(
            !seen.contains(&SessionActivity::BackgroundRunning),
            "a turn with no background shell must never report BackgroundRunning: {seen:?}"
        );
        assert_ne!(
            task_status(&context, &handle),
            TaskStatus::Running,
            "plain turn end must clear the running status"
        );
    });

    drop(directory);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn background_shell_started_after_the_turn_ended_reports_background_running() {
    let (handle, context) = provisioned_handle_context();
    let dir = scratch_dir("background-live-after-turn");
    let directory = BlockingSessionDirectory::new(dir.clone());
    let reported = record_reports(&directory, &context);

    with_test_acp_program(&fake_acp_fixture(), || {
        with_test_acp_extra_args(&["--background-shell-after-turn"], || {
            directory
                .acquire(&handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt(&handle, "start background work after the turn".to_string())
                .expect("submit prompt");
        });

        let seen = wait_for_within(&reported, AFTER_TURN_DEADLINE, |activity| {
            activity.contains(&SessionActivity::BackgroundRunning)
        });
        assert!(
            seen.contains(&SessionActivity::TurnEnded),
            "the turn ends before the shell starts: {seen:?}"
        );
        assert_eq!(
            task_status(&context, &handle),
            TaskStatus::Running,
            "background work started after the turn must read as the task still running"
        );

        let seen = wait_for_within(&reported, AFTER_TURN_DEADLINE, turn_ended_after_background);
        assert!(
            turn_ended_after_background(&seen),
            "TurnEnded must follow BackgroundRunning: {seen:?}"
        );
    });

    drop(directory);
    let _ = std::fs::remove_dir_all(dir);
}

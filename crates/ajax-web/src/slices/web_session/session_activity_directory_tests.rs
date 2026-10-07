//! #1069: task evidence must flow through host transcript append, not WS flush.

use super::test_support::{scratch_dir, BlockingSessionDirectory};
use super::{record_session_activity, SessionActivity, SessionServerEvent};
use ajax_core::registry::Registry;
use ajax_core::ui_state::{derive_operator_status, TaskStatus};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, SystemTime},
};

fn provisioned_handle_context() -> (
    String,
    Arc<Mutex<ajax_core::commands::CommandContext<ajax_core::registry::InMemoryRegistry>>>,
) {
    let mut task = crate::test_support::fix_login_task();
    task.set_skip_interactive_agent(true);
    let handle = task.qualified_handle();
    let context = Arc::new(Mutex::new(crate::test_support::context_with_tasks(
        &["web"],
        vec![task],
    )));
    (handle, context)
}

fn wire_report(
    directory: &BlockingSessionDirectory,
    context: &Arc<
        Mutex<ajax_core::commands::CommandContext<ajax_core::registry::InMemoryRegistry>>,
    >,
) {
    let ctx = Arc::clone(context);
    directory
        .inner()
        .set_report_session_activity(Arc::new(move |qualified_handle, activity| {
            record_session_activity(
                &mut ctx.lock().expect("context lock"),
                qualified_handle,
                activity,
                SystemTime::now(),
            )
            .is_ok()
        }));
}

fn task_status(
    context: &Mutex<ajax_core::commands::CommandContext<ajax_core::registry::InMemoryRegistry>>,
    handle: &str,
) -> TaskStatus {
    let context = context.lock().expect("context lock");
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == handle)
        .expect("task");
    derive_operator_status(task).status
}

/// Drives PromptAccepted then TurnEnd through TaskSessionDirectory append (no WS).
/// Would pass only when evidence is reported from append_to_log, not WS flush.
#[test]
fn issue_1069_append_path_clears_agent_working_without_websocket() {
    let (handle, context) = provisioned_handle_context();
    let directory = BlockingSessionDirectory::new(scratch_dir("issue-1069-append"));
    wire_report(&directory, &context);

    directory.record(
        &handle,
        SessionServerEvent::PromptAccepted {
            client_message_id: "c1".to_string(),
        },
    );
    assert_eq!(
        task_status(&context, &handle),
        TaskStatus::Running,
        "prompt_accepted must report Agent working"
    );

    directory.record(
        &handle,
        SessionServerEvent::TurnEnd {
            stop_reason: Some("end_turn".to_string()),
        },
    );

    let context = context.lock().expect("context lock");
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == handle)
        .expect("task");
    let status = derive_operator_status(task);
    assert_ne!(
        status.status,
        TaskStatus::Running,
        "turn_end via append must retract Agent working"
    );
    assert_eq!(status.explanation.as_deref(), Some("Response ready"));
}

/// A failed persist must not commit reporter state so turn_end can retry (#1069).
#[test]
fn issue_1069_failed_report_retries_turn_end_on_next_append() {
    let (handle, context) = provisioned_handle_context();
    let directory = BlockingSessionDirectory::new(scratch_dir("issue-1069-retry"));
    let allow_turn_end = Arc::new(AtomicBool::new(false));
    let ctx = Arc::clone(&context);
    let allow = Arc::clone(&allow_turn_end);
    directory
        .inner()
        .set_report_session_activity(Arc::new(move |qualified_handle, activity| {
            if activity == SessionActivity::TurnEnded && !allow.load(Ordering::SeqCst) {
                return false;
            }
            record_session_activity(
                &mut ctx.lock().expect("context lock"),
                qualified_handle,
                activity,
                SystemTime::now(),
            )
            .is_ok()
        }));

    directory.record(
        &handle,
        SessionServerEvent::PromptAccepted {
            client_message_id: "c1".to_string(),
        },
    );
    directory.record(
        &handle,
        SessionServerEvent::TurnEnd {
            stop_reason: Some("end_turn".to_string()),
        },
    );
    assert_eq!(
        task_status(&context, &handle),
        TaskStatus::Running,
        "first failed turn_end report must not commit reporter state"
    );

    allow_turn_end.store(true, Ordering::SeqCst);
    directory.record(
        &handle,
        SessionServerEvent::Message {
            role: "agent".to_string(),
            text: "follow-up".to_string(),
            content_blocks: Vec::new(),
            item_id: "m1".to_string(),
            message_id: None,
        },
    );

    assert_ne!(
        task_status(&context, &handle),
        TaskStatus::Running,
        "retried turn_end must clear Agent working"
    );
}

/// #1132: when turn_end reporting is deferred (control lane busy / try_lock miss),
/// the session poll tick must apply it without another transcript append.
#[test]
fn issue_1132_deferred_turn_end_retries_on_session_poll_without_later_append() {
    let (handle, context) = provisioned_handle_context();
    let directory = BlockingSessionDirectory::new(scratch_dir("issue-1132-poll-retry"));
    let allow_turn_end = Arc::new(AtomicBool::new(false));
    let ctx = Arc::clone(&context);
    let allow = Arc::clone(&allow_turn_end);
    directory
        .inner()
        .set_report_session_activity(Arc::new(move |qualified_handle, activity| {
            if activity == SessionActivity::TurnEnded && !allow.load(Ordering::SeqCst) {
                return false;
            }
            record_session_activity(
                &mut ctx.lock().expect("context lock"),
                qualified_handle,
                activity,
                SystemTime::now(),
            )
            .is_ok()
        }));

    directory.record(
        &handle,
        SessionServerEvent::PromptAccepted {
            client_message_id: "c1".to_string(),
        },
    );
    directory.record(
        &handle,
        SessionServerEvent::TurnEnd {
            stop_reason: Some("end_turn".to_string()),
        },
    );
    assert_eq!(
        task_status(&context, &handle),
        TaskStatus::Running,
        "deferred turn_end must not commit reporter state yet"
    );

    allow_turn_end.store(true, Ordering::SeqCst);
    thread::sleep(Duration::from_millis(150));

    assert_ne!(
        task_status(&context, &handle),
        TaskStatus::Running,
        "poll tick must apply deferred turn_end without a later append (#1132)"
    );
}

// #1176: a turn cut off by a host restart left the task on "Agent working".
#[test]
fn issue_1176_turn_interrupted_by_restart_clears_agent_working() {
    use crate::adapters::web_session_store::prompt_ledger::{self, PromptLedger};

    let (handle, context) = provisioned_handle_context();
    record_session_activity(
        &mut context.lock().expect("context lock"),
        &handle,
        SessionActivity::TurnStarted,
        SystemTime::now(),
    )
    .expect("turn started");
    assert_eq!(task_status(&context, &handle), TaskStatus::Running);

    let dir = scratch_dir("issue-1176-restart");
    let mut ledger = PromptLedger::default();
    ledger.upsert_queued(
        "orphan".into(),
        "orphan".into(),
        "orphan".into(),
        Vec::new(),
    );
    assert!(ledger.mark_dispatching("orphan"));
    prompt_ledger::persist(&dir, &handle, &ledger).expect("seed ledger");

    let directory = BlockingSessionDirectory::new(dir.clone());
    wire_report(&directory, &context);
    crate::adapters::web_session_acp::with_test_acp_program(
        &super::test_support::fake_acp_fixture(),
        || {
            directory
                .acquire(
                    &handle,
                    &dir,
                    "auto",
                    ajax_core::models::AgentClient::Cursor,
                )
                .expect("acquire");
        },
    );

    assert_ne!(
        task_status(&context, &handle),
        TaskStatus::Running,
        "an interrupted turn must not leave the task reported as working"
    );
    let _ = std::fs::remove_dir_all(dir);
}

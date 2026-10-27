use super::test_support::{scratch_dir, BlockingSessionDirectory};
use super::{record_session_activity, SessionActivity, SessionServerEvent};
use crate::adapters::web_session_acp::with_test_acp_program;
use ajax_core::registry::Registry;
use ajax_core::ui_state::{derive_operator_status, TaskStatus};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant, SystemTime},
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

fn record_reports(
    directory: &BlockingSessionDirectory,
    context: &Arc<
        Mutex<ajax_core::commands::CommandContext<ajax_core::registry::InMemoryRegistry>>,
    >,
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

pub(super) fn record_activities(
    directory: &BlockingSessionDirectory,
) -> Arc<Mutex<Vec<SessionActivity>>> {
    let reported = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&reported);
    directory
        .inner()
        .set_report_session_activity(Arc::new(move |_qualified_handle, activity| {
            sink.lock().expect("reported lock").push(activity);
            true
        }));
    reported
}

pub(super) fn wait_for_last_activity(
    reported: &Mutex<Vec<SessionActivity>>,
    activity: SessionActivity,
) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let last = reported.lock().expect("reported lock").last().copied();
        if last == Some(activity) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {activity:?}; last reported: {last:?}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn issue_1439_actor_ending_mid_turn_clears_agent_working() {
    let (handle, context) = provisioned_handle_context();
    let dir = scratch_dir("issue-1439-end-running");
    let directory = BlockingSessionDirectory::new(dir.clone());
    let reported = record_reports(&directory, &context);

    with_test_acp_program(&super::test_support::fake_acp_fixture(), || {
        directory
            .acquire(
                &handle,
                &dir,
                "auto",
                ajax_core::models::AgentClient::Cursor,
            )
            .expect("acquire");
    });
    directory.record(
        &handle,
        SessionServerEvent::PromptAccepted {
            client_message_id: "c1".to_string(),
        },
    );
    wait_for_last_activity(&reported, SessionActivity::TurnStarted);
    assert_eq!(task_status(&context, &handle), TaskStatus::Running);

    directory.detach_session(&handle);

    assert_ne!(
        task_status(&context, &handle),
        TaskStatus::Running,
        "an actor that ends mid-turn must settle its last running report"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn issue_1439_actor_ending_settled_reports_nothing_extra() {
    let (handle, _) = provisioned_handle_context();
    let dir = scratch_dir("issue-1439-end-settled");
    let directory = BlockingSessionDirectory::new(dir.clone());
    let reported = record_activities(&directory);

    with_test_acp_program(&super::test_support::fake_acp_fixture(), || {
        directory
            .acquire(
                &handle,
                &dir,
                "auto",
                ajax_core::models::AgentClient::Cursor,
            )
            .expect("acquire");
        directory
            .submit_prompt(&handle, "finishes".to_string())
            .expect("submit prompt");
    });
    wait_for_last_activity(&reported, SessionActivity::TurnEnded);
    let before = reported.lock().expect("reported lock").clone();

    directory.detach_session(&handle);

    assert_eq!(
        *reported.lock().expect("reported lock"),
        before,
        "a settled actor must not report anything when it ends"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn issue_1439_first_acquire_settles_a_running_card_with_nothing_running() {
    use ajax_core::registry::SqliteRegistryStore;

    let (handle, context) = provisioned_handle_context();
    let dir = scratch_dir("issue-1439-first-attach-running");
    let directory = BlockingSessionDirectory::new(dir.clone());
    let reported = record_reports(&directory, &context);
    let fixture = super::test_support::fake_acp_fixture();

    with_test_acp_program(&fixture, || {
        directory
            .acquire(
                &handle,
                &dir,
                "auto",
                ajax_core::models::AgentClient::Cursor,
            )
            .expect("first acquire");
    });
    directory.detach_session(&handle);

    record_session_activity(
        &mut context.lock().expect("context lock"),
        &handle,
        SessionActivity::TurnStarted,
        SystemTime::now(),
    )
    .expect("turn started");
    SqliteRegistryStore::new(dir.join("ajax.db"))
        .save(&context.lock().expect("context lock").registry)
        .expect("seed registry");

    with_test_acp_program(&fixture, || {
        directory
            .acquire(
                &handle,
                &dir,
                "auto",
                ajax_core::models::AgentClient::Cursor,
            )
            .expect("second acquire");
    });
    directory.detach_session(&handle);

    assert!(
        reported
            .lock()
            .expect("reported lock")
            .contains(&SessionActivity::TurnFailed),
        "a fresh attach must settle a card that still reads running"
    );
    assert_ne!(
        task_status(&context, &handle),
        TaskStatus::Running,
        "opening the chat must clear a stale Agent working card"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn issue_1439_first_acquire_of_an_idle_card_reports_nothing() {
    let (handle, _) = provisioned_handle_context();
    let dir = scratch_dir("issue-1439-first-attach-idle");
    let directory = BlockingSessionDirectory::new(dir.clone());
    let reported = record_activities(&directory);

    with_test_acp_program(&super::test_support::fake_acp_fixture(), || {
        directory
            .acquire(
                &handle,
                &dir,
                "auto",
                ajax_core::models::AgentClient::Cursor,
            )
            .expect("acquire");
    });
    directory.detach_session(&handle);

    assert_eq!(
        *reported.lock().expect("reported lock"),
        Vec::<SessionActivity>::new(),
        "an idle chat's first attach must report nothing"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn issue_1439_running_chat_without_dispatching_prompt_settles_on_startup() {
    use ajax_core::registry::SqliteRegistryStore;

    let (handle, context) = provisioned_handle_context();
    record_session_activity(
        &mut context.lock().expect("context lock"),
        &handle,
        SessionActivity::TurnStarted,
        SystemTime::now(),
    )
    .expect("turn started");
    assert_eq!(task_status(&context, &handle), TaskStatus::Running);

    let dir = scratch_dir("issue-1439-startup-running");
    SqliteRegistryStore::new(dir.join("ajax.db"))
        .save(&context.lock().expect("context lock").registry)
        .expect("seed registry");

    let directory = BlockingSessionDirectory::new(dir.clone());
    let reported = record_reports(&directory, &context);
    directory
        .inner()
        .requeue_recovered_handles_for_test(vec![handle.clone()]);
    directory
        .inner()
        .startup_dispatch_handles()
        .expect("startup dispatch");

    assert!(
        reported
            .lock()
            .expect("reported lock")
            .contains(&SessionActivity::TurnFailed),
        "startup must settle a running chat that has no dispatching prompt before any actor attaches"
    );
    assert_ne!(
        task_status(&context, &handle),
        TaskStatus::Running,
        "a chat stuck running after a host restart must clear its status"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn issue_1439_recovered_handle_with_a_live_actor_is_not_settled() {
    let (handle, _) = provisioned_handle_context();
    let dir = scratch_dir("issue-1439-recovered-live-actor");
    let directory = BlockingSessionDirectory::new(dir.clone());
    let reported = record_activities(&directory);

    with_test_acp_program(&super::test_support::fake_acp_fixture(), || {
        directory
            .acquire(
                &handle,
                &dir,
                "auto",
                ajax_core::models::AgentClient::Cursor,
            )
            .expect("acquire");
    });
    let store = ajax_core::registry::SqliteRegistryStore::new(dir.join("ajax.db"));
    let run_id = store
        .latest_run_for_task(&handle)
        .expect("read run")
        .expect("acquire records a run")
        .run_id;
    store
        .commit_execution(&ajax_core::registry::ExecutionCommit {
            run_id,
            run_update: Some(ajax_core::registry::RunUpdate {
                owner_boot_id: Some(Some("foreign-boot".to_owned())),
                ..Default::default()
            }),
            ..Default::default()
        })
        .expect("record foreign owner boot");
    directory
        .inner()
        .requeue_recovered_handles_for_test(vec![handle.clone()]);
    directory.inner().heartbeat_execution_ownership();

    assert!(
        !reported
            .lock()
            .expect("reported lock")
            .contains(&SessionActivity::TurnFailed),
        "a recovered chat that gained a live actor before the report must not be settled as stopped"
    );
    directory.detach_session(&handle);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn issue_1475_first_attach_waits_for_a_deferred_report() {
    use ajax_core::registry::SqliteRegistryStore;

    let (handle, context) = provisioned_handle_context();
    let dir = scratch_dir("issue-1475-attach-deferred");
    let directory = BlockingSessionDirectory::new(dir.clone());
    let reported = record_reports(&directory, &context);
    let pending_handle = handle.clone();
    directory
        .inner()
        .set_deferred_activity_pending(Arc::new(move |candidate: &str| {
            candidate == pending_handle
        }));
    let fixture = super::test_support::fake_acp_fixture();

    with_test_acp_program(&fixture, || {
        directory
            .acquire(
                &handle,
                &dir,
                "auto",
                ajax_core::models::AgentClient::Cursor,
            )
            .expect("first acquire");
    });
    directory.detach_session(&handle);

    record_session_activity(
        &mut context.lock().expect("context lock"),
        &handle,
        SessionActivity::TurnStarted,
        SystemTime::now(),
    )
    .expect("turn started");
    SqliteRegistryStore::new(dir.join("ajax.db"))
        .save(&context.lock().expect("context lock").registry)
        .expect("seed registry");

    with_test_acp_program(&fixture, || {
        directory
            .acquire(
                &handle,
                &dir,
                "auto",
                ajax_core::models::AgentClient::Cursor,
            )
            .expect("second acquire");
    });
    directory.detach_session(&handle);

    assert!(
        !reported
            .lock()
            .expect("reported lock")
            .contains(&SessionActivity::TurnFailed),
        "a first attach must not settle while a deferred report for the chat is still queued"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn issue_1439_dropping_a_chat_mid_turn_reports_no_settle() {
    let (handle, _) = provisioned_handle_context();
    let dir = scratch_dir("issue-1439-drop-running");
    let directory = BlockingSessionDirectory::new(dir.clone());
    let reported = record_activities(&directory);

    with_test_acp_program(&super::test_support::fake_acp_fixture(), || {
        directory
            .acquire(
                &handle,
                &dir,
                "auto",
                ajax_core::models::AgentClient::Cursor,
            )
            .expect("acquire");
    });
    directory.record(
        &handle,
        SessionServerEvent::PromptAccepted {
            client_message_id: "c1".to_string(),
        },
    );
    wait_for_last_activity(&reported, SessionActivity::TurnStarted);

    directory.drop_session(&handle);

    assert!(
        !reported
            .lock()
            .expect("reported lock")
            .contains(&SessionActivity::TurnFailed),
        "a chat dropped mid-turn is removed, so its actor must not settle a status"
    );
    let _ = std::fs::remove_dir_all(dir);
}

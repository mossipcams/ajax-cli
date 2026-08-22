use super::super::{
    test_support::{fake_acp_fixture, pump_until, scratch_dir, BlockingSessionDirectory},
    SessionServerEvent,
};
use crate::adapters::web_session_acp::{with_test_acp_extra_args, with_test_acp_program};
use ajax_core::{
    agent_notification::{AgentNotification, AgentNotificationDeliveryStatus, CiFailedCheck},
    models::{AgentClient, Task, TaskId},
};
use std::time::Duration;

#[test]
fn persisted_acp_resume_queues_once_behind_busy_turn() {
    let dir = scratch_dir("ci-delivery");
    let handle = "web/ci-delivery";
    let task = Task::new(
        TaskId::new("task-1"),
        "web",
        "ci-delivery",
        "CI",
        "ajax/ci",
        "main",
        &dir,
        "web-ci",
        "task",
        AgentClient::Cursor,
    );
    let notify = |pr| AgentNotification::CiFailed {
        episode_id: format!("ci-failed:task-1:{pr}:abc:1"),
        task_id: task.id.clone(),
        pr_number: pr,
        head_sha: "abc".to_string(),
        failed_checks: vec![CiFailedCheck {
            name: "CI".to_string(),
            link: Some("https://github.test/actions/runs/1".to_string()),
            identity: Some("run:1".to_string()),
        }],
    };
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            let first = BlockingSessionDirectory::new(dir.clone());
            first
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .unwrap();
            first.release(handle);
            first.drop_session(handle);

            let resumed = BlockingSessionDirectory::new(dir.clone());
            let deliver = |notification| {
                resumed.runtime_handle().block_on(super::deliver(
                    resumed.inner(),
                    &task,
                    notification,
                ))
            };
            let first_notification = notify(41);
            let queued_notification = notify(42);
            assert_eq!(
                deliver(&first_notification).unwrap(),
                AgentNotificationDeliveryStatus::Accepted
            );
            assert_eq!(
                deliver(&queued_notification).unwrap(),
                AgentNotificationDeliveryStatus::Queued
            );
            resumed.cancel(handle, true).unwrap();
            pump_until(&resumed, handle, Duration::from_secs(5), |events| {
                events.iter().any(|event| {
                    matches!(event, SessionServerEvent::Message { role, text, .. }
                    if role == "user" && text.contains("PR #42"))
                })
            });
        })
    });
    let _ = std::fs::remove_dir_all(dir);
}

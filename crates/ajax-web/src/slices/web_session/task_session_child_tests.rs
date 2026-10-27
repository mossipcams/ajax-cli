use super::test_support::{fake_acp_fixture, scratch_dir, BlockingSessionDirectory};
use super::SessionStorage;
use crate::adapters::web_session_acp::with_test_acp_program;

use ajax_core::models::AgentClient;

#[test]
fn acquire_records_the_child_identity() {
    let dir = scratch_dir("child-identity-acquire");
    let script = fake_acp_fixture();
    let handle = "web/child-identity";
    let directory = BlockingSessionDirectory::new(dir.clone());

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");

        let pid = directory.child_id(handle).expect("child id");
        let run = SessionStorage::under(dir.clone())
            .registry_store()
            .latest_run_for_task(handle)
            .unwrap()
            .unwrap();

        assert_eq!(run.child, Some((pid as i64, pid as i64)));
        let expected_token = super::process_identity::start_token(pid as i64);
        assert!(expected_token.is_some());
        assert_eq!(run.child_started, expected_token);
        assert_eq!(
            run.owner_boot_id,
            Some(directory.inner().ownership_boot_id())
        );
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn respawn_updates_the_recorded_pid() {
    let dir = scratch_dir("child-identity-respawn");
    let script = fake_acp_fixture();
    let handle = "web/child-identity-respawn";
    let directory = BlockingSessionDirectory::new(dir.clone());

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("first acquire");
        let pid1 = directory.child_id(handle).expect("first child id");

        directory.kill_host_for_test(handle);

        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("second acquire respawns the child");
        let pid2 = directory.child_id(handle).expect("second child id");
        assert_ne!(pid1, pid2);

        let run = SessionStorage::under(dir.clone())
            .registry_store()
            .latest_run_for_task(handle)
            .unwrap()
            .unwrap();

        assert_eq!(run.child, Some((pid2 as i64, pid2 as i64)));
        let expected_token = super::process_identity::start_token(pid2 as i64);
        assert!(expected_token.is_some());
        assert_eq!(run.child_started, expected_token);
    });

    let _ = std::fs::remove_dir_all(dir);
}

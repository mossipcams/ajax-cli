//! Tests for [`super::session_client::SessionClient::child_pid`]: one per transport,
//! each spawned through the existing test seams (fake Pi RPC program, fake Claude
//! Agent SDK module, fake ACP program) and verified with a `kill(pid, 0)` probe.

use std::path::Path;

use ajax_core::models::AgentClient;

use super::claude_sdk_spawn::with_test_claude_sdk_module;
use super::session_client::{with_test_pi_program, SessionClient};
use super::{with_test_acp_extra_args, with_test_acp_program};

fn assert_live_process(pid: u32) {
    assert!(
        nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), None).is_ok(),
        "expected a live process for pid {pid}"
    );
}

#[test]
fn pi_child_pid_reports_live_process_and_clears_after_shutdown() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_pi_rpc.js");
    with_test_pi_program(&fixture, &[], || {
        let (mut client, _report) = SessionClient::spawn_with_operator_pin(
            AgentClient::Pi,
            Path::new(env!("CARGO_MANIFEST_DIR")),
            "default",
            None,
        )
        .expect("fake pi rpc spawn must succeed");

        let pid = client.child_pid().expect("live child has a pid");
        assert!(pid > 1, "pid {pid} must be a real process id");
        assert_live_process(pid);

        client.shutdown();
        assert_eq!(client.child_pid(), None, "shut-down pi host reports no pid");
    });
}

#[test]
fn claude_child_pid_reports_live_process_and_clears_after_shutdown() {
    let fake_sdk = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_claude_sdk.mjs");
    with_test_claude_sdk_module(&fake_sdk, || {
        let (mut client, _report) = SessionClient::spawn_with_operator_pin(
            AgentClient::Claude,
            Path::new(env!("CARGO_MANIFEST_DIR")),
            "default",
            None,
        )
        .expect("fake claude sdk spawn must succeed");

        let pid = client.child_pid().expect("live child has a pid");
        assert!(pid > 1, "pid {pid} must be a real process id");
        assert_live_process(pid);

        client.shutdown();
        assert_eq!(
            client.child_pid(),
            None,
            "shut-down claude host reports no pid"
        );
    });
}

#[test]
fn acp_child_pid_reports_live_process() {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_acp.js");
    let dir = std::env::temp_dir().join(format!(
        "ajax-web-pid-tests-acp-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&[], || {
            let (mut client, _report) =
                SessionClient::spawn_with_operator_pin(AgentClient::Cursor, &dir, "default", None)
                    .expect("fake acp spawn must succeed");

            let pid = client.child_pid().expect("live child has a pid");
            assert!(pid > 1, "pid {pid} must be a real process id");
            assert_live_process(pid);

            client.shutdown();
        })
    });

    let _ = std::fs::remove_dir_all(dir);
}

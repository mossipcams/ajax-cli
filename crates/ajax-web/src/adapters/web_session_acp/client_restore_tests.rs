//! Restore-contract tests for [`super::client`]: a stored session id means
//! restore — never a silent fresh `session/new`
//! ([#1151](https://github.com/mossipcams/ajax-cli/issues/1151)).

use super::client::{
    is_restore_unavailable, restore_unavailable_session_id, AcpStdioClient, SpawnReport,
    RESTORE_UNAVAILABLE_MARKER,
};
use super::{with_test_acp_extra_args, with_test_acp_program};
use ajax_core::models::AgentClient;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

fn fake_acp_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_acp.js")
}

fn scratch_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ajax-web-acp-restore-tests-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Count of `session/new` requests the fake agent served from the shared
/// state dir: a restored spawn must not add to it.
fn session_new_count(dir: &std::path::Path) -> usize {
    std::fs::read_to_string(dir.join(".fake-acp-session-new-count"))
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(0)
}

/// `AcpStdioClient` is not `Debug`, so `expect_err` cannot be used on spawn
/// results; this helper keeps the failure context.
fn spawn_error(result: Result<(AcpStdioClient, SpawnReport), String>, context: &str) -> String {
    match result {
        Ok(_) => panic!("{context}: spawn unexpectedly succeeded"),
        Err(error) => error,
    }
}

#[test]
fn shutdown_close_prevents_resume_when_advertised() {
    let dir = scratch_dir("shutdown-close-no-resume");
    let script = fake_acp_fixture();
    let marker = dir.join(".fake-acp-session-close-called");

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--session-close"], || {
            let (mut client, first_report) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("first spawn");
            assert!(first_report.close_advertised);
            let session_id = client.session_id().to_string();
            assert!(client.shutdown().is_none());
            assert!(marker.exists(), "shutdown must send session/close");

            let error = spawn_error(
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, Some(&session_id)),
                "spawn after close must fail closed",
            );
            assert!(
                is_restore_unavailable(&error),
                "a closed session must be a typed restore failure, got: {error}"
            );
            assert_eq!(
                restore_unavailable_session_id(&error).as_deref(),
                Some(session_id.as_str())
            );
        });
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression #1151: a stored session id means restore — a failing
// session/load must be a typed error, never a silent fresh session/new.
// Pre-#1151 this test asserted the silent fresh-session fallback.
#[test]
fn fake_load_fail_errors_without_session_new_issue_1151() {
    let dir = scratch_dir("load-fail-restore");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let (client, _first_report) =
            AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("first spawn");
        let resume_id = client.session_id().to_string();
        drop(client);
        assert_eq!(session_new_count(&dir), 1);

        with_test_acp_extra_args(&["--load-fail"], || {
            let error = spawn_error(
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, Some(&resume_id)),
                "restore spawn must fail closed on session/load failure",
            );
            assert!(
                is_restore_unavailable(&error),
                "expected typed restore failure, got: {error}"
            );
            assert_eq!(
                restore_unavailable_session_id(&error).as_deref(),
                Some(resume_id.as_str()),
                "typed error must carry the stored session id"
            );
        });
        assert_eq!(
            session_new_count(&dir),
            1,
            "failed restore must not silently send session/new (#1151)"
        );
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression #1151: a harness advertising neither session/resume nor
// loadSession cannot restore a stored id; that is a typed error, not a
// silent fresh session.
#[test]
fn restore_requires_resume_or_load_capability_issue_1151() {
    let dir = scratch_dir("no-load-session");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let (client, _first_report) =
            AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("first spawn");
        let resume_id = client.session_id().to_string();
        drop(client);
        assert_eq!(session_new_count(&dir), 1);

        with_test_acp_extra_args(&["--no-load-session"], || {
            let error = spawn_error(
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, Some(&resume_id)),
                "spawn must fail closed without restore capabilities",
            );
            assert!(error.contains(RESTORE_UNAVAILABLE_MARKER));
            assert!(error.contains("does not advertise"));
        });
        assert_eq!(session_new_count(&dir), 1);
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression #1151: a restored session is never dropped just because the
// operator pin could not be proven; the apply error surfaces instead and no
// fresh session/new runs behind the stored id.
#[test]
fn restored_session_survives_unproven_pin_issue_1151() {
    let dir = scratch_dir("restore-pin-refused");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let (client, _first_report) =
            AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("first spawn");
        let resume_id = client.session_id().to_string();
        drop(client);
        assert_eq!(session_new_count(&dir), 1);

        with_test_acp_extra_args(&["--model-refuse"], || {
            let (client2, report) = AcpStdioClient::spawn_with_operator_pin(
                AgentClient::Cursor,
                &dir,
                "composer-2.5",
                Some(&resume_id),
            )
            .expect("restored spawn must keep the session");
            assert!(
                report.resumed,
                "a successful restore must survive an unproven model pin"
            );
            assert_eq!(client2.session_id(), resume_id);
            assert!(
                report.model_apply_error.is_some(),
                "the refused pin must surface as a model apply error"
            );
        });
        assert_eq!(
            session_new_count(&dir),
            1,
            "an unproven pin must not respawn a fresh session behind a restored one (#1151)"
        );
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression #1151: bridges such as pi-acp replay the whole transcript inside
// session/load, so restore gets its own larger budget; a slow load still
// restores within it.
#[test]
fn slow_session_load_still_restores_issue_1151() {
    let dir = scratch_dir("load-delay");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let (client, _first_report) =
            AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("first spawn");
        let resume_id = client.session_id().to_string();
        drop(client);

        with_test_acp_extra_args(&["--load-delay=250"], || {
            let (client2, report) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, Some(&resume_id))
                    .expect("slow load must restore within the restore budget");
            assert!(report.resumed);
            assert_eq!(client2.session_id(), resume_id);
        });
        assert_eq!(session_new_count(&dir), 1);
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression #1151: exceeding the restore budget is the typed error — never
// a silent fresh session.
#[test]
fn restore_timeout_is_a_typed_error_issue_1151() {
    let dir = scratch_dir("load-timeout");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let (client, _first_report) =
            AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("first spawn");
        let resume_id = client.session_id().to_string();
        drop(client);

        super::client::with_test_restore_timeout(100, || {
            with_test_acp_extra_args(&["--load-delay=2000"], || {
                let error = spawn_error(
                    AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, Some(&resume_id)),
                    "a load exceeding the restore budget must fail closed",
                );
                assert!(
                    is_restore_unavailable(&error),
                    "restore timeout must be a typed error, got: {error}"
                );
            });
        });
        assert_eq!(session_new_count(&dir), 1);
    });

    let _ = fs::remove_dir_all(dir);
}

//! Restore and remember-context integration tests for [`super::client`].

use super::client::{
    is_restore_unavailable, restore_unavailable_session_id, AcpStdioClient, SpawnOutcome,
};
use super::{with_test_acp_extra_args, with_test_acp_program};
use ajax_core::models::AgentClient;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

fn fake_acp_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_acp.js")
}

fn scratch_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ajax-web-acp-tests-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn with_fake_acp_state_dir<F, R>(dir: &Path, f: F) -> R
where
    F: FnOnce() -> R,
{
    std::env::set_var("FAKE_ACP_STATE_DIR", dir);
    let result = f();
    std::env::remove_var("FAKE_ACP_STATE_DIR");
    result
}

fn read_recorded_methods(dir: &Path) -> Vec<String> {
    let path = dir.join(".fake-acp-methods");
    let raw = fs::read_to_string(path).unwrap_or_default();
    serde_json::from_str(&raw).unwrap_or_default()
}

fn assert_restore_unavailable(error: &str, session_id: &str) {
    assert!(
        is_restore_unavailable(error),
        "expected restore unavailable, got: {error}"
    );
    assert_eq!(
        restore_unavailable_session_id(error).as_deref(),
        Some(session_id),
        "stored session id must be preserved in error"
    );
}

fn assert_no_session_new(methods: &[String]) {
    assert!(
        !methods.iter().any(|method| method == "session/new"),
        "stored-id restore must not fall back to session/new, saw: {methods:?}"
    );
}

// Regression #1031: resume/load transcript replay must not reach JSONL after install.
#[test]
fn fake_spawn_outcome_created_on_initial_attach() {
    let dir = scratch_dir("outcome-created");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let (_client, report) =
            AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn");
        assert_eq!(report.outcome, SpawnOutcome::Created);
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn fake_spawn_outcome_restored_on_resume() {
    let dir = scratch_dir("outcome-restored-resume");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let (client, first_report) =
            AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("first spawn");
        assert_eq!(first_report.outcome, SpawnOutcome::Created);
        let session_id = client.session_id().to_string();
        drop(client);

        let (client2, second_report) =
            AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, Some(&session_id))
                .expect("resume spawn");
        assert_eq!(
            second_report.outcome,
            SpawnOutcome::Restored {
                session_id: session_id.clone()
            }
        );
        assert_eq!(client2.session_id(), session_id);
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn fake_spawn_outcome_restored_on_load_fallback() {
    let dir = scratch_dir("outcome-restored-load");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let (client, _) =
            AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("first spawn");
        let session_id = client.session_id().to_string();
        drop(client);

        with_test_acp_extra_args(&["--resume-fail"], || {
            let (client2, report) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, Some(&session_id))
                    .expect("restore spawn");
            assert_eq!(
                report.outcome,
                SpawnOutcome::Restored {
                    session_id: session_id.clone()
                }
            );
            assert_eq!(client2.session_id(), session_id);
        });
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn fake_resume_drains_replayed_session_updates() {
    let dir = scratch_dir("resume-drain");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let (client, first_report) =
            AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("first spawn");
        assert_eq!(first_report.outcome, SpawnOutcome::Created);
        let session_id = client.session_id().to_string();
        drop(client);

        let (client2, second_report) =
            AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, Some(&session_id))
                .expect("resume spawn");
        assert_eq!(
            second_report.outcome,
            SpawnOutcome::Restored {
                session_id: session_id.clone()
            }
        );
        assert_eq!(client2.session_id(), session_id);
        assert!(
            client2.poll_event().is_none(),
            "replayed session/update must be drained after session/load"
        );
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn drop_without_close_still_resumes_when_close_advertised() {
    let dir = scratch_dir("drop-detach-resume");
    let script = fake_acp_fixture();
    let marker = dir.join(".fake-acp-session-close-called");

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--session-close"], || {
            let (client, first_report) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("first spawn");
            assert!(first_report.close_advertised);
            let session_id = client.session_id().to_string();
            drop(client);
            assert!(!marker.exists(), "Drop must detach, not session/close");

            let (client2, second_report) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, Some(&session_id))
                    .expect("resume spawn");
            assert_eq!(
                second_report.outcome,
                SpawnOutcome::Restored {
                    session_id: session_id.clone()
                },
                "detached sessions must remain loadable"
            );
            assert_eq!(client2.session_id(), session_id);
        });
    });

    let _ = fs::remove_dir_all(dir);
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

            let error = AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, Some(&session_id))
                .err()
                .unwrap_or_else(|| panic!("closed sessions must fail restore closed"));
            assert_restore_unavailable(&error, &session_id);
        });
    });

    let _ = fs::remove_dir_all(dir);
}

// T6: stored id + failed resume/load must never session/new and returns RestoreUnavailable.
#[test]
fn stored_id_load_fail_returns_restore_unavailable_without_session_new() {
    let dir = scratch_dir("restore-unavail-load-fail");
    let script = fake_acp_fixture();

    with_fake_acp_state_dir(&dir, || {
        with_test_acp_program(&script, || {
            let (client, _) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("first spawn");
            let session_id = client.session_id().to_string();
            drop(client);

            with_test_acp_extra_args(&["--load-fail", "--record-methods"], || {
                let error =
                    AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, Some(&session_id))
                        .err()
                        .unwrap_or_else(|| panic!("load failure must fail restore closed"));
                assert_restore_unavailable(&error, &session_id);
                assert_no_session_new(&read_recorded_methods(&dir));
            });
        });
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn stored_id_resume_and_load_fail_returns_restore_unavailable_without_session_new() {
    let dir = scratch_dir("restore-unavail-resume-load-fail");
    let script = fake_acp_fixture();

    with_fake_acp_state_dir(&dir, || {
        with_test_acp_program(&script, || {
            let (client, _) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("first spawn");
            let session_id = client.session_id().to_string();
            drop(client);

            with_test_acp_extra_args(
                &[
                    "--resume",
                    "--resume-fail",
                    "--load-fail",
                    "--record-methods",
                ],
                || {
                    let error =
                        AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, Some(&session_id))
                            .err()
                            .unwrap_or_else(|| {
                                panic!("resume and load failure must fail restore closed")
                            });
                    assert_restore_unavailable(&error, &session_id);
                    assert_no_session_new(&read_recorded_methods(&dir));
                },
            );
        });
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn stored_id_missing_restore_capability_returns_restore_unavailable_without_session_new() {
    let dir = scratch_dir("restore-unavail-no-capability");
    let script = fake_acp_fixture();

    with_fake_acp_state_dir(&dir, || {
        with_test_acp_program(&script, || {
            let (client, _) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("first spawn");
            let session_id = client.session_id().to_string();
            drop(client);

            with_test_acp_extra_args(&["--no-load-session", "--record-methods"], || {
                let error =
                    AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, Some(&session_id))
                        .err()
                        .unwrap_or_else(|| {
                            panic!("missing restore capability must fail restore closed")
                        });
                assert_restore_unavailable(&error, &session_id);
                assert_no_session_new(&read_recorded_methods(&dir));
            });
        });
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn stored_id_restore_timeout_returns_restore_unavailable_without_session_new() {
    let dir = scratch_dir("restore-unavail-timeout");
    let script = fake_acp_fixture();

    with_fake_acp_state_dir(&dir, || {
        with_test_acp_program(&script, || {
            let (client, _) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("first spawn");
            let session_id = client.session_id().to_string();
            drop(client);

            with_test_acp_extra_args(&["--hang-load", "--record-methods"], || {
                let error =
                    AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, Some(&session_id))
                        .err()
                        .unwrap_or_else(|| panic!("restore timeout must fail restore closed"));
                assert_restore_unavailable(&error, &session_id);
                assert_no_session_new(&read_recorded_methods(&dir));
            });
        });
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn initial_attach_without_stored_id_still_session_new() {
    let dir = scratch_dir("restore-unavail-initial-new");
    let script = fake_acp_fixture();

    with_fake_acp_state_dir(&dir, || {
        with_test_acp_program(&script, || {
            with_test_acp_extra_args(&["--record-methods"], || {
                let (_client, report) =
                    AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn");
                assert_eq!(report.outcome, SpawnOutcome::Created);
                assert!(
                    read_recorded_methods(&dir).contains(&"session/new".to_string()),
                    "initial attach must still call session/new"
                );
            });
        });
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn failed_session_resume_falls_back_to_session_load() {
    let dir = scratch_dir("resume-falls-back-to-load");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let (client, _) =
            AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("first spawn");
        let session_id = client.session_id().to_string();
        drop(client);

        with_test_acp_extra_args(&["--resume-fail"], || {
            let (client, report) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, Some(&session_id))
                    .expect("restore spawn");
            assert_eq!(
                report.outcome,
                SpawnOutcome::Restored {
                    session_id: session_id.clone()
                },
                "session/load should follow a failed resume"
            );
            assert_eq!(client.session_id(), session_id);
            assert!(client.poll_event().is_none(), "load replay must be drained");
        });
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn fake_load_fail_returns_restore_unavailable() {
    let dir = scratch_dir("load-fail");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        let (client, _first_report) =
            AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("first spawn");
        let resume_id = client.session_id().to_string();
        drop(client);

        with_test_acp_extra_args(&["--load-fail"], || {
            let error = AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, Some(&resume_id))
                .err()
                .unwrap_or_else(|| panic!("spawn after load fail must not create fresh context"));
            assert_restore_unavailable(&error, &resume_id);
        });
    });

    let _ = fs::remove_dir_all(dir);
}

use super::client::{AcpClientEvent, AcpStdioClient};
use super::{with_test_acp_extra_args, with_test_acp_program};
use ajax_core::models::AgentClient;
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn fake_acp_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_acp.js")
}

fn scratch_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ajax-web-spawn-tests-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}
#[test]
fn cursor_parameterized_picker_applies_grok_high_without_fast_issue_979() {
    let dir = scratch_dir("model-cursor-parameterized-grok-979");
    let script = fake_acp_fixture();
    let catalog_id = "cursor-grok-4.6-high";

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(
            &[
                "--cursor-parameterized-models",
                "--cli-default-model",
                "--ignore-spawn-model-once",
            ],
            || {
                let (client, report) = AcpStdioClient::spawn_with_operator_pin(
                    AgentClient::Cursor,
                    &dir,
                    catalog_id,
                    None,
                )
                .expect("spawn");
                assert!(
                    report.model_apply_error.is_none(),
                    "parameterized apply must satisfy Grok High: {:?}",
                    report.model_apply_error
                );
                assert_eq!(report.applied_model, "grok-4.6");
                assert_ne!(report.applied_model, "composer-2.5");
                assert_ne!(report.applied_model, "composer-2.5[fast=true]");
                let options = report
                    .config_options
                    .as_deref()
                    .expect("parameterized apply must advertise config options");
                assert!(
                    super::config_options::pin_satisfied(Some(options), catalog_id, true),
                    "parameterized apply must satisfy Grok High pin"
                );
                let fast = super::config_options::model_config_boolean_option(options)
                    .expect("parameterized harness must advertise Fast");
                assert_eq!(
                    super::config_options::read_fast_current_value(fast),
                    Some(false),
                    "Grok High must apply Fast=false"
                );
                let deadline = Instant::now() + Duration::from_millis(500);
                while Instant::now() < deadline {
                    let Some(event) = client.wait_event(Duration::from_millis(50)) else {
                        continue;
                    };
                    if let AcpClientEvent::SessionUpdate(update) = event {
                        let text = serde_json::to_string(&update).unwrap();
                        assert!(
                            !text.contains("model:session/set_config_option:cursor-grok-4.6-high"),
                            "must not send catalog id as set_config_option value: {text}"
                        );
                    }
                }
            },
        );
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn spawn_with_operator_pin_recovery_waits_for_prior_child_shutdown_issue_989() {
    let dir = scratch_dir("spawn-recover-exclusive-989");
    let script = fake_acp_fixture();
    let lock = dir.join(".fake-acp-exclusive-lock");
    let _ = fs::remove_file(&lock);
    let catalog_id = "composer-2.5-fast";

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(
            &[
                "--exclusive-session-new",
                "--cursor-models",
                "--cli-default-model",
                "--ignore-spawn-model-once",
                "--refuse-in-band-once",
            ],
            || {
                let (_client, report) = AcpStdioClient::spawn_with_operator_pin(
                    AgentClient::Cursor,
                    &dir,
                    catalog_id,
                    None,
                )
                .expect("recovery spawn must not fail session/new with incoming_transport_closed");
                assert!(
                    report.model_apply_error.is_none(),
                    "composer fast pin must apply after recovery: {:?}",
                    report.model_apply_error
                );
                assert_eq!(report.applied_model, "composer-2.5[fast=true]");
            },
        );
    });

    let _ = fs::remove_dir_all(dir);
}

//! Spawn and in-band model apply integration tests for [`super::client`].

use super::client::{acp_args_for_program, AcpClientEvent, AcpStdioClient};
use super::operator_pin_satisfied;
use super::{with_test_acp_extra_args, with_test_acp_program};
use ajax_core::adapters::{
    cursor_catalog_to_acp_in_band_token, cursor_catalog_to_acp_spawn_token,
    cursor_unspecified_spawn_satisfied, CURSOR_DEFAULT_SPAWN_MODEL,
};
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
        "ajax-web-acp-spawn-tests-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}
fn cursor_launch() -> ajax_core::adapters::AcpLaunch {
    ajax_core::adapters::acp_launch_for_agent(AgentClient::Cursor).expect("cursor acp launch")
}

#[test]
fn cursor_acp_command_prefers_agent_binary() {
    let candidates = cursor_launch().candidates;
    assert_eq!(candidates[0].0, "agent");
    assert_eq!(candidates[0].1, &["acp"][..]);
    assert_eq!(candidates[1].0, "cursor");
    assert_eq!(candidates[1].1, &["agent", "acp"][..]);
}

#[test]
fn cursor_acp_args_insert_model_before_acp() {
    let launch = cursor_launch();
    assert_eq!(
        acp_args_for_program(launch, &["acp"], Some("composer-2.5")),
        vec!["--model", "composer-2.5", "acp"]
    );
    assert_eq!(
        acp_args_for_program(launch, &["agent", "acp"], Some("gpt-5.6-sol-medium")),
        vec!["agent", "--model", "gpt-5.6-sol-medium", "acp"]
    );
    assert_eq!(
        acp_args_for_program(launch, &["acp"], Some("auto")),
        vec!["--model", CURSOR_DEFAULT_SPAWN_MODEL, "acp"]
    );
    assert_eq!(
        acp_args_for_program(launch, &["acp"], None),
        vec!["--model", CURSOR_DEFAULT_SPAWN_MODEL, "acp"]
    );
}

// The bridges take no `--model` on argv; a pinned model must not leak onto them.
#[test]
fn bridge_acp_args_never_carry_a_model_flag() {
    for agent in [AgentClient::Codex, AgentClient::Claude, AgentClient::Pi] {
        let launch = ajax_core::adapters::acp_launch_for_agent(agent).expect("bridge acp launch");
        assert!(
            !launch.model_pins_at_spawn(),
            "{agent:?} must not pin at spawn"
        );
        assert!(
            acp_args_for_program(launch, launch.candidates[0].1, Some("composer-2.5")).is_empty(),
            "{agent:?} argv must stay bare"
        );
    }
}

// Native first: a harness that grows its own `acp` subcommand must be used
// directly instead of its packaged adapter. Recorded per harness in core.
#[test]
fn every_bridge_harness_names_the_cli_that_could_speak_acp_natively() {
    use ajax_core::adapters::acp_launch_for_agent;

    assert_eq!(
        acp_launch_for_agent(AgentClient::Cursor)
            .expect("cursor")
            .native_program,
        None,
        "cursor's candidates are already its own binary"
    );
    for (agent, program) in [
        (AgentClient::Codex, "codex"),
        (AgentClient::Claude, "claude"),
        (AgentClient::Pi, "pi"),
    ] {
        assert_eq!(
            acp_launch_for_agent(agent).expect("bridge").native_program,
            Some(program),
            "{agent:?} should prefer its own CLI once it advertises acp"
        );
    }
}

// Each harness family takes its model a different way; the bridges would
// silently keep their own default if the client skipped the in-band call.
#[test]
fn spawn_selects_the_model_in_band_for_bridge_harnesses() {
    use ajax_core::adapters::{acp_launch_for_agent, AcpModelSelection};

    for agent in [AgentClient::Codex, AgentClient::Claude, AgentClient::Pi] {
        assert_eq!(
            acp_launch_for_agent(agent).expect("bridge").model_selection,
            AcpModelSelection::ConfigOption,
            "{agent:?} selects its model through a config option"
        );
    }

    let script = fake_acp_fixture();
    for agent in [AgentClient::Codex, AgentClient::Claude] {
        let dir = scratch_dir(&format!("model-in-band-{agent:?}"));
        with_test_acp_program(&script, || {
            let (_client, report) =
                AcpStdioClient::spawn(agent, &dir, Some("composer-2.5"), None).expect("spawn");
            assert_eq!(report.applied_model, "composer-2.5");
            assert!(
                report.model_apply_error.is_none(),
                "{agent:?} apply error: {:?}",
                report.model_apply_error
            );
        });
        let _ = fs::remove_dir_all(dir);
    }
}

// Regression for #952: Cursor must apply in-band when session/new advertises model.
#[test]
fn cursor_applies_in_band_when_model_is_advertised_issue_952() {
    let dir = scratch_dir("model-cursor-in-band-952");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--ignore-spawn-model-once"], || {
            let (client, report) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, Some("composer-2.5"), None)
                    .expect("spawn");
            assert_eq!(report.applied_model, "composer-2.5");
            assert!(report.model_apply_error.is_none());
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                if let Some(AcpClientEvent::SessionUpdate(update)) =
                    client.wait_event(Duration::from_millis(100))
                {
                    let text = serde_json::to_string(&update).unwrap();
                    if text.contains("model:session/set_config_option:composer-2.5") {
                        return;
                    }
                }
            }
            panic!("cursor never applied its model in band when advertised");
        });
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression for #954: Cursor spawn catalog ids must not be sent as handshake values.
#[test]
fn cursor_spawn_catalog_id_skips_in_band_when_not_advertised_issue_954() {
    let dir = scratch_dir("model-cursor-catalog-954");
    let script = fake_acp_fixture();
    let catalog_id = "cursor-grok-4.6-high";

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--cursor-models"], || {
            let (client, report) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, Some(catalog_id), None)
                    .expect("spawn");
            assert!(
                report.model_apply_error.is_none(),
                "catalog id in a different id space must not refuse: {:?}",
                report.model_apply_error
            );
            assert!(
                super::config_options::pin_satisfied(
                    report.config_options.as_deref(),
                    catalog_id,
                    true
                ),
                "applied {:?} must satisfy pin without sending catalog id on wire",
                report.applied_model
            );
            assert_eq!(report.applied_model, "grok-4.6[effort=high,fast=false]");
            assert_ne!(report.applied_model, catalog_id);
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                if let Some(AcpClientEvent::SessionUpdate(update)) =
                    client.wait_event(Duration::from_millis(100))
                {
                    let text = serde_json::to_string(&update).unwrap();
                    assert!(
                        !text.contains("model:session/set_config_option:cursor-grok-4.6-high"),
                        "must not send catalog id as handshake value: {text}"
                    );
                }
            }
        });
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression for #979: mapped spawn token must run Grok High, not Composer Fast.
#[test]
fn cursor_spawn_catalog_pin_runs_mapped_acp_model_issue_979() {
    let dir = scratch_dir("model-cursor-cli-default-979");
    let script = fake_acp_fixture();
    let catalog_id = "cursor-grok-4.6-high";
    let _mapped = cursor_catalog_to_acp_spawn_token(catalog_id);

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--cli-default-model", "--cursor-models"], || {
            let (_client, report) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, Some(catalog_id), None)
                    .expect("spawn");
            assert!(
                report.model_apply_error.is_none(),
                "mapped spawn must satisfy {catalog_id}, not Composer Fast: {:?}",
                report.model_apply_error
            );
            assert!(
                super::config_options::pin_satisfied(
                    report.config_options.as_deref(),
                    catalog_id,
                    true
                ),
                "applied {:?} must satisfy {catalog_id} pin",
                report.applied_model
            );
            assert_eq!(report.applied_model, "grok-4.6[effort=high,fast=false]");
            assert_ne!(report.applied_model, "composer-2.5[fast=true]");
        });
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression for #979: resume with wrong applied model must recover like fresh attach.
#[test]
fn cursor_spawn_recovers_after_resume_composer_fast_issue_979() {
    let dir = scratch_dir("model-cursor-recover-resume-979");
    let script = fake_acp_fixture();
    let catalog_id = "cursor-grok-4.6-high";
    let _mapped = cursor_catalog_to_acp_spawn_token(catalog_id);

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(
            &[
                "--resume",
                "--cli-default-model",
                "--cursor-models",
                "--ignore-spawn-model-once",
                "--refuse-in-band-once",
            ],
            || {
                let (_client, report) = AcpStdioClient::spawn_with_operator_pin(
                    AgentClient::Cursor,
                    &dir,
                    catalog_id,
                    Some("fake-sess-1"),
                )
                .expect("spawn");
                assert!(
                    report.model_apply_error.is_none(),
                    "recovery must satisfy {catalog_id}, not Composer Fast: {:?}",
                    report.model_apply_error
                );
                assert!(
                    super::config_options::pin_satisfied(
                        report.config_options.as_deref(),
                        catalog_id,
                        true
                    ),
                    "recovery must satisfy {catalog_id}, applied {:?}",
                    report.applied_model
                );
                assert_eq!(report.applied_model, "grok-4.6[effort=high,fast=false]");
                assert_ne!(report.applied_model, "composer-2.5");
                assert!(
                    !report.resumed,
                    "recovery must respawn with session/new, not resume/load"
                );
            },
        );
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression for #979: respawn once when spawn argv and in-band apply both leave CLI default.
#[test]
fn cursor_spawn_recovers_after_cli_default_and_refused_in_band_issue_979() {
    let dir = scratch_dir("model-cursor-recover-979");
    let script = fake_acp_fixture();
    let catalog_id = "cursor-grok-4.6-high";
    let _mapped = cursor_catalog_to_acp_spawn_token(catalog_id);

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(
            &[
                "--cli-default-model",
                "--cursor-models",
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
                .expect("spawn");
                assert!(
                    report.model_apply_error.is_none(),
                    "recovery must satisfy {catalog_id}, not Composer Fast: {:?}",
                    report.model_apply_error
                );
                assert!(
                    super::config_options::pin_satisfied(
                        report.config_options.as_deref(),
                        catalog_id,
                        true
                    ),
                    "recovery must satisfy {catalog_id}, applied {:?}",
                    report.applied_model
                );
                assert_eq!(report.applied_model, "grok-4.6[effort=high,fast=false]");
                assert_ne!(report.applied_model, "composer-2.5");
            },
        );
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression for #979: unspecified Cursor attach must not accept Composer Fast.
#[test]
fn cursor_unspecified_spawn_recovers_onto_mapped_default_issue_979() {
    let dir = scratch_dir("model-cursor-recover-default-979");
    let script = fake_acp_fixture();
    let mapped = CURSOR_DEFAULT_SPAWN_MODEL;

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(
            &[
                "--cli-default-model",
                "--cursor-models",
                "--ignore-spawn-model-once",
                "--refuse-in-band-once",
            ],
            || {
                let (_client, report) = AcpStdioClient::spawn_with_operator_pin(
                    AgentClient::Cursor,
                    &dir,
                    "auto",
                    None,
                )
                .expect("spawn");
                assert!(
                    report.model_apply_error.is_none(),
                    "recovery must run {mapped:?}, not Composer Fast: {:?}",
                    report.model_apply_error
                );
                assert!(
                    cursor_unspecified_spawn_satisfied(&report.applied_model),
                    "recovery must run {mapped:?}, applied {:?}",
                    report.applied_model
                );
                assert_ne!(report.applied_model, "composer-2.5");
            },
        );
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression for #979: unspecified Cursor attach must not accept Composer Fast.
#[test]
fn cursor_unspecified_spawn_runs_mapped_default_not_composer_fast_issue_979() {
    let dir = scratch_dir("model-cursor-unspecified-979");
    let script = fake_acp_fixture();
    let mapped = CURSOR_DEFAULT_SPAWN_MODEL;

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--cli-default-model", "--cursor-models"], || {
            let (_client, report) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn");
            assert!(
                report.model_apply_error.is_none(),
                "unspecified spawn must run {mapped:?}, not Composer Fast: {:?}",
                report.model_apply_error
            );
            assert!(
                cursor_unspecified_spawn_satisfied(&report.applied_model),
                "applied {:?} must satisfy unspecified spawn default",
                report.applied_model
            );
            assert_ne!(report.applied_model, "composer-2.5");
        });
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression for #984: Sol High catalog pin passes catalog id on spawn argv and
// satisfies the pin via mapped in-band ACP apply ([#989] spawn passthrough).
#[test]
fn cursor_spawn_catalog_pin_runs_sol_high_mapped_acp_model_issue_984() {
    let dir = scratch_dir("model-cursor-sol-high-984");
    let script = fake_acp_fixture();
    let catalog_id = "gpt-5.6-sol-high";
    let spawn_token = cursor_catalog_to_acp_spawn_token(catalog_id);
    let in_band = cursor_catalog_to_acp_in_band_token(catalog_id);
    let launch = cursor_launch();

    assert_eq!(
        spawn_token, catalog_id,
        "spawn argv must keep catalog id unchanged"
    );
    assert_eq!(
        acp_args_for_program(launch, &["acp"], Some(catalog_id)),
        vec!["--model", catalog_id, "acp"]
    );
    assert_eq!(in_band, "gpt-5.6-sol[effort=high,fast=false]");

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--cli-default-model", "--cursor-models"], || {
            let (_client, report) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, Some(catalog_id), None)
                    .expect("spawn");
            assert!(
                report.model_apply_error.is_none(),
                "Sol High pin must apply cleanly, not Composer Fast: {:?}",
                report.model_apply_error
            );
            assert!(
                operator_pin_satisfied(catalog_id, &report.applied_model, true),
                "applied {:?} must satisfy Sol High pin {catalog_id}",
                report.applied_model
            );
            assert_ne!(report.applied_model, "composer-2.5[fast=true]");
            assert_ne!(
                report.applied_model, "gpt-5.6-sol[effort=high,fast=true]",
                "Sol High pin must not accept Fast variant"
            );
        });
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression #997: live handshake without non-Fast bracket leaves typed error, child kept.
#[test]
fn cursor_grok_high_errors_when_live_handshake_omits_non_fast_bracket_issue_997() {
    let dir = scratch_dir("model-cursor-unadvertised-grok-997");
    let script = fake_acp_fixture();
    let catalog_id = "cursor-grok-4.6-high";

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(
            &[
                "--cursor-live-models",
                "--cli-default-model",
                "--ignore-spawn-model-once",
            ],
            || {
                let (_client, report) = AcpStdioClient::spawn_with_operator_pin(
                    AgentClient::Cursor,
                    &dir,
                    catalog_id,
                    None,
                )
                .expect("spawn");
                assert!(
                    report.model_apply_error.is_some(),
                    "missing non-Fast advertisement must refuse pin"
                );
                assert_ne!(report.applied_model, "grok-4.6");
            },
        );
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression for live Cursor handshake: Grok High is only `fast=true`; recover must
// not leave Composer Fast after spawn/apply failures.
#[test]
fn cursor_grok_high_recovers_on_live_handshake_after_composer_fast_issue_979() {
    let dir = scratch_dir("model-cursor-live-grok-979");
    let script = fake_acp_fixture();
    let catalog_id = "cursor-grok-4.6-high";
    let mapped = cursor_catalog_to_acp_spawn_token(catalog_id);
    assert_eq!(mapped, catalog_id);

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(
            &[
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
                .expect("spawn");
                assert!(
                    report.model_apply_error.is_none(),
                    "live-shaped recover must satisfy {catalog_id}, not Composer Fast: {:?}",
                    report.model_apply_error
                );
                assert!(
                    super::config_options::pin_satisfied(
                        report.config_options.as_deref(),
                        catalog_id,
                        true
                    ),
                    "live-shaped recover must satisfy {catalog_id}, applied {:?}",
                    report.applied_model
                );
                assert_eq!(report.applied_model, "grok-4.6[effort=high,fast=false]");
                assert_ne!(report.applied_model, "composer-2.5");
            },
        );
    });

    let _ = fs::remove_dir_all(dir);
}

// Regression for #954: ConfigOption-only harnesses still refuse unadvertised pins.
#[test]
fn bridge_errors_when_pin_not_advertised_issue_954() {
    let dir = scratch_dir("model-bridge-not-advertised-954");
    let script = fake_acp_fixture();
    let not_advertised = "cursor-grok-4.6-high";

    with_test_acp_program(&script, || {
        let (_client, report) =
            AcpStdioClient::spawn(AgentClient::Codex, &dir, Some(not_advertised), None)
                .expect("spawn");
        assert!(
            report.model_apply_error.is_some(),
            "ConfigOption harness must error when pin is not advertised"
        );
        assert!(report
            .model_apply_error
            .as_deref()
            .is_some_and(|error| error.contains(not_advertised)));
    });

    let _ = fs::remove_dir_all(dir);
}

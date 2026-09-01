use super::test_support::BlockingSessionDirectory;
use super::ws_bridge::{should_send_keepalive, MAX_SESSION_FRAME_BYTES, SESSION_PING_INTERVAL};
use super::{
    apply_client_message, ApplyClientMessageOutcome, SessionClientMessage, SessionServerEvent,
    SessionSnapshot,
};
use crate::adapters::web_session_acp::{
    with_test_acp_extra_args, with_test_acp_program, SessionConfigValue,
};
use ajax_core::models::AgentClient;
use std::{path::PathBuf, time::Duration};

fn scratch_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ajax-web-bridge-{label}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn fake_acp_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_acp.js")
}

fn model_config_message(model: &str) -> SessionClientMessage {
    SessionClientMessage::SetConfigOption {
        config_id: "model".to_string(),
        value: SessionConfigValue::Select(model.to_string()),
    }
}

#[test]
fn max_session_frame_bytes_is_8_mib() {
    assert_eq!(MAX_SESSION_FRAME_BYTES, 8 * 1024 * 1024);
}

#[test]
fn keepalive_waits_for_silence_then_pings() {
    assert!(!should_send_keepalive(Duration::ZERO));
    assert!(!should_send_keepalive(
        SESSION_PING_INTERVAL - Duration::from_millis(1)
    ));
    assert!(should_send_keepalive(SESSION_PING_INTERVAL));
    assert!(should_send_keepalive(SESSION_PING_INTERVAL * 3));
}

#[test]
fn set_config_option_accepts_only_string_or_boolean_values() {
    assert!(serde_json::from_str::<SessionClientMessage>(
        r#"{"type":"set_config_option","configId":"model","value":"composer-2.5"}"#
    )
    .is_ok());
    assert!(serde_json::from_str::<SessionClientMessage>(
        r#"{"type":"set_config_option","configId":"fast","value":true}"#
    )
    .is_ok());
    assert!(serde_json::from_str::<SessionClientMessage>(
        r#"{"type":"set_config_option","configId":"fast","value":{"type":"boolean","value":true}}"#
    )
    .is_err());
}

#[test]
fn apply_client_message_rejects_legacy_set_model_wire() {
    assert!(serde_json::from_str::<SessionClientMessage>(
        r#"{"type":"set_model","model":"composer-2.5"}"#
    )
    .is_err());
}

#[test]
fn apply_client_message_prompt_records_user_message_immediately() {
    let dir = scratch_dir("prompt-flush");
    let handle = "web/prompt-flush";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        let mut generation = directory.generation(handle);
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(apply_client_message(
            directory.inner(),
            handle,
            &dir,
            SessionClientMessage::Prompt {
                text: "hello".to_string(),
                content_blocks: vec![],
                client_message_id: "prompt-1".to_string(),
            },
            &mut generation,
            None,
        ))
        .expect("prompt");

        let (events, _) = directory.read_from(handle, 0);
        assert!(events.iter().any(|event| {
            matches!(
                event,
                SessionServerEvent::Message { role, text, .. }
                    if role == "user" && text == "hello"
            )
        }));
    });

    let _ = std::fs::remove_dir_all(dir);
}

// Regression for issue #931: in-session set_config_option persists the model id
// after a successful live apply; persistence failure leaves the child running.
#[test]
fn apply_client_message_set_config_option_persists_after_in_band_apply() {
    let dir = scratch_dir("set-model-persist");
    let handle = "web/set-model";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();
    let persisted = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));
    let persisted_for_closure = std::sync::Arc::clone(&persisted);

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        let before = directory.child_id(handle).expect("child");
        let session_before =
            crate::adapters::web_session_store::load::<SessionServerEvent>(&dir, handle)
                .acp_session_id;
        let mut generation = directory.generation(handle);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let persist: super::PersistSessionModel = std::sync::Arc::new(move |model: &str| {
            *persisted_for_closure.lock().unwrap() = Some(model.to_string());
            Ok(())
        });
        rt.block_on(apply_client_message(
            directory.inner(),
            handle,
            &dir,
            SessionClientMessage::SetConfigOption {
                config_id: "model".to_string(),
                value: SessionConfigValue::Select("composer-2.5".to_string()),
            },
            &mut generation,
            Some(persist),
        ))
        .expect("set config option");

        assert_eq!(persisted.lock().unwrap().as_deref(), Some("composer-2.5"));
        assert_eq!(directory.child_id(handle), Some(before));
        let session_after =
            crate::adapters::web_session_store::load::<SessionServerEvent>(&dir, handle)
                .acp_session_id;
        assert_eq!(session_after, session_before);
        super::test_support::pump_until(&directory, handle, Duration::from_secs(5), |events| {
            events.iter().any(|event| {
                matches!(
                    event,
                    SessionServerEvent::Message { text, .. }
                        if text.contains("model:session/set_config_option:composer-2.5")
                )
            })
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

// Regression #1014: each live model axis refreshes the complete restart pin.
#[test]
fn apply_client_message_set_config_option_persists_effort_and_fast_toggles() {
    let dir = scratch_dir("set-fast-persist");
    let handle = "web/set-fast";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();
    let persisted = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));
    let persisted_for_closure = std::sync::Arc::clone(&persisted);

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(
            &["--cursor-parameterized-models", "--cli-default-model"],
            || {
                directory
                    .acquire(handle, &dir, "auto", AgentClient::Cursor)
                    .expect("acquire");
                let before = directory.child_id(handle).expect("child");
                let mut generation = directory.generation(handle);
                let rt = tokio::runtime::Runtime::new().unwrap();
                let persist: super::PersistSessionModel =
                    std::sync::Arc::new(move |model: &str| {
                        *persisted_for_closure.lock().unwrap() = Some(model.to_string());
                        Ok(())
                    });
                rt.block_on(apply_client_message(
                    directory.inner(),
                    handle,
                    &dir,
                    SessionClientMessage::SetConfigOption {
                        config_id: "effort".to_string(),
                        value: SessionConfigValue::Select("medium".to_string()),
                    },
                    &mut generation,
                    Some(std::sync::Arc::clone(&persist)),
                ))
                .expect("set effort config option");
                assert_eq!(
                    persisted.lock().unwrap().as_deref(),
                    Some("grok-4.6|effort=medium|fast=false")
                );

                rt.block_on(apply_client_message(
                    directory.inner(),
                    handle,
                    &dir,
                    SessionClientMessage::SetConfigOption {
                        config_id: "fast".to_string(),
                        value: SessionConfigValue::Boolean(true),
                    },
                    &mut generation,
                    Some(persist),
                ))
                .expect("set fast config option");

                assert_eq!(
                    persisted.lock().unwrap().as_deref(),
                    Some("grok-4.6|effort=medium|fast=true")
                );
                assert_eq!(directory.child_id(handle), Some(before));
            },
        );
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn apply_client_message_set_config_option_leaves_child_unchanged_when_persist_fails() {
    let dir = scratch_dir("set-model-persist-fail");
    let handle = "web/set-model-fail";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        let before = directory.child_id(handle).expect("child");
        let mut generation = directory.generation(handle);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let persist: super::PersistSessionModel =
            std::sync::Arc::new(|_model: &str| Err("registry write failed".to_string()));
        let outcome = rt
            .block_on(apply_client_message(
                directory.inner(),
                handle,
                &dir,
                SessionClientMessage::SetConfigOption {
                    config_id: "model".to_string(),
                    value: SessionConfigValue::Select("composer-2.5".to_string()),
                },
                &mut generation,
                Some(persist),
            ))
            .expect("apply succeeds even when persist fails");
        assert_eq!(
            outcome,
            ApplyClientMessageOutcome::ModelChanged {
                persist_warning: Some(
                    "Model changed but could not save to task — registry write failed".to_string()
                ),
            }
        );
        assert_eq!(directory.child_id(handle), Some(before));
        directory.pump(handle);
        let (events, _) = directory.read_from(handle, 0);
        assert!(!events.iter().any(|event| matches!(
            event,
            SessionServerEvent::Error { message }
                if message.contains("could not save to task")
        )));
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn apply_client_message_set_config_option_surfaces_worker_stop_without_respawn_issue_962() {
    let dir = scratch_dir("set-model-worker-stop");
    let handle = "web/set-model-worker-stop";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        let mut generation = directory.generation(handle);
        let generation_before = generation;
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(directory.inner().clone().drop_session(handle));
        let persist: super::PersistSessionModel = std::sync::Arc::new(|_model: &str| Ok(()));
        let error = rt
            .block_on(apply_client_message(
                directory.inner(),
                handle,
                &dir,
                model_config_message("composer-2.5"),
                &mut generation,
                Some(persist),
            ))
            .unwrap_err();
        assert!(
            error.contains("session slot missing"),
            "unexpected error: {error}"
        );
        assert_eq!(generation, generation_before);
    });

    let _ = std::fs::remove_dir_all(dir);
}

// Regression for issue #942: set_config_option must publish the applied model on attach
// and keep it after the next prompt without replacing the ACP child in-band.
#[test]
fn apply_client_message_set_config_option_keeps_host_model_after_prompt_issue_942() {
    let dir = scratch_dir("set-model-prompt-942");
    let handle = "web/set-model-prompt";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        let before = directory.child_id(handle).expect("child");
        let mut generation = directory.generation(handle);
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(apply_client_message(
            directory.inner(),
            handle,
            &dir,
            model_config_message("composer-2.5"),
            &mut generation,
            None,
        ))
        .expect("set model");

        assert_eq!(directory.child_id(handle), Some(before));
        let attach = rt.block_on(directory.inner().attach_snapshot(
            handle,
            "composer-2.5".to_string(),
            None,
        ));
        assert_eq!(attach.snapshot.model, "composer-2.5");
        assert_eq!(attach.generation, generation);

        rt.block_on(apply_client_message(
            directory.inner(),
            handle,
            &dir,
            SessionClientMessage::Prompt {
                text: "hello".to_string(),
                content_blocks: vec![],
                client_message_id: "prompt-942".to_string(),
            },
            &mut generation,
            None,
        ))
        .expect("prompt");

        super::test_support::pump_until(&directory, handle, Duration::from_secs(5), |events| {
            events.iter().any(|event| match event {
                SessionServerEvent::TurnEnd { .. } => true,
                SessionServerEvent::Message { text, .. } => text == "pong",
                _ => false,
            })
        });

        let after_prompt = rt.block_on(directory.inner().attach_snapshot(
            handle,
            "composer-2.5".to_string(),
            None,
        ));
        assert_eq!(after_prompt.snapshot.model, "composer-2.5");
    });

    let _ = std::fs::remove_dir_all(dir);
}

// Regression for #979: Switch to Grok High must keep the child alive and apply mapped ACP id.
#[test]
fn apply_client_message_set_config_option_grok_high_keeps_child_alive_issue_979() {
    use ajax_core::adapters::cursor_catalog_to_acp_in_band_token;

    let dir = scratch_dir("set-model-grok-high-979");
    let handle = "web/set-model-grok-high";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();
    let catalog_id = "cursor-grok-4.6-high";
    let mapped = cursor_catalog_to_acp_in_band_token(catalog_id);

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--cursor-models", "--cli-default-model"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            let before = directory.child_id(handle).expect("child");
            let mut generation = directory.generation(handle);
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(apply_client_message(
                directory.inner(),
                handle,
                &dir,
                model_config_message(&mapped),
                &mut generation,
                None,
            ))
            .expect("set model");

            assert_eq!(directory.child_id(handle), Some(before));
            directory.pump(handle);
            let attach = rt.block_on(directory.inner().attach_snapshot(
                handle,
                catalog_id.to_string(),
                None,
            ));
            assert_eq!(attach.snapshot.model, mapped);
            assert_ne!(attach.snapshot.model, "composer-2.5[fast=true]");
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

// In-band refusal falls back to one respawn; child id changes only on that path.
#[test]
fn apply_client_message_set_config_option_refusal_keeps_child_alive() {
    let dir = scratch_dir("set-model-respawn-fallback");
    let handle = "web/set-model-respawn";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--refuse-in-band-once", "--cursor-models"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            let before = directory.child_id(handle).expect("child");
            let mut generation = directory.generation(handle);
            let generation_before = generation;
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(apply_client_message(
                directory.inner(),
                handle,
                &dir,
                model_config_message("composer-2.5"),
                &mut generation,
                None,
            ));

            assert!(result.is_err());
            assert_eq!(directory.child_id(handle), Some(before));
            assert_eq!(generation, generation_before);
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

// Regression for #952: snapshot.model is harness-reported applied state, not the
// attach-plan pin when the harness refuses the operator selection.
#[test]
fn attach_snapshot_reports_applied_model_not_desired_pin_issue_952() {
    let dir = scratch_dir("snapshot-applied-952");
    let handle = "web/snapshot-applied";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--model-refuse"], || {
            directory
                .acquire(handle, &dir, "composer-2.5", AgentClient::Cursor)
                .expect("acquire");
            let rt = tokio::runtime::Runtime::new().unwrap();
            let attach = rt.block_on(directory.inner().attach_snapshot(
                handle,
                "composer-2.5".to_string(),
                None,
            ));
            assert_eq!(attach.snapshot.model, "harness-default");
            assert_ne!(attach.snapshot.model, "composer-2.5");
            let (events, _) = directory.read_from(handle, 0);
            assert!(events.iter().any(|event| matches!(
                event,
                SessionServerEvent::Error { message }
                    if message.contains("session model") && message.contains("composer-2.5")
            )));
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

// Regression for #989: respawn fallback must shut down the live child before session/new.
#[test]
fn apply_client_message_set_config_option_respawns_dead_child_issue_989() {
    let dir = scratch_dir("set-model-respawn-transport-989");
    let handle = "web/set-model-respawn-989";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();
    let lock = dir.join(".fake-acp-exclusive-lock");
    let _ = std::fs::remove_file(&lock);

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(
            &[
                "--exclusive-session-new",
                "--cursor-models",
                "--cli-default-model",
            ],
            || {
                directory
                    .acquire(handle, &dir, "auto", AgentClient::Cursor)
                    .expect("acquire");
                let before = directory.child_id(handle).expect("child");
                directory.kill_host_for_test(handle);
                let mut generation = directory.generation(handle);
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(apply_client_message(
                    directory.inner(),
                    handle,
                    &dir,
                    model_config_message(
                        &ajax_core::adapters::cursor_catalog_to_acp_in_band_token(
                            "cursor-grok-4.6-high",
                        ),
                    ),
                    &mut generation,
                    None,
                ))
                .expect("set model");

                assert_ne!(directory.child_id(handle), Some(before));
                assert!(directory.inner().has_live_entry(handle));
            },
        );
    });

    let _ = std::fs::remove_dir_all(dir);
}

// Regression for #989: bridge harness respawn fallback also requires a lone stdio owner.
#[test]
fn apply_client_message_set_config_option_codex_respawns_dead_child_issue_989() {
    let dir = scratch_dir("set-model-codex-respawn-989");
    let handle = "web/set-model-codex-respawn-989";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();
    let lock = dir.join(".fake-acp-exclusive-lock");
    let _ = std::fs::remove_file(&lock);

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--exclusive-session-new"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Codex)
                .expect("acquire");
            let before = directory.child_id(handle).expect("child");
            directory.kill_host_for_test(handle);
            let mut generation = directory.generation(handle);
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(apply_client_message(
                directory.inner(),
                handle,
                &dir,
                model_config_message("composer-2.5"),
                &mut generation,
                None,
            ))
            .expect("codex set model");

            assert_ne!(directory.child_id(handle), Some(before));
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn swap_resets_context_only_for_cross_harness() {
    use super::model_change::swap_resets_harness_context;

    assert!(!swap_resets_harness_context(AgentClient::Cursor, "cursor"));
    assert!(!swap_resets_harness_context(AgentClient::Codex, "codex"));
    assert!(swap_resets_harness_context(AgentClient::Cursor, "claude"));
    assert!(swap_resets_harness_context(AgentClient::Codex, "pi"));
}

fn snapshot_option(snapshot: &SessionSnapshot, id: &str) -> serde_json::Value {
    snapshot
        .session_config_options
        .as_ref()
        .expect("snapshot advertised config options")
        .iter()
        .find(|option| option.id == id)
        .map(|option| option.current_value.clone())
        .unwrap_or(serde_json::Value::Null)
}

fn events_contain_text(events: &[SessionServerEvent], needle: &str) -> bool {
    events.iter().any(|event| match event {
        SessionServerEvent::Message { text, .. } => text.contains(needle),
        _ => false,
    })
}

/// Fixture-backed product path: create pin → prompt nonce → live axes →
/// snapshot → persist → reconnect → restart → cross-harness reset.
#[test]
fn product_flow_create_live_switch_reload_and_cross_harness() {
    let dir = scratch_dir("product-flow");
    let handle = "web/product-flow";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();
    let nonce = format!("nonce-prove-task-model-mvp-{}", std::process::id());
    let create_pin = "grok-4.6|effort=medium|fast=false";
    let persisted = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));
    let persisted_for_closure = std::sync::Arc::clone(&persisted);

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--cursor-parameterized-models"], || {
            directory
                .acquire(handle, &dir, create_pin, AgentClient::Cursor)
                .expect("create acquire");
            let child_live = directory.child_id(handle).expect("child");
            let session_live =
                crate::adapters::web_session_store::load::<SessionServerEvent>(&dir, handle)
                    .acp_session_id;
            let rt = tokio::runtime::Runtime::new().unwrap();
            let created = rt.block_on(directory.inner().attach_snapshot(
                handle,
                create_pin.to_string(),
                None,
            ));
            assert_eq!(
                snapshot_option(&created.snapshot, "model"),
                serde_json::json!("grok-4.6")
            );
            assert_eq!(
                snapshot_option(&created.snapshot, "effort"),
                serde_json::json!("medium")
            );
            assert_eq!(
                snapshot_option(&created.snapshot, "fast"),
                serde_json::json!(false)
            );
            eprintln!(
                "PROOF create pin={create_pin} child={child_live} session_present={}",
                session_live.is_some()
            );

            let mut generation = directory.generation(handle);
            rt.block_on(apply_client_message(
                directory.inner(),
                handle,
                &dir,
                SessionClientMessage::Prompt {
                    text: nonce.clone(),
                    content_blocks: vec![],
                    client_message_id: "prompt-nonce".to_string(),
                },
                &mut generation,
                None,
            ))
            .expect("prompt nonce");
            directory.pump(handle);
            let (after_prompt, _) = directory.read_from(handle, 0);
            assert!(
                events_contain_text(&after_prompt, &nonce),
                "prompt nonce missing from transcript: {after_prompt:?}"
            );
            eprintln!("PROOF prompt nonce={nonce}");

            let persist: super::PersistSessionModel = std::sync::Arc::new(move |model: &str| {
                *persisted_for_closure.lock().unwrap() = Some(model.to_string());
                Ok(())
            });
            rt.block_on(apply_client_message(
                directory.inner(),
                handle,
                &dir,
                SessionClientMessage::SetConfigOption {
                    config_id: "effort".to_string(),
                    value: SessionConfigValue::Select("low".to_string()),
                },
                &mut generation,
                Some(std::sync::Arc::clone(&persist)),
            ))
            .expect("live effort");
            rt.block_on(apply_client_message(
                directory.inner(),
                handle,
                &dir,
                SessionClientMessage::SetConfigOption {
                    config_id: "fast".to_string(),
                    value: SessionConfigValue::Boolean(true),
                },
                &mut generation,
                Some(std::sync::Arc::clone(&persist)),
            ))
            .expect("live fast");
            rt.block_on(apply_client_message(
                directory.inner(),
                handle,
                &dir,
                SessionClientMessage::SetConfigOption {
                    config_id: "model".to_string(),
                    value: SessionConfigValue::Select("gpt-5.6-sol".to_string()),
                },
                &mut generation,
                Some(persist),
            ))
            .expect("live model");

            assert_eq!(directory.child_id(handle), Some(child_live));
            let session_after =
                crate::adapters::web_session_store::load::<SessionServerEvent>(&dir, handle)
                    .acp_session_id;
            assert_eq!(session_after, session_live);
            let expected_pin = "gpt-5.6-sol|effort=low|fast=true";
            assert_eq!(persisted.lock().unwrap().as_deref(), Some(expected_pin));
            let live = rt.block_on(directory.inner().attach_snapshot(
                handle,
                expected_pin.to_string(),
                None,
            ));
            assert_eq!(
                snapshot_option(&live.snapshot, "model"),
                serde_json::json!("gpt-5.6-sol")
            );
            assert_eq!(
                snapshot_option(&live.snapshot, "effort"),
                serde_json::json!("low")
            );
            assert_eq!(
                snapshot_option(&live.snapshot, "fast"),
                serde_json::json!(true)
            );
            let (after_live, _) = directory.read_from(handle, 0);
            assert!(events_contain_text(&after_live, &nonce));
            eprintln!("PROOF live pin={expected_pin} same_child=true same_session=true");

            directory.kill_host_for_test(handle);
            directory
                .acquire(handle, &dir, expected_pin, AgentClient::Cursor)
                .expect("reload acquire");
            let restarted = rt.block_on(directory.inner().attach_snapshot(
                handle,
                expected_pin.to_string(),
                None,
            ));
            assert_eq!(
                snapshot_option(&restarted.snapshot, "model"),
                serde_json::json!("gpt-5.6-sol")
            );
            assert_eq!(
                snapshot_option(&restarted.snapshot, "effort"),
                serde_json::json!("low")
            );
            assert_eq!(
                snapshot_option(&restarted.snapshot, "fast"),
                serde_json::json!(true)
            );
            let (after_reload, _) = directory.read_from(handle, 0);
            assert!(events_contain_text(&after_reload, &nonce));
            eprintln!("PROOF reload pin={expected_pin} nonce_retained=true");

            rt.block_on(directory.inner().reset_harness_context(
                handle,
                &dir,
                AgentClient::Claude,
                "auto",
            ))
            .expect("cross-harness reset");
            directory.pump(handle);
            let (after_swap, _) = directory.read_from(handle, 0);
            assert!(events_contain_text(&after_swap, &nonce));
            assert!(events_contain_text(
                &after_swap,
                "Client switched harness. Context reset."
            ));
            assert_ne!(directory.child_id(handle), Some(child_live));
            let mut generation = directory.generation(handle);
            rt.block_on(apply_client_message(
                directory.inner(),
                handle,
                &dir,
                SessionClientMessage::Prompt {
                    text: format!("{nonce}-after-swap"),
                    content_blocks: vec![],
                    client_message_id: "prompt-after-swap".to_string(),
                },
                &mut generation,
                None,
            ))
            .expect("prompt after swap");
            directory.pump(handle);
            let (after_new_prompt, _) = directory.read_from(handle, 0);
            assert!(events_contain_text(
                &after_new_prompt,
                &format!("{nonce}-after-swap")
            ));
            eprintln!("PROOF cross-harness nonce_retained=true new_prompt=true");
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn apply_client_message_clear_resets_context_and_keeps_transcript() {
    let dir = scratch_dir("clear-context");
    let handle = "web/clear-context";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        let child_before = directory.child_id(handle);
        let mut generation = directory.generation(handle);
        let generation_before = generation;
        let rt = tokio::runtime::Runtime::new().unwrap();
        let nonce = "before-clear-nonce";
        rt.block_on(apply_client_message(
            directory.inner(),
            handle,
            &dir,
            SessionClientMessage::Prompt {
                text: nonce.to_string(),
                content_blocks: vec![],
                client_message_id: "prompt-before-clear".to_string(),
            },
            &mut generation,
            None,
        ))
        .expect("prompt before clear");
        directory.pump(handle);

        rt.block_on(apply_client_message(
            directory.inner(),
            handle,
            &dir,
            SessionClientMessage::Clear,
            &mut generation,
            None,
        ))
        .expect("clear");
        directory.pump(handle);

        let (events, _) = directory.read_from(handle, 0);
        assert!(events_contain_text(&events, nonce));
        assert!(events_contain_text(&events, "Context cleared."));
        assert!(!events.iter().any(|event| matches!(
            event,
            SessionServerEvent::Message { role, text, .. }
                if role == "user" && text == "/clear"
        )));
        assert_ne!(directory.child_id(handle), child_before);
        assert!(generation > generation_before);

        rt.block_on(apply_client_message(
            directory.inner(),
            handle,
            &dir,
            SessionClientMessage::Prompt {
                text: format!("{nonce}-after-clear"),
                content_blocks: vec![],
                client_message_id: "prompt-after-clear".to_string(),
            },
            &mut generation,
            None,
        ))
        .expect("prompt after clear");
        directory.pump(handle);
        let (after_prompt, _) = directory.read_from(handle, 0);
        assert!(events_contain_text(
            &after_prompt,
            &format!("{nonce}-after-clear")
        ));
    });

    let _ = std::fs::remove_dir_all(dir);
}

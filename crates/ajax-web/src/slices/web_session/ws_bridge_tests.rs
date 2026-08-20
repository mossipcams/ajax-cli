use super::test_support::BlockingSessionDirectory;
use super::ws_bridge::{should_send_keepalive, MAX_SESSION_FRAME_BYTES, SESSION_PING_INTERVAL};
use super::{apply_client_message, SessionClientMessage, SessionServerEvent, TaskSessionDirectory};
use crate::adapters::web_session_acp::{with_test_acp_extra_args, with_test_acp_program};
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

#[test]
fn max_session_frame_bytes_is_256_kib() {
    assert_eq!(MAX_SESSION_FRAME_BYTES, 256 * 1024);
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

#[tokio::test]
async fn apply_client_message_rejects_invalid_model() {
    let directory = TaskSessionDirectory::new(std::env::temp_dir());
    let mut generation = 0;
    let error = apply_client_message(
        &directory,
        "web/fix-login",
        std::path::Path::new("/tmp"),
        SessionClientMessage::SetModel {
            model: "bad model".to_string(),
        },
        &mut generation,
        None,
    )
    .await
    .unwrap_err();
    assert!(error.contains("whitespace"));
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

// Regression for issue #931: in-session set_model must persist on the task
// before the host applies the pin in-band; persistence failure leaves the slot
// unchanged and returns a typed error.
#[test]
fn apply_client_message_set_model_persists_before_in_band_apply() {
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
            SessionClientMessage::SetModel {
                model: "composer-2.5".to_string(),
            },
            &mut generation,
            Some(persist),
        ))
        .expect("set model");

        assert_eq!(persisted.lock().unwrap().as_deref(), Some("composer-2.5"));
        assert_eq!(directory.child_id(handle), Some(before));
        let session_after =
            crate::adapters::web_session_store::load::<SessionServerEvent>(&dir, handle)
                .acp_session_id;
        assert_eq!(session_after, session_before);
        directory.pump(handle);
        let (events, _) = directory.read_from(handle, 0);
        assert!(events.iter().any(|event| matches!(
            event,
            SessionServerEvent::Message { text, .. }
                if text.contains("model:session/set_config_option:composer-2.5")
        )));
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn apply_client_message_set_model_leaves_child_unchanged_when_persist_fails() {
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
        let error = rt
            .block_on(apply_client_message(
                directory.inner(),
                handle,
                &dir,
                SessionClientMessage::SetModel {
                    model: "composer-2.5".to_string(),
                },
                &mut generation,
                Some(persist),
            ))
            .unwrap_err();
        assert!(error.contains("registry write failed"));
        assert_eq!(directory.child_id(handle), Some(before));
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn apply_client_message_set_model_surfaces_worker_stop_without_respawn_issue_962() {
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
                SessionClientMessage::SetModel {
                    model: "composer-2.5".to_string(),
                },
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

// Regression for issue #942: set_model must publish the applied model on attach
// and keep it after the next prompt without replacing the ACP child in-band.
#[test]
fn apply_client_message_set_model_keeps_host_model_after_prompt_issue_942() {
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
            SessionClientMessage::SetModel {
                model: "composer-2.5".to_string(),
            },
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
fn apply_client_message_set_model_grok_high_keeps_child_alive_issue_979() {
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
                SessionClientMessage::SetModel {
                    model: catalog_id.to_string(),
                },
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
fn apply_client_message_set_model_respawns_when_in_band_refused() {
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
                SessionClientMessage::SetModel {
                    model: "composer-2.5".to_string(),
                },
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
fn apply_client_message_set_model_respawn_shuts_down_live_child_before_session_new_issue_989() {
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
                let mut generation = directory.generation(handle);
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(apply_client_message(
                    directory.inner(),
                    handle,
                    &dir,
                    SessionClientMessage::SetModel {
                        model: "cursor-grok-4.6-high".to_string(),
                    },
                    &mut generation,
                    None,
                ))
                .expect("set model");

                assert_eq!(directory.child_id(handle), Some(before));
                assert!(directory.inner().has_live_entry(handle));
            },
        );
    });

    let _ = std::fs::remove_dir_all(dir);
}

// Regression for #989: bridge harness respawn fallback also requires a lone stdio owner.
#[test]
fn apply_client_message_set_model_codex_respawn_shuts_down_live_child_issue_989() {
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
            let mut generation = directory.generation(handle);
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(apply_client_message(
                directory.inner(),
                handle,
                &dir,
                SessionClientMessage::SetModel {
                    model: "composer-2.5".to_string(),
                },
                &mut generation,
                None,
            ))
            .expect("codex set model");

            assert_eq!(directory.child_id(handle), Some(before));
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

#[test]
fn apply_persisted_model_keeps_live_child_for_same_harness_swap() {
    use super::model_change::apply_persisted_model;

    let dir = scratch_dir("same-harness-apply-persisted");
    let handle = "web/same-harness-swap";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        let child_before = directory.child_id(handle).expect("child");
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(apply_persisted_model(
            directory.inner(),
            handle,
            &dir,
            Some("composer-2.5"),
        ))
        .expect("apply persisted model");

        assert_eq!(directory.child_id(handle), Some(child_before));
        assert!(directory.inner().has_live_entry(handle));
    });

    let _ = std::fs::remove_dir_all(dir);
}

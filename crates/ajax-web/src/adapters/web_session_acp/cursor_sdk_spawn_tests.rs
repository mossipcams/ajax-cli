use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::client::AcpSpawnError;
use super::cursor_sdk_spawn::*;

fn fake_sdk() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fake_cursor_sdk.mjs")
        .to_path_buf()
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_temp_dir() -> PathBuf {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("ajax-cursor-sdk-spawn-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp worktree");
    dir
}

#[test]
fn node_version_floor() {
    for ok in ["v22.13.0", "v22.23.3", "v24.0.1", "22.13.0\n"] {
        assert!(node_version_ok(ok), "{ok} should meet the floor");
    }
    for bad in ["v22.12.9", "v20.19.0", "", "garbage"] {
        assert!(!node_version_ok(bad), "{bad} should not meet the floor");
    }
}

#[test]
fn module_is_resolved_only_from_the_given_root() {
    let root = fresh_temp_dir();
    assert!(sdk_module_from_npm_root(&root).is_none());
    let expected = root
        .join("@cursor")
        .join("sdk")
        .join("dist")
        .join("esm")
        .join("index.js");
    std::fs::create_dir_all(expected.parent().expect("parent dir")).expect("mkdir parents");
    std::fs::write(&expected, "// fake sdk entry\n").expect("write fake module");
    assert_eq!(sdk_module_from_npm_root(&root), Some(expected));
}

#[test]
fn spawn_applies_a_legacy_pin() {
    let worktree = fresh_temp_dir();
    with_test_cursor_sdk_module(&fake_sdk(), || {
        let (client, report) = spawn_cursor_sdk(&worktree, "cursor-grok-4.6-low", None)
            .expect("spawn with a legacy pin");
        assert_eq!(report.applied_model, "grok-4.6|effort=low");
        assert!(report.model_apply_error.is_none());
        assert!(!report.resumed);
        let options = report.config_options.expect("config options advertised");
        assert!(!options.is_empty());
        client.shutdown();
    });
}

#[test]
fn spawn_applies_an_encoded_pin() {
    let worktree = fresh_temp_dir();
    with_test_cursor_sdk_module(&fake_sdk(), || {
        let (client, report) = spawn_cursor_sdk(&worktree, "grok-4.6|effort=low", None)
            .expect("spawn with an encoded pin");
        assert_eq!(report.applied_model, "grok-4.6|effort=low");
        client.shutdown();
    });
}

#[test]
fn spawn_reports_an_unknown_pin_without_failing() {
    let worktree = fresh_temp_dir();
    with_test_cursor_sdk_module(&fake_sdk(), || {
        let (client, report) =
            spawn_cursor_sdk(&worktree, "nope-model", None).expect("unknown pin does not fail");
        let error = report.model_apply_error.expect("pin error reported");
        assert!(error.contains("unknown model"), "{error}");
        assert_eq!(report.applied_model, "default");
        client.shutdown();
    });
}

#[test]
fn spawn_with_an_unspecified_pin_stays_on_default() {
    let worktree = fresh_temp_dir();
    with_test_cursor_sdk_module(&fake_sdk(), || {
        let (client, report) = spawn_cursor_sdk(&worktree, "", None).expect("unspecified pin");
        assert_eq!(report.applied_model, "default");
        assert!(report.model_apply_error.is_none());
        client.shutdown();
    });
}

#[test]
fn restore_of_a_missing_agent_fails_closed() {
    let worktree = fresh_temp_dir();
    with_test_cursor_sdk_module(&fake_sdk(), || {
        let result = spawn_cursor_sdk(&worktree, "", Some("agent-missing"));
        assert!(matches!(result, Err(AcpSpawnError::Restore(_))));
    });
}

#[test]
fn authentication_failures_name_the_fix() {
    let authed = spawn_error("Authentication required".to_string(), None);
    match authed {
        AcpSpawnError::Message(text) => {
            assert!(text.contains("Settings > Cursor"), "{text}");
            assert!(text.starts_with("Authentication required"), "{text}")
        }
        other => panic!("expected a message, got: {other:?}"),
    }
    let plain = spawn_error("boom".to_string(), None);
    match plain {
        AcpSpawnError::Message(text) => assert_eq!(text, "boom"),
        other => panic!("expected a message, got: {other:?}"),
    }
}

#[test]
fn spawn_report_advertises_image_prompts() {
    let worktree = fresh_temp_dir();
    with_test_cursor_sdk_module(&fake_sdk(), || {
        let (client, report) = spawn_cursor_sdk(&worktree, "cursor-grok-4.6-low", None)
            .expect("spawn with legacy pin");

        assert!(
            report.prompt_capabilities.image,
            "image prompt capability must be advertised"
        );
        client.shutdown();
    });
}

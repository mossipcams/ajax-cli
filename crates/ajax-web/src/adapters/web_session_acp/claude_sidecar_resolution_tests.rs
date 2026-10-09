//! Tests that the Claude Agent SDK sidecar never resolves a module itself.
//! Without `--sdk-module` it must fail closed with an error that only names the
//! flag (nothing is imported, no global root is consulted); its source must not
//! contain any package-import / global-root fallback machinery; and an explicit
//! fake override still loads and answers init successfully. Spawns a real
//! `node` child process behind [`JsonlProcess`] JSONL framing, mirroring the
//! style of [`claude_sidecar_override_tests`].

use std::path::{Path, PathBuf};
use std::sync::mpsc::TryRecvError;
use std::thread::sleep;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::jsonl_process::{JsonlProcess, JsonlRecord};

const STEP_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// The crate manifest directory.
fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// The real sidecar module under the crate.
fn sidecar_module() -> PathBuf {
    manifest_dir().join("sidecar/claude_sdk_sidecar.mjs")
}

/// Spawn `node <sidecar>` with no `--sdk-module`, using the crate directory as
/// the working directory (as a task worktree would see it). Nothing in that
/// environment should let the sidecar resolve an SDK by itself.
fn spawn_bare() -> JsonlProcess {
    let manifest = manifest_dir();
    let args: Vec<String> = vec![sidecar_module().to_string_lossy().into_owned()];
    JsonlProcess::spawn(Path::new("node"), &args, manifest)
        .expect("claude sdk sidecar must spawn (is node installed?)")
}

/// Spawn `node <sidecar> --sdk-module <fake>` from the crate directory.
fn spawn_with_fake_module() -> JsonlProcess {
    let manifest = manifest_dir();
    let args: Vec<String> = vec![
        sidecar_module().to_string_lossy().into_owned(),
        "--sdk-module".to_string(),
        manifest
            .join("tests/fixtures/fake_claude_sdk.mjs")
            .to_string_lossy()
            .into_owned(),
    ];
    JsonlProcess::spawn(Path::new("node"), &args, manifest)
        .expect("claude sdk sidecar must spawn (is node installed?)")
}

/// Send `init` and wait for the matching response record.
fn init_response(process: &mut JsonlProcess, cwd: &Path) -> Value {
    let id = process
        .send("init", json!({ "cwd": cwd.to_string_lossy() }))
        .unwrap_or_else(|error| panic!("send init: {error}"));

    let deadline = Instant::now() + STEP_TIMEOUT;
    loop {
        match process.try_recv() {
            Ok(JsonlRecord::Record(value)) => {
                if value.get("id").and_then(Value::as_str) == Some(id.as_str())
                    && value["type"] == "response"
                {
                    return value;
                }
            }
            Ok(JsonlRecord::Error(text)) => {
                panic!("sidecar protocol error while waiting for init: {text}")
            }
            Ok(JsonlRecord::Exited) => {
                panic!(
                    "sidecar exited before answering init; stderr: {}",
                    process.stderr_tail()
                )
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                panic!(
                    "record stream ended before init response; stderr: {}",
                    process.stderr_tail()
                )
            }
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for init; stderr: {}",
            process.stderr_tail()
        );
        sleep(POLL_INTERVAL);
    }
}

#[test]
fn bare_sidecar_refuses_to_resolve_the_sdk_itself() {
    let mut process = spawn_bare();
    let response = init_response(&mut process, manifest_dir());

    assert_eq!(
        response["success"], false,
        "init must fail closed: {response}"
    );
    let error = response["error"].as_str().unwrap_or_default();
    assert!(
        error.contains("--sdk-module"),
        "error must name the missing flag: {error}"
    );
    // Nothing is resolved by the sidecar itself, so none of the old fallbacks appear.
    assert!(
        !error.contains("package import"),
        "unexpected fallback in error: {error}"
    );
    assert!(
        !error.contains("npm root"),
        "unexpected fallback in error: {error}"
    );
    assert!(
        !error.contains("claude-agent-acp"),
        "unexpected fallback in error: {error}"
    );
}

#[test]
fn sidecar_source_has_no_sdk_resolution_machinery() {
    let source = include_str!("../../../sidecar/claude_sdk_sidecar.mjs");
    // Forbidden substrings built from split literals so this test file is not a grep hit.
    let exec_file_sync = concat!("execFile", "Sync");
    let npm = concat!("n", "pm");
    let package_import_prefix = concat!("import(", "\"@", "anthropic-", "ai");
    assert!(
        !source.contains(exec_file_sync),
        "sidecar must not shell out for a module root"
    );
    assert!(
        !source.contains(npm),
        "sidecar must never consult an npm root"
    );
    assert!(
        !source.contains(package_import_prefix),
        "sidecar must not import the SDK by bare package name"
    );
}

#[test]
fn explicit_fake_module_still_answers_init_success() {
    let mut process = spawn_with_fake_module();
    let response = init_response(&mut process, manifest_dir());

    assert_eq!(
        response["success"], true,
        "explicit override must load: {response}"
    );
}

//! Spawn path for the Cursor SDK transport: launch resolution, the Node
//! version floor, fail-closed restore, operator-pin application and the spawn
//! report. Mirrors `claude_sdk_spawn`.
//!
//! Credentials: the sidecar resolves the Cursor key itself. When
//! `CURSOR_API_KEY` is set in the ajax-web process environment, the spawn argv
//! carries it as `--api-key <value>` (env only; never read from config files
//! or task fields). Otherwise the sidecar falls back to the protected SDK
//! store `~/.cursor/sdk/auth.json` (0600). Only the non-secret source
//! (`env` or `store`) leaves the sidecar, and the key is never logged or
//! written to task state or JSONL.
// A later packet wires this module into the SessionClient variant.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::client::{AcpSpawnError, RestoreFailure, RestoreMethod, SpawnReport};
use super::cursor_sdk_client::CursorSdkClient;
use super::PromptCapabilityDescriptor;

const INSTALL_HINT: &str = "npm install -g @cursor/sdk";
/// Shown with an SDK failure when the host has no usable credential: neither
/// `CURSOR_API_KEY` in the ajax-web environment nor a stored SDK login. Kept
/// for store-less hosts; a set `CURSOR_API_KEY` takes precedence and is passed
/// on argv as `--api-key`.
const LOGIN_HINT: &str = "log in with the Cursor SDK as the ajax-web host user: run Cursor.auth.login() from @cursor/sdk (credentials in ~/.cursor/sdk/auth.json; renew every 90 days)";
/// `@cursor/sdk` requires Node.js 22.13; Claude's Node 20 floor is unaffected.
const MIN_NODE: (u32, u32) = (22, 13);

/// argv for `node <sidecar.mjs>` (tests) or appended after embedded launch args (prod).
/// `--api-key` is appended only when `api_key` is non-empty (the `CURSOR_API_KEY` env value).
pub(super) fn sidecar_args(sdk_module: Option<&str>, api_key: Option<&str>) -> Vec<String> {
    let mut args = vec![format!(
        "{}/sidecar/cursor_sdk_sidecar.mjs",
        env!("CARGO_MANIFEST_DIR")
    )];
    if let Some(module) = sdk_module {
        args.push("--sdk-module".to_string());
        args.push(module.to_string());
    }
    if let Some(key) = api_key.filter(|s| !s.is_empty()) {
        args.extend(["--api-key".to_string(), key.to_string()]);
    }
    args
}

/// The host key for the spawn argv: `CURSOR_API_KEY` from the process environment
/// only. Under `cfg(test)` a fixture key is returned instead so integration tests
/// stay hermetic and never depend on the developer's real environment.
fn cursor_api_key_from_env() -> Option<String> {
    if let Ok(key) = std::env::var("CURSOR_API_KEY") {
        if !key.is_empty() {
            return Some(key);
        }
    }
    #[cfg(test)]
    {
        return Some("ajax-test-fixture-cursor-api-key".to_string());
    }
    #[cfg(not(test))]
    {
        None
    }
}

/// Whether `node --version` output (e.g. `v22.13.0`) meets the Cursor SDK floor.
pub(super) fn node_version_ok(text: &str) -> bool {
    let mut parts = text.trim().trim_start_matches('v').split('.');
    let major = parts.next().and_then(|p| p.parse::<u32>().ok());
    let minor = parts.next().and_then(|p| p.parse::<u32>().ok());
    match (major, minor) {
        (Some(major), Some(minor)) => (major, minor) >= MIN_NODE,
        _ => false,
    }
}

fn node_version(program: &Path) -> Option<String> {
    let output = Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// The SDK entry point under a global npm root, when installed there.
pub(super) fn sdk_module_from_npm_root(root: &Path) -> Option<PathBuf> {
    let path = root
        .join("@cursor")
        .join("sdk")
        .join("dist")
        .join("esm")
        .join("index.js");
    path.is_file().then_some(path)
}

pub(super) fn cursor_launch() -> Result<(PathBuf, Vec<String>), AcpSpawnError> {
    #[cfg(test)]
    if let Some(module) = cursor_sdk_module_override() {
        let api_key = cursor_api_key_from_env();
        return Ok((
            PathBuf::from("node"),
            sidecar_args(Some(&module.to_string_lossy()), api_key.as_deref()),
        ));
    }
    let program = crate::adapters::program::resolve_program("node").ok_or_else(|| {
        AcpSpawnError::Message("node is not installed - install Node.js 22.13 or newer".to_string())
    })?;
    let version = node_version(&program).unwrap_or_default();
    if !node_version_ok(&version) {
        return Err(AcpSpawnError::Message(format!(
            "Cursor chat needs Node.js 22.13 or newer; found {}",
            if version.is_empty() {
                "an unknown version"
            } else {
                version.as_str()
            }
        )));
    }
    // Resolve the SDK only from the user's global npm root and load it as an
    // explicit absolute path, so a task worktree can never redirect it.
    let root = crate::adapters::program::global_npm_root().ok_or_else(|| {
        AcpSpawnError::Message(format!(
            "no loadable @cursor/sdk module: npm root -g failed - {INSTALL_HINT}"
        ))
    })?;
    let module = sdk_module_from_npm_root(&root).ok_or_else(|| {
        AcpSpawnError::Message(format!(
            "no loadable @cursor/sdk module under {} - {INSTALL_HINT}",
            root.display()
        ))
    })?;
    let mut args = super::cursor_sdk_client::sidecar_launch_args(Some(&module.to_string_lossy()));
    if let Some(key) = cursor_api_key_from_env() {
        args.extend(["--api-key".to_string(), key]);
    }
    Ok((program, args))
}

/// Classify a spawn failure: restore failures fail closed, authentication
/// failures name the fix, everything else passes through.
pub(super) fn spawn_error(text: String, resume_session_id: Option<&str>) -> AcpSpawnError {
    if let Some(id) = resume_session_id {
        if text.contains("not found") || text.contains("resumed a different session") {
            return AcpSpawnError::Restore(RestoreFailure::Rejected {
                session_id: id.to_string(),
                method: RestoreMethod::Resume,
                reason: text,
            });
        }
    }
    let lowered = text.to_ascii_lowercase();
    if lowered.contains("authentication")
        || lowered.contains("api key")
        || lowered.contains("unauthorized")
    {
        return AcpSpawnError::Message(format!("{text} - {LOGIN_HINT}"));
    }
    AcpSpawnError::Message(text)
}

pub(super) fn spawn_cursor_sdk(
    worktree_path: &Path,
    operator_pin: &str,
    resume_session_id: Option<&str>,
) -> Result<(CursorSdkClient, SpawnReport<super::NeutralConfigOption>), AcpSpawnError> {
    let (program, args) = cursor_launch()?;
    // Spawn on the default model, then apply the pin: an unknown pin is
    // reported without failing the session, as the Claude path does.
    let client = CursorSdkClient::spawn(
        &program,
        &args,
        worktree_path,
        None,
        resume_session_id,
        super::client::HANDSHAKE_TIMEOUT,
    )
    .map_err(|text| spawn_error(text, resume_session_id))?;
    let model_apply_error = apply_pin_to_cursor(&client, operator_pin);
    let report = SpawnReport::<super::NeutralConfigOption> {
        load_session_advertised: true,
        restore_advertised: true,
        close_advertised: false,
        resumed: resume_session_id.is_some(),
        applied_model: client.applied_model(),
        model_apply_error,
        config_options: Some(client.config_options()),
        prompt_capabilities: PromptCapabilityDescriptor {
            // Images are forwarded to the SDK; embedded context is not yet.
            image: true,
            embedded_context: false,
        },
    };
    Ok((client, report))
}

/// Apply a persisted pin in any of its historical encodings
/// (`grok-4.6|effort=high`, `cursor-grok-4.6-high`, `grok-4.6[effort=high]`).
/// Returns the first error; a parameter the model does not have is skipped.
pub(super) fn apply_pin_to_cursor(client: &CursorSdkClient, pin: &str) -> Option<String> {
    if super::is_unspecified_model(Some(pin)) {
        return None;
    }
    let intent = ajax_core::adapters::parse_cursor_model_intent(pin)?;
    if let Err(err) = client.set_model(&intent.base) {
        return Some(err);
    }
    let mut first_error = None;
    let flag = |value: bool| if value { "true" } else { "false" };
    let options = [
        ("effort", intent.effort.clone()),
        ("fast", intent.fast.map(|v| flag(v).to_string())),
        ("thinking", intent.thinking.map(|v| flag(v).to_string())),
    ];
    for (id, value) in options {
        let Some(value) = value else { continue };
        if let Err(err) = client.set_option(id, &value) {
            if !err.starts_with("unsupported Cursor option") {
                first_error.get_or_insert(err);
            }
        }
    }
    first_error
}

#[cfg(test)]
static TEST_CURSOR_SDK_MODULE: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

#[cfg(test)]
pub(super) fn cursor_sdk_module_override() -> Option<PathBuf> {
    TEST_CURSOR_SDK_MODULE.lock().unwrap().clone()
}

#[cfg(test)]
pub(crate) fn with_test_cursor_sdk_module<F, R>(sdk_module: &Path, f: F) -> R
where
    F: FnOnce() -> R,
{
    *TEST_CURSOR_SDK_MODULE.lock().unwrap() = Some(sdk_module.to_path_buf());
    let result = f();
    *TEST_CURSOR_SDK_MODULE.lock().unwrap() = None;
    result
}

#[cfg(test)]
mod sidecar_args_tests {
    use super::sidecar_args;

    #[test]
    fn sidecar_args_includes_api_key_flag_when_set() {
        let args = sidecar_args(Some("/fake/sdk.mjs"), Some("k"));
        assert_eq!(
            args,
            vec![
                format!(
                    "{}/sidecar/cursor_sdk_sidecar.mjs",
                    env!("CARGO_MANIFEST_DIR")
                ),
                "--sdk-module".to_string(),
                "/fake/sdk.mjs".to_string(),
                "--api-key".to_string(),
                "k".to_string(),
            ]
        );
    }

    #[test]
    fn sidecar_args_omits_api_key_when_unset_or_empty() {
        let base = vec![
            format!(
                "{}/sidecar/cursor_sdk_sidecar.mjs",
                env!("CARGO_MANIFEST_DIR")
            ),
            "--sdk-module".to_string(),
            "/fake/sdk.mjs".to_string(),
        ];
        assert_eq!(sidecar_args(Some("/fake/sdk.mjs"), None), base);
        assert_eq!(sidecar_args(Some("/fake/sdk.mjs"), Some("")), base);
        assert_eq!(
            sidecar_args(None, None),
            vec![format!(
                "{}/sidecar/cursor_sdk_sidecar.mjs",
                env!("CARGO_MANIFEST_DIR")
            )]
        );
    }
}

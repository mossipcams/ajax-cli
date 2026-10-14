//! Spawn path for the Claude Agent SDK transport: launch resolution, fail-closed restore, operator-pin application and the spawn report. Mirrors the Pi path in session_client.rs.
// A later packet wires this module into the SessionClient variant.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use super::claude_sdk_client::ClaudeSdkClient;
use super::client::{AcpSpawnError, RestoreFailure, RestoreMethod, SpawnReport};
use super::PromptCapabilityDescriptor;
/// Resolve the Node program and its sidecar spawn arguments.
pub(super) fn claude_launch() -> Result<(PathBuf, Vec<String>), AcpSpawnError> {
    #[cfg(test)]
    if let Some(module) = claude_sdk_module_override() {
        return Ok((
            PathBuf::from("node"),
            vec![
                format!(
                    "{}/sidecar/claude_sdk_sidecar.mjs",
                    env!("CARGO_MANIFEST_DIR")
                ),
                "--sdk-module".to_string(),
                module.to_string_lossy().into_owned(),
            ],
        ));
    }
    let program = crate::adapters::program::resolve_program("node").ok_or_else(|| {
        AcpSpawnError::Message("node is not installed - install Node.js 20 or newer".to_string())
    })?;
    // Resolve the SDK from a trusted location (the user's global npm root,
    // queried from the home directory) and load it only as an explicit
    // absolute path, so a repository .npmrc or node_modules in a task
    // worktree can never redirect module resolution.
    let root = crate::adapters::program::global_npm_root().ok_or_else(|| {
        AcpSpawnError::Message(
            "no loadable claude-agent-sdk module: npm root -g failed - npm install -g @anthropic-ai/claude-agent-sdk".into(),
        )
    })?;
    let module = sdk_module_from_npm_root(&root).ok_or_else(|| {
        AcpSpawnError::Message(format!(
            "no loadable claude-agent-sdk module under {} - npm install -g @anthropic-ai/claude-agent-sdk",
            root.display()
        ))
    })?;
    Ok((
        program,
        super::claude_sdk_client::sidecar_launch_args(Some(&module.to_string_lossy())),
    ))
}

/// The first existing Claude Agent SDK entry point under a global npm root:
/// the direct package location, then the copy nested under the ACP bundle.
/// Pure; returns None when neither exists.
pub(super) fn sdk_module_from_npm_root(root: &Path) -> Option<PathBuf> {
    const DIRECT: &[&str] = &["@anthropic-ai", "claude-agent-sdk", "sdk.mjs"];
    const NESTED: &[&str] = &[
        "@agentclientprotocol",
        "claude-agent-acp",
        "node_modules",
        "@anthropic-ai",
        "claude-agent-sdk",
        "sdk.mjs",
    ];
    for parts in [DIRECT, NESTED] {
        let mut path = root.to_path_buf();
        for part in parts.iter().copied() {
            path.push(part);
        }
        if path.is_file() {
            return Some(path);
        }
    }
    None
}
/// Claude Agent SDK spawn path. The `operator_pin` is applied after the
/// restore check and the report is built from the post-pin client; a pin error
/// never fails the spawn, it is surfaced as `model_apply_error`.
pub(super) fn spawn_claude_sdk(
    worktree_path: &Path,
    operator_pin: &str,
    resume_session_id: Option<&str>,
) -> Result<(ClaudeSdkClient, SpawnReport<super::NeutralConfigOption>), AcpSpawnError> {
    let (program, args) = claude_launch()?;
    let client = ClaudeSdkClient::spawn(
        &program,
        &args,
        worktree_path,
        None,
        resume_session_id,
        super::client::HANDSHAKE_TIMEOUT,
    )
    .map_err(|text| {
        // Fail closed on restore: never fall back to a fresh session.
        if let Some(id) = resume_session_id {
            if text.contains("session not found") || text.contains("resumed a different session") {
                return AcpSpawnError::Restore(RestoreFailure::Rejected {
                    session_id: id.to_string(),
                    method: RestoreMethod::Resume,
                    reason: text,
                });
            }
        }
        if text.contains("no loadable claude-agent-sdk module")
            && !text.contains("npm install -g @anthropic-ai/claude-agent-sdk")
        {
            return AcpSpawnError::Message(format!(
                "{text} - npm install -g @anthropic-ai/claude-agent-sdk"
            ));
        }
        AcpSpawnError::Message(text)
    })?;

    let model_apply_error = apply_pin_to_claude(&client, operator_pin);

    let report = SpawnReport::<super::NeutralConfigOption> {
        load_session_advertised: true,
        restore_advertised: true,
        close_advertised: false,
        resumed: resume_session_id.is_some(),
        applied_model: client.applied_model(),
        model_apply_error,
        config_options: Some(client.config_options()),
        prompt_capabilities: PromptCapabilityDescriptor {
            // ponytail: images are not forwarded yet.
            image: false,
            embedded_context: false,
        },
    };

    Ok((client, report))
}

/// Apply a persisted operator pin (`model|configId=value|...`) to a Claude Agent SDK
/// client. Returns the first error text, if any; a parse failure or an
/// unspecified model applies nothing. Errors never stop later steps from
/// applying. Unknown option ids are ignored.
pub(super) fn apply_pin_to_claude(client: &ClaudeSdkClient, pin: &str) -> Option<String> {
    let selection = ajax_core::adapters::parse_model_selection(pin)?;
    let mut first_error = None;
    if !super::is_unspecified_model(Some(&selection.model)) {
        if let Err(err) = client.set_model(&selection.model) {
            first_error.get_or_insert(err);
        }
    }
    for (id, value) in &selection.options {
        if id == "effort" {
            if let Err(err) = client.set_effort(value) {
                first_error.get_or_insert(err);
            }
        }
    }
    first_error
}

#[cfg(test)]
static TEST_CLAUDE_SDK_MODULE: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

#[cfg(test)]
pub(super) fn claude_sdk_module_override() -> Option<PathBuf> {
    TEST_CLAUDE_SDK_MODULE.lock().unwrap().clone()
}

/// Run `f` with the fake Claude Agent SDK module installed as `sdk_module`;
/// mirrors [`super::session_client::with_test_pi_program`].
#[cfg(test)]
pub(crate) fn with_test_claude_sdk_module<F, R>(sdk_module: &Path, f: F) -> R
where
    F: FnOnce() -> R,
{
    *TEST_CLAUDE_SDK_MODULE.lock().unwrap() = Some(sdk_module.to_path_buf());
    let result = f();
    *TEST_CLAUDE_SDK_MODULE.lock().unwrap() = None;
    result
}

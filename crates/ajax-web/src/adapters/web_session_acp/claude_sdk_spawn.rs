//! Spawn path for the Claude Agent SDK transport: launch resolution, fail-closed restore, operator-pin application and the spawn report. Mirrors the Pi path in session_client.rs.
// A later packet wires this module into the SessionClient variant.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use ajax_core::models::AgentClient;

use super::claude_sdk_client::ClaudeSdkClient;
use super::client::{AcpSpawnError, RestoreFailure, RestoreMethod, SpawnReport};
use super::PromptCapabilityDescriptor;

/// Whether `agent` spawns a Claude Agent SDK session.
///
/// Gated off in production: no agent takes the SDK path until the core launch
/// table supports it. Tests opt in with [`with_test_claude_sdk_module`].
pub(super) fn uses_claude_sdk(agent: AgentClient) -> bool {
    #[cfg(not(test))]
    {
        // ponytail: the core launch-table transport for Claude lands in a later packet; until then no agent takes the SDK path
        _ = agent;
        false
    }
    #[cfg(test)]
    {
        matches!(agent, AgentClient::Claude) && claude_sdk_module_override().is_some()
    }
}

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
    Ok((program, super::claude_sdk_client::sidecar_launch_args(None)))
}

/// Claude Agent SDK spawn path. The `operator_pin` is applied after the
/// restore check and the report is built from the post-pin client; a pin error
/// never fails the spawn, it is surfaced as `model_apply_error`.
pub(super) fn spawn_claude_sdk(
    worktree_path: &Path,
    operator_pin: &str,
    resume_session_id: Option<&str>,
) -> Result<(ClaudeSdkClient, SpawnReport), AcpSpawnError> {
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
        AcpSpawnError::Message(text)
    })?;

    let model_apply_error = apply_pin_to_claude(&client, operator_pin);

    let report = SpawnReport {
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
thread_local! {
    static TEST_CLAUDE_SDK_MODULE: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn claude_sdk_module_override() -> Option<PathBuf> {
    TEST_CLAUDE_SDK_MODULE.with(|slot| slot.borrow().clone())
}

/// Run `f` with the fake Claude Agent SDK module installed as `sdk_module`;
/// mirrors [`super::session_client::with_test_pi_program`].
#[cfg(test)]
pub(crate) fn with_test_claude_sdk_module<F, R>(sdk_module: &Path, f: F) -> R
where
    F: FnOnce() -> R,
{
    TEST_CLAUDE_SDK_MODULE.with(|slot| *slot.borrow_mut() = Some(sdk_module.to_path_buf()));
    let result = f();
    TEST_CLAUDE_SDK_MODULE.with(|slot| *slot.borrow_mut() = None);
    result
}

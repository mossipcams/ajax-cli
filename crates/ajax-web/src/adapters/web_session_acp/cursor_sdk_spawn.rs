#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::adapters::program::sandbox_enabled_for;

use super::client::{AcpSpawnError, RestoreFailure, RestoreMethod, SpawnReport};
use super::cursor_sdk_client::CursorSdkClient;
use super::PromptCapabilityDescriptor;

const INSTALL_HINT: &str = "npm install -g @cursor/sdk";
const LOGIN_HINT: &str = "Cursor rejected the SDK credential - sign in again from Settings > Cursor in Ajax Web, or set CURSOR_API_KEY for the ajax-web host";
const MIN_NODE: (u32, u32) = (22, 13);
pub(super) const CONTAINER_SDK_MODULE: &str = "/opt/ajax/cursor-sdk/dist/esm/index.js";

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

pub(super) fn sdk_module_from_npm_root(root: &Path) -> Option<PathBuf> {
    let path = root
        .join("@cursor")
        .join("sdk")
        .join("dist")
        .join("esm")
        .join("index.js");
    path.is_file().then_some(path)
}
fn resolve_node_and_sdk() -> Result<(PathBuf, PathBuf), AcpSpawnError> {
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
    Ok((program, module))
}
pub(super) fn cursor_launch(worktree: &Path) -> Result<(PathBuf, Vec<String>), AcpSpawnError> {
    cursor_launch_for(sandbox_enabled_for(worktree))
}

fn cursor_launch_for(sandboxed: bool) -> Result<(PathBuf, Vec<String>), AcpSpawnError> {
    if sandboxed {
        return Ok((
            PathBuf::from("node"),
            super::cursor_sdk_client::sidecar_launch_args(Some(CONTAINER_SDK_MODULE)),
        ));
    }
    #[cfg(test)]
    if let Some(module) = cursor_sdk_module_override() {
        return Ok((
            PathBuf::from("node"),
            vec![
                format!(
                    "{}/sidecar/cursor_sdk_sidecar.mjs",
                    env!("CARGO_MANIFEST_DIR")
                ),
                "--sdk-module".to_string(),
                module.to_string_lossy().into_owned(),
            ],
        ));
    }
    let (program, module) = resolve_node_and_sdk()?;
    Ok((
        program,
        super::cursor_sdk_client::sidecar_launch_args(Some(&module.to_string_lossy())),
    ))
}

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
pub(super) const LOGIN_SOURCE: &str = include_str!("../../../sidecar/cursor_sdk_login.mjs");

pub(super) fn cursor_login_launch(
    mode: &str,
    store_path: Option<&Path>,
) -> Result<(PathBuf, Vec<String>), AcpSpawnError> {
    #[cfg(test)]
    if let Some(module) = cursor_sdk_module_override() {
        let mut args = vec![
            format!(
                "{}/sidecar/cursor_sdk_login.mjs",
                env!("CARGO_MANIFEST_DIR")
            ),
            "--sdk-module".to_string(),
            module.to_string_lossy().into_owned(),
            "--mode".to_string(),
            mode.to_string(),
        ];
        if let Some(path) = store_path {
            args.push("--store-path".to_string());
            args.push(path.to_string_lossy().into_owned());
        }
        return Ok((PathBuf::from("node"), args));
    }
    let (_program, module) = resolve_node_and_sdk()?;
    let mut args =
        super::rpc_handshake::sidecar_launch_args(LOGIN_SOURCE, Some(&module.to_string_lossy()));
    args.push("--mode".to_string());
    args.push(mode.to_string());
    if let Some(path) = store_path {
        args.push("--store-path".to_string());
        args.push(path.to_string_lossy().into_owned());
    }
    Ok((_program, args))
}

pub(super) fn spawn_cursor_sdk(
    worktree_path: &Path,
    operator_pin: &str,
    resume_session_id: Option<&str>,
) -> Result<(CursorSdkClient, SpawnReport<super::NeutralConfigOption>), AcpSpawnError> {
    let (program, args) = cursor_launch(worktree_path)?;
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
            image: true,
            embedded_context: false,
        },
    };
    Ok((client, report))
}

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
static TEST_CURSOR_SDK_MODULE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
pub(super) fn cursor_sdk_module_override() -> Option<PathBuf> {
    TEST_CURSOR_SDK_MODULE.lock().unwrap().clone()
}

#[cfg(test)]
pub(crate) fn with_test_cursor_sdk_module<F, R>(sdk_module: &Path, f: F) -> R
where
    F: FnOnce() -> R,
{
    let _guard = TEST_CURSOR_SDK_MODULE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *TEST_CURSOR_SDK_MODULE.lock().unwrap() = Some(sdk_module.to_path_buf());
    let result = f();
    *TEST_CURSOR_SDK_MODULE.lock().unwrap() = None;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sandboxed_cursor_sidecar_runs_bare_node_against_the_image_sdk() {
        let (program, args) = cursor_launch_for(true).expect("docker launch needs no host sdk");

        assert_eq!(program, PathBuf::from("node"));
        assert!(args.ends_with(&[
            "--".to_string(),
            "--sdk-module".to_string(),
            CONTAINER_SDK_MODULE.to_string(),
        ]));
    }

    #[test]
    fn an_unsandboxed_task_launches_the_host_sdk_and_skips_the_docker_wrap() {
        use crate::adapters::program::{
            sandboxed_program, with_test_sandbox, with_test_unsandboxed, TEST_DOCKER,
        };

        let host_module = Path::new("/ajax-test-sdk/index.js");
        let unsandboxed = Path::new("/ajax-test-worktrees/unsandboxed");
        let sandboxed = Path::new("/ajax-test-worktrees/sandboxed");
        with_test_sandbox(true, || {
            with_test_unsandboxed(unsandboxed, || {
                with_test_cursor_sdk_module(host_module, || {
                    let (program, args) = cursor_launch(unsandboxed).expect("native launch");
                    assert!(args.contains(&host_module.to_string_lossy().into_owned()));
                    assert!(!args.contains(&CONTAINER_SDK_MODULE.to_string()));
                    let (launched, _) = sandboxed_program(&program, &args, unsandboxed, &[])
                        .expect("native launch needs no wrap");
                    assert_eq!(launched, program);

                    let (program, args) = cursor_launch(sandboxed).expect("sandboxed launch");
                    assert!(args.contains(&CONTAINER_SDK_MODULE.to_string()));
                    let (launched, _) = sandboxed_program(&program, &args, sandboxed, &[])
                        .expect("sandboxed launch is wrapped");
                    assert_eq!(launched, PathBuf::from(TEST_DOCKER));
                })
            })
        });
    }
}

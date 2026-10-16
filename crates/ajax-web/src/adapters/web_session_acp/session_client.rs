//! Unified session client for the web session slice.
//!
//! Wraps an ACP stdio client, a Pi RPC client, a Claude Agent SDK,
//! or a Cursor SDK
//! client behind exactly the
//! method set the web session slice already uses on `AcpStdioClient`.
//! Spawning routes through [`SessionClient::spawn_with_operator_pin`].

use std::path::{Path, PathBuf};
use std::time::Duration;

use agent_client_protocol::schema::v1::{ContentBlock, ResourceLink, TextContent};

use super::agent_event::{AgentEvent, NativeCommand, NativeImage};
use super::available_command_descriptors::AvailableCommandDescriptor;
use super::claude_sdk_client::ClaudeSdkClient;
use super::claude_sdk_spawn::{apply_pin_to_claude, spawn_claude_sdk};

use super::client::{
    AcpSpawnError, AcpStdioClient, RestoreFailure, RestoreMethod, SpawnReport, HANDSHAKE_TIMEOUT,
};
use super::cursor_sdk_client::CursorSdkClient;
use super::cursor_sdk_spawn::{apply_pin_to_cursor, spawn_cursor_sdk};
use super::pi_rpc_client::PiRpcClient;
use super::rpc_client::RpcClientCore;
use super::{
    ApplyModelOutcome, CancelOutcome, NeutralConfigOption, PromptCapabilityDescriptor,
    SessionConfigValue,
};

use ajax_core::models::AgentClient;

/// The spawn report shape the web session slice receives: ACP fields with a
/// harness-neutral [`NeutralConfigOption`] list.
pub type SessionSpawnReport = SpawnReport<NeutralConfigOption>;

/// The apply-config outcome shape the web session slice receives.
pub type SessionConfigOutcome = ApplyModelOutcome<NeutralConfigOption>;

/// One live session transport: ACP stdio, Pi RPC, Claude Agent SDK, or
/// Cursor Agent SDK.
pub enum SessionClient {
    Acp(AcpStdioClient),
    Pi(PiSession),
    Claude(ClaudeSession),
    Cursor(CursorSession),
}

/// State for the Pi RPC variant of [`SessionClient`].
pub struct PiSession {
    client: PiRpcClient,
    session_id: String,
    session_new_result: serde_json::Value,
}

/// State for the Claude Agent SDK variant of [`SessionClient`].
pub struct ClaudeSession {
    client: ClaudeSdkClient,
    session_id: String,
    session_new_result: serde_json::Value,
}

/// State for the Cursor SDK variant of [`SessionClient`].
pub struct CursorSession {
    client: CursorSdkClient,
    session_id: String,
    session_new_result: serde_json::Value,
}

/// Minimal event surface shared by the ACP stdio client and the unified
/// session client, so drain helpers can serve both.
pub trait AcpEventSource {
    fn session_new_result(&self) -> &serde_json::Value;
    fn poll_event(&self) -> Option<AgentEvent>;

    /// Slash commands newly advertised since the last drain. Native sources
    /// yield the handshake list exactly once (one-shot, like an event), so a
    /// repeated drain delivers no snapshot; empty for sources without a
    /// native command list.
    fn take_available_commands(&self) -> Vec<AvailableCommandDescriptor> {
        Vec::new()
    }
}

impl AcpEventSource for AcpStdioClient {
    fn session_new_result(&self) -> &serde_json::Value {
        self.session_new_result()
    }

    fn poll_event(&self) -> Option<AgentEvent> {
        self.poll_event().map(AgentEvent::Acp)
    }
}

/// Maps native harness commands onto the wire descriptor shape.
fn native_command_descriptors(commands: &[NativeCommand]) -> Vec<AvailableCommandDescriptor> {
    commands
        .iter()
        .map(|command| AvailableCommandDescriptor {
            name: command.name.clone(),
            description: command.description.clone(),
            input_hint: command.input_hint.clone(),
        })
        .collect()
}
impl AcpEventSource for SessionClient {
    fn session_new_result(&self) -> &serde_json::Value {
        SessionClient::session_new_result(self)
    }

    fn poll_event(&self) -> Option<AgentEvent> {
        SessionClient::poll_event(self)
    }

    fn take_available_commands(&self) -> Vec<AvailableCommandDescriptor> {
        match self.transport() {
            Transport::Acp(_) => Vec::new(),
            Transport::Native(core) => native_command_descriptors(&core.take_available_commands()),
        }
    }
}

/// The two shapes a transport takes for the methods every harness implements identically.
enum Transport<'a> {
    Acp(&'a AcpStdioClient),
    Native(&'a RpcClientCore),
}

enum TransportMut<'a> {
    Acp(&'a mut AcpStdioClient),
    Native(&'a RpcClientCore),
}

impl SessionClient {
    fn transport(&self) -> Transport<'_> {
        match self {
            SessionClient::Acp(client) => Transport::Acp(client),
            SessionClient::Pi(session) => Transport::Native(session.client.core()),
            SessionClient::Claude(session) => Transport::Native(session.client.core()),
            SessionClient::Cursor(session) => Transport::Native(session.client.core()),
        }
    }

    fn transport_mut(&mut self) -> TransportMut<'_> {
        match self {
            SessionClient::Acp(client) => TransportMut::Acp(client),
            SessionClient::Pi(session) => TransportMut::Native(session.client.core()),
            SessionClient::Claude(session) => TransportMut::Native(session.client.core()),
            SessionClient::Cursor(session) => TransportMut::Native(session.client.core()),
        }
    }

    pub fn from_acp(client: AcpStdioClient) -> Self {
        SessionClient::Acp(client)
    }

    pub fn from_pi(client: PiRpcClient) -> Self {
        let session_id = client.session_id();
        let session_new_result = serde_json::json!({ "sessionId": session_id });
        SessionClient::Pi(PiSession {
            client,
            session_id,
            session_new_result,
        })
    }

    pub fn from_claude(client: ClaudeSdkClient) -> Self {
        let session_id = client.session_id();
        let session_new_result = serde_json::json!({ "sessionId": session_id });
        SessionClient::Claude(ClaudeSession {
            client,
            session_id,
            session_new_result,
        })
    }

    pub fn from_cursor(client: CursorSdkClient) -> Self {
        let session_id = client.session_id();
        let session_new_result = serde_json::json!({ "sessionId": session_id });
        SessionClient::Cursor(CursorSession {
            client,
            session_id,
            session_new_result,
        })
    }

    /// Spawn the live session for `agent` at `worktree_path`, resuming
    /// `resume_session_id` when given. The transport is selected by
    /// `transport_for`: Cursor and Claude agents take their SDK paths,
    /// Pi agents take the Pi RPC path (Pi in production), and every other
    /// agent goes through the ACP stdio spawn unchanged.
    pub fn spawn_with_operator_pin(
        agent: AgentClient,
        worktree_path: &Path,
        operator_pin: &str,
        resume_session_id: Option<&str>,
    ) -> Result<(SessionClient, SessionSpawnReport), AcpSpawnError> {
        match transport_for(agent) {
            ajax_core::adapters::HarnessTransport::CursorSdk => {
                spawn_cursor_sdk(worktree_path, operator_pin, resume_session_id)
                    .map(|(client, report)| (SessionClient::from_cursor(client), report))
            }
            ajax_core::adapters::HarnessTransport::ClaudeSdk => {
                spawn_claude_sdk(worktree_path, operator_pin, resume_session_id)
                    .map(|(client, report)| (SessionClient::from_claude(client), report))
            }
            ajax_core::adapters::HarnessTransport::PiRpc => {
                SessionClient::spawn_pi_rpc(worktree_path, operator_pin, resume_session_id)
            }
            ajax_core::adapters::HarnessTransport::Acp => AcpStdioClient::spawn_with_operator_pin(
                agent,
                worktree_path,
                operator_pin,
                resume_session_id,
            )
            .map(|(client, report)| (SessionClient::from_acp(client), report.into_neutral())),
        }
    }

    /// Pi RPC spawn path. The `operator_pin` is applied after the restore
    /// check and the report is built from the post-pin client; a pin error
    /// never fails the spawn, it is surfaced as `model_apply_error`.
    fn spawn_pi_rpc(
        worktree_path: &Path,
        operator_pin: &str,
        resume_session_id: Option<&str>,
    ) -> Result<(SessionClient, SessionSpawnReport), AcpSpawnError> {
        let (program, extra_args) = pi_program_and_args()?;
        let client = PiRpcClient::spawn(
            &program,
            &extra_args,
            worktree_path,
            resume_session_id,
            HANDSHAKE_TIMEOUT,
        )
        .map_err(AcpSpawnError::Message)?;

        // Fail closed on restore: never fall back to a fresh session.
        if let Some(expected) = resume_session_id {
            let actual = client.session_id();
            if actual != expected {
                client.shutdown();
                return Err(AcpSpawnError::Restore(RestoreFailure::Rejected {
                    session_id: expected.to_string(),
                    method: RestoreMethod::Resume,
                    reason: format!("Pi started session {actual} instead of resuming {expected}"),
                }));
            }
        }

        let model_apply_error = apply_pin_to_pi(&client, operator_pin);

        let report = SessionSpawnReport {
            load_session_advertised: true,
            restore_advertised: true,
            close_advertised: false,
            resumed: resume_session_id.is_some(),
            applied_model: client.applied_model(),
            model_apply_error,
            config_options: Some(client.config_options()),
            prompt_capabilities: PromptCapabilityDescriptor {
                image: false,
                embedded_context: false,
            },
        };
        Ok((SessionClient::from_pi(client), report))
    }

    pub fn session_id(&self) -> &str {
        match self {
            SessionClient::Acp(client) => client.session_id(),
            SessionClient::Pi(session) => &session.session_id,
            SessionClient::Claude(session) => &session.session_id,
            SessionClient::Cursor(session) => &session.session_id,
        }
    }

    pub fn shutdown(&mut self) -> Option<String> {
        // RPC shutdown returns a session ID, not the optional error used by the slice.
        match self.transport_mut() {
            TransportMut::Acp(client) => return client.shutdown(),
            TransportMut::Native(core) => {
                core.shutdown();
            }
        }
        None
    }

    pub fn detach(&mut self) -> Option<String> {
        match self.transport_mut() {
            TransportMut::Acp(client) => client.detach(),
            // RPC transports have no detached-host semantics.
            TransportMut::Native(_) => self.shutdown(),
        }
    }

    pub fn host_exited(&mut self) -> bool {
        match self.transport_mut() {
            TransportMut::Acp(client) => client.host_exited(),
            TransportMut::Native(core) => core.host_exited(),
        }
    }

    #[cfg(test)]
    pub(crate) fn kill_host_for_test(&mut self) {
        // Native transports have no kill hook, so they fall back to shutdown.
        match self.transport_mut() {
            TransportMut::Acp(client) => client.kill_host_for_test(),
            TransportMut::Native(core) => {
                core.shutdown();
            }
        }
    }

    /// OS process id of the live agent child while the transport is alive. The
    /// ACP and Pi variants always report a pid; the Claude and Cursor SDK
    /// variants report `None` once their host has been shut down.
    /// `None` once its host has been shut down.
    pub fn child_pid(&self) -> Option<u32> {
        match self.transport() {
            Transport::Acp(client) => Some(client.child_id()),
            Transport::Native(core) => core.child_pid(),
        }
    }

    #[cfg(test)]
    pub(crate) fn child_id(&self) -> u32 {
        self.child_pid().unwrap_or(0)
    }

    pub fn poll_event(&self) -> Option<AgentEvent> {
        match self.transport() {
            Transport::Acp(client) => client.poll_event().map(AgentEvent::Acp),
            Transport::Native(core) => core.poll_event(),
        }
    }

    pub fn wait_event(&self, timeout: Duration) -> Option<AgentEvent> {
        match self.transport() {
            Transport::Acp(client) => client.wait_event(timeout).map(AgentEvent::Acp),
            Transport::Native(core) => core.wait_event(timeout),
        }
    }

    pub fn begin_prompt(&mut self, blocks: &[ContentBlock]) -> Result<u64, String> {
        match self {
            SessionClient::Acp(client) => client.begin_prompt(blocks),
            SessionClient::Pi(session) => session.client.begin_prompt(&prompt_text(blocks)),
            SessionClient::Claude(session) => session.client.begin_prompt(&prompt_text(blocks)),
            SessionClient::Cursor(session) => {
                let (text, images) = prompt_parts(blocks);
                session.client.begin_prompt_with_images(&text, &images)
            }
        }
    }

    pub(crate) fn cancel(&mut self) -> Result<CancelOutcome, String> {
        match self.transport_mut() {
            TransportMut::Acp(client) => return client.cancel(),
            TransportMut::Native(core) => {
                if core.prompt_in_flight() || core.host_exited() {
                    core.cancel()?;
                }
            }
        }
        // Releasing an idle live RPC host needs no abort; transport failures still propagate.
        Ok(CancelOutcome {
            permissions: vec![],
            elicitations: vec![],
        })
    }

    pub(crate) fn prompt_in_flight(&self) -> bool {
        match self.transport() {
            Transport::Acp(client) => client.prompt_in_flight(),
            Transport::Native(core) => core.prompt_in_flight(),
        }
    }

    pub fn respond_client_request(
        &mut self,
        id: &serde_json::Value,
        result: serde_json::Value,
    ) -> Result<(), String> {
        match self {
            SessionClient::Acp(client) => client.respond_client_request(id, result),
            // ponytail: follow up when Pi RPC needs request responses.
            SessionClient::Pi(_) => {
                Err("Pi RPC mode has no permission or elicitation requests".to_string())
            }
            SessionClient::Claude(_) => Err(
                "Claude SDK mode auto-approves permissions and declines elicitation".to_string(),
            ),
            SessionClient::Cursor(_) => {
                Err("Cursor SDK mode has no permission or elicitation requests".to_string())
            }
        }
    }

    pub fn respond_elicitation(
        &mut self,
        request_id: &str,
        action: &str,
        content: Option<&serde_json::Value>,
    ) -> Result<(), String> {
        match self {
            SessionClient::Acp(client) => {
                let action = super::sdk_elicitation::action_from_wire(action, content)?;
                client.respond_elicitation(request_id, action)
            }
            SessionClient::Pi(session) => {
                let fields = if action == "accept" {
                    let mut fields = serde_json::Map::new();
                    if let Some(serde_json::Value::Object(obj)) = content {
                        if let Some(confirmed) = obj.get("confirmed").filter(|v| v.is_boolean()) {
                            fields.insert("confirmed".to_string(), confirmed.clone());
                        }
                        if let Some(value) = obj.get("value").filter(|v| v.is_string()) {
                            fields.insert("value".to_string(), value.clone());
                        }
                    }
                    if fields.is_empty() {
                        return Err("Pi dialog answer needs `confirmed` or `value`".to_string());
                    }
                    serde_json::Value::Object(fields)
                } else {
                    serde_json::json!({ "cancelled": true })
                };
                session.client.respond_dialog(request_id, fields)
            }
            SessionClient::Claude(_) => Err(
                "Claude SDK mode auto-approves permissions and declines elicitation".to_string(),
            ),
            SessionClient::Cursor(_) => {
                Err("Cursor SDK mode has no permission or elicitation requests".to_string())
            }
        }
    }

    pub fn session_new_result(&self) -> &serde_json::Value {
        match self {
            SessionClient::Acp(client) => client.session_new_result(),
            SessionClient::Pi(session) => &session.session_new_result,
            SessionClient::Claude(session) => &session.session_new_result,
            SessionClient::Cursor(session) => &session.session_new_result,
        }
    }

    pub fn apply_model_pin(&self, desired_model: &str) -> Result<SessionConfigOutcome, String> {
        match self {
            SessionClient::Acp(client) => client
                .apply_model_pin(desired_model)
                .map(ApplyModelOutcome::into_neutral),
            SessionClient::Claude(session) => {
                let error = apply_pin_to_claude(&session.client, desired_model);
                Ok(SessionConfigOutcome {
                    applied_model: session.client.applied_model(),
                    config_options: Some(session.client.config_options()),
                    error,
                })
            }

            SessionClient::Cursor(session) => {
                let error = apply_pin_to_cursor(&session.client, desired_model);
                Ok(SessionConfigOutcome {
                    applied_model: session.client.applied_model(),
                    config_options: Some(session.client.config_options()),
                    error,
                })
            }
            SessionClient::Pi(session) => {
                let error = apply_pin_to_pi(&session.client, desired_model);
                Ok(SessionConfigOutcome {
                    applied_model: session.client.applied_model(),
                    config_options: Some(session.client.config_options()),
                    error,
                })
            }
        }
    }

    pub fn apply_config_option(
        &self,
        config_id: &str,
        value: SessionConfigValue,
    ) -> Result<SessionConfigOutcome, String> {
        match self {
            SessionClient::Acp(client) => client
                .apply_config_option(config_id, super::wire_value_to_session_value(value))
                .map(ApplyModelOutcome::into_neutral),
            SessionClient::Claude(session) => {
                let error = match (config_id, value) {
                    ("model", SessionConfigValue::Select(value)) => {
                        session.client.set_model(&value).err()
                    }
                    ("effort", SessionConfigValue::Select(value)) => {
                        session.client.set_effort(&value).err()
                    }
                    _ => Some(format!("unsupported Claude option {config_id}")),
                };
                Ok(SessionConfigOutcome {
                    applied_model: session.client.applied_model(),
                    config_options: Some(session.client.config_options()),
                    error,
                })
            }

            SessionClient::Cursor(session) => {
                let error = match (config_id, value) {
                    ("model", SessionConfigValue::Select(value)) => {
                        session.client.set_model(&value).err()
                    }
                    ("mode", SessionConfigValue::Select(value)) => {
                        session.client.set_mode(&value).err()
                    }
                    // Every other select is a parameter of the current model (effort, fast, ...).
                    (id, SessionConfigValue::Select(value)) => {
                        session.client.set_option(id, &value).err()
                    }
                    _ => Some(format!("unsupported Cursor option {config_id}")),
                };
                Ok(SessionConfigOutcome {
                    applied_model: session.client.applied_model(),
                    config_options: Some(session.client.config_options()),
                    error,
                })
            }
            SessionClient::Pi(session) => {
                let error = match (config_id, value) {
                    ("model", SessionConfigValue::Select(value)) => {
                        session.client.set_model(&value).err()
                    }
                    ("thought_level", SessionConfigValue::Select(value)) => {
                        session.client.set_thinking_level(&value).err()
                    }
                    _ => Some(format!("unsupported Pi option {config_id}")),
                };
                Ok(SessionConfigOutcome {
                    applied_model: session.client.applied_model(),
                    config_options: Some(session.client.config_options()),
                    error,
                })
            }
        }
    }
}

/// The transport `agent` spawns on, from the core launch table.
///
/// Under test a native transport is selected only while its program/module
/// override is installed, so ordinary tests keep the ACP path.
pub(super) fn transport_for(agent: AgentClient) -> ajax_core::adapters::HarnessTransport {
    let transport = ajax_core::adapters::acp_launch_for_agent(agent)
        .map_or(ajax_core::adapters::HarnessTransport::Acp, |launch| {
            launch.transport
        });
    #[cfg(test)]
    {
        let installed = match transport {
            ajax_core::adapters::HarnessTransport::Acp => true,
            ajax_core::adapters::HarnessTransport::PiRpc => pi_program_override().is_some(),
            ajax_core::adapters::HarnessTransport::ClaudeSdk => {
                super::claude_sdk_spawn::claude_sdk_module_override().is_some()
            }
            ajax_core::adapters::HarnessTransport::CursorSdk => {
                super::cursor_sdk_spawn::cursor_sdk_module_override().is_some()
            }
        };
        if !installed {
            return ajax_core::adapters::HarnessTransport::Acp;
        }
    }
    transport
}

/// Apply a persisted operator pin (`model|configId=value|...`) to a Pi RPC
/// client. Returns the first error text, if any; a parse failure or an
/// unspecified model applies nothing. Errors never stop later steps from
/// applying.
fn apply_pin_to_pi(client: &PiRpcClient, pin: &str) -> Option<String> {
    let selection = ajax_core::adapters::parse_model_selection(pin)?;
    let mut first_error = None;
    if !super::is_unspecified_model(Some(&selection.model)) {
        if let Err(err) = client.set_model(&selection.model) {
            first_error.get_or_insert(err);
        }
    }
    for (id, value) in &selection.options {
        if id == "thought_level" {
            if let Err(err) = client.set_thinking_level(value) {
                first_error.get_or_insert(err);
            }
        }
    }
    first_error
}

/// Resolve the Pi program and its extra spawn arguments.
pub(super) fn pi_program_and_args() -> Result<(PathBuf, Vec<String>), AcpSpawnError> {
    #[cfg(test)]
    if let Some(pair) = pi_program_override() {
        return Ok(pair);
    }
    let program = crate::adapters::program::resolve_program("pi").ok_or_else(|| {
        AcpSpawnError::Message(
            "pi is not installed - npm install -g @earendil-works/pi-coding-agent".to_string(),
        )
    })?;
    Ok((program, Vec::new()))
}

#[cfg(test)]
static TEST_PI_PROGRAM: std::sync::Mutex<Option<(PathBuf, Vec<String>)>> =
    std::sync::Mutex::new(None);

#[cfg(test)]
fn pi_program_override() -> Option<(PathBuf, Vec<String>)> {
    TEST_PI_PROGRAM.lock().unwrap().clone()
}

/// Run `f` with the Pi program override installed as `path` plus
/// `extra_args`; mirrors [`super::client::with_test_acp_program`].
#[cfg(test)]
pub(crate) fn with_test_pi_program<F, R>(path: &Path, extra_args: &[&str], f: F) -> R
where
    F: FnOnce() -> R,
{
    let args: Vec<String> = extra_args.iter().map(|arg| arg.to_string()).collect();
    *TEST_PI_PROGRAM.lock().unwrap() = Some((path.to_path_buf(), args));
    let result = f();
    *TEST_PI_PROGRAM.lock().unwrap() = None;
    result
}

/// Split ACP content blocks into the plain-text prompt and its images, for
/// transports that accept images natively.
pub(super) fn prompt_parts(blocks: &[ContentBlock]) -> (String, Vec<NativeImage>) {
    let mut parts: Vec<String> = Vec::new();
    let mut images = Vec::new();
    for block in blocks {
        match block {
            ContentBlock::Text(tc) => parts.push(tc.text.clone()),
            ContentBlock::ResourceLink(rl) => parts.push(rl.uri.clone()),
            ContentBlock::Image(image) => images.push(NativeImage {
                data: image.data.clone(),
                mime_type: image.mime_type.clone(),
            }),
            // ponytail: audio and embedded resources are not forwarded yet.
            _ => parts.push("[attachment omitted]".to_string()),
        }
    }
    (parts.join("\n\n"), images)
}

/// Flatten ACP content blocks into a single plain-text prompt.
pub(super) fn prompt_text(blocks: &[ContentBlock]) -> String {
    let mut parts: Vec<String> = Vec::new();

    for block in blocks {
        match block {
            ContentBlock::Text(tc @ TextContent { .. }) => {
                parts.push(tc.text.clone());
            }
            ContentBlock::ResourceLink(rl @ ResourceLink { .. }) => {
                parts.push(rl.uri.clone());
            }
            // ponytail: images are not forwarded yet — only a placeholder is emitted
            _ => {
                parts.push("[image omitted]".to_string());
            }
        }
    }

    parts.join("\n\n")
}

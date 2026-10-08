//! Unified session client for the web session slice.
//!
//! Wraps either an ACP stdio client or a Pi RPC client behind exactly the
//! method set the web session slice already uses on `AcpStdioClient`.
//! Spawning routes through [`SessionClient::spawn_with_operator_pin`].

#![allow(dead_code)] // not every method has a call site yet

#[cfg(test)]
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    ContentBlock, ElicitationAction, SessionConfigOptionValue,
};

use super::client::{
    AcpClientEvent, AcpSpawnError, AcpStdioClient, RestoreFailure, RestoreMethod, SpawnReport,
    HANDSHAKE_TIMEOUT,
};
use super::pi_rpc_client::PiRpcClient;
use super::{ApplyModelOutcome, CancelOutcome, PromptCapabilityDescriptor};

use ajax_core::models::AgentClient;

/// One live session transport: ACP stdio or Pi RPC.
pub enum SessionClient {
    Acp(AcpStdioClient),
    Pi(PiSession),
}

/// State for the Pi RPC variant of [`SessionClient`].
pub struct PiSession {
    client: PiRpcClient,
    session_id: String,
    session_new_result: serde_json::Value,
}

/// Minimal event surface shared by the ACP stdio client and the unified
/// session client, so drain helpers can serve both.
pub trait AcpEventSource {
    fn session_new_result(&self) -> &serde_json::Value;
    fn poll_event(&self) -> Option<AcpClientEvent>;
}

impl AcpEventSource for AcpStdioClient {
    fn session_new_result(&self) -> &serde_json::Value {
        self.session_new_result()
    }

    fn poll_event(&self) -> Option<AcpClientEvent> {
        self.poll_event()
    }
}

impl AcpEventSource for SessionClient {
    fn session_new_result(&self) -> &serde_json::Value {
        SessionClient::session_new_result(self)
    }

    fn poll_event(&self) -> Option<AcpClientEvent> {
        SessionClient::poll_event(self)
    }
}
impl SessionClient {
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

    /// Spawn the live session for `agent` at `worktree_path`, resuming
    /// `resume_session_id` when given. Agents selected for Pi RPC by the core
    /// launch table take the Pi path (Pi in production); every other agent goes
    /// through the ACP stdio spawn unchanged.
    pub fn spawn_with_operator_pin(
        agent: AgentClient,
        worktree_path: &Path,
        operator_pin: &str,
        resume_session_id: Option<&str>,
    ) -> Result<(SessionClient, SpawnReport), AcpSpawnError> {
        if uses_pi_rpc(agent) {
            SessionClient::spawn_pi_rpc(worktree_path, operator_pin, resume_session_id)
        } else {
            AcpStdioClient::spawn_with_operator_pin(
                agent,
                worktree_path,
                operator_pin,
                resume_session_id,
            )
            .map(|(client, report)| (SessionClient::from_acp(client), report))
        }
    }

    /// Pi RPC spawn path. The `operator_pin` is applied after the restore
    /// check and the report is built from the post-pin client; a pin error
    /// never fails the spawn, it is surfaced as `model_apply_error`.
    fn spawn_pi_rpc(
        worktree_path: &Path,
        operator_pin: &str,
        resume_session_id: Option<&str>,
    ) -> Result<(SessionClient, SpawnReport), AcpSpawnError> {
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

        let report = SpawnReport {
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
        }
    }

    pub fn shutdown(&mut self) -> Option<String> {
        match self {
            SessionClient::Acp(client) => client.shutdown(),
            SessionClient::Pi(session) => session.client.shutdown(),
        }
    }

    pub fn detach(&mut self) -> Option<String> {
        match self {
            SessionClient::Acp(client) => client.detach(),
            // Pi RPC has no detached-host semantics; shutting down is the only exit.
            SessionClient::Pi(session) => session.client.shutdown(),
        }
    }

    pub fn host_exited(&mut self) -> bool {
        match self {
            SessionClient::Acp(client) => client.host_exited(),
            SessionClient::Pi(session) => session.client.host_exited(),
        }
    }

    #[cfg(test)]
    pub(crate) fn kill_host_for_test(&mut self) {
        match self {
            SessionClient::Acp(client) => client.kill_host_for_test(),
            // ponytail: PiRpcClient has no kill hook yet; use shutdown.
            SessionClient::Pi(session) => {
                session.client.shutdown();
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn child_id(&self) -> u32 {
        match self {
            SessionClient::Acp(client) => client.child_id(),
            // ponytail: PiRpcClient exposes no pid yet; follow up to surface one.
            SessionClient::Pi(_) => 0,
        }
    }

    pub fn poll_event(&self) -> Option<AcpClientEvent> {
        match self {
            SessionClient::Acp(client) => client.poll_event(),
            SessionClient::Pi(session) => session.client.poll_event(),
        }
    }

    pub fn wait_event(&self, timeout: Duration) -> Option<AcpClientEvent> {
        match self {
            SessionClient::Acp(client) => client.wait_event(timeout),
            SessionClient::Pi(session) => session.client.wait_event(timeout),
        }
    }

    pub fn begin_prompt(&mut self, blocks: &[ContentBlock]) -> Result<u64, String> {
        match self {
            SessionClient::Acp(client) => client.begin_prompt(blocks),
            SessionClient::Pi(session) => session.client.begin_prompt(blocks),
        }
    }

    pub(crate) fn cancel(&mut self) -> Result<CancelOutcome, String> {
        match self {
            SessionClient::Acp(client) => client.cancel(),
            SessionClient::Pi(session) => session.client.cancel().map(|()| CancelOutcome {
                permissions: vec![],
                elicitations: vec![],
            }),
        }
    }

    pub(crate) fn prompt_in_flight(&self) -> bool {
        match self {
            SessionClient::Acp(client) => client.prompt_in_flight(),
            SessionClient::Pi(session) => session.client.prompt_in_flight(),
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
        }
    }

    pub fn respond_elicitation(
        &mut self,
        request_id: &str,
        action: ElicitationAction,
    ) -> Result<(), String> {
        match self {
            SessionClient::Acp(client) => client.respond_elicitation(request_id, action),
            // ponytail: follow up when Pi RPC needs elicitation responses.
            SessionClient::Pi(_) => {
                Err("Pi RPC mode has no permission or elicitation requests".to_string())
            }
        }
    }

    pub fn session_new_result(&self) -> &serde_json::Value {
        match self {
            SessionClient::Acp(client) => client.session_new_result(),
            SessionClient::Pi(session) => &session.session_new_result,
        }
    }

    pub fn apply_model_pin(&self, desired_model: &str) -> Result<ApplyModelOutcome, String> {
        match self {
            SessionClient::Acp(client) => client.apply_model_pin(desired_model),
            SessionClient::Pi(session) => {
                let error = apply_pin_to_pi(&session.client, desired_model);
                Ok(ApplyModelOutcome {
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
        value: SessionConfigOptionValue,
    ) -> Result<ApplyModelOutcome, String> {
        match self {
            SessionClient::Acp(client) => client.apply_config_option(config_id, value),
            SessionClient::Pi(session) => {
                let error = match (config_id, &value) {
                    ("model", SessionConfigOptionValue::ValueId { value }) => {
                        let model = value.0.to_string();
                        session.client.set_model(&model).err()
                    }
                    ("thought_level", SessionConfigOptionValue::ValueId { value }) => {
                        let level = value.0.to_string();
                        session.client.set_thinking_level(&level).err()
                    }
                    _ => Some(format!("unsupported Pi option {config_id}")),
                };
                Ok(ApplyModelOutcome {
                    applied_model: session.client.applied_model(),
                    config_options: Some(session.client.config_options()),
                    error,
                })
            }
        }
    }
}

/// Whether `agent` spawns a Pi RPC session.
///
/// The selector follows the core launch table: an agent whose `AcpLaunch`
/// transport is `PiRpc` takes the Pi path (Pi in production), and every other
/// agent stays on the ACP stdio spawn.
pub(super) fn uses_pi_rpc(agent: AgentClient) -> bool {
    let pi_rpc = ajax_core::adapters::acp_launch_for_agent(agent)
        .is_some_and(|launch| launch.transport == ajax_core::adapters::HarnessTransport::PiRpc);
    #[cfg(not(test))]
    {
        pi_rpc
    }
    #[cfg(test)]
    {
        pi_rpc && pi_program_override().is_some()
    }
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
thread_local! {
    static TEST_PI_PROGRAM: RefCell<Option<(PathBuf, Vec<String>)>> = const { RefCell::new(None) };
}

#[cfg(test)]
fn pi_program_override() -> Option<(PathBuf, Vec<String>)> {
    TEST_PI_PROGRAM.with(|slot| slot.borrow().clone())
}

/// Run `f` with the Pi program override installed as `path` plus
/// `extra_args`; mirrors [`super::client::with_test_acp_program`].
#[cfg(test)]
pub(crate) fn with_test_pi_program<F, R>(path: &Path, extra_args: &[&str], f: F) -> R
where
    F: FnOnce() -> R,
{
    let args: Vec<String> = extra_args.iter().map(|arg| arg.to_string()).collect();
    TEST_PI_PROGRAM.with(|slot| *slot.borrow_mut() = Some((path.to_path_buf(), args)));
    let result = f();
    TEST_PI_PROGRAM.with(|slot| *slot.borrow_mut() = None);
    result
}

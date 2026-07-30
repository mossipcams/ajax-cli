use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime},
};

use agent_client_protocol::{
    schema::{v2, MaybeUndefined, ProtocolVersion},
    AcpAgent, AcpAgentConfig, Agent, Client, ConnectTo, ConnectionTo, ErrorCode,
};
use ajax_core::acp_status::{AcpActionKind, AcpSessionState, AcpStatusObservation, AcpStopReason};
use clap::ArgMatches;

use crate::{
    agent_acp_console::{AgentAcpConsole, ConsoleEvent},
    agent_acp_snapshot::{read_snapshot, AcpSnapshotPublisher, HEARTBEAT_INTERVAL_MILLIS},
    CliError,
};

static GENERATION_SUFFIX: AtomicU64 = AtomicU64::new(0);

const AUTHENTICATION_UNSUPPORTED: &str = "No supported authentication methods are available.";
const AUTHENTICATION_LOGIN_FAILED: &str = "Authentication failed.";
const AUTHENTICATION_CANCELLED: &str = "Authentication cancelled.";
const AUTHENTICATION_START_FAILED: &str = "Session start failed after authentication.";

struct SessionFold {
    state: AcpSessionState,
    tools: HashMap<v2::ToolCallId, ToolDetail>,
    tool_order: Vec<v2::ToolCallId>,
    plans: HashMap<v2::PlanId, Vec<PlanEntryDetail>>,
    plan_order: Vec<v2::PlanId>,
    usage: Option<(u64, u64)>,
    pending_permission: Option<v2::RequestPermissionRequest>,
    pending_elicitation: Option<v2::CreateElicitationRequest>,
}

struct PermissionRequest {
    request: v2::RequestPermissionRequest,
    responder: agent_client_protocol::Responder<v2::RequestPermissionResponse>,
}

struct ElicitationRequest {
    request: v2::CreateElicitationRequest,
    responder: agent_client_protocol::Responder<v2::CreateElicitationResponse>,
}

struct ToolDetail {
    title: Option<String>,
    status: Option<v2::ToolCallStatus>,
}

struct PlanEntryDetail {
    content: String,
    status: v2::PlanEntryStatus,
}

struct SessionLifecycleState {
    publisher: AcpSnapshotPublisher,
    session_id: Option<String>,
}

enum SessionEnd {
    Graceful,
    UnexpectedEof,
    PublishFailed(CliError),
    HostFailed(String),
}

impl Default for SessionFold {
    fn default() -> Self {
        Self {
            state: AcpSessionState::Connecting,
            tools: HashMap::new(),
            tool_order: Vec::new(),
            plans: HashMap::new(),
            plan_order: Vec::new(),
            usage: None,
            pending_permission: None,
            pending_elicitation: None,
        }
    }
}

impl SessionFold {
    fn set_state(&mut self, state: AcpSessionState) {
        self.state = state;
    }

    fn permission_detail(&self) -> Option<String> {
        let request = self.pending_permission.as_ref()?;
        let mut detail = request.title.clone();
        if let Some(description) = &request.description {
            if !detail.is_empty() {
                detail.push_str(": ");
            }
            detail.push_str(description);
        }
        for (index, option) in request.options.iter().enumerate() {
            detail.push_str(&format!(" · {}. {}", index + 1, option.name));
        }
        Some(detail)
    }

    fn elicitation_detail(&self) -> Option<String> {
        let request = self.pending_elicitation.as_ref()?;
        let v2::ElicitationMode::Form(form) = &request.mode else {
            return Some(request.message.clone());
        };
        let mut detail = request.message.clone();
        for (name, type_label) in elicitation_field_labels(&form.requested_schema) {
            detail.push_str(&format!(" · {name}: {type_label}"));
        }
        Some(detail)
    }

    fn apply_update(&mut self, update: &v2::SessionUpdate) {
        match update {
            v2::SessionUpdate::StateUpdate(update) => {
                self.state = match update {
                    v2::StateUpdate::Running(_) => AcpSessionState::Running,
                    v2::StateUpdate::Idle(update) => AcpSessionState::Idle(
                        update.stop_reason.as_ref().map(|reason| match reason {
                            v2::StopReason::EndTurn => AcpStopReason::EndTurn,
                            v2::StopReason::Cancelled => AcpStopReason::Cancelled,
                            v2::StopReason::MaxTokens => AcpStopReason::MaxTokens,
                            v2::StopReason::MaxTurnRequests => AcpStopReason::MaxTurnRequests,
                            v2::StopReason::Refusal => AcpStopReason::Refusal,
                            v2::StopReason::Other(reason) => AcpStopReason::Other(reason.clone()),
                            reason => AcpStopReason::Other(format!("{reason:?}")),
                        }),
                    ),
                    v2::StateUpdate::RequiresAction(_) => {
                        AcpSessionState::RequiresAction(AcpActionKind::Input)
                    }
                    v2::StateUpdate::Other(update) => AcpSessionState::Other(update.state.clone()),
                    update => AcpSessionState::Other(format!("{update:?}")),
                };
            }
            v2::SessionUpdate::ToolCallUpdate(update) => {
                let id = update.tool_call_id.clone();
                let tool = self.tools.entry(id.clone()).or_insert(ToolDetail {
                    title: None,
                    status: None,
                });
                match &update.title {
                    MaybeUndefined::Undefined => {}
                    MaybeUndefined::Null => tool.title = None,
                    MaybeUndefined::Value(title) => tool.title = Some(title.clone()),
                }
                match &update.status {
                    MaybeUndefined::Undefined => {}
                    MaybeUndefined::Null => tool.status = None,
                    MaybeUndefined::Value(status) => tool.status = Some(status.clone()),
                }
                self.tool_order.retain(|existing| existing != &id);
                self.tool_order.push(id);
            }
            v2::SessionUpdate::PlanUpdate(update) => {
                if let v2::PlanUpdateContent::Items(items) = &update.plan {
                    let id = items.plan_id.clone();
                    let entries = items
                        .entries
                        .iter()
                        .map(|entry| PlanEntryDetail {
                            content: entry.content.clone(),
                            status: entry.status.clone(),
                        })
                        .collect();
                    self.plans.insert(id.clone(), entries);
                    self.plan_order.retain(|existing| existing != &id);
                    self.plan_order.push(id);
                }
            }
            v2::SessionUpdate::UsageUpdate(update) => {
                self.usage = Some((update.used, update.size));
            }
            _ => {}
        }
    }

    fn observation(&self) -> AcpStatusObservation {
        if self.pending_permission.is_some() {
            return AcpStatusObservation {
                state: AcpSessionState::RequiresAction(AcpActionKind::Permission),
                detail: self.permission_detail(),
            };
        }
        if self.pending_elicitation.is_some() {
            return AcpStatusObservation {
                state: AcpSessionState::RequiresAction(AcpActionKind::Input),
                detail: self.elicitation_detail(),
            };
        }

        let tool = self.tool_order.iter().rev().find_map(|id| {
            let tool = self.tools.get(id)?;
            let status = tool.status.as_ref()?;
            if matches!(status, v2::ToolCallStatus::Completed) {
                return None;
            }
            let label = tool.title.as_deref().unwrap_or(id.0.as_ref());
            Some(if matches!(status, v2::ToolCallStatus::Failed) {
                format!("{label} failed")
            } else {
                label.to_owned()
            })
        });
        let plan = self.plan_order.iter().rev().find_map(|id| {
            let entries = self.plans.get(id)?;
            let completed = entries
                .iter()
                .filter(|entry| matches!(entry.status, v2::PlanEntryStatus::Completed))
                .count();
            let current = entries
                .iter()
                .find(|entry| matches!(entry.status, v2::PlanEntryStatus::InProgress))
                .or_else(|| {
                    entries
                        .iter()
                        .find(|entry| matches!(entry.status, v2::PlanEntryStatus::Pending))
                });
            let mut detail = format!("Plan {completed}/{}", entries.len());
            if let Some(entry) = current {
                detail.push_str(": ");
                detail.push_str(&entry.content);
            }
            Some(detail)
        });
        let context = self
            .usage
            .map(|(used, size)| format!("Context {used}/{size}"));
        let detail = tool
            .into_iter()
            .chain(plan)
            .chain(context)
            .collect::<Vec<_>>()
            .join(" · ");

        AcpStatusObservation {
            state: self.state.clone(),
            detail: (!detail.is_empty()).then_some(detail),
        }
    }
}

pub(crate) fn run_agent_acp_command(matches: &ArgMatches) -> Result<String, CliError> {
    let task_id = matches
        .get_one::<String>("task-id")
        .ok_or_else(|| CliError::CommandFailed("agent ACP task id is required".to_string()))?;
    let state_root = matches
        .get_one::<String>("state-root")
        .map(PathBuf::from)
        .ok_or_else(|| CliError::CommandFailed("agent ACP state root is required".to_string()))?;
    let program = matches
        .get_one::<String>("program")
        .ok_or_else(|| CliError::CommandFailed("agent ACP program is required".to_string()))?;
    let adapter_args = matches
        .get_many::<String>("agent-args")
        .map(|values| values.cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let agent = AcpAgent::new(AcpAgentConfig::new(program).args(adapter_args));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(acp_initialization_error)?;
    let cwd = std::env::current_dir().map_err(|error| {
        CliError::CommandFailed(format!(
            "failed to determine ACP session directory: {error}"
        ))
    })?;

    runtime.block_on(async {
        let console = AgentAcpConsole::spawn_stdio();
        run_session_lifecycle(agent, task_id, &state_root, &cwd, console).await
    })?;
    Ok(String::new())
}

#[cfg(test)]
async fn negotiate_v2(agent: impl ConnectTo<Client>) -> Result<(), CliError> {
    Client
        .v2()
        .connect_with(agent, async |connection| {
            initialize_v2(&connection).await.map(|_| ())
        })
        .await
        .map_err(acp_initialization_error)
}

async fn initialize_v2(
    connection: &ConnectionTo<Agent>,
) -> agent_client_protocol::Result<Vec<v2::AuthMethod>> {
    let response = connection
        .send_request(
            v2::InitializeRequest::new(
                ProtocolVersion::V2,
                v2::Implementation::new("ajax-cli", env!("CARGO_PKG_VERSION")),
            )
            .capabilities(v2::ClientCapabilities::new().elicitation(
                v2::ElicitationCapabilities::new().form(v2::ElicitationFormCapabilities::new()),
            )),
        )
        .block_task()
        .await?;
    if response.protocol_version != ProtocolVersion::V2 {
        return Err(agent_client_protocol::Error::internal_error()
            .data("peer did not confirm ACP protocol version 2"));
    }
    Ok(response.auth_methods)
}

pub(crate) async fn run_session_lifecycle<W: std::io::Write>(
    agent: impl ConnectTo<Client>,
    task_id: &str,
    state_root: &Path,
    cwd: &Path,
    mut console: AgentAcpConsole<W>,
) -> Result<(), CliError> {
    let now = unix_millis();
    let cached_session_id =
        read_snapshot(state_root, task_id, now)?.and_then(|snapshot| snapshot.session_id);
    let lifecycle = Arc::new(Mutex::new(SessionLifecycleState {
        publisher: AcpSnapshotPublisher::claim(
            state_root,
            task_id,
            &format!(
                "{}-{}-{}",
                std::process::id(),
                now,
                GENERATION_SUFFIX.fetch_add(1, Ordering::Relaxed)
            ),
            cached_session_id.clone(),
            AcpStatusObservation {
                state: AcpSessionState::Connecting,
                detail: None,
            },
            now,
        )?,
        session_id: cached_session_id.clone(),
    }));
    let failure_lifecycle = Arc::clone(&lifecycle);
    let (updates_tx, mut updates_rx) = tokio::sync::mpsc::unbounded_channel();
    let (host_fail_tx, mut host_fail_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (permission_tx, mut permission_rx) =
        tokio::sync::mpsc::unbounded_channel::<PermissionRequest>();
    let (elicitation_tx, mut elicitation_rx) =
        tokio::sync::mpsc::unbounded_channel::<ElicitationRequest>();

    let result = Client
        .v2()
        .on_receive_notification(
            async move |notification: v2::UpdateSessionNotification, _connection| {
                let _ = updates_tx.send(notification);
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            {
                let permission_tx = permission_tx.clone();
                async move |request: v2::RequestPermissionRequest, responder, _connection| {
                    let _ = permission_tx.send(PermissionRequest { request, responder });
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let elicitation_tx = elicitation_tx.clone();
                async move |request: v2::CreateElicitationRequest, responder, _connection| {
                    let enqueue = matches!(
                        &request.mode,
                        v2::ElicitationMode::Form(v2::ElicitationFormMode {
                            scope: v2::ElicitationScope::Session(_),
                            ..
                        })
                    );
                    if enqueue {
                        let _ = elicitation_tx.send(ElicitationRequest { request, responder });
                    } else {
                        let _ = responder.respond(v2::CreateElicitationResponse::new(
                            v2::ElicitationAction::Decline,
                        ));
                    }
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(agent, async move |connection| {
            let auth_methods = initialize_v2(&connection).await?;

            let session_id = match start_session(&connection, &cached_session_id, cwd).await {
                Ok(session_id) => session_id,
                Err(error) if is_auth_required(&error) => {
                    let agent_methods = supported_agent_auth_methods(&auth_methods);
                    if agent_methods.is_empty() {
                        return Ok(SessionEnd::HostFailed(
                            AUTHENTICATION_UNSUPPORTED.to_owned(),
                        ));
                    }

                    loop {
                        if let Err(error) = publish_pre_session(
                            &lifecycle,
                            AcpStatusObservation {
                                state: AcpSessionState::RequiresAction(
                                    AcpActionKind::Authentication,
                                ),
                                detail: authentication_detail(&agent_methods),
                            },
                        ) {
                            return Ok(SessionEnd::PublishFailed(error));
                        }
                        let method_names = agent_methods
                            .iter()
                            .map(|method| method.name.as_str())
                            .collect::<Vec<_>>();
                        if let Err(error) = console.render_authentication_prompt(&method_names) {
                            return Ok(SessionEnd::HostFailed(host_failure_detail(error)));
                        }

                        let event = match console.next_event().await {
                            Some(event) => event,
                            None => {
                                return Ok(SessionEnd::HostFailed(
                                    AUTHENTICATION_CANCELLED.to_owned(),
                                ));
                            }
                        };
                        match event {
                            ConsoleEvent::Interrupt | ConsoleEvent::InputError(_) => {
                                return Ok(SessionEnd::HostFailed(
                                    AUTHENTICATION_CANCELLED.to_owned(),
                                ));
                            }
                            ConsoleEvent::PromptLine(line) => {
                                let Some(index) =
                                    auth_method_index_for_line(&line, agent_methods.len())
                                else {
                                    if let Err(error) =
                                        console.render_authentication_validation_error()
                                    {
                                        return Ok(SessionEnd::HostFailed(host_failure_detail(
                                            error,
                                        )));
                                    }
                                    continue;
                                };
                                if connection
                                    .send_request(v2::LoginAuthRequest::new(
                                        agent_methods[index].method_id.clone(),
                                    ))
                                    .block_task()
                                    .await
                                    .is_err()
                                {
                                    return Ok(SessionEnd::HostFailed(
                                        AUTHENTICATION_LOGIN_FAILED.to_owned(),
                                    ));
                                }
                                break;
                            }
                        }
                    }

                    match start_session(&connection, &cached_session_id, cwd).await {
                        Ok(session_id) => session_id,
                        Err(_) => {
                            return Ok(SessionEnd::HostFailed(
                                AUTHENTICATION_START_FAILED.to_owned(),
                            ));
                        }
                    }
                }
                Err(error) => return Err(error),
            };

            let mut fold = SessionFold::default();
            fold.set_state(AcpSessionState::Running);
            if let Err(error) = publish(&lifecycle, &session_id, fold.observation()) {
                return Ok(SessionEnd::PublishFailed(error));
            }
            let mut active_permission = None::<PermissionRequest>;
            let mut active_elicitation = None::<ElicitationRequest>;
            let mut heartbeat =
                tokio::time::interval(Duration::from_millis(HEARTBEAT_INTERVAL_MILLIS as u64));

            loop {
                tokio::select! {
                    biased;
                    Some(notification) = updates_rx.recv() => {
                        if notification.session_id.0.as_ref() != session_id {
                            continue;
                        }
                        fold.apply_update(&notification.update);
                        if let Err(error) = console.render_update(&notification.update) {
                            return Ok(SessionEnd::HostFailed(host_failure_detail(error)));
                        }
                        if let Err(error) = publish(&lifecycle, &session_id, fold.observation()) {
                            return Ok(SessionEnd::PublishFailed(error));
                        }
                    }
                    Some(permission) = permission_rx.recv(), if active_permission.is_none() && active_elicitation.is_none() => {
                        if permission.request.session_id.0.as_ref() != session_id {
                            let _ = permission.responder.respond(
                                v2::RequestPermissionResponse::new(
                                    v2::RequestPermissionOutcome::Cancelled,
                                ),
                            );
                            continue;
                        }
                        fold.pending_permission = Some(permission.request.clone());
                        if let Err(error) = publish(&lifecycle, &session_id, fold.observation()) {
                            return Ok(SessionEnd::PublishFailed(error));
                        }
                        if let Err(error) = console.render_permission_prompt(&permission.request) {
                            return Ok(SessionEnd::HostFailed(host_failure_detail(error)));
                        }
                        active_permission = Some(permission);
                    }
                    Some(elicitation) = elicitation_rx.recv(), if active_permission.is_none() && active_elicitation.is_none() => {
                        let supported = matches!(
                            elicitation.request.scope(),
                            v2::ElicitationScope::Session(scope)
                                if scope.session_id.0.as_ref() == session_id
                        ) && matches!(
                            &elicitation.request.mode,
                            v2::ElicitationMode::Form(form)
                                if is_supported_elicitation_schema(&form.requested_schema)
                        );
                        if !supported {
                            if let Err(error) = respond_elicitation_action(
                                elicitation.responder,
                                v2::ElicitationAction::Decline,
                            ) {
                                return Ok(SessionEnd::HostFailed(host_failure_detail(error)));
                            }
                            continue;
                        }
                        fold.pending_elicitation = Some(elicitation.request.clone());
                        if let Err(error) = publish(&lifecycle, &session_id, fold.observation()) {
                            return Ok(SessionEnd::PublishFailed(error));
                        }
                        if let v2::ElicitationMode::Form(form) = &elicitation.request.mode {
                            if let Err(error) = console.render_elicitation_prompt(
                                &elicitation.request.message,
                                &elicitation_field_labels(&form.requested_schema),
                            ) {
                                return Ok(SessionEnd::HostFailed(host_failure_detail(error)));
                            }
                        }
                        active_elicitation = Some(elicitation);
                    }
                    Some(event) = console.next_event() => {
                        if let Some(active) = active_permission.take() {
                            let outcome =
                                permission_outcome_for_line(&event, &active.request.options);
                            if let ConsoleEvent::InputError(detail) = &event {
                                return Ok(SessionEnd::HostFailed(host_failure_detail(
                                    CliError::CommandFailed(format!(
                                        "ACP console input failed: {detail}"
                                    )),
                                )));
                            }
                            if matches!(event, ConsoleEvent::Interrupt) {
                                if let Err(error) = respond_permission_outcome(
                                    active.responder,
                                    v2::RequestPermissionOutcome::Cancelled,
                                ) {
                                    return Ok(SessionEnd::HostFailed(host_failure_detail(error)));
                                }
                                if let Err(error) = clear_permission_snapshot(&mut fold, &lifecycle, &session_id) {
                                    return Ok(error);
                                }
                                if let Err(error) =
                                    handle_console_event(&connection, &session_id, event, &host_fail_tx)
                                {
                                    return Ok(SessionEnd::HostFailed(host_failure_detail(error)));
                                }
                                continue;
                            }
                            if let Err(error) =
                                respond_permission_outcome(active.responder, outcome)
                            {
                                return Ok(SessionEnd::HostFailed(host_failure_detail(error)));
                            }
                            if let Err(error) = clear_permission_snapshot(&mut fold, &lifecycle, &session_id) {
                                return Ok(error);
                            }
                            continue;
                        }
                        if let Some(active) = active_elicitation.take() {
                            if let ConsoleEvent::InputError(detail) = &event {
                                return Ok(SessionEnd::HostFailed(host_failure_detail(
                                    CliError::CommandFailed(format!(
                                        "ACP console input failed: {detail}"
                                    )),
                                )));
                            }
                            if matches!(event, ConsoleEvent::Interrupt) {
                                if let Err(error) = respond_elicitation_action(
                                    active.responder,
                                    v2::ElicitationAction::Cancel,
                                ) {
                                    return Ok(SessionEnd::HostFailed(host_failure_detail(error)));
                                }
                                if let Err(error) =
                                    clear_elicitation_snapshot(&mut fold, &lifecycle, &session_id)
                                {
                                    return Ok(error);
                                }
                                if let Err(error) =
                                    handle_console_event(&connection, &session_id, event, &host_fail_tx)
                                {
                                    return Ok(SessionEnd::HostFailed(host_failure_detail(error)));
                                }
                                continue;
                            }
                            if let ConsoleEvent::PromptLine(line) = &event {
                                let schema = match &active.request.mode {
                                    v2::ElicitationMode::Form(form) => &form.requested_schema,
                                    _ => &v2::ElicitationSchema::new(),
                                };
                                match elicitation_content_from_line(line, schema) {
                                    Ok(content) => {
                                        if let Err(error) = respond_elicitation_action(
                                            active.responder,
                                            v2::ElicitationAction::Accept(
                                                v2::ElicitationAcceptAction::new()
                                                    .content(content),
                                            ),
                                        ) {
                                            return Ok(SessionEnd::HostFailed(
                                                host_failure_detail(error),
                                            ));
                                        }
                                        if let Err(error) = clear_elicitation_snapshot(
                                            &mut fold,
                                            &lifecycle,
                                            &session_id,
                                        ) {
                                            return Ok(error);
                                        }
                                        continue;
                                    }
                                    Err(detail) => {
                                        if let Err(error) =
                                            console.render_elicitation_validation_error(&detail)
                                        {
                                            return Ok(SessionEnd::HostFailed(
                                                host_failure_detail(error),
                                            ));
                                        }
                                        active_elicitation = Some(active);
                                        continue;
                                    }
                                }
                            }
                        }
                        if let Err(error) =
                            handle_console_event(&connection, &session_id, event, &host_fail_tx)
                        {
                            return Ok(SessionEnd::HostFailed(host_failure_detail(error)));
                        }
                    }
                    Some(detail) = host_fail_rx.recv() => {
                        return Ok(SessionEnd::HostFailed(detail));
                    }
                    _ = heartbeat.tick() => {
                        if let Err(error) = publish(&lifecycle, &session_id, fold.observation()) {
                            return Ok(SessionEnd::PublishFailed(error));
                        }
                    }
                    () = connection.incoming_closed() => {
                        if matches!(fold.observation().state, AcpSessionState::Idle(_)) {
                            return Ok(SessionEnd::Graceful);
                        }
                        return Ok(SessionEnd::UnexpectedEof);
                    }
                }
            }
        })
        .await;

    let detail = match result {
        Ok(SessionEnd::Graceful) => return Ok(()),
        Ok(SessionEnd::UnexpectedEof) => "ACP adapter exited unexpectedly".to_owned(),
        Ok(SessionEnd::PublishFailed(error)) => return Err(error),
        Ok(SessionEnd::HostFailed(detail)) => detail,
        Err(error) => format!("ACP v2 session failed: {error}"),
    };
    publish_failure(&failure_lifecycle, &detail)?;
    Err(CliError::CommandFailed(detail))
}

fn publish_pre_session(
    lifecycle: &Mutex<SessionLifecycleState>,
    observation: AcpStatusObservation,
) -> Result<(), CliError> {
    let mut lifecycle = lifecycle
        .lock()
        .map_err(|_| CliError::CommandFailed("ACP snapshot publisher poisoned".to_owned()))?;
    let session_id = lifecycle.session_id.clone();
    lifecycle
        .publisher
        .publish(session_id, observation, unix_millis())
        .map(|_| ())
}

async fn start_session(
    connection: &ConnectionTo<Agent>,
    cached_session_id: &Option<String>,
    cwd: &Path,
) -> Result<String, agent_client_protocol::Error> {
    if let Some(session_id) = cached_session_id {
        connection
            .send_request(v2::ResumeSessionRequest::new(
                session_id.clone(),
                cwd.to_path_buf(),
            ))
            .block_task()
            .await?;
        Ok(session_id.clone())
    } else {
        Ok(connection
            .send_request(v2::NewSessionRequest::new(cwd.to_path_buf()))
            .block_task()
            .await?
            .session_id
            .0
            .to_string())
    }
}

fn supported_agent_auth_methods(methods: &[v2::AuthMethod]) -> Vec<v2::AuthMethodAgent> {
    methods
        .iter()
        .filter_map(|method| match method {
            v2::AuthMethod::Agent(agent) => Some(agent.clone()),
            _ => None,
        })
        .collect()
}

fn authentication_detail(methods: &[v2::AuthMethodAgent]) -> Option<String> {
    if methods.is_empty() {
        return None;
    }
    Some(
        methods
            .iter()
            .enumerate()
            .map(|(index, method)| format!("{}. {}", index + 1, method.name))
            .collect::<Vec<_>>()
            .join(" · "),
    )
}

fn auth_method_index_for_line(line: &str, method_count: usize) -> Option<usize> {
    let choice = line.trim().parse::<usize>().ok()?;
    if choice == 0 || choice > method_count {
        return None;
    }
    Some(choice - 1)
}

fn is_auth_required(error: &agent_client_protocol::Error) -> bool {
    error.code == ErrorCode::AuthRequired
}

fn publish(
    lifecycle: &Mutex<SessionLifecycleState>,
    session_id: &str,
    observation: AcpStatusObservation,
) -> Result<(), CliError> {
    let mut lifecycle = lifecycle
        .lock()
        .map_err(|_| CliError::CommandFailed("ACP snapshot publisher poisoned".to_owned()))?;
    lifecycle.session_id = Some(session_id.to_owned());
    let session_id = lifecycle.session_id.clone();
    lifecycle
        .publisher
        .publish(session_id, observation, unix_millis())
        .map(|_| ())
}

fn publish_failure(lifecycle: &Mutex<SessionLifecycleState>, detail: &str) -> Result<(), CliError> {
    let mut lifecycle = lifecycle
        .lock()
        .map_err(|_| CliError::CommandFailed("ACP snapshot publisher poisoned".to_owned()))?;
    let session_id = lifecycle.session_id.clone();
    lifecycle
        .publisher
        .publish(
            session_id,
            AcpStatusObservation {
                state: AcpSessionState::Failed,
                detail: Some(detail.to_owned()),
            },
            unix_millis(),
        )
        .map(|_| ())
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn acp_initialization_error(error: impl std::fmt::Display) -> CliError {
    CliError::CommandFailed(format!("ACP v2 initialization failed: {error}"))
}

fn clear_permission_snapshot(
    fold: &mut SessionFold,
    lifecycle: &Mutex<SessionLifecycleState>,
    session_id: &str,
) -> Result<(), SessionEnd> {
    fold.pending_permission = None;
    publish(lifecycle, session_id, fold.observation()).map_err(SessionEnd::PublishFailed)
}

fn clear_elicitation_snapshot(
    fold: &mut SessionFold,
    lifecycle: &Mutex<SessionLifecycleState>,
    session_id: &str,
) -> Result<(), SessionEnd> {
    fold.pending_elicitation = None;
    publish(lifecycle, session_id, fold.observation()).map_err(SessionEnd::PublishFailed)
}

fn is_supported_elicitation_schema(schema: &v2::ElicitationSchema) -> bool {
    if let Some(required) = &schema.required {
        for name in required {
            if !schema.properties.contains_key(name) {
                return false;
            }
        }
    }
    schema
        .properties
        .values()
        .all(is_supported_elicitation_property)
}

fn is_supported_elicitation_property(property: &v2::ElicitationPropertySchema) -> bool {
    match property {
        v2::ElicitationPropertySchema::String(string) => {
            !(string.pattern.is_some()
                || string.enum_values.as_ref().is_some_and(Vec::is_empty)
                || string.one_of.as_ref().is_some_and(Vec::is_empty)
                || matches!(
                    (string.min_length, string.max_length),
                    (Some(min), Some(max)) if min > max
                )
                || string.enum_values.is_some() && string.one_of.is_some())
        }
        v2::ElicitationPropertySchema::Integer(integer) => !matches!(
            (integer.minimum, integer.maximum),
            (Some(min), Some(max)) if min > max
        ),
        v2::ElicitationPropertySchema::Number(number) => !matches!(
            (number.minimum, number.maximum),
            (Some(min), Some(max)) if min > max
        ),
        v2::ElicitationPropertySchema::Boolean(_) => true,
        v2::ElicitationPropertySchema::Array(array) => {
            !matches!((array.min_items, array.max_items), (Some(min), Some(max)) if min > max)
                && match &array.items {
                    v2::MultiSelectItems::String(items) => !items.values.is_empty(),
                    v2::MultiSelectItems::Titled(items) => !items.options.is_empty(),
                    _ => false,
                }
        }
        _ => false,
    }
}

fn elicitation_field_labels(schema: &v2::ElicitationSchema) -> Vec<(String, &'static str)> {
    schema
        .properties
        .iter()
        .filter_map(|(name, property)| {
            let type_label = match property {
                v2::ElicitationPropertySchema::String(_) => "string",
                v2::ElicitationPropertySchema::Integer(_) => "integer",
                v2::ElicitationPropertySchema::Number(_) => "number",
                v2::ElicitationPropertySchema::Boolean(_) => "boolean",
                v2::ElicitationPropertySchema::Array(_) => "string array",
                _ => return None,
            };
            Some((name.clone(), type_label))
        })
        .collect()
}

fn elicitation_content_from_line(
    line: &str,
    schema: &v2::ElicitationSchema,
) -> Result<BTreeMap<String, v2::ElicitationContentValue>, String> {
    if schema.properties.is_empty() {
        let value: serde_json::Value = match serde_json::from_str(line.trim()) {
            Ok(value) => value,
            Err(_) => return Err(crate::agent_acp_console::ELICITATION_VALIDATION_ERROR.to_owned()),
        };
        let object = match value.as_object() {
            Some(object) => object,
            None => return Err(crate::agent_acp_console::ELICITATION_VALIDATION_ERROR.to_owned()),
        };
        if object.is_empty() {
            return Ok(BTreeMap::new());
        }
        return Err(crate::agent_acp_console::ELICITATION_VALIDATION_ERROR.to_owned());
    }

    let value: serde_json::Value =
        serde_json::from_str(line.trim()).map_err(|_| "Expected a JSON object.".to_owned())?;
    let object = value
        .as_object()
        .ok_or_else(|| "Expected a JSON object.".to_owned())?;

    for key in object.keys() {
        if !schema.properties.contains_key(key) {
            return Err("Unexpected field.".to_owned());
        }
    }

    if let Some(required) = &schema.required {
        for name in required {
            if !object.contains_key(name) {
                return Err(format!("{name}: required"));
            }
        }
    }

    let mut content = BTreeMap::new();
    for (name, property) in &schema.properties {
        let Some(value) = object.get(name) else {
            continue;
        };
        match property {
            v2::ElicitationPropertySchema::String(string) => {
                let Some(text) = value.as_str() else {
                    return Err(format!("{name}: expected string"));
                };
                let length = text.chars().count() as u32;
                if string.min_length.is_some_and(|min| length < min) {
                    return Err(format!("{name}: too short"));
                }
                if string.max_length.is_some_and(|max| length > max) {
                    return Err(format!("{name}: too long"));
                }
                if let Some(enum_values) = &string.enum_values {
                    if !enum_values.iter().any(|allowed| allowed == text) {
                        return Err(format!("{name}: invalid choice"));
                    }
                } else if let Some(one_of) = &string.one_of {
                    if !one_of.iter().any(|option| option.value == text) {
                        return Err(format!("{name}: invalid choice"));
                    }
                }
                content.insert(
                    name.clone(),
                    v2::ElicitationContentValue::String(text.to_owned()),
                );
            }
            v2::ElicitationPropertySchema::Integer(integer) => {
                let Some(number) = value.as_i64() else {
                    return Err(format!("{name}: expected integer"));
                };
                if integer.minimum.is_some_and(|min| number < min) {
                    return Err(format!("{name}: below minimum"));
                }
                if integer.maximum.is_some_and(|max| number > max) {
                    return Err(format!("{name}: above maximum"));
                }
                content.insert(name.clone(), v2::ElicitationContentValue::Integer(number));
            }
            v2::ElicitationPropertySchema::Number(number) => {
                let Some(number_value) = value.as_f64() else {
                    return Err(format!("{name}: expected number"));
                };
                if number.minimum.is_some_and(|min| number_value < min) {
                    return Err(format!("{name}: below minimum"));
                }
                if number.maximum.is_some_and(|max| number_value > max) {
                    return Err(format!("{name}: above maximum"));
                }
                content.insert(
                    name.clone(),
                    v2::ElicitationContentValue::Number(number_value),
                );
            }
            v2::ElicitationPropertySchema::Boolean(_) => {
                let Some(flag) = value.as_bool() else {
                    return Err(format!("{name}: expected boolean"));
                };
                content.insert(name.clone(), v2::ElicitationContentValue::Boolean(flag));
            }
            v2::ElicitationPropertySchema::Array(array) => {
                let Some(elements) = value.as_array() else {
                    return Err(format!("{name}: expected array"));
                };
                let mut values = Vec::with_capacity(elements.len());
                match &array.items {
                    v2::MultiSelectItems::String(items) => {
                        for element in elements {
                            let Some(text) = element.as_str() else {
                                return Err(format!("{name}: expected string array"));
                            };
                            if !items.values.iter().any(|allowed| allowed == text) {
                                return Err(format!("{name}: invalid choice"));
                            }
                            values.push(text.to_owned());
                        }
                    }
                    v2::MultiSelectItems::Titled(items) => {
                        for element in elements {
                            let Some(text) = element.as_str() else {
                                return Err(format!("{name}: expected string array"));
                            };
                            if !items.options.iter().any(|option| option.value == text) {
                                return Err(format!("{name}: invalid choice"));
                            }
                            values.push(text.to_owned());
                        }
                    }
                    v2::MultiSelectItems::Other(_) => {
                        return Err(format!("{name}: unsupported field"));
                    }
                    _ => {
                        return Err(format!("{name}: unsupported field"));
                    }
                }
                if array
                    .min_items
                    .is_some_and(|min| (values.len() as u64) < min)
                {
                    return Err(format!("{name}: too few items"));
                }
                if array
                    .max_items
                    .is_some_and(|max| (values.len() as u64) > max)
                {
                    return Err(format!("{name}: too many items"));
                }
                content.insert(
                    name.clone(),
                    v2::ElicitationContentValue::StringArray(values),
                );
            }
            _ => return Err(format!("{name}: unsupported field")),
        }
    }

    Ok(content)
}

fn respond_elicitation_action(
    responder: agent_client_protocol::Responder<v2::CreateElicitationResponse>,
    action: v2::ElicitationAction,
) -> Result<(), CliError> {
    responder
        .respond(v2::CreateElicitationResponse::new(action))
        .map_err(acp_initialization_error)
}

fn host_failure_detail(error: CliError) -> String {
    error.to_string()
}

fn acp_prompt_failure_detail() -> String {
    "ACP prompt failed.".to_owned()
}

fn permission_outcome_for_line(
    event: &ConsoleEvent,
    options: &[v2::PermissionOption],
) -> v2::RequestPermissionOutcome {
    let ConsoleEvent::PromptLine(line) = event else {
        return v2::RequestPermissionOutcome::Cancelled;
    };
    let Ok(choice) = line.trim().parse::<usize>() else {
        return v2::RequestPermissionOutcome::Cancelled;
    };
    if choice == 0 || choice > options.len() {
        return v2::RequestPermissionOutcome::Cancelled;
    }
    v2::RequestPermissionOutcome::Selected(v2::SelectedPermissionOutcome::new(
        options[choice - 1].option_id.clone(),
    ))
}

fn respond_permission_outcome(
    responder: agent_client_protocol::Responder<v2::RequestPermissionResponse>,
    outcome: v2::RequestPermissionOutcome,
) -> Result<(), CliError> {
    responder
        .respond(v2::RequestPermissionResponse::new(outcome))
        .map_err(acp_initialization_error)
}

fn handle_console_event(
    connection: &ConnectionTo<Agent>,
    session_id: &str,
    event: ConsoleEvent,
    host_fail_tx: &tokio::sync::mpsc::UnboundedSender<String>,
) -> Result<(), CliError> {
    match event {
        ConsoleEvent::PromptLine(line) => {
            let fail_tx = host_fail_tx.clone();
            connection
                .send_request(v2::PromptRequest::new(session_id, vec![line.into()]))
                .on_receiving_result(async move |result| {
                    if result.is_err() {
                        let _ = fail_tx.send(acp_prompt_failure_detail());
                    }
                    Ok(())
                })
                .map_err(acp_initialization_error)?;
        }
        ConsoleEvent::Interrupt => {
            connection
                .send_notification(v2::CancelSessionNotification::new(session_id))
                .map_err(acp_initialization_error)?;
        }
        ConsoleEvent::InputError(detail) => {
            return Err(CliError::CommandFailed(format!(
                "ACP console input failed: {detail}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
        time::Duration,
    };

    use crate::agent_acp_snapshot::AcpRuntimeSnapshot;
    use agent_client_protocol::{
        schema::{v1, v2, ProtocolVersion},
        Agent, Channel, Client, ConnectTo,
    };
    use ajax_core::acp_status::{
        AcpActionKind, AcpSessionState, AcpStatusObservation, AcpStopReason,
    };

    use super::{
        negotiate_v2, read_snapshot, run_session_lifecycle, unix_millis, AcpSnapshotPublisher,
        SessionFold,
    };
    use crate::agent_acp_console::AgentAcpConsole;

    static LIFECYCLE_TEST_SUFFIX: AtomicU64 = AtomicU64::new(0);

    struct LifecycleTempRoot(PathBuf);

    impl LifecycleTempRoot {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "ajax-acp-lifecycle-{}-{}",
                std::process::id(),
                LIFECYCLE_TEST_SUFFIX.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).expect("create lifecycle test root");
            Self(path)
        }
    }

    impl Drop for LifecycleTempRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[derive(Debug, PartialEq)]
    enum SessionRequest {
        New(PathBuf),
        Resume(String, PathBuf),
    }

    fn lifecycle_agent(
        new_session_id: String,
        update: Option<v2::StateUpdate>,
        requests: tokio::sync::mpsc::UnboundedSender<SessionRequest>,
    ) -> impl ConnectTo<Client> + 'static {
        let resume_requests = requests.clone();
        let new_update = update.clone();
        let resume_update = update;

        Agent
            .v2()
            .on_receive_request(
                async |request: v2::InitializeRequest, responder, _cx| {
                    assert_eq!(request.protocol_version, ProtocolVersion::V2);
                    responder.respond(v2::InitializeResponse::new(
                        ProtocolVersion::V2,
                        v2::Implementation::new("lifecycle-test-agent", "1"),
                    ))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |request: v2::NewSessionRequest, responder, cx| {
                    requests
                        .send(SessionRequest::New(request.cwd.0))
                        .expect("request log open");
                    responder.respond(v2::NewSessionResponse::new(new_session_id.clone()))?;
                    if let Some(update) = new_update.clone() {
                        cx.send_notification(v2::UpdateSessionNotification::new(
                            new_session_id.clone(),
                            v2::SessionUpdate::StateUpdate(update),
                        ))?;
                    }
                    Ok(())
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |request: v2::ResumeSessionRequest, responder, cx| {
                    let session_id = request.session_id.0.to_string();
                    resume_requests
                        .send(SessionRequest::Resume(session_id.clone(), request.cwd.0))
                        .expect("request log open");
                    responder.respond(v2::ResumeSessionResponse::new())?;
                    if let Some(update) = resume_update.clone() {
                        cx.send_notification(v2::UpdateSessionNotification::new(
                            session_id,
                            v2::SessionUpdate::StateUpdate(update),
                        ))?;
                    }
                    Ok(())
                },
                agent_client_protocol::on_receive_request!(),
            )
    }

    async fn drive_lifecycle(
        state_root: PathBuf,
        task_id: &'static str,
        cwd: PathBuf,
        new_session_id: &str,
        update: Option<v2::StateUpdate>,
        state_before_eof: AcpSessionState,
        replace_generation: bool,
    ) -> (Result<(), super::CliError>, SessionRequest) {
        let (client_transport, agent_transport) = Channel::duplex();
        let (requests_tx, mut requests_rx) = tokio::sync::mpsc::unbounded_channel();
        let agent = lifecycle_agent(new_session_id.to_owned(), update, requests_tx);
        let agent_task = tokio::spawn(agent.connect_to(agent_transport));
        let lifecycle_root = state_root.clone();
        let lifecycle_cwd = cwd.clone();
        let lifecycle_task = tokio::spawn(async move {
            run_session_lifecycle(
                client_transport,
                task_id,
                &lifecycle_root,
                &lifecycle_cwd,
                AgentAcpConsole::closed(),
            )
            .await
        });

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let state = read_snapshot(&state_root, task_id, unix_millis())
                    .expect("read lifecycle snapshot")
                    .map(|snapshot| snapshot.observation.state);
                if state.as_ref() == Some(&state_before_eof) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("lifecycle did not publish the expected pre-EOF state");
        let request = requests_rx.recv().await.expect("session request recorded");

        if replace_generation {
            cache_session(
                &state_root,
                task_id,
                "replacement-generation",
                "newer-session",
                unix_millis(),
            );
        }

        agent_task.abort();
        let _ = agent_task.await;
        let result = tokio::time::timeout(Duration::from_secs(2), lifecycle_task)
            .await
            .expect("lifecycle did not stop after EOF")
            .expect("lifecycle task panicked");
        (result, request)
    }

    fn idle(reason: v2::StopReason) -> v2::StateUpdate {
        v2::StateUpdate::Idle(v2::IdleStateUpdate::new().stop_reason(reason))
    }

    fn lifecycle_snapshot(state_root: &Path, task_id: &str) -> AcpRuntimeSnapshot {
        read_snapshot(state_root, task_id, unix_millis())
            .unwrap()
            .unwrap()
    }

    fn cache_session(
        state_root: &Path,
        task_id: &str,
        generation: &str,
        session_id: &str,
        now: u128,
    ) {
        AcpSnapshotPublisher::claim(
            state_root,
            task_id,
            generation,
            Some(session_id.to_owned()),
            AcpStatusObservation {
                state: AcpSessionState::Running,
                detail: None,
            },
            now,
        )
        .unwrap();
    }

    fn plan_entry(content: &str, status: v2::PlanEntryStatus) -> v2::PlanEntry {
        v2::PlanEntry::new(content, v2::PlanEntryPriority::Medium, status)
    }

    #[test]
    fn folds_updates() {
        let mut fold = SessionFold::default();
        fold.set_state(AcpSessionState::Running);

        fold.apply_update(&v2::SessionUpdate::ToolCallUpdate(
            v2::ToolCallUpdate::new("tool-1").status(v2::ToolCallStatus::InProgress),
        ));
        assert_eq!(
            fold.observation(),
            AcpStatusObservation {
                state: AcpSessionState::Running,
                detail: Some("tool-1".to_owned()),
            }
        );

        fold.apply_update(&v2::SessionUpdate::ToolCallUpdate(
            v2::ToolCallUpdate::new("tool-1").title("Compile"),
        ));
        assert_eq!(fold.observation().detail.as_deref(), Some("Compile"));

        fold.apply_update(&v2::SessionUpdate::ToolCallUpdate(
            v2::ToolCallUpdate::new("tool-1").status(v2::ToolCallStatus::Failed),
        ));
        assert_eq!(
            fold.observation(),
            AcpStatusObservation {
                state: AcpSessionState::Running,
                detail: Some("Compile failed".to_owned()),
            }
        );

        fold.apply_update(&v2::SessionUpdate::PlanUpdate(v2::PlanUpdate::new(
            v2::PlanUpdateContent::Items(v2::PlanItems::new(
                "plan-1",
                vec![
                    plan_entry("Inspect", v2::PlanEntryStatus::Completed),
                    plan_entry("Implement", v2::PlanEntryStatus::InProgress),
                    plan_entry("Verify", v2::PlanEntryStatus::Pending),
                ],
            )),
        )));
        assert_eq!(
            fold.observation().detail.as_deref(),
            Some("Compile failed · Plan 1/3: Implement")
        );

        fold.apply_update(&v2::SessionUpdate::PlanUpdate(v2::PlanUpdate::new(
            v2::PlanUpdateContent::Items(v2::PlanItems::new(
                "plan-1",
                vec![
                    plan_entry("New done", v2::PlanEntryStatus::Completed),
                    plan_entry("Ship", v2::PlanEntryStatus::Pending),
                ],
            )),
        )));
        assert_eq!(
            fold.observation().detail.as_deref(),
            Some("Compile failed · Plan 1/2: Ship")
        );

        fold.apply_update(&v2::SessionUpdate::PlanUpdate(v2::PlanUpdate::new(
            v2::PlanUpdateContent::Other(v2::OtherPlanUpdateContent::new(
                "_future",
                "plan-1",
                std::collections::BTreeMap::new(),
            )),
        )));
        assert_eq!(
            fold.observation().detail.as_deref(),
            Some("Compile failed · Plan 1/2: Ship")
        );

        fold.apply_update(&v2::SessionUpdate::UsageUpdate(v2::UsageUpdate::new(
            10, 100,
        )));
        fold.apply_update(&v2::SessionUpdate::UsageUpdate(v2::UsageUpdate::new(20, 0)));
        assert_eq!(
            fold.observation().detail.as_deref(),
            Some("Compile failed · Plan 1/2: Ship · Context 20/0")
        );

        fold.set_state(AcpSessionState::Idle(Some(AcpStopReason::EndTurn)));
        fold.apply_update(&v2::SessionUpdate::ToolCallUpdate(
            v2::ToolCallUpdate::new("tool-2")
                .title("Cleanup")
                .status(v2::ToolCallStatus::InProgress),
        ));
        assert_eq!(
            fold.observation(),
            AcpStatusObservation {
                state: AcpSessionState::Idle(Some(AcpStopReason::EndTurn)),
                detail: Some("Cleanup · Plan 1/2: Ship · Context 20/0".to_owned()),
            }
        );
    }

    #[test]
    fn session_lifecycle_folds_typed_foreground_states() {
        let mut fold = SessionFold::default();
        let cases = [
            (
                v2::StateUpdate::Running(v2::RunningStateUpdate::new()),
                AcpSessionState::Running,
            ),
            (
                v2::StateUpdate::RequiresAction(v2::RequiresActionStateUpdate::new()),
                AcpSessionState::RequiresAction(AcpActionKind::Input),
            ),
            (
                idle(v2::StopReason::Cancelled),
                AcpSessionState::Idle(Some(AcpStopReason::Cancelled)),
            ),
            (
                v2::StateUpdate::Other(v2::OtherStateUpdate::new(
                    "_paused",
                    std::collections::BTreeMap::new(),
                )),
                AcpSessionState::Other("_paused".to_owned()),
            ),
        ];
        for (update, expected) in cases {
            fold.apply_update(&v2::SessionUpdate::StateUpdate(update));
            assert_eq!(fold.observation().state, expected);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn session_lifecycle_new_session_retains_returned_id_and_end_turn_eof_succeeds() {
        let root = LifecycleTempRoot::new();
        let cwd = root.0.join("new-worktree");
        let (result, requests) = drive_lifecycle(
            root.0.clone(),
            "repo/new",
            cwd.clone(),
            "returned-session",
            Some(idle(v2::StopReason::EndTurn)),
            AcpSessionState::Idle(Some(AcpStopReason::EndTurn)),
            false,
        )
        .await;

        result.expect("idle/end_turn EOF should be graceful");
        assert_eq!(requests, SessionRequest::New(cwd));
        let snapshot = lifecycle_snapshot(&root.0, "repo/new");
        assert_eq!(snapshot.session_id.as_deref(), Some("returned-session"));
        assert_eq!(
            snapshot.observation.state,
            AcpSessionState::Idle(Some(AcpStopReason::EndTurn))
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn session_lifecycle_resumes_exact_cached_id_and_cwd() {
        let root = LifecycleTempRoot::new();
        let task_id = "repo/resume";
        let cwd = root.0.join("resume-worktree");
        cache_session(
            &root.0,
            task_id,
            "cached-generation",
            "cached-session",
            unix_millis(),
        );

        let (result, requests) = drive_lifecycle(
            root.0.clone(),
            task_id,
            cwd.clone(),
            "unused-new-session",
            Some(idle(v2::StopReason::EndTurn)),
            AcpSessionState::Idle(Some(AcpStopReason::EndTurn)),
            false,
        )
        .await;

        result.expect("resumed idle session should succeed");
        assert_eq!(
            requests,
            SessionRequest::Resume("cached-session".to_owned(), cwd)
        );
        let snapshot = lifecycle_snapshot(&root.0, task_id);
        assert_eq!(snapshot.session_id.as_deref(), Some("cached-session"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn session_lifecycle_cancelled_idle_eof_succeeds() {
        let root = LifecycleTempRoot::new();
        let (result, _) = drive_lifecycle(
            root.0.clone(),
            "repo/cancelled",
            root.0.join("cancelled-worktree"),
            "cancelled-session",
            Some(idle(v2::StopReason::Cancelled)),
            AcpSessionState::Idle(Some(AcpStopReason::Cancelled)),
            false,
        )
        .await;

        result.expect("idle/cancelled EOF should be graceful");
        let snapshot = lifecycle_snapshot(&root.0, "repo/cancelled");
        assert_eq!(
            snapshot.observation.state,
            AcpSessionState::Idle(Some(AcpStopReason::Cancelled))
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn session_lifecycle_active_eof_errors_and_publishes_failed_snapshot() {
        let root = LifecycleTempRoot::new();
        let (result, _) = drive_lifecycle(
            root.0.clone(),
            "repo/active",
            root.0.join("active-worktree"),
            "active-session",
            None,
            AcpSessionState::Running,
            false,
        )
        .await;

        let error = result.expect_err("active EOF should fail");
        assert!(error
            .to_string()
            .contains("ACP adapter exited unexpectedly"));
        let snapshot = lifecycle_snapshot(&root.0, "repo/active");
        assert_eq!(snapshot.session_id.as_deref(), Some("active-session"));
        assert_eq!(snapshot.observation.state, AcpSessionState::Failed);
        assert_eq!(
            snapshot.observation.detail.as_deref(),
            Some("ACP adapter exited unexpectedly")
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn session_lifecycle_stale_active_cache_selects_new_session() {
        let root = LifecycleTempRoot::new();
        let task_id = "repo/stale";
        let cwd = root.0.join("stale-worktree");
        cache_session(&root.0, task_id, "stale-generation", "stale-session", 0);

        let (result, requests) = drive_lifecycle(
            root.0.clone(),
            task_id,
            cwd.clone(),
            "fresh-session",
            Some(idle(v2::StopReason::EndTurn)),
            AcpSessionState::Idle(Some(AcpStopReason::EndTurn)),
            false,
        )
        .await;

        result.expect("new session selected from stale cache should succeed");
        assert_eq!(requests, SessionRequest::New(cwd));
        let snapshot = lifecycle_snapshot(&root.0, task_id);
        assert_eq!(snapshot.session_id.as_deref(), Some("fresh-session"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn session_lifecycle_propagates_failed_final_publish() {
        let root = LifecycleTempRoot::new();
        let (result, _) = drive_lifecycle(
            root.0.clone(),
            "repo/replaced",
            root.0.join("replaced-worktree"),
            "known-session",
            None,
            AcpSessionState::Running,
            true,
        )
        .await;

        let error = result.expect_err("active EOF should fail");
        assert!(
            error.to_string().contains("generation mismatch"),
            "final publish failure was discarded: {error}"
        );
        let snapshot = lifecycle_snapshot(&root.0, "repo/replaced");
        assert_eq!(snapshot.session_id.as_deref(), Some("newer-session"));
        assert_eq!(snapshot.observation.state, AcpSessionState::Running);
    }

    #[test]
    fn requests_elicitation_values_schema_predicate_accepts_plain_string_and_integer() {
        assert!(super::is_supported_elicitation_schema(
            &v2::ElicitationSchema::new()
        ));
        let empty_required: v2::ElicitationSchema = serde_json::from_value(serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        }))
        .expect("empty required schema");
        assert!(super::is_supported_elicitation_schema(&empty_required));
        let required_without_properties: v2::ElicitationSchema =
            serde_json::from_value(serde_json::json!({
                "type": "object",
                "properties": {},
                "required": ["name"]
            }))
            .expect("required-only schema");
        assert!(!super::is_supported_elicitation_schema(
            &required_without_properties
        ));

        let plain = v2::ElicitationSchema::new()
            .string("cluster", true)
            .property("replicas", v2::IntegerPropertySchema::new(), false);
        assert!(super::is_supported_elicitation_schema(&plain));

        let with_pattern: v2::ElicitationSchema = serde_json::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "pattern": "^[a-z]+$" }
            }
        }))
        .expect("pattern schema");
        assert!(!super::is_supported_elicitation_schema(&with_pattern));

        let with_min: v2::ElicitationSchema = serde_json::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "count": { "type": "integer", "minimum": 1 }
            }
        }))
        .expect("minimum schema");
        assert!(super::is_supported_elicitation_schema(&with_min));

        let boolean_with_default: v2::ElicitationSchema =
            serde_json::from_value(serde_json::json!({
                "type": "object",
                "properties": {
                    "active": { "type": "boolean", "default": true }
                }
            }))
            .expect("boolean default schema");
        assert!(super::is_supported_elicitation_schema(
            &boolean_with_default
        ));
    }

    #[test]
    fn elicitation_primitives_predicate_accepts_unconstrained_number_boolean_and_array() {
        let schema = v2::ElicitationSchema::new()
            .property("rating", v2::NumberPropertySchema::new(), true)
            .boolean("active", false)
            .property(
                "tags",
                v2::MultiSelectPropertySchema::new(vec!["alpha".into(), "beta".into()]),
                true,
            );
        assert!(super::is_supported_elicitation_schema(&schema));
        assert_eq!(
            super::elicitation_field_labels(&schema),
            vec![
                ("active".to_owned(), "boolean"),
                ("rating".to_owned(), "number"),
                ("tags".to_owned(), "string array"),
            ]
        );

        let titled = v2::ElicitationSchema::new().property(
            "colors",
            v2::MultiSelectPropertySchema::titled(vec![v2::EnumOption::new(
                "ALLOWED-WIRE-SECRET-99",
                "Secret Red",
            )]),
            true,
        );
        assert!(super::is_supported_elicitation_schema(&titled));
        assert_eq!(
            super::elicitation_field_labels(&titled),
            vec![("colors".to_owned(), "string array")]
        );
    }

    #[test]
    fn elicitation_primitives_converter_returns_typed_variants() {
        let schema = v2::ElicitationSchema::new()
            .property("rating", v2::NumberPropertySchema::new(), true)
            .boolean("active", true)
            .property(
                "tags",
                v2::MultiSelectPropertySchema::new(vec!["alpha".into(), "beta".into()]),
                true,
            );

        let plain = super::elicitation_content_from_line(
            r#"{"rating":9.5,"active":true,"tags":["alpha"]}"#,
            &schema,
        )
        .expect("plain primitives");
        assert_eq!(
            plain.get("rating"),
            Some(&v2::ElicitationContentValue::Number(9.5))
        );
        assert_eq!(
            plain.get("active"),
            Some(&v2::ElicitationContentValue::Boolean(true))
        );
        assert_eq!(
            plain.get("tags"),
            Some(&v2::ElicitationContentValue::StringArray(vec![
                "alpha".to_owned()
            ]))
        );

        let titled_schema = v2::ElicitationSchema::new().property(
            "colors",
            v2::MultiSelectPropertySchema::titled(vec![v2::EnumOption::new("red", "Red")]),
            true,
        );
        let titled = super::elicitation_content_from_line(r#"{"colors":["red"]}"#, &titled_schema)
            .expect("titled array");
        assert_eq!(
            titled.get("colors"),
            Some(&v2::ElicitationContentValue::StringArray(vec![
                "red".to_owned()
            ]))
        );
    }

    #[test]
    fn elicitation_primitives_converter_rejects_wrong_types_and_invalid_choices_safely() {
        let schema = v2::ElicitationSchema::new()
            .property("rating", v2::NumberPropertySchema::new(), true)
            .boolean("active", true)
            .property(
                "tags",
                v2::MultiSelectPropertySchema::new(vec!["ALLOWED-WIRE-SECRET-99".into()]),
                true,
            );

        let secret_value = "SECRET-SUPPLIED-VALUE-42";
        let number_error = super::elicitation_content_from_line(
            &format!(
                r#"{{"rating":"{secret_value}","active":true,"tags":["ALLOWED-WIRE-SECRET-99"]}}"#
            ),
            &schema,
        )
        .expect_err("rating must be a number");
        assert_eq!(number_error, "rating: expected number");
        assert!(!number_error.contains(secret_value));

        let boolean_error = super::elicitation_content_from_line(
            r#"{"rating":1.0,"active":"yes","tags":["ALLOWED-WIRE-SECRET-99"]}"#,
            &schema,
        )
        .expect_err("active must be a boolean");
        assert_eq!(boolean_error, "active: expected boolean");

        let choice_error = super::elicitation_content_from_line(
            &format!(r#"{{"rating":1.0,"active":true,"tags":["{secret_value}"]}}"#),
            &schema,
        )
        .expect_err("tags must match allowed values");
        assert_eq!(choice_error, "tags: invalid choice");
        assert!(!choice_error.contains(secret_value));
        assert!(!choice_error.contains("ALLOWED-WIRE-SECRET-99"));
    }

    #[test]
    fn elicitation_primitives_predicate_declines_unsupported_constraints() {
        let inverted_number: v2::ElicitationSchema = serde_json::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "rating": { "type": "number", "minimum": 10.0, "maximum": 1.0 }
            }
        }))
        .expect("inverted number bounds schema");
        assert!(!super::is_supported_elicitation_schema(&inverted_number));

        let inverted_array = v2::ElicitationSchema::new().property(
            "tags",
            v2::MultiSelectPropertySchema::new(vec!["alpha".into()])
                .min_items(3)
                .max_items(1),
            true,
        );
        assert!(!super::is_supported_elicitation_schema(&inverted_array));

        let empty_items: v2::ElicitationSchema = serde_json::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "tags": { "type": "array", "items": { "type": "string", "enum": [] } }
            }
        }))
        .expect("empty array items schema");
        assert!(!super::is_supported_elicitation_schema(&empty_items));

        let other_items: v2::ElicitationSchema = serde_json::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "tokens": {
                    "type": "array",
                    "items": { "type": "_token", "enum": ["repo"] }
                }
            }
        }))
        .expect("other array items schema");
        assert!(!super::is_supported_elicitation_schema(&other_items));
    }

    #[test]
    fn requests_elicitation_values_parser_rejects_missing_unknown_and_type_errors() {
        let schema = v2::ElicitationSchema::new()
            .string("cluster", true)
            .property("replicas", v2::IntegerPropertySchema::new(), false);

        let missing = super::elicitation_content_from_line(r#"{"replicas":2}"#, &schema)
            .expect_err("missing required cluster");
        assert_eq!(missing, "cluster: required");

        let secret_key = "SECRET-UNKNOWN-KEY-99";
        let unknown = super::elicitation_content_from_line(
            &format!(r#"{{"cluster":"prod","replicas":3,"{secret_key}":"x"}}"#),
            &schema,
        )
        .expect_err("unknown field");
        assert_eq!(unknown, "Unexpected field.");
        assert!(!unknown.contains(secret_key));

        let type_error =
            super::elicitation_content_from_line(r#"{"cluster":3,"replicas":2}"#, &schema)
                .expect_err("wrong type");
        assert_eq!(type_error, "cluster: expected string");

        let optional_omitted =
            super::elicitation_content_from_line(r#"{"cluster":"prod","replicas":2}"#, &schema)
                .expect("valid with optional omitted");
        assert_eq!(
            optional_omitted.get("cluster"),
            Some(&v2::ElicitationContentValue::String("prod".to_owned()))
        );
        assert_eq!(
            optional_omitted.get("replicas"),
            Some(&v2::ElicitationContentValue::Integer(2))
        );

        let without_optional =
            super::elicitation_content_from_line(r#"{"cluster":"prod"}"#, &schema)
                .expect("valid without optional field");
        assert!(!without_optional.contains_key("replicas"));
    }

    #[test]
    fn elicitation_constraints_predicate_defaults_and_declines() {
        for value in [
            serde_json::json!({"type":"object","properties":{"name":{"type":"string","minLength":1,"maxLength":3,"format":"email","default":"D","enum":["a"]}}}),
            serde_json::json!({"type":"object","properties":{"count":{"type":"integer","minimum":1,"maximum":5,"default":2}}}),
            serde_json::json!({"type":"object","properties":{"rating":{"type":"number","minimum":0.0,"maximum":10.0}}}),
            serde_json::json!({"type":"object","properties":{"active":{"type":"boolean","default":true}}}),
            serde_json::json!({"type":"object","properties":{"tags":{"type":"array","minItems":1,"maxItems":2,"items":{"type":"string","enum":["x"]}}}}),
        ] {
            let schema: v2::ElicitationSchema = serde_json::from_value(value).expect("schema");
            assert!(super::is_supported_elicitation_schema(&schema));
        }
        for value in [
            serde_json::json!({"type":"object","properties":{"name":{"type":"string","pattern":"^a$"}}}),
            serde_json::json!({"type":"object","properties":{"name":{"type":"string","enum":["a"],"oneOf":[{"const":"b","title":"B"}]}}}),
            serde_json::json!({"type":"object","properties":{"name":{"type":"string","enum":[]}}}),
            serde_json::json!({"type":"object","properties":{"name":{"type":"string","oneOf":[]}}}),
            serde_json::json!({"type":"object","properties":{"loc":{"type":"location"}}}),
            serde_json::json!({"type":"object","properties":{"name":{"type":"string","minLength":5,"maxLength":2}}}),
            serde_json::json!({"type":"object","properties":{"count":{"type":"integer","minimum":9,"maximum":1}}}),
            serde_json::json!({"type":"object","properties":{"tags":{"type":"array","items":{"type":"string","enum":[]}}}}),
            serde_json::json!({"type":"object","properties":{"tokens":{"type":"array","items":{"type":"_token","enum":["repo"]}}}}),
        ] {
            let schema: v2::ElicitationSchema = serde_json::from_value(value).expect("schema");
            assert!(!super::is_supported_elicitation_schema(&schema));
        }
        let omit: v2::ElicitationSchema = serde_json::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "default": "SECRET-DEFAULT-VALUE-11" },
                "count": { "type": "integer", "default": 9 }
            }
        }))
        .expect("default schema");
        let content = super::elicitation_content_from_line("{}", &omit).expect("empty object");
        assert!(!content.contains_key("name") && !content.contains_key("count"));
    }

    #[test]
    fn elicitation_constraints_converter_valid_boundaries() {
        let schema: v2::ElicitationSchema = serde_json::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "minLength": 2, "maxLength": 4, "enum": ["ok", "long"] },
                "count": { "type": "integer", "minimum": 1, "maximum": 3 },
                "rating": { "type": "number", "minimum": 0.5, "maximum": 2.5 },
                "region": { "type": "string", "oneOf": [{ "const": "us", "title": "US" }] },
                "tags": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 2,
                    "items": { "type": "string", "enum": ["alpha", "beta"] }
                }
            }
        }))
        .expect("bounded schema");
        let got = super::elicitation_content_from_line(
            r#"{"name":"ok","count":3,"rating":0.5,"region":"us","tags":["alpha","beta"]}"#,
            &schema,
        )
        .expect("bounded values");
        assert_eq!(
            got.get("name"),
            Some(&v2::ElicitationContentValue::String("ok".into()))
        );
        assert_eq!(
            got.get("count"),
            Some(&v2::ElicitationContentValue::Integer(3))
        );
        assert_eq!(
            got.get("rating"),
            Some(&v2::ElicitationContentValue::Number(0.5))
        );
        assert_eq!(
            got.get("region"),
            Some(&v2::ElicitationContentValue::String("us".into()))
        );
        assert_eq!(
            got.get("tags"),
            Some(&v2::ElicitationContentValue::StringArray(vec![
                "alpha".into(),
                "beta".into()
            ]))
        );
        let opposite = super::elicitation_content_from_line(
            r#"{"name":"long","count":1,"rating":2.5,"region":"us","tags":["alpha"]}"#,
            &schema,
        )
        .expect("opposite bounded values");
        assert_eq!(
            opposite.get("name"),
            Some(&v2::ElicitationContentValue::String("long".into()))
        );
        assert_eq!(
            opposite.get("count"),
            Some(&v2::ElicitationContentValue::Integer(1))
        );
        assert_eq!(
            opposite.get("rating"),
            Some(&v2::ElicitationContentValue::Number(2.5))
        );
        assert_eq!(
            opposite.get("tags"),
            Some(&v2::ElicitationContentValue::StringArray(vec![
                "alpha".into()
            ]))
        );
    }

    #[test]
    fn elicitation_constraints_converter_safe_errors() {
        const SECRET: &str = "SECRET-SUPPLIED-VALUE-42";
        const WIRE: &str = "ok99";
        let schema: v2::ElicitationSchema = serde_json::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "minLength": 3, "maxLength": 5, "enum": [WIRE] },
                "count": { "type": "integer", "minimum": 2, "maximum": 4 },
                "rating": { "type": "number", "minimum": 1.0, "maximum": 5.0 },
                "tags": {
                    "type": "array",
                    "minItems": 2,
                    "maxItems": 3,
                    "items": { "type": "string", "enum": [WIRE] }
                }
            }
        }))
        .expect("bounded schema");
        for (line, expected) in [
            (
                format!(r#"{{"name":"x","count":3,"rating":3.0,"tags":["{WIRE}","{WIRE}"]}}"#),
                "name: too short",
            ),
            (
                format!(
                    r#"{{"name":"{SECRET}","count":3,"rating":3.0,"tags":["{WIRE}","{WIRE}"]}}"#
                ),
                "name: too long",
            ),
            (
                format!(
                    r#"{{"name":"{WIRE}","count":"{SECRET}","rating":3.0,"tags":["{WIRE}","{WIRE}"]}}"#
                ),
                "count: expected integer",
            ),
            (
                format!(r#"{{"name":"{WIRE}","count":3,"rating":3.0,"tags":["{WIRE}"]}}"#),
                "tags: too few items",
            ),
            (
                format!(
                    r#"{{"name":"{WIRE}","count":3,"rating":3.0,"tags":["{WIRE}","{WIRE}","{WIRE}","{WIRE}"]}}"#
                ),
                "tags: too many items",
            ),
            (
                r#"{"name":"bad","count":3,"rating":3.0,"tags":["WIRE","WIRE"]}"#
                    .replace("WIRE", WIRE),
                "name: invalid choice",
            ),
            (
                format!(r#"{{"name":"{WIRE}","count":9,"rating":3.0,"tags":["{WIRE}","{WIRE}"]}}"#),
                "count: above maximum",
            ),
            (
                format!(r#"{{"name":"{WIRE}","count":3,"rating":0.5,"tags":["{WIRE}","{WIRE}"]}}"#),
                "rating: below minimum",
            ),
            (
                format!(r#"{{"name":"{WIRE}","count":3,"rating":9.0,"tags":["{WIRE}","{WIRE}"]}}"#),
                "rating: above maximum",
            ),
        ] {
            let error = super::elicitation_content_from_line(&line, &schema).expect_err("invalid");
            assert_eq!(error, expected);
            assert!(!error.contains(SECRET));
            assert!(!error.contains(WIRE));
            assert!(!error.contains("bad"));
        }
    }

    #[test]
    fn prompt_failure_detail_does_not_include_peer_error() {
        const MARKER: &str = "SECRET-PEER-PROMPT-ERROR-42";
        let (host_fail_tx, mut host_fail_rx) = tokio::sync::mpsc::unbounded_channel();
        let result: agent_client_protocol::Result<()> =
            Err(agent_client_protocol::Error::internal_error().data(MARKER));
        if result.is_err() {
            let _ = host_fail_tx.send(super::acp_prompt_failure_detail());
        }
        let detail = host_fail_rx.try_recv().expect("prompt failure published");
        assert_eq!(detail, "ACP prompt failed.");
        assert!(!detail.contains(MARKER));
    }

    #[test]
    fn negotiation_accepts_v2() {
        let agent = Agent.v2().on_receive_request(
            async |request: v2::InitializeRequest, responder, _cx| {
                assert_eq!(request.protocol_version, ProtocolVersion::V2);
                responder.respond(v2::InitializeResponse::new(
                    ProtocolVersion::V2,
                    v2::Implementation::new("test-v2-agent", "1"),
                ))
            },
            agent_client_protocol::on_receive_request!(),
        );
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");

        runtime
            .block_on(negotiate_v2(agent))
            .expect("v2 negotiation should succeed");
    }

    #[test]
    fn negotiation_rejects_v1() {
        let agent = Agent.builder().on_receive_request(
            async |request: v1::InitializeRequest, responder, _cx| {
                responder.respond(v1::InitializeResponse::new(request.protocol_version))
            },
            agent_client_protocol::on_receive_request!(),
        );
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");

        let error = runtime
            .block_on(negotiate_v2(agent))
            .expect_err("v1-only peers must be rejected");
        assert!(
            error.to_string().contains("ACP v2")
                || error.to_string().contains("protocol version 2"),
            "{error}"
        );
    }
}

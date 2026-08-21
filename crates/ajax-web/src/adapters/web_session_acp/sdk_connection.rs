//! Official ACP SDK connection actor behind the synchronous Web Session hub.

use super::apply_model::{
    apply_config_option, apply_model_pin, sync_live_model_config, ApplyModelOutcome,
};
use super::client::{AcpClientEvent, HANDSHAKE_TIMEOUT};
use super::config_options::{
    mode_option, read_model_applied, replace_config_options, select_value_advertised,
    sync_session_result_config_options,
};
use agent_client_protocol::{
    on_receive_notification, on_receive_request,
    schema::{
        v1::{
            BooleanConfigOptionCapabilities, CancelNotification, ClientCapabilities,
            ClientSessionCapabilities, ContentBlock, Implementation, InitializeRequest,
            LoadSessionRequest, NewSessionRequest, PermissionOption, PermissionOptionKind,
            PromptRequest, RequestPermissionOutcome, RequestPermissionRequest,
            RequestPermissionResponse, ResumeSessionRequest, SelectedPermissionOutcome,
            SessionConfigKind, SessionConfigOption, SessionConfigOptionsCapabilities,
            SessionNotification, SessionUpdate, SetSessionConfigOptionRequest, TextContent,
        },
        ProtocolVersion,
    },
    Agent, Client, ConnectionTo, Lines, Responder, UntypedMessage,
};
use ajax_core::{adapters::acp_launch_for_agent, models::AgentClient};
use blocking::Unblock;
use futures::{AsyncBufReadExt, AsyncWriteExt, StreamExt};
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::{ChildStdin, ChildStdout},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
        Arc, Mutex,
    },
};
use tokio::sync::mpsc::UnboundedReceiver;

pub(super) struct ConnectionReady {
    pub session_id: String,
    pub session_new_result: Value,
    pub config_options: Option<Vec<SessionConfigOption>>,
    pub load_session_advertised: bool,
    pub resumed: bool,
    pub applied_model: String,
    pub model_apply_error: Option<String>,
}

pub(super) enum ClientCommand {
    Prompt {
        id: u64,
        text: String,
        result: Sender<Result<(), String>>,
    },
    Cancel {
        /// Request ids of the permission prompts this cancel answered, so the
        /// host can record them as resolved.
        result: Sender<Result<Vec<String>, String>>,
    },
    RespondPermission {
        request_id: String,
        approved: bool,
        result: Sender<Result<(), String>>,
    },
    ApplyModelPin {
        desired_model: String,
        result: Sender<Result<ApplyModelOutcome, String>>,
    },
    ApplyConfigOption {
        config_id: String,
        value: agent_client_protocol::schema::v1::SessionConfigOptionValue,
        result: Sender<Result<ApplyModelOutcome, String>>,
    },
    Shutdown,
}

struct LiveSession {
    session_id: String,
    session_result: Value,
    config_options: Option<Vec<SessionConfigOption>>,
    agent: AgentClient,
}

struct PendingPermission {
    responder: Responder<RequestPermissionResponse>,
    options: Vec<PermissionOption>,
}

type PendingPermissions = Arc<Mutex<HashMap<String, PendingPermission>>>;

pub(super) struct RunOptions {
    pub stdin: ChildStdin,
    pub stdout: ChildStdout,
    pub commands: UnboundedReceiver<ClientCommand>,
    pub events: Sender<AcpClientEvent>,
    pub ready: Sender<Result<ConnectionReady, String>>,
    pub busy: Arc<AtomicBool>,
    pub agent: AgentClient,
    pub cwd: PathBuf,
    /// Operator catalog pin used for in-band apply (may differ from spawn argv).
    pub apply_pin: Option<String>,
    pub resume_session_id: Option<String>,
}

/// The host currently implements permission replies only. Keep filesystem and
/// terminal capabilities false until their worktree-scoped handlers exist.
///
/// Advertise Cursor's vendor `parameterizedModelPicker` extra so the harness
/// exposes apply axes. The operator surface still lists one exploded catalog
/// id per effort, not a separate effort select
/// ([#979](https://github.com/mossipcams/ajax-cli/issues/979)).
pub(super) fn client_capabilities() -> ClientCapabilities {
    let mut meta = agent_client_protocol::schema::v1::Meta::new();
    meta.insert(
        "parameterizedModelPicker".to_string(),
        serde_json::Value::Bool(true),
    );
    ClientCapabilities::new()
        .session(ClientSessionCapabilities::new().config_options(
            SessionConfigOptionsCapabilities::new().boolean(BooleanConfigOptionCapabilities::new()),
        ))
        .meta(meta)
}

pub(super) fn run(options: RunOptions) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = options
                .ready
                .send(Err(format!("failed to start ACP runtime: {error}")));
            return;
        }
    };
    runtime.block_on(run_async(options));
}

async fn run_async(options: RunOptions) {
    let RunOptions {
        stdin,
        stdout,
        commands,
        events,
        ready,
        busy,
        agent,
        cwd,
        apply_pin,
        resume_session_id,
        ..
    } = options;
    let permissions: PendingPermissions = Arc::new(Mutex::new(HashMap::new()));
    let connection_events = events.clone();
    let notification_events = events.clone();
    let live_session: Arc<Mutex<Option<LiveSession>>> = Arc::new(Mutex::new(None));
    let live_for_notifications = Arc::clone(&live_session);
    let transport = traced_transport(stdin, stdout, events.clone());

    let connection_result = Client
        .builder()
        .name("ajax-web")
        .on_receive_notification(
            async move |notification: UntypedMessage, _connection| {
                if notification.method() != "session/update" {
                    return Ok(());
                }
                let params = notification.params().clone();
                let event = match serde_json::from_value::<SessionNotification>(params.clone()) {
                    Ok(notification) => {
                        if let SessionUpdate::ConfigOptionUpdate(update) = notification.update {
                            let applied = read_model_applied(Some(&update.config_options))
                                .unwrap_or_default();
                            if let Some(live) = live_for_notifications.lock().unwrap().as_mut() {
                                replace_config_options(
                                    &mut live.config_options,
                                    update.config_options.clone(),
                                );
                                sync_session_result_config_options(
                                    &mut live.session_result,
                                    &update.config_options,
                                );
                            }
                            AcpClientEvent::ConfigOptionsUpdated {
                                applied_model: applied,
                                config_options: update.config_options,
                            }
                        } else {
                            AcpClientEvent::SessionUpdate(Box::new(notification))
                        }
                    }
                    Err(_) => AcpClientEvent::UnknownSessionUpdate(params),
                };
                let _ = notification_events.send(event);
                Ok(())
            },
            on_receive_notification!(),
        )
        .on_receive_request(
            async move |request: RequestPermissionRequest, responder, _connection| {
                let request_id = responder.id().to_string();
                let outcome = auto_approve_permission(&request.options);
                if matches!(outcome, RequestPermissionOutcome::Cancelled) {
                    tracing::warn!(
                        target: "ajax_web",
                        request_id = %request_id,
                        "ACP permission request has no allow options; cancelling"
                    );
                }
                responder
                    .respond(RequestPermissionResponse::new(outcome))
                    .map_err(agent_client_protocol::Error::into_internal_error)?;
                Ok(())
            },
            on_receive_request!(),
        )
        .connect_with(transport, async move |connection| {
            let started = initialize_session(
                &connection,
                agent,
                &cwd,
                apply_pin.as_deref(),
                resume_session_id.as_deref(),
            )
            .await;
            let connection_ready = match started {
                Ok(started) => started,
                Err(error) => {
                    let _ = ready.send(Err(error));
                    return Ok(());
                }
            };
            let live = LiveSession {
                session_id: connection_ready.session_id.clone(),
                session_result: connection_ready.session_new_result.clone(),
                config_options: connection_ready.config_options.clone(),
                agent,
            };
            *live_session.lock().unwrap() = Some(live);
            if ready.send(Ok(connection_ready)).is_err() {
                return Ok(());
            }
            command_loop(
                connection,
                commands,
                connection_events.clone(),
                busy,
                permissions,
                live_session,
            )
            .await;
            Ok(())
        })
        .await;

    if let Err(error) = connection_result {
        let _ = events.send(AcpClientEvent::Error(format!(
            "ACP connection failed: {error}"
        )));
    }
    let _ = events.send(AcpClientEvent::Exited);
}

fn traced_transport(
    stdin: ChildStdin,
    stdout: ChildStdout,
    events: Sender<AcpClientEvent>,
) -> Lines<
    impl futures::Sink<String, Error = std::io::Error> + Send + 'static,
    impl futures::Stream<Item = std::io::Result<String>> + Send + 'static,
> {
    let incoming = futures::io::BufReader::new(Unblock::new(stdout))
        .lines()
        .inspect(move |line| {
            if let Ok(line) = line {
                if serde_json::from_str::<Value>(line).is_err() {
                    let _ = events.send(AcpClientEvent::Error(
                        "ACP agent wrote malformed JSON to stdout".to_string(),
                    ));
                }
            }
        });
    let outgoing = futures::sink::unfold(Unblock::new(stdin), async |mut stdin, line: String| {
        stdin.write_all(line.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        Ok::<_, std::io::Error>(stdin)
    });
    Lines::new(outgoing, incoming)
}

async fn initialize_session(
    connection: &ConnectionTo<Agent>,
    agent: AgentClient,
    cwd: &PathBuf,
    apply_pin: Option<&str>,
    resume_session_id: Option<&str>,
) -> Result<ConnectionReady, String> {
    let initialize = InitializeRequest::new(ProtocolVersion::V1)
        .client_capabilities(client_capabilities())
        .client_info(Implementation::new("ajax-web", env!("CARGO_PKG_VERSION")).title("Ajax Web"));
    let initialized = tokio::time::timeout(
        HANDSHAKE_TIMEOUT,
        connection.send_request(initialize).block_task(),
    )
    .await
    .map_err(|_| timeout_error("initialize"))?
    .map_err(|error| format!("ACP initialize failed: {error}"))?;
    if initialized.protocol_version != ProtocolVersion::V1 {
        return Err(format!(
            "unsupported ACP protocol version: {:?}",
            initialized.protocol_version
        ));
    }

    let load_session_advertised = initialized.agent_capabilities.load_session;
    let resume_advertised = initialized
        .agent_capabilities
        .session_capabilities
        .resume
        .is_some();
    let mut resumed = false;
    let mut session_id = None;
    let mut config_options = None;
    if let Some(resume_id) = resume_session_id {
        if resume_advertised {
            if let Some(response) = send_resume(connection, resume_id, cwd).await {
                resumed = true;
                config_options = response.config_options;
            }
        }
        if !resumed && load_session_advertised {
            if let Some(response) = send_load(connection, resume_id, cwd).await {
                resumed = true;
                config_options = response.config_options;
            }
        }
        if resumed {
            session_id = Some(resume_id.to_string());
        }
    }

    let (session_id, mut session_new_result) = match session_id {
        Some(session_id) => {
            let value = serde_json::json!({ "sessionId": session_id });
            (session_id, value)
        }
        None => {
            let response = tokio::time::timeout(
                HANDSHAKE_TIMEOUT,
                connection
                    .send_request(NewSessionRequest::new(cwd))
                    .block_task(),
            )
            .await
            .map_err(|_| timeout_error("session/new"))?
            .map_err(|error| format!("ACP session/new failed: {error}"))?;
            let value = serde_json::to_value(&response)
                .map_err(|error| format!("invalid session/new response: {error}"))?;
            config_options = response.config_options;
            (response.session_id.to_string(), value)
        }
    };
    if let Some(options) = config_options.as_deref() {
        sync_session_result_config_options(&mut session_new_result, options);
    }
    apply_permission_config(connection, agent, &session_id, config_options.as_deref()).await;
    let model_pins_at_spawn =
        acp_launch_for_agent(agent).is_some_and(|launch| launch.model_pins_at_spawn());
    let ApplyModelOutcome {
        applied_model,
        config_options: applied_config_options,
        error: model_apply_error,
    } = apply_model_pin(
        connection,
        &session_id,
        &session_new_result,
        config_options.as_deref(),
        apply_pin,
        model_pins_at_spawn,
    )
    .await;
    if let Some(options) = applied_config_options.as_deref() {
        config_options = applied_config_options.clone();
        sync_session_result_config_options(&mut session_new_result, options);
    }

    Ok(ConnectionReady {
        session_id,
        session_new_result,
        config_options,
        load_session_advertised,
        resumed,
        applied_model,
        model_apply_error,
    })
}

async fn send_resume(
    connection: &ConnectionTo<Agent>,
    session_id: &str,
    cwd: &Path,
) -> Option<agent_client_protocol::schema::v1::ResumeSessionResponse> {
    tokio::time::timeout(
        HANDSHAKE_TIMEOUT,
        connection
            .send_request(ResumeSessionRequest::new(
                session_id.to_string(),
                cwd.to_path_buf(),
            ))
            .block_task(),
    )
    .await
    .ok()?
    .ok()
}

async fn send_load(
    connection: &ConnectionTo<Agent>,
    session_id: &str,
    cwd: &Path,
) -> Option<agent_client_protocol::schema::v1::LoadSessionResponse> {
    tokio::time::timeout(
        HANDSHAKE_TIMEOUT,
        connection
            .send_request(LoadSessionRequest::new(
                session_id.to_string(),
                cwd.to_path_buf(),
            ))
            .block_task(),
    )
    .await
    .ok()?
    .ok()
}

/// Documented full-access mode select values, in preferred apply order.
const FULL_ACCESS_MODE_VALUES: &[&str] =
    &["agent-full-access", "bypassPermissions", "agent", "code"];

pub(super) fn preferred_permission_config(
    config_options: Option<&[SessionConfigOption]>,
) -> Option<(String, &'static str)> {
    let option = mode_option(config_options?)?;
    let SessionConfigKind::Select(select) = &option.kind else {
        return None;
    };
    let value = FULL_ACCESS_MODE_VALUES
        .iter()
        .find(|&&value| select_value_advertised(select, value))?;
    (select.current_value.0.as_ref() != *value).then_some((option.id.0.to_string(), *value))
}

async fn apply_permission_config(
    connection: &ConnectionTo<Agent>,
    agent: AgentClient,
    session_id: &str,
    config_options: Option<&[SessionConfigOption]>,
) {
    let Some((config_id, value)) = preferred_permission_config(config_options) else {
        return;
    };
    let result = tokio::time::timeout(
        HANDSHAKE_TIMEOUT,
        connection
            .send_request(SetSessionConfigOptionRequest::new(
                session_id.to_string(),
                config_id,
                value,
            ))
            .block_task(),
    )
    .await;
    if !matches!(result, Ok(Ok(_))) {
        tracing::warn!(target: "ajax_web", agent = ?agent, value, "ACP permission config refused");
    }
}

async fn command_loop(
    connection: ConnectionTo<Agent>,
    mut commands: UnboundedReceiver<ClientCommand>,
    events: Sender<AcpClientEvent>,
    busy: Arc<AtomicBool>,
    permissions: PendingPermissions,
    live_session: Arc<Mutex<Option<LiveSession>>>,
) {
    while let Some(command) = commands.recv().await {
        match command {
            ClientCommand::ApplyModelPin {
                desired_model,
                result,
            } => {
                let Some((agent, session_id, session_result, config_options)) =
                    live_session.lock().unwrap().as_ref().map(|live| {
                        (
                            live.agent,
                            live.session_id.clone(),
                            live.session_result.clone(),
                            live.config_options.clone(),
                        )
                    })
                else {
                    break;
                };
                let model_pins_at_spawn =
                    acp_launch_for_agent(agent).is_some_and(|launch| launch.model_pins_at_spawn());
                let outcome = apply_model_pin(
                    &connection,
                    &session_id,
                    &session_result,
                    config_options.as_deref(),
                    Some(&desired_model),
                    model_pins_at_spawn,
                )
                .await;
                if let Ok(mut live_guard) = live_session.lock() {
                    if let Some(live) = live_guard.as_mut() {
                        if outcome.error.is_none() {
                            if let Some(options) = outcome.config_options.clone() {
                                replace_config_options(&mut live.config_options, options.clone());
                                sync_session_result_config_options(
                                    &mut live.session_result,
                                    &options,
                                );
                            } else if let Some(options) = live.config_options.as_mut() {
                                sync_live_model_config(
                                    &mut live.session_result,
                                    options,
                                    &outcome.applied_model,
                                );
                            }
                        }
                    }
                }
                let _ = result.send(Ok(outcome));
            }
            ClientCommand::ApplyConfigOption {
                config_id,
                value,
                result,
            } => {
                let Some((session_id, config_options)) = live_session
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(|live| (live.session_id.clone(), live.config_options.clone()))
                else {
                    break;
                };
                let outcome = apply_config_option(
                    &connection,
                    &session_id,
                    &config_id,
                    value,
                    config_options.as_deref(),
                )
                .await;
                if outcome.error.is_none() {
                    if let Ok(mut live_guard) = live_session.lock() {
                        if let Some(live) = live_guard.as_mut() {
                            if let Some(options) = outcome.config_options.clone() {
                                replace_config_options(&mut live.config_options, options.clone());
                                sync_session_result_config_options(
                                    &mut live.session_result,
                                    &options,
                                );
                            } else if let Some(options) = live.config_options.as_mut() {
                                sync_live_model_config(
                                    &mut live.session_result,
                                    options,
                                    &outcome.applied_model,
                                );
                            }
                        }
                    }
                }
                let _ = result.send(Ok(outcome));
            }
            ClientCommand::Prompt { id, text, result } => {
                let mut live_guard = live_session.lock().unwrap();
                let Some(live) = live_guard.as_mut() else {
                    break;
                };
                let request = PromptRequest::new(
                    live.session_id.clone(),
                    vec![ContentBlock::Text(TextContent::new(text))],
                );
                let sent = connection.send_request(request);
                let prompt_events = events.clone();
                let prompt_busy = Arc::clone(&busy);
                let spawned = connection.spawn(async move {
                    let response = sent.block_task().await;
                    prompt_busy.store(false, Ordering::Release);
                    let result = response
                        .map_err(|error| error.to_string())
                        .and_then(|response| {
                            serde_json::to_value(response).map_err(|error| error.to_string())
                        });
                    let _ = prompt_events.send(AcpClientEvent::RequestFinished {
                        id,
                        method: "session/prompt",
                        result,
                    });
                    Ok(())
                });
                if let Err(error) = spawned {
                    busy.store(false, Ordering::Release);
                    let _ = result.send(Err(error.to_string()));
                } else {
                    let _ = result.send(Ok(()));
                }
            }
            ClientCommand::Cancel { result } => {
                let mut live_guard = live_session.lock().unwrap();
                let Some(live) = live_guard.as_mut() else {
                    break;
                };
                let cancelled = cancel_permissions(&permissions);
                let sent =
                    connection.send_notification(CancelNotification::new(live.session_id.clone()));
                let _ = result.send(match sent {
                    Ok(()) => Ok(cancelled),
                    Err(error) => Err(error.to_string()),
                });
            }
            ClientCommand::RespondPermission {
                request_id,
                approved,
                result,
            } => {
                let response = respond_permission(&permissions, &request_id, approved);
                let _ = result.send(response);
            }
            ClientCommand::Shutdown => {
                break;
            }
        }
    }
}

fn auto_approve_permission(options: &[PermissionOption]) -> RequestPermissionOutcome {
    allow_permission_outcome(options).unwrap_or(RequestPermissionOutcome::Cancelled)
}

fn allow_permission_outcome(options: &[PermissionOption]) -> Option<RequestPermissionOutcome> {
    for kind in [
        PermissionOptionKind::AllowAlways,
        PermissionOptionKind::AllowOnce,
    ] {
        if let Some(option) = options.iter().find(|option| option.kind == kind) {
            return Some(RequestPermissionOutcome::Selected(
                SelectedPermissionOutcome::new(option.option_id.clone()),
            ));
        }
    }
    None
}

fn permission_outcome_for(
    options: &[PermissionOption],
    approved: bool,
) -> Option<RequestPermissionOutcome> {
    if approved {
        return allow_permission_outcome(options);
    }
    let selected = options.iter().find(|option| {
        matches!(
            option.kind,
            PermissionOptionKind::RejectOnce | PermissionOptionKind::RejectAlways
        )
    })?;
    Some(RequestPermissionOutcome::Selected(
        SelectedPermissionOutcome::new(selected.option_id.clone()),
    ))
}

fn respond_permission(
    permissions: &PendingPermissions,
    request_id: &str,
    approved: bool,
) -> Result<(), String> {
    let pending = permissions
        .lock()
        .unwrap()
        .remove(request_id)
        .ok_or_else(|| "ACP permission request is no longer pending".to_string())?;
    let outcome = permission_outcome_for(&pending.options, approved)
        .unwrap_or(RequestPermissionOutcome::Cancelled);
    pending
        .responder
        .respond(RequestPermissionResponse::new(outcome))
        .map_err(|error| error.to_string())
}

/// Answer every pending permission request with the cancelled outcome and
/// report which ones were answered.
fn cancel_permissions(permissions: &PendingPermissions) -> Vec<String> {
    let pending: Vec<_> = permissions.lock().unwrap().drain().collect();
    pending
        .into_iter()
        .map(|(request_id, item)| {
            let _ = item.responder.respond(RequestPermissionResponse::new(
                RequestPermissionOutcome::Cancelled,
            ));
            request_id
        })
        .collect()
}

fn timeout_error(method: &str) -> String {
    format!("{method} timed out after {}s", HANDSHAKE_TIMEOUT.as_secs())
}

//! Official ACP SDK connection actor behind the synchronous Web Session hub.

use super::client::{AcpClientEvent, HANDSHAKE_TIMEOUT};
use agent_client_protocol::{
    on_receive_notification, on_receive_request,
    schema::{
        v1::{
            CancelNotification, ClientCapabilities, ContentBlock, Implementation,
            InitializeRequest, LoadSessionRequest, NewSessionRequest, PermissionOption,
            PermissionOptionKind, PromptRequest, RequestPermissionOutcome,
            RequestPermissionRequest, RequestPermissionResponse, ResumeSessionRequest,
            SelectedPermissionOutcome, SessionNotification, SetSessionConfigOptionRequest,
            TextContent,
        },
        ProtocolVersion,
    },
    Agent, Client, ConnectionTo, Lines, Responder, UntypedMessage,
};
use ajax_core::{
    adapters::{acp_launch_for_agent, parse_model_selection, AcpModelSelection},
    models::AgentClient,
};
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
    pub load_session_advertised: bool,
    pub resumed: bool,
}

pub(super) enum ClientCommand {
    Prompt {
        id: u64,
        text: String,
        result: Sender<Result<(), String>>,
    },
    Cancel {
        result: Sender<Result<(), String>>,
    },
    RespondPermission {
        request_id: String,
        approved: bool,
        result: Sender<Result<(), String>>,
    },
    Shutdown,
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
    pub model: Option<String>,
    pub resume_session_id: Option<String>,
}

/// The host currently implements permission replies only. Keep filesystem and
/// terminal capabilities false until their worktree-scoped handlers exist.
pub(super) fn client_capabilities() -> ClientCapabilities {
    ClientCapabilities::default()
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
        model,
        resume_session_id,
    } = options;
    let permissions: PendingPermissions = Arc::new(Mutex::new(HashMap::new()));
    let connection_events = events.clone();
    let notification_events = events.clone();
    let permission_events = events.clone();
    let permission_store = Arc::clone(&permissions);
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
                    Ok(notification) => AcpClientEvent::SessionUpdate(Box::new(notification)),
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
                let id = serde_json::to_value(responder.id())
                    .map_err(agent_client_protocol::Error::into_internal_error)?;
                let params = serde_json::to_value(&request)
                    .map_err(agent_client_protocol::Error::into_internal_error)?;
                permission_store.lock().unwrap().insert(
                    request_id,
                    PendingPermission {
                        responder,
                        options: request.options.clone(),
                    },
                );
                let _ = permission_events.send(AcpClientEvent::ClientRequest {
                    id,
                    method: "session/request_permission".to_string(),
                    params,
                });
                Ok(())
            },
            on_receive_request!(),
        )
        .connect_with(transport, async move |connection| {
            let started = initialize_session(
                &connection,
                agent,
                &cwd,
                model.as_deref(),
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
            let session_id = connection_ready.session_id.clone();
            if ready.send(Ok(connection_ready)).is_err() {
                return Ok(());
            }
            command_loop(
                connection,
                commands,
                connection_events.clone(),
                busy,
                permissions,
                session_id,
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
    model: Option<&str>,
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
    if let Some(resume_id) = resume_session_id {
        let mut restored = false;
        if resume_advertised {
            restored = send_resume(connection, resume_id, cwd).await;
        }
        if !restored && load_session_advertised {
            restored = send_load(connection, resume_id, cwd).await;
        }
        if restored {
            resumed = true;
            session_id = Some(resume_id.to_string());
        }
    }

    let (session_id, session_new_result) = match session_id {
        Some(session_id) => (session_id, Value::Null),
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
            (response.session_id.to_string(), value)
        }
    };
    apply_model(connection, agent, &session_id, model).await;

    Ok(ConnectionReady {
        session_id,
        session_new_result,
        load_session_advertised,
        resumed,
    })
}

async fn send_resume(connection: &ConnectionTo<Agent>, session_id: &str, cwd: &Path) -> bool {
    matches!(
        tokio::time::timeout(
            HANDSHAKE_TIMEOUT,
            connection
                .send_request(ResumeSessionRequest::new(
                    session_id.to_string(),
                    cwd.to_path_buf()
                ))
                .block_task(),
        )
        .await,
        Ok(Ok(_))
    )
}

async fn send_load(connection: &ConnectionTo<Agent>, session_id: &str, cwd: &Path) -> bool {
    matches!(
        tokio::time::timeout(
            HANDSHAKE_TIMEOUT,
            connection
                .send_request(LoadSessionRequest::new(
                    session_id.to_string(),
                    cwd.to_path_buf()
                ))
                .block_task(),
        )
        .await,
        Ok(Ok(_))
    )
}

async fn apply_model(
    connection: &ConnectionTo<Agent>,
    agent: AgentClient,
    session_id: &str,
    model: Option<&str>,
) {
    let Some(raw) = model.map(str::trim).filter(|model| !model.is_empty()) else {
        return;
    };
    let Some(launch) = acp_launch_for_agent(agent) else {
        return;
    };
    if raw == "auto" || matches!(launch.model_selection, AcpModelSelection::SpawnArg) {
        return;
    }
    let Some(selection) = parse_model_selection(raw) else {
        return;
    };
    let mut settings = vec![("model".to_string(), selection.model)];
    settings.extend(selection.options);
    for (config_id, value) in settings {
        let request = SetSessionConfigOptionRequest::new(
            session_id.to_string(),
            config_id.clone(),
            value.as_str(),
        );
        let result = tokio::time::timeout(
            HANDSHAKE_TIMEOUT,
            connection.send_request(request).block_task(),
        )
        .await;
        if !matches!(result, Ok(Ok(_))) {
            tracing::warn!(
                target: "ajax_web",
                agent = ?agent,
                config_id = %config_id,
                "ACP model selection refused"
            );
        }
    }
}

async fn command_loop(
    connection: ConnectionTo<Agent>,
    mut commands: UnboundedReceiver<ClientCommand>,
    events: Sender<AcpClientEvent>,
    busy: Arc<AtomicBool>,
    permissions: PendingPermissions,
    session_id: String,
) {
    while let Some(command) = commands.recv().await {
        match command {
            ClientCommand::Prompt { id, text, result } => {
                let request = PromptRequest::new(
                    session_id.clone(),
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
                cancel_permissions(&permissions);
                let sent = connection
                    .send_notification(CancelNotification::new(session_id.clone()))
                    .map_err(|error| error.to_string());
                let _ = result.send(sent);
            }
            ClientCommand::RespondPermission {
                request_id,
                approved,
                result,
            } => {
                let response = respond_permission(&permissions, &request_id, approved);
                let _ = result.send(response);
            }
            ClientCommand::Shutdown => break,
        }
    }
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
    let selected = pending.options.iter().find(|option| {
        matches!(
            (approved, option.kind),
            (
                true,
                PermissionOptionKind::AllowOnce | PermissionOptionKind::AllowAlways
            ) | (
                false,
                PermissionOptionKind::RejectOnce | PermissionOptionKind::RejectAlways
            )
        )
    });
    let outcome = selected.map_or(RequestPermissionOutcome::Cancelled, |option| {
        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(option.option_id.clone()))
    });
    pending
        .responder
        .respond(RequestPermissionResponse::new(outcome))
        .map_err(|error| error.to_string())
}

fn cancel_permissions(permissions: &PendingPermissions) {
    let pending: Vec<_> = permissions
        .lock()
        .unwrap()
        .drain()
        .map(|(_, item)| item)
        .collect();
    for item in pending {
        let _ = item.responder.respond(RequestPermissionResponse::new(
            RequestPermissionOutcome::Cancelled,
        ));
    }
}

fn timeout_error(method: &str) -> String {
    format!("{method} timed out after {}s", HANDSHAKE_TIMEOUT.as_secs())
}

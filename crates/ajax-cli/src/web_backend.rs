use ajax_core::{
    adapters::CommandRunner,
    commands::{self, CommandContext},
    models::OperatorAction,
    output::{CockpitView, InboxResponse, ReposResponse, TaskCard},
    registry::InMemoryRegistry,
    runtime_refresh::refresh_runtime_context,
    task_operations::task_command::{
        execute_task_command_operation, plan_task_command_operation, TaskCommandKind,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{
    command_error,
    context::{load_context, save_context},
    dispatch::execute_observed_drop,
    web_companion_push, web_companion_tls, CliContextPaths, CliError,
};

pub(crate) struct HttpResponse {
    pub(crate) status_code: u16,
    pub(crate) content_type: &'static str,
    pub(crate) body: Vec<u8>,
}

/// A compiled-in static web asset served from `crates/ajax-cli/web/`.
struct StaticAsset {
    content_type: &'static str,
    body: &'static [u8],
}

/// Routes a request path to a compiled-in static asset, if one exists.
fn static_asset(path: &str) -> Option<StaticAsset> {
    match path {
        "/app.css" => Some(StaticAsset {
            content_type: "text/css; charset=utf-8",
            body: include_bytes!("../web/app.css"),
        }),
        "/app.js" => Some(StaticAsset {
            content_type: "text/javascript; charset=utf-8",
            body: include_bytes!("../web/app.js"),
        }),
        "/manifest.webmanifest" => Some(StaticAsset {
            content_type: "application/manifest+json; charset=utf-8",
            body: include_bytes!("../web/manifest.webmanifest"),
        }),
        "/sw.js" => Some(StaticAsset {
            content_type: "text/javascript; charset=utf-8",
            body: include_bytes!("../web/sw.js"),
        }),
        "/icons/icon-192.png" => Some(StaticAsset {
            content_type: "image/png",
            body: include_bytes!("../web/icons/icon-192.png"),
        }),
        "/icons/icon-512.png" => Some(StaticAsset {
            content_type: "image/png",
            body: include_bytes!("../web/icons/icon-512.png"),
        }),
        "/icons/icon-maskable-512.png" => Some(StaticAsset {
            content_type: "image/png",
            body: include_bytes!("../web/icons/icon-maskable-512.png"),
        }),
        "/icons/apple-touch-icon.png" => Some(StaticAsset {
            content_type: "image/png",
            body: include_bytes!("../web/icons/apple-touch-icon.png"),
        }),
        _ => None,
    }
}

#[derive(Serialize)]
struct MobileCockpitView {
    repos: ReposResponse,
    cards: Vec<MobileTaskCard>,
    inbox: InboxResponse,
}

#[derive(Serialize)]
struct MobileTaskCard {
    id: String,
    qualified_handle: String,
    title: String,
    ui_state: String,
    status_label: String,
    lifecycle: String,
    primary_action: String,
    available_actions: Vec<String>,
    live_summary: Option<String>,
}

#[derive(Deserialize)]
struct MobileActionRequest {
    task_handle: String,
    action: String,
}

pub(crate) fn render_mobile_shell() -> String {
    include_str!("../web/index.html").to_string()
}

pub(crate) fn cockpit_json(
    context: &CommandContext<InMemoryRegistry>,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&mobile_cockpit_view(context))
}

pub(crate) fn handle_http_request(
    method: &str,
    path: &str,
    _body: &str,
    context: &CommandContext<InMemoryRegistry>,
) -> Result<HttpResponse, serde_json::Error> {
    if method != "GET" {
        return Ok(text_response(405, "method not allowed"));
    }

    match path {
        "/" => Ok(HttpResponse {
            status_code: 200,
            content_type: "text/html; charset=utf-8",
            body: render_mobile_shell().into_bytes(),
        }),
        "/api/cockpit" => Ok(HttpResponse {
            status_code: 200,
            content_type: "application/json; charset=utf-8",
            body: cockpit_json(context)?.into_bytes(),
        }),
        _ => match static_asset(path) {
            Some(asset) => Ok(HttpResponse {
                status_code: 200,
                content_type: asset.content_type,
                body: asset.body.to_vec(),
            }),
            None => Ok(text_response(404, "not found")),
        },
    }
}

pub(crate) fn handle_http_request_with_runner_and_paths(
    method: &str,
    path: &str,
    body: &str,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut impl CommandRunner,
    paths: Option<&CliContextPaths>,
) -> Result<HttpResponse, CliError> {
    match (method, path) {
        ("POST", "/api/actions") => return handle_action_request(body, context, runner, paths),
        ("GET", "/api/cockpit") => {
            return handle_refreshed_cockpit_request(context, runner, paths);
        }
        ("GET", "/api/push/config") => return handle_push_config(paths),
        ("POST", "/api/push/subscribe") => return handle_push_subscribe(body, paths),
        ("POST", "/api/push/unsubscribe") => return handle_push_unsubscribe(body, paths),
        _ => {}
    }

    handle_http_request(method, path, body, context)
        .map_err(|error| CliError::JsonSerialization(error.to_string()))
}

/// Serves the VAPID application server key the browser needs to subscribe.
fn handle_push_config(paths: Option<&CliContextPaths>) -> Result<HttpResponse, CliError> {
    let dir = companion_state_dir(paths)?;
    let keys = web_companion_push::load_or_create_vapid_keys(&dir)?;
    json_response(
        200,
        serde_json::json!({
            "public_key": keys.public_key,
        }),
    )
}

/// Stores a browser push subscription so the companion can notify it later.
fn handle_push_subscribe(
    body: &str,
    paths: Option<&CliContextPaths>,
) -> Result<HttpResponse, CliError> {
    let subscription: web_companion_push::PushSubscription = match serde_json::from_str(body) {
        Ok(subscription) => subscription,
        Err(error) => {
            return json_response(
                400,
                serde_json::json!({ "ok": false, "error": error.to_string() }),
            );
        }
    };
    let dir = companion_state_dir(paths)?;
    web_companion_push::add_subscription(&dir, subscription)?;
    json_response(200, serde_json::json!({ "ok": true }))
}

/// Removes a browser push subscription.
fn handle_push_unsubscribe(
    body: &str,
    paths: Option<&CliContextPaths>,
) -> Result<HttpResponse, CliError> {
    let request: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    let Some(endpoint) = request.get("endpoint").and_then(serde_json::Value::as_str) else {
        return json_response(
            400,
            serde_json::json!({ "ok": false, "error": "endpoint is required" }),
        );
    };
    let dir = companion_state_dir(paths)?;
    web_companion_push::remove_subscription(&dir, endpoint)?;
    json_response(200, serde_json::json!({ "ok": true }))
}

fn handle_refreshed_cockpit_request(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut impl CommandRunner,
    paths: Option<&CliContextPaths>,
) -> Result<HttpResponse, CliError> {
    if let Some(paths) = paths {
        *context = load_context(paths)?;
    }
    let state_changed = refresh_runtime_context(context, runner).map_err(command_error)?;
    if state_changed {
        if let Some(paths) = paths {
            save_context(paths, context)?;
        }
    }
    json_response(
        200,
        serde_json::to_value(mobile_cockpit_view(context))
            .map_err(|error| CliError::JsonSerialization(error.to_string()))?,
    )
}

fn handle_action_request(
    body: &str,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut impl CommandRunner,
    paths: Option<&CliContextPaths>,
) -> Result<HttpResponse, CliError> {
    let request: MobileActionRequest = serde_json::from_str(body)
        .map_err(|error| CliError::JsonSerialization(error.to_string()))?;
    let Some(action) = OperatorAction::from_label(&request.action) else {
        return json_response(
            400,
            serde_json::json!({
                "ok": false,
                "error": format!("unknown action: {}", request.action),
            }),
        );
    };

    if action == OperatorAction::Resume {
        return json_response(
            409,
            serde_json::json!({
                "ok": false,
                "error": "resume requires native cockpit task entry",
            }),
        );
    }
    if action == OperatorAction::Start {
        return json_response(
            400,
            serde_json::json!({
                "ok": false,
                "error": "start requires task title input",
            }),
        );
    }

    let state_changed = execute_mobile_action(action, &request.task_handle, context, runner)?;
    if state_changed {
        if let Some(paths) = paths {
            save_context(paths, context)?;
        }
    }
    json_response(
        200,
        serde_json::json!({
            "ok": true,
            "state_changed": state_changed,
            "cockpit": mobile_cockpit_view(context),
        }),
    )
}

pub(crate) fn serve_mobile_web(
    host: &str,
    port: u16,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut impl CommandRunner,
) -> Result<(), CliError> {
    serve_mobile_web_with_paths(host, port, context, runner, None)
}

pub(crate) fn serve_mobile_web_with_paths(
    host: &str,
    port: u16,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut impl CommandRunner,
    paths: Option<&CliContextPaths>,
) -> Result<(), CliError> {
    let cert_dir = companion_state_dir(paths)?;
    let identity = web_companion_tls::load_or_create_identity(&cert_dir)?;
    let tls_config = web_companion_tls::tls_server_config(&identity)?;

    let listener = TcpListener::bind((host, port))
        .map_err(|error| CliError::CommandFailed(format!("web bind failed: {error}")))?;
    eprintln!("Ajax mobile web listening on https://{host}:{port}");

    // The connection loop and the attention poller share one context, so it is
    // guarded by a mutex. A scoped thread lets the poller borrow it without
    // requiring a `'static` clone.
    let shared = Mutex::new(context);
    std::thread::scope(|scope| {
        let poller_state = &shared;
        let poller_dir = cert_dir.clone();
        scope.spawn(move || run_attention_poller(poller_state, &poller_dir));

        // A failed TLS handshake or connection error must not take down the
        // companion: log it and keep accepting connections.
        for stream in listener.incoming() {
            let stream = match stream {
                Ok(stream) => stream,
                Err(error) => {
                    eprintln!("Ajax web accept error: {error}");
                    continue;
                }
            };
            let mut guard = shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let context: &mut CommandContext<InMemoryRegistry> = &mut guard;
            if let Err(error) = serve_tls_connection(stream, &tls_config, context, runner, paths) {
                eprintln!("Ajax web connection error: {error}");
            }
        }
    });

    Ok(())
}

/// Interval between attention-poll cycles.
const ATTENTION_POLL_INTERVAL: Duration = Duration::from_secs(15);

/// Periodically rebuilds the cockpit view and sends a push notification for
/// every task that has newly entered the attention inbox.
fn run_attention_poller(state: &Mutex<&mut CommandContext<InMemoryRegistry>>, dir: &Path) {
    let mut known: HashSet<String> = HashSet::new();
    loop {
        std::thread::sleep(ATTENTION_POLL_INTERVAL);
        let current = {
            let guard = state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            attention_handles(&commands::rebuild_cockpit_view(&**guard))
        };
        for handle in newly_attention(&known, &current) {
            let notification = web_companion_push::PushNotification {
                title: "Ajax task needs attention".to_string(),
                body: handle.clone(),
                tag: handle.clone(),
            };
            if let Err(error) = web_companion_push::send_to_all(dir, &notification) {
                eprintln!("Ajax web push notification failed: {error}");
            }
        }
        known = current;
    }
}

/// The set of task handles currently in the attention inbox.
fn attention_handles(view: &CockpitView) -> HashSet<String> {
    view.inbox
        .items
        .iter()
        .map(|item| item.task_handle.clone())
        .collect()
}

/// The handles present in `current` but not in `previous`, sorted for
/// deterministic notification ordering.
fn newly_attention(previous: &HashSet<String>, current: &HashSet<String>) -> Vec<String> {
    let mut added: Vec<String> = current.difference(previous).cloned().collect();
    added.sort();
    added
}

/// Resolves the directory the companion persists its files in (TLS identity,
/// VAPID keys, push subscriptions): the directory holding the Ajax state
/// database.
fn companion_state_dir(paths: Option<&CliContextPaths>) -> Result<PathBuf, CliError> {
    let state_file = match paths {
        Some(paths) => paths.state_file.clone(),
        None => crate::context::default_context_paths()?.state_file,
    };
    state_file
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| CliError::CommandFailed("web companion directory unresolved".to_string()))
}

fn serve_tls_connection(
    tcp: TcpStream,
    tls_config: &Arc<rustls::ServerConfig>,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut impl CommandRunner,
    paths: Option<&CliContextPaths>,
) -> Result<(), CliError> {
    let connection = rustls::ServerConnection::new(Arc::clone(tls_config))
        .map_err(|error| CliError::CommandFailed(format!("web tls session failed: {error}")))?;
    let stream = rustls::StreamOwned::new(connection, tcp);
    serve_connection(stream, context, runner, paths)
}

fn serve_connection<S: Read + Write>(
    mut stream: S,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut impl CommandRunner,
    paths: Option<&CliContextPaths>,
) -> Result<(), CliError> {
    let mut buffer = [0_u8; 8192];
    let bytes_read = stream
        .read(&mut buffer)
        .map_err(|error| CliError::CommandFailed(format!("web request read failed: {error}")))?;
    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    let Some(request_line) = request.lines().next() else {
        return write_http_response(stream, text_response(400, "bad request"));
    };
    let mut parts = request_line.split_whitespace();
    let Some(method) = parts.next() else {
        return write_http_response(stream, text_response(400, "bad request"));
    };
    let Some(path) = parts.next() else {
        return write_http_response(stream, text_response(400, "bad request"));
    };
    let path = path.split('?').next().unwrap_or(path);
    let body = request.split("\r\n\r\n").nth(1).unwrap_or("");
    let response =
        handle_http_request_with_runner_and_paths(method, path, body, context, runner, paths)?;

    write_http_response(stream, response)
}

fn write_http_response<S: Write>(mut stream: S, response: HttpResponse) -> Result<(), CliError> {
    let status_text = match response.status_code {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Internal Server Error",
    };
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status_code,
        status_text,
        response.content_type,
        response.body.len()
    );
    stream
        .write_all(head.as_bytes())
        .and_then(|_| stream.write_all(&response.body))
        .and_then(|_| stream.flush())
        .map_err(|error| CliError::CommandFailed(format!("web response write failed: {error}")))
}

fn text_response(status_code: u16, body: impl Into<String>) -> HttpResponse {
    HttpResponse {
        status_code,
        content_type: "text/plain; charset=utf-8",
        body: body.into().into_bytes(),
    }
}

fn json_response(status_code: u16, value: serde_json::Value) -> Result<HttpResponse, CliError> {
    Ok(HttpResponse {
        status_code,
        content_type: "application/json; charset=utf-8",
        body: serde_json::to_vec(&value)
            .map_err(|error| CliError::JsonSerialization(error.to_string()))?,
    })
}

fn execute_mobile_action(
    action: OperatorAction,
    task_handle: &str,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut impl CommandRunner,
) -> Result<bool, CliError> {
    if action == OperatorAction::Drop {
        return execute_observed_drop(context, task_handle, true, runner)
            .map(|rendered| rendered.state_changed);
    }

    let kind = match action {
        OperatorAction::Review => TaskCommandKind::Review,
        OperatorAction::Ship => TaskCommandKind::Ship,
        OperatorAction::Repair => TaskCommandKind::Repair,
        OperatorAction::Start | OperatorAction::Resume | OperatorAction::Drop => {
            return Err(CliError::CommandFailed(format!(
                "unsupported mobile action: {}",
                action.as_str()
            )));
        }
    };
    let plan = plan_task_command_operation(context, kind, task_handle, commands::OpenMode::Attach)
        .map_err(command_error)?;
    let confirmed = !plan.requires_confirmation;
    execute_task_command_operation(context, kind, task_handle, &plan, confirmed, runner)
        .map(|(_outputs, state_changed)| state_changed)
        .map_err(|(error, state_changed)| {
            let error = command_error(error);
            if state_changed {
                error.after_state_change()
            } else {
                error
            }
        })
}

fn mobile_cockpit_view(context: &CommandContext<InMemoryRegistry>) -> MobileCockpitView {
    let view = commands::rebuild_cockpit_view(context);
    MobileCockpitView {
        repos: view.repos,
        cards: view.cards.iter().map(mobile_task_card).collect(),
        inbox: view.inbox,
    }
}

fn mobile_task_card(card: &TaskCard) -> MobileTaskCard {
    MobileTaskCard {
        id: card.id.as_str().to_string(),
        qualified_handle: card.qualified_handle.clone(),
        title: card.title.clone(),
        ui_state: card.ui_state.as_str().to_string(),
        status_label: card.status_label.clone(),
        lifecycle: format!("{:?}", card.lifecycle),
        primary_action: card.primary_action.as_str().to_string(),
        available_actions: card
            .available_actions
            .iter()
            .map(|action| action.as_str().to_string())
            .collect(),
        live_summary: card.live_summary.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        cockpit_json, handle_http_request, handle_http_request_with_runner_and_paths,
        newly_attention, render_mobile_shell, serve_connection,
    };
    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::io::{Cursor, Read, Write};
    use std::rc::Rc;

    /// An in-memory bidirectional stream for exercising the generic connection
    /// path without binding a socket.
    struct MockStream {
        input: Cursor<Vec<u8>>,
        output: Rc<RefCell<Vec<u8>>>,
    }

    impl Read for MockStream {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.input.read(buf)
        }
    }

    impl Write for MockStream {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.output.borrow_mut().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    use ajax_core::{
        adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec},
        commands::CommandContext,
        config::{Config, ManagedRepo},
        models::{
            AgentClient, GitStatus, LifecycleStatus, Task, TaskId, TmuxStatus, WorktrunkStatus,
        },
        registry::{InMemoryRegistry, Registry, RegistryStore, SqliteRegistryStore},
    };

    #[test]
    fn mobile_shell_is_responsive_and_loads_cockpit_data() {
        let html = render_mobile_shell();

        assert!(html.contains("<!doctype html>"));
        assert!(html.contains("name=\"viewport\""));
        assert!(html.contains("width=device-width"));
        assert!(html.contains("href=\"/app.css\""));
        assert!(html.contains("src=\"/app.js\""));
    }

    #[test]
    fn mobile_shell_exposes_redesigned_structure() {
        let html = render_mobile_shell();

        assert!(html.contains("id=\"inbox\""));
        assert!(html.contains("id=\"repos\""));
        assert!(html.contains("id=\"offline-banner\""));
        assert!(html.contains("id=\"install-button\""));
        assert!(html.contains("id=\"refresh-button\""));
    }

    #[test]
    fn cockpit_json_serializes_the_current_cockpit_projection() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let json = cockpit_json(&context).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["repos"]["repos"], serde_json::json!([]));
        assert_eq!(value["cards"], serde_json::json!([]));
        assert_eq!(value["inbox"]["items"], serde_json::json!([]));
    }

    #[test]
    fn http_router_serves_mobile_shell_and_cockpit_json() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

        let shell = handle_http_request("GET", "/", "", &context).unwrap();
        assert_eq!(shell.status_code, 200);
        assert_eq!(shell.content_type, "text/html; charset=utf-8");
        assert!(String::from_utf8_lossy(&shell.body).contains("Ajax Mobile Cockpit"));

        let cockpit = handle_http_request("GET", "/api/cockpit", "", &context).unwrap();
        assert_eq!(cockpit.status_code, 200);
        assert_eq!(cockpit.content_type, "application/json; charset=utf-8");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&cockpit.body).unwrap()["cards"],
            serde_json::json!([])
        );
    }

    #[test]
    fn http_router_serves_static_css_and_js() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

        let css = handle_http_request("GET", "/app.css", "", &context).unwrap();
        assert_eq!(css.status_code, 200);
        assert_eq!(css.content_type, "text/css; charset=utf-8");
        assert!(!css.body.is_empty());

        let js = handle_http_request("GET", "/app.js", "", &context).unwrap();
        assert_eq!(js.status_code, 200);
        assert_eq!(js.content_type, "text/javascript; charset=utf-8");
        assert!(!js.body.is_empty());
    }

    #[test]
    fn http_router_serves_web_manifest() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

        let manifest = handle_http_request("GET", "/manifest.webmanifest", "", &context).unwrap();
        assert_eq!(manifest.status_code, 200);
        assert_eq!(
            manifest.content_type,
            "application/manifest+json; charset=utf-8"
        );

        let value: serde_json::Value = serde_json::from_slice(&manifest.body).unwrap();
        assert!(value["name"].is_string());
        assert_eq!(value["display"], "standalone");
        assert!(value["start_url"].is_string());
        assert!(value["icons"]
            .as_array()
            .is_some_and(|icons| !icons.is_empty()));
    }

    #[test]
    fn http_router_serves_app_icons() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

        for path in [
            "/icons/icon-192.png",
            "/icons/icon-512.png",
            "/icons/icon-maskable-512.png",
            "/icons/apple-touch-icon.png",
        ] {
            let icon = handle_http_request("GET", path, "", &context).unwrap();
            assert_eq!(icon.status_code, 200, "{path}");
            assert_eq!(icon.content_type, "image/png", "{path}");
            assert!(
                icon.body
                    .starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]),
                "{path} is not a PNG"
            );
        }
    }

    #[test]
    fn app_script_wires_cockpit_actions_and_install_prompt() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

        let app = handle_http_request("GET", "/app.js", "", &context).unwrap();
        let script = String::from_utf8_lossy(&app.body);
        assert!(script.contains("/api/cockpit"));
        assert!(script.contains("cache: \"no-store\""));
        assert!(script.contains("const REFRESH_INTERVAL_MS = 1000"));
        assert!(script.contains("refreshInFlight"));
        assert!(script.contains("/api/actions"));
        assert!(script.contains("beforeinstallprompt"));
    }

    #[test]
    fn service_worker_and_app_handle_push_notifications() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

        let sw = handle_http_request("GET", "/sw.js", "", &context).unwrap();
        let sw_text = String::from_utf8_lossy(&sw.body);
        assert!(sw_text.contains("ajax-cockpit-v2"));
        assert!(sw_text.contains("\"push\""));
        assert!(sw_text.contains("notificationclick"));
        assert!(sw_text.contains("showNotification"));

        let app = handle_http_request("GET", "/app.js", "", &context).unwrap();
        let app_text = String::from_utf8_lossy(&app.body);
        assert!(app_text.contains("pushManager.subscribe"));
        assert!(app_text.contains("/api/push/config"));
        assert!(app_text.contains("/api/push/subscribe"));
    }

    #[test]
    fn http_router_serves_service_worker_and_app_registers_it() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

        let sw = handle_http_request("GET", "/sw.js", "", &context).unwrap();
        assert_eq!(sw.status_code, 200);
        assert_eq!(sw.content_type, "text/javascript; charset=utf-8");
        assert!(!sw.body.is_empty());

        let app = handle_http_request("GET", "/app.js", "", &context).unwrap();
        assert!(String::from_utf8_lossy(&app.body).contains("serviceWorker.register"));
    }

    #[test]
    fn http_router_reports_unknown_routes_and_unsupported_methods() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

        let missing = handle_http_request("GET", "/missing", "", &context).unwrap();
        assert_eq!(missing.status_code, 404);
        assert!(String::from_utf8_lossy(&missing.body).contains("not found"));

        let unsupported = handle_http_request("POST", "/", "", &context).unwrap();
        assert_eq!(unsupported.status_code, 405);
        assert!(String::from_utf8_lossy(&unsupported.body).contains("method not allowed"));
    }

    #[test]
    fn action_endpoint_guards_resume_for_native_cockpit() {
        let mut context = reviewable_context();
        let mut runner = OkRunner;

        let response = handle_http_request_with_runner_and_paths(
            "POST",
            "/api/actions",
            r#"{"task_handle":"web/fix-login","action":"resume"}"#,
            &mut context,
            &mut runner,
            None,
        )
        .unwrap();

        assert_eq!(response.status_code, 409);
        assert!(String::from_utf8_lossy(&response.body).contains("resume requires native cockpit"));
    }

    #[test]
    fn action_endpoint_executes_non_interactive_task_actions() {
        let mut context = reviewable_context();
        let mut runner = OkRunner;

        let response = handle_http_request_with_runner_and_paths(
            "POST",
            "/api/actions",
            r#"{"task_handle":"web/fix-login","action":"review"}"#,
            &mut context,
            &mut runner,
            None,
        )
        .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();

        assert_eq!(response.status_code, 200);
        assert_eq!(body["ok"], true);
        assert_eq!(
            body["cockpit"]["cards"][0]["qualified_handle"],
            "web/fix-login"
        );
    }

    #[test]
    fn cockpit_api_refreshes_live_task_status_before_rendering() {
        let mut context = reviewable_context();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Active;
        let mut runner = LiveRefreshRunner;

        let response = handle_http_request_with_runner_and_paths(
            "GET",
            "/api/cockpit",
            "",
            &mut context,
            &mut runner,
            None,
        )
        .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();

        assert_eq!(response.status_code, 200);
        assert_eq!(body["cards"][0]["qualified_handle"], "web/fix-login");
        assert_eq!(body["cards"][0]["live_summary"], "agent running");
    }

    #[test]
    fn cockpit_api_reloads_task_state_from_disk_before_rendering() {
        let root = std::env::temp_dir().join(format!("ajax-web-reload-{}", std::process::id()));
        let paths = super::CliContextPaths::new(root.join("config.toml"), root.join("state.db"));
        let saved_context = reviewable_context();
        SqliteRegistryStore::new(&paths.state_file)
            .save(&saved_context.registry)
            .unwrap();
        let mut server_context =
            CommandContext::new(Config::default(), InMemoryRegistry::default());
        let mut runner = LiveRefreshRunner;

        let response = handle_http_request_with_runner_and_paths(
            "GET",
            "/api/cockpit",
            "",
            &mut server_context,
            &mut runner,
            Some(&paths),
        )
        .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();

        assert_eq!(response.status_code, 200);
        assert_eq!(body["cards"][0]["qualified_handle"], "web/fix-login");
        assert_eq!(body["cards"][0]["live_summary"], "agent running");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn serve_connection_serves_a_request_over_a_generic_stream() {
        let mut context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let mut runner = OkRunner;
        let output = Rc::new(RefCell::new(Vec::new()));
        let stream = MockStream {
            input: Cursor::new(b"GET /app.css HTTP/1.1\r\nHost: ajax\r\n\r\n".to_vec()),
            output: Rc::clone(&output),
        };

        serve_connection(stream, &mut context, &mut runner, None).unwrap();

        let written = String::from_utf8_lossy(&output.borrow()).into_owned();
        assert!(written.starts_with("HTTP/1.1 200 OK"), "{written}");
        assert!(written.contains("Content-Type: text/css"), "{written}");
    }

    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("ajax-web-be-{tag}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn push_config_and_subscribe_endpoints_round_trip() {
        let mut context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let mut runner = OkRunner;
        let dir = scratch_dir("push");
        let paths = super::CliContextPaths::new(dir.join("config.toml"), dir.join("ajax.db"));

        let config = handle_http_request_with_runner_and_paths(
            "GET",
            "/api/push/config",
            "",
            &mut context,
            &mut runner,
            Some(&paths),
        )
        .unwrap();
        assert_eq!(config.status_code, 200);
        let config_body: serde_json::Value = serde_json::from_slice(&config.body).unwrap();
        assert_eq!(config_body["public_key"].as_array().map(Vec::len), Some(65));

        let subscribe = handle_http_request_with_runner_and_paths(
            "POST",
            "/api/push/subscribe",
            r#"{"endpoint":"https://push.example/x","keys":{"p256dh":"k","auth":"a"}}"#,
            &mut context,
            &mut runner,
            Some(&paths),
        )
        .unwrap();
        assert_eq!(subscribe.status_code, 200);
        assert_eq!(crate::web_companion_push::load_subscriptions(&dir).len(), 1);

        let unsubscribe = handle_http_request_with_runner_and_paths(
            "POST",
            "/api/push/unsubscribe",
            r#"{"endpoint":"https://push.example/x"}"#,
            &mut context,
            &mut runner,
            Some(&paths),
        )
        .unwrap();
        assert_eq!(unsubscribe.status_code, 200);
        assert!(crate::web_companion_push::load_subscriptions(&dir).is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn newly_attention_returns_only_freshly_added_handles() {
        let previous: HashSet<String> = ["web/a".to_string(), "web/b".to_string()]
            .into_iter()
            .collect();
        let current: HashSet<String> = [
            "web/b".to_string(),
            "web/c".to_string(),
            "web/d".to_string(),
        ]
        .into_iter()
        .collect();

        assert_eq!(newly_attention(&previous, &current), vec!["web/c", "web/d"]);
        assert!(newly_attention(&current, &current).is_empty());
    }

    struct OkRunner;

    impl CommandRunner for OkRunner {
        fn run(&mut self, _command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            Ok(CommandOutput {
                status_code: 0,
                stdout: "diff stat".to_string(),
                stderr: String::new(),
            })
        }
    }

    struct LiveRefreshRunner;

    impl CommandRunner for LiveRefreshRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            let stdout = match command.args.as_slice() {
                [command, ..] if command == "list-sessions" => "ajax-web-fix-login\n",
                [_, repo, subcommand, action, flag]
                    if repo == "/repo/web"
                        && subcommand == "worktree"
                        && action == "list"
                        && flag == "--porcelain" =>
                {
                    "worktree /repo/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /repo/web__worktrees/ajax-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n"
                }
                [_, repo, subcommand, format]
                    if repo == "/repo/web"
                        && subcommand == "branch"
                        && format == "--format=%(refname:short)" =>
                {
                    "main\najax/fix-login\n"
                }
                [command, ..] if command == "list-windows" => {
                    "ajax-web-fix-login\tworktrunk\t/repo/web__worktrees/ajax-fix-login\n"
                }
                [command, ..] if command == "capture-pane" => "codex is working\n",
                _ => "",
            };

            Ok(CommandOutput {
                status_code: 0,
                stdout: stdout.to_string(),
                stderr: String::new(),
            })
        }
    }

    fn reviewable_context() -> CommandContext<InMemoryRegistry> {
        let mut context = CommandContext::new(
            Config {
                repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
                ..Config::default()
            },
            InMemoryRegistry::default(),
        );
        let mut task = Task::new(
            TaskId::new("task-1"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            "/repo/web__worktrees/ajax-fix-login",
            "ajax-web-fix-login",
            "worktrunk",
            AgentClient::Codex,
        );
        task.lifecycle_status = LifecycleStatus::Reviewable;
        task.git_status = Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        });
        task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
        task.worktrunk_status = Some(WorktrunkStatus::present(
            "worktrunk",
            "/repo/web__worktrees/ajax-fix-login",
        ));
        context.registry.create_task(task).unwrap();
        context
    }
}

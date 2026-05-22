use ajax_core::{
    adapters::CommandRunner,
    commands::{self, CommandContext},
    models::OperatorAction,
    output::{InboxResponse, ReposResponse, TaskCard},
    registry::InMemoryRegistry,
    task_operations::task_command::{
        execute_task_command_operation, plan_task_command_operation, TaskCommandKind,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
};

use crate::{
    command_error, context::save_context, dispatch::execute_observed_drop, CliContextPaths,
    CliError,
};

pub(crate) struct HttpResponse {
    pub(crate) status_code: u16,
    pub(crate) content_type: &'static str,
    pub(crate) body: String,
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
    r##"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Ajax Mobile Cockpit</title>
  <style>
    :root {
      color-scheme: light;
      --ink: #171b1f;
      --muted: #58626d;
      --paper: #f7f4ed;
      --panel: #ffffff;
      --line: #d7d0c5;
      --accent: #0f766e;
      --warn: #b45309;
      --danger: #b91c1c;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      background: linear-gradient(180deg, #f7f4ed 0%, #edf4f1 100%);
      color: var(--ink);
    }
    header {
      position: sticky;
      top: 0;
      z-index: 2;
      padding: 16px;
      border-bottom: 1px solid var(--line);
      background: rgba(247, 244, 237, 0.92);
      backdrop-filter: blur(12px);
    }
    h1 { margin: 0; font-size: 1.15rem; }
    main { width: min(760px, 100%); margin: 0 auto; padding: 14px; }
    #task-list { display: grid; gap: 10px; }
    .card {
      border: 1px solid var(--line);
      border-radius: 8px;
      background: var(--panel);
      padding: 14px;
      box-shadow: 0 1px 0 rgba(23, 27, 31, 0.04);
    }
    .row { display: flex; justify-content: space-between; gap: 12px; align-items: start; }
    .handle { font-weight: 700; overflow-wrap: anywhere; }
    .state { color: var(--accent); font-weight: 700; white-space: nowrap; }
    .title { margin-top: 6px; color: var(--muted); }
    .actions { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 12px; }
    button {
      min-height: 40px;
      border: 1px solid var(--line);
      border-radius: 8px;
      background: #f8faf9;
      color: var(--ink);
      font: inherit;
      font-weight: 700;
      padding: 8px 12px;
    }
    .empty { color: var(--muted); padding: 24px 4px; text-align: center; }
  </style>
</head>
<body>
  <header><h1>Ajax Mobile Cockpit</h1></header>
  <main>
    <section id="task-list" aria-live="polite"></section>
  </main>
  <script>
    const list = document.querySelector("#task-list");
    function button(action, task) {
      const el = document.createElement("button");
      el.type = "button";
      el.textContent = action;
      el.dataset.action = action;
      el.dataset.task = task.qualified_handle;
      return el;
    }
    async function runAction(el) {
      el.disabled = true;
      const response = await fetch("/api/actions", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          task_handle: el.dataset.task,
          action: el.dataset.action
        })
      });
      const payload = await response.json();
      if (payload.cockpit) render(payload.cockpit);
      else await refresh();
      if (!response.ok && payload.error) window.alert(payload.error);
    }
    function render(data) {
      list.replaceChildren();
      if (!data.cards.length) {
        const empty = document.createElement("div");
        empty.className = "empty";
        empty.textContent = "No active Ajax tasks";
        list.append(empty);
        return;
      }
      for (const task of data.cards) {
        const card = document.createElement("article");
        card.className = "card";
        const row = document.createElement("div");
        row.className = "row";
        const handle = document.createElement("div");
        handle.className = "handle";
        handle.textContent = task.qualified_handle;
        const state = document.createElement("div");
        state.className = "state";
        state.textContent = task.ui_state;
        row.append(handle, state);
        const title = document.createElement("div");
        title.className = "title";
        title.textContent = task.live_summary || task.status_label || task.title;
        const actions = document.createElement("div");
        actions.className = "actions";
        for (const action of task.available_actions) actions.append(button(action, task));
        card.append(row, title, actions);
        list.append(card);
      }
    }
    async function refresh() {
      const response = await fetch("/api/cockpit");
      render(await response.json());
    }
    list.addEventListener("click", (event) => {
      const el = event.target.closest("button[data-action]");
      if (el) runAction(el).finally(() => { el.disabled = false; });
    });
    refresh();
  </script>
</body>
</html>"##
        .to_string()
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
            body: render_mobile_shell(),
        }),
        "/api/cockpit" => Ok(HttpResponse {
            status_code: 200,
            content_type: "application/json; charset=utf-8",
            body: cockpit_json(context)?,
        }),
        _ => Ok(text_response(404, "not found")),
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
    if method == "POST" && path == "/api/actions" {
        return handle_action_request(body, context, runner, paths);
    }

    handle_http_request(method, path, body, context)
        .map_err(|error| CliError::JsonSerialization(error.to_string()))
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
    let listener = TcpListener::bind((host, port))
        .map_err(|error| CliError::CommandFailed(format!("web bind failed: {error}")))?;
    eprintln!("Ajax mobile web listening on http://{host}:{port}");

    for stream in listener.incoming() {
        let stream = stream
            .map_err(|error| CliError::CommandFailed(format!("web accept failed: {error}")))?;
        serve_connection(stream, context, runner, paths)?;
    }

    Ok(())
}

fn serve_connection(
    mut stream: TcpStream,
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

fn write_http_response(mut stream: TcpStream, response: HttpResponse) -> Result<(), CliError> {
    let status_text = match response.status_code {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Internal Server Error",
    };
    let body = response.body.as_bytes();
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status_code,
        status_text,
        response.content_type,
        body.len()
    );
    stream
        .write_all(head.as_bytes())
        .and_then(|_| stream.write_all(body))
        .map_err(|error| CliError::CommandFailed(format!("web response write failed: {error}")))
}

fn text_response(status_code: u16, body: impl Into<String>) -> HttpResponse {
    HttpResponse {
        status_code,
        content_type: "text/plain; charset=utf-8",
        body: body.into(),
    }
}

fn json_response(status_code: u16, value: serde_json::Value) -> Result<HttpResponse, CliError> {
    Ok(HttpResponse {
        status_code,
        content_type: "application/json; charset=utf-8",
        body: serde_json::to_string(&value)
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
        render_mobile_shell,
    };
    use ajax_core::{
        adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec},
        commands::CommandContext,
        config::{Config, ManagedRepo},
        models::{
            AgentClient, GitStatus, LifecycleStatus, Task, TaskId, TmuxStatus, WorktrunkStatus,
        },
        registry::{InMemoryRegistry, Registry},
    };

    #[test]
    fn mobile_shell_is_responsive_and_loads_cockpit_data() {
        let html = render_mobile_shell();

        assert!(html.contains("<!doctype html>"));
        assert!(html.contains("name=\"viewport\""));
        assert!(html.contains("width=device-width"));
        assert!(html.contains("id=\"task-list\""));
        assert!(html.contains("/api/cockpit"));
        assert!(html.contains("/api/actions"));
        assert!(html.contains("method: \"POST\""));
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
        assert!(shell.body.contains("Ajax Mobile Cockpit"));

        let cockpit = handle_http_request("GET", "/api/cockpit", "", &context).unwrap();
        assert_eq!(cockpit.status_code, 200);
        assert_eq!(cockpit.content_type, "application/json; charset=utf-8");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&cockpit.body).unwrap()["cards"],
            serde_json::json!([])
        );
    }

    #[test]
    fn http_router_reports_unknown_routes_and_unsupported_methods() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

        let missing = handle_http_request("GET", "/missing", "", &context).unwrap();
        assert_eq!(missing.status_code, 404);
        assert!(missing.body.contains("not found"));

        let unsupported = handle_http_request("POST", "/", "", &context).unwrap();
        assert_eq!(unsupported.status_code, 405);
        assert!(unsupported.body.contains("method not allowed"));
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
        assert!(response.body.contains("resume requires native cockpit"));
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
        let body: serde_json::Value = serde_json::from_str(&response.body).unwrap();

        assert_eq!(response.status_code, 200);
        assert_eq!(body["ok"], true);
        assert_eq!(
            body["cockpit"]["cards"][0]["qualified_handle"],
            "web/fix-login"
        );
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

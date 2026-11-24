use crate::adapters::http::json_value_response;
use crate::runtime::bridge::RuntimeBridge;
use crate::runtime::state::WebAppState;
use ajax_core::{
    adapters::{
        task_sandbox::{set_task_sandbox_from, task_sandbox_from, TaskSandbox},
        CommandRunner,
    },
    registry::{InMemoryRegistry, Registry as _},
};
use axum::{body::Bytes, extract::State, response::Response as AxumResponse};
use std::{
    env,
    path::{Path, PathBuf},
};

const LOCKED_BY_ENV: &str =
    "Docker sandbox is turned off for every task by AJAX_SANDBOX=off; unset AJAX_SANDBOX to change this task";

#[derive(Debug)]
struct SandboxReply {
    status: u16,
    body: serde_json::Value,
}

impl SandboxReply {
    fn error(status: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            body: serde_json::json!({ "ok": false, "error": message.into() }),
        }
    }

    fn into_response(self) -> AxumResponse {
        json_value_response(self.status, self.body)
    }
}

fn sandbox_reply(sandbox: &TaskSandbox) -> SandboxReply {
    SandboxReply {
        status: 200,
        body: serde_json::json!({
            "ok": true,
            "enabled": sandbox.enabled,
            "lockedByEnv": sandbox.locked_by_env,
        }),
    }
}

fn read_reply(home: Option<&Path>, env_value: Option<&str>, worktree: &Path) -> SandboxReply {
    sandbox_reply(&task_sandbox_from(home, env_value, worktree))
}

fn write_reply(
    home: Option<&Path>,
    env_value: Option<&str>,
    worktree: &Path,
    enabled: bool,
) -> SandboxReply {
    if task_sandbox_from(home, env_value, worktree).locked_by_env {
        return SandboxReply::error(409, LOCKED_BY_ENV);
    }
    match set_task_sandbox_from(home, worktree, enabled) {
        Ok(()) => read_reply(home, env_value, worktree),
        Err(error) => SandboxReply::error(500, error.to_string()),
    }
}

fn parse_request(body: &[u8]) -> Result<bool, SandboxReply> {
    let compact: Vec<u8> = body
        .iter()
        .copied()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    match compact.as_slice() {
        br#"{"enabled":true}"# => Ok(true),
        br#"{"enabled":false}"# => Ok(false),
        _ => Err(SandboxReply::error(
            400,
            r#"invalid sandbox request: expected {"enabled": true|false}"#,
        )),
    }
}

fn find_task_worktree(registry: &InMemoryRegistry, handle: &str) -> Result<PathBuf, SandboxReply> {
    registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == handle)
        .map(|task| task.worktree_path.clone())
        .ok_or_else(|| SandboxReply::error(404, "task not found"))
}

fn task_worktree<C, B>(state: &WebAppState<C, B>, handle: &str) -> Result<PathBuf, SandboxReply>
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let guard = state.shared();
    find_task_worktree(&guard.context.registry, handle)
}

fn process_environment() -> (Option<PathBuf>, Option<String>) {
    (
        env::var_os("HOME").map(PathBuf::from),
        env::var("AJAX_SANDBOX").ok(),
    )
}

pub(crate) async fn axum_task_sandbox_get<C, B>(
    State(state): State<WebAppState<C, B>>,
    handle: String,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let reply = match task_worktree(&state, &handle) {
        Ok(worktree) => {
            let (home, env_value) = process_environment();
            read_reply(home.as_deref(), env_value.as_deref(), &worktree)
        }
        Err(reply) => reply,
    };
    reply.into_response()
}

pub(crate) async fn axum_task_sandbox_post<C, B>(
    State(state): State<WebAppState<C, B>>,
    handle: String,
    body: Bytes,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let reply = match parse_request(&body) {
        Ok(enabled) => match task_worktree(&state, &handle) {
            Ok(worktree) => {
                let (home, env_value) = process_environment();
                write_reply(home.as_deref(), env_value.as_deref(), &worktree, enabled)
            }
            Err(reply) => reply,
        },
        Err(reply) => reply,
    };
    reply.into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;
    use std::fs;

    fn scratch_home(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/task-sandbox-web-tests")
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fix_login_worktree() -> PathBuf {
        test_support::fix_login_task().worktree_path
    }

    #[test]
    fn get_reports_default_enabled_and_unlocked() {
        let home = scratch_home("get-default");
        let reply = read_reply(Some(&home), None, &fix_login_worktree());

        assert_eq!(reply.status, 200);
        assert_eq!(
            reply.body,
            serde_json::json!({ "ok": true, "enabled": true, "lockedByEnv": false })
        );
    }

    #[test]
    fn post_off_then_get_shows_disabled_and_post_on_restores() {
        let home = scratch_home("off-then-on");
        let worktree = fix_login_worktree();

        let off = write_reply(Some(&home), None, &worktree, false);
        assert_eq!(off.status, 200);
        assert_eq!(off.body["enabled"], false);
        assert_eq!(
            read_reply(Some(&home), None, &worktree).body["enabled"],
            false
        );

        let on = write_reply(Some(&home), None, &worktree, true);
        assert_eq!(on.status, 200);
        assert_eq!(on.body["enabled"], true);
        assert_eq!(
            read_reply(Some(&home), None, &worktree).body["enabled"],
            true
        );
    }

    #[test]
    fn env_locked_get_reports_locked_and_post_returns_409_without_writing() {
        let home = scratch_home("env-locked");
        let worktree = fix_login_worktree();

        let read = read_reply(Some(&home), Some("off"), &worktree);
        assert_eq!(
            read.body,
            serde_json::json!({ "ok": true, "enabled": false, "lockedByEnv": true })
        );

        let reply = write_reply(Some(&home), Some("off"), &worktree, true);
        assert_eq!(reply.status, 409);
        assert_eq!(reply.body["ok"], false);
        assert!(reply.body["error"]
            .as_str()
            .is_some_and(|error| error.contains("AJAX_SANDBOX")));

        let reply = write_reply(Some(&home), Some("off"), &worktree, false);
        assert_eq!(reply.status, 409);
        assert_eq!(
            read_reply(Some(&home), None, &worktree).body["enabled"],
            true
        );
    }

    #[test]
    fn unknown_task_is_not_found() {
        let context = test_support::context_with_fix_login_task();

        let reply = find_task_worktree(&context.registry, "web/missing").unwrap_err();

        assert_eq!(reply.status, 404);
        assert_eq!(
            reply.body,
            serde_json::json!({ "ok": false, "error": "task not found" })
        );
        assert_eq!(
            find_task_worktree(&context.registry, "web/fix-login").unwrap(),
            fix_login_worktree()
        );
    }

    #[test]
    fn malformed_body_is_bad_request() {
        assert_eq!(parse_request(b"not json").unwrap_err().status, 400);
        assert_eq!(parse_request(b"{}").unwrap_err().status, 400);
        assert_eq!(
            parse_request(br#"{"enabled": true, "extra": 1}"#)
                .unwrap_err()
                .status,
            400
        );
        assert_eq!(
            parse_request(br#"{"enabled": "yes"}"#).unwrap_err().status,
            400
        );
        assert!(parse_request(br#"{"enabled": false}"#).is_ok_and(|enabled| !enabled));
        assert!(parse_request(b"{ \"enabled\" : true }\n").is_ok_and(|enabled| enabled));
    }

    #[test]
    fn storage_failure_is_internal_error_with_text() {
        let reply = write_reply(None, None, &fix_login_worktree(), false);

        assert_eq!(reply.status, 500);
        assert_eq!(reply.body["ok"], false);
        assert!(reply.body["error"]
            .as_str()
            .is_some_and(|error| error.contains("HOME")));
    }

    #[test]
    fn sandbox_routes_require_browser_session() {
        use crate::runtime::{api_access_policy, ApiAccess};

        assert_eq!(
            api_access_policy("GET", "/api/tasks/web%2Ffix-login/sandbox"),
            ApiAccess::BrowserSessionRequired
        );
        assert_eq!(
            api_access_policy("POST", "/api/tasks/web%2Ffix-login/sandbox"),
            ApiAccess::BrowserSessionRequired
        );
    }
}

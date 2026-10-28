use crate::adapters::{
    program::{harness_command, resolve_program},
    web_session_acp::{CursorCredentialStatus, CursorLogin},
};
use ajax_core::adapters::agent::PI_DEFAULT_MODEL;
use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    io::Read,
    process::{Child, Command, ExitStatus, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const WORKER_FAILED: &str = "agent auth worker failed";
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const PROBE_POLL_INTERVAL: Duration = Duration::from_millis(20);
const PROBE_READ_GRACE: Duration = Duration::from_millis(500);
const UNDETERMINED: &str = "Could not determine sign-in status";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Harness {
    Claude,
    Codex,
    Cursor,
    Pi,
}

const HARNESSES: [Harness; 4] = [
    Harness::Claude,
    Harness::Codex,
    Harness::Cursor,
    Harness::Pi,
];

impl Harness {
    fn name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Cursor => "cursor",
            Self::Pi => "pi",
        }
    }

    fn sign_in_hint(self) -> &'static str {
        match self {
            Self::Claude => "Run `claude auth login` on the host",
            Self::Codex => "Run `codex login` on the host",
            Self::Cursor => "Sign in to Cursor in Settings",
            Self::Pi => "Run `pi` on the host and sign in with /login",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessAuthStatus {
    pub harness: &'static str,
    pub installed: bool,
    pub signed_in: Option<bool>,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AgentAuthReport {
    pub harnesses: Vec<HarnessAuthStatus>,
}

struct ProbeOutput {
    success: bool,
    stdout: String,
}

pub async fn axum_agent_auth_status() -> axum::response::Response {
    match tokio::task::spawn_blocking(probe_all).await {
        Ok(report) => Json(report).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": WORKER_FAILED })),
        )
            .into_response(),
    }
}

fn probe_all() -> AgentAuthReport {
    let receivers = HARNESSES.map(|harness| {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let _ = sender.send(probe_harness(harness));
        });
        (harness, receiver)
    });
    let deadline = Instant::now() + PROBE_TIMEOUT;
    let harnesses = receivers
        .into_iter()
        .map(|(harness, receiver)| {
            receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|_| undetermined(harness))
        })
        .collect();
    AgentAuthReport { harnesses }
}

fn probe_harness(harness: Harness) -> HarnessAuthStatus {
    match harness {
        Harness::Claude => cli_status(
            harness,
            cli_command("claude", &["auth", "status", "--json"]),
            |output| claude_signed_in(&output.stdout),
        ),
        Harness::Codex => cli_status(
            harness,
            cli_command("codex", &["login", "status"]),
            codex_signed_in,
        ),
        Harness::Cursor => {
            let installed = resolve_program("node").is_some();
            let status = installed
                .then(|| CursorLogin::global().status().ok())
                .flatten();
            cursor_status(installed, status.as_ref())
        }
        Harness::Pi => cli_status(
            harness,
            cli_command(
                "pi",
                &[
                    "auth",
                    "check",
                    "--provider",
                    pi_default_provider(),
                    "--json",
                    "--no-refresh",
                ],
            ),
            |output| pi_signed_in(&output.stdout),
        ),
    }
}

fn undetermined(harness: Harness) -> HarnessAuthStatus {
    HarnessAuthStatus {
        harness: harness.name(),
        installed: true,
        signed_in: None,
        detail: detail(harness, true, None),
    }
}

fn cli_command(program: &str, args: &[&str]) -> Option<Command> {
    let mut command = harness_command(program)?;
    command.args(args);
    Some(command)
}

fn cli_status(
    harness: Harness,
    command: Option<Command>,
    signed_in: fn(&ProbeOutput) -> Option<bool>,
) -> HarnessAuthStatus {
    let installed = command.is_some();
    let output = command.and_then(|command| run_probe(command, PROBE_TIMEOUT));
    let signed_in = output.as_ref().and_then(signed_in);
    HarnessAuthStatus {
        harness: harness.name(),
        installed,
        signed_in,
        detail: detail(harness, installed, signed_in),
    }
}

fn cursor_status(installed: bool, status: Option<&CursorCredentialStatus>) -> HarnessAuthStatus {
    let signed_in = status.map(cursor_signed_in);
    let rejected = status.is_some_and(|status| status.valid == Some(false));
    HarnessAuthStatus {
        harness: Harness::Cursor.name(),
        installed,
        signed_in,
        detail: if rejected {
            Some("Cursor rejected the saved key".to_string())
        } else {
            detail(Harness::Cursor, installed, signed_in)
        },
    }
}

fn detail(harness: Harness, installed: bool, signed_in: Option<bool>) -> Option<String> {
    if !installed {
        return Some(format!("{} is not installed on the host", harness.name()));
    }
    match signed_in {
        Some(true) if harness == Harness::Pi => Some(format!(
            "Checked default provider {}",
            pi_default_provider()
        )),
        Some(true) => None,
        Some(false) => Some(harness.sign_in_hint().to_string()),
        None => Some(UNDETERMINED.to_string()),
    }
}

fn run_probe(mut command: Command, timeout: Duration) -> Option<ProbeOutput> {
    let deadline = Instant::now() + timeout;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut text = String::new();
        let _ = sender.send(stdout.read_to_string(&mut text).map(|_| text));
    });
    let status = wait_with_timeout(&mut child, timeout)?;
    let read_timeout = deadline
        .saturating_duration_since(Instant::now())
        .max(PROBE_READ_GRACE);
    let stdout = receiver.recv_timeout(read_timeout).ok()?.ok()?;
    Some(ProbeOutput {
        success: status.success(),
        stdout,
    })
}

fn wait_with_timeout(child: &mut Child, timeout: Duration) -> Option<ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait().ok()? {
            Some(status) => return Some(status),
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => thread::sleep(PROBE_POLL_INTERVAL),
        }
    }
}

fn pi_default_provider() -> &'static str {
    PI_DEFAULT_MODEL
        .split_once('/')
        .map_or(PI_DEFAULT_MODEL, |(provider, _)| provider)
}

fn claude_signed_in(stdout: &str) -> Option<bool> {
    serde_json::from_str::<Value>(stdout)
        .ok()?
        .get("loggedIn")?
        .as_bool()
}

fn pi_signed_in(stdout: &str) -> Option<bool> {
    let status = serde_json::from_str::<Value>(stdout)
        .ok()?
        .get("status")?
        .as_str()?
        .to_string();
    Some(status == "ready")
}

fn codex_signed_in(output: &ProbeOutput) -> Option<bool> {
    Some(output.success)
}

fn cursor_signed_in(status: &CursorCredentialStatus) -> bool {
    status.present && status.valid == Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(success: bool, stdout: &str) -> ProbeOutput {
        ProbeOutput {
            success,
            stdout: stdout.to_string(),
        }
    }

    fn cursor(present: bool, valid: Option<bool>) -> CursorCredentialStatus {
        CursorCredentialStatus {
            present,
            valid,
            email: None,
            expires_at_ms: None,
            error: None,
        }
    }

    #[test]
    fn claude_logged_in_field_maps_to_signed_in() {
        assert_eq!(
            claude_signed_in(r#"{"loggedIn":true,"authMethod":"claude.ai","email":"a@b.c"}"#),
            Some(true)
        );
        assert_eq!(claude_signed_in(r#"{"loggedIn":false}"#), Some(false));
    }

    #[test]
    fn claude_output_without_logged_in_or_json_is_undetermined() {
        assert_eq!(claude_signed_in(r#"{"authMethod":"claude.ai"}"#), None);
        assert_eq!(claude_signed_in(r#"{"loggedIn":"yes"}"#), None);
        assert_eq!(claude_signed_in("not json at all"), None);
        assert_eq!(claude_signed_in(""), None);
    }

    #[test]
    fn pi_ready_status_maps_to_signed_in() {
        assert_eq!(
            pi_signed_in(r#"{"status":"ready","provider":"opencode-go","authType":"api_key"}"#),
            Some(true)
        );
        assert_eq!(pi_signed_in(r#"{"status":"missing"}"#), Some(false));
    }

    #[test]
    fn pi_output_without_status_or_json_is_undetermined() {
        assert_eq!(pi_signed_in(r#"{"provider":"local"}"#), None);
        assert_eq!(pi_signed_in(r#"{"status":3}"#), None);
        assert_eq!(pi_signed_in("garbage"), None);
    }

    #[test]
    fn pi_probe_uses_the_default_model_provider_prefix() {
        assert_eq!(pi_default_provider(), "local");
    }

    #[test]
    fn codex_exit_status_maps_to_signed_in() {
        assert_eq!(
            codex_signed_in(&output(true, "Logged in using ChatGPT")),
            Some(true)
        );
        assert_eq!(
            codex_signed_in(&output(false, "Not logged in")),
            Some(false)
        );
    }

    #[test]
    fn cursor_signed_in_requires_present_and_valid() {
        assert!(cursor_signed_in(&cursor(true, Some(true))));
        assert!(!cursor_signed_in(&cursor(true, None)));
        assert!(!cursor_signed_in(&cursor(false, Some(true))));
    }

    #[test]
    fn cursor_rejected_key_says_so_without_leaking_status_text() {
        let status = cursor_status(true, Some(&cursor(true, Some(false))));
        assert_eq!(status.signed_in, Some(false));
        assert_eq!(
            status.detail.as_deref(),
            Some("Cursor rejected the saved key")
        );
    }

    #[test]
    fn cursor_without_a_status_is_undetermined_and_missing_node_is_uninstalled() {
        let unknown = cursor_status(true, None);
        assert_eq!(unknown.signed_in, None);
        assert_eq!(unknown.detail.as_deref(), Some(UNDETERMINED));

        let missing = cursor_status(false, None);
        assert!(!missing.installed);
        assert_eq!(missing.signed_in, None);
    }

    #[test]
    fn signed_out_detail_points_at_the_host_command() {
        assert_eq!(
            detail(Harness::Claude, true, Some(false)).as_deref(),
            Some("Run `claude auth login` on the host")
        );
        assert_eq!(
            detail(Harness::Codex, true, Some(false)).as_deref(),
            Some("Run `codex login` on the host")
        );
        assert_eq!(
            detail(Harness::Pi, true, Some(false)).as_deref(),
            Some("Run `pi` on the host and sign in with /login")
        );
        assert_eq!(detail(Harness::Claude, true, Some(true)), None);
    }

    #[test]
    fn pi_signed_in_detail_names_the_checked_provider() {
        assert_eq!(
            detail(Harness::Pi, true, Some(true)),
            Some(format!(
                "Checked default provider {}",
                pi_default_provider()
            ))
        );
    }

    #[test]
    fn probe_does_not_wait_for_a_grandchild_holding_stdout_open() {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 3 & echo hi"]);
        let started = Instant::now();
        let _ = run_probe(command, Duration::from_secs(1));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn missing_binary_is_reported_as_not_installed() {
        let status = cli_status(Harness::Pi, None, |output| pi_signed_in(&output.stdout));
        assert!(!status.installed);
        assert_eq!(status.signed_in, None);
        assert_eq!(
            status.detail.as_deref(),
            Some("pi is not installed on the host")
        );
    }

    #[test]
    fn harnesses_are_reported_in_the_fixed_order() {
        let names = HARNESSES.map(Harness::name);
        assert_eq!(names, ["claude", "codex", "cursor", "pi"]);
    }

    #[test]
    fn report_serializes_as_camel_case_harness_list() {
        let report = AgentAuthReport {
            harnesses: vec![HarnessAuthStatus {
                harness: "codex",
                installed: true,
                signed_in: None,
                detail: Some(UNDETERMINED.to_string()),
            }],
        };
        assert_eq!(
            serde_json::to_value(report).unwrap(),
            json!({
                "harnesses": [{
                    "harness": "codex",
                    "installed": true,
                    "signedIn": null,
                    "detail": UNDETERMINED
                }]
            })
        );
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalAttachPlan {
    pub qualified_handle: String,
    pub tmux_session: String,
    pub task_window: String,
}

use portable_pty::{Child, CommandBuilder};
use std::time::Duration;

pub(crate) const TERMINAL_CHILD_CLEANUP_WAIT_TIMEOUT: Duration = Duration::from_secs(2);
pub(crate) const RESIZE_WAIT_TIMEOUT: Duration = Duration::from_millis(500);
pub(crate) const RESIZE_SETTLE_QUIET: Duration = Duration::from_millis(150);

pub const MAX_INPUT_FRAME_BYTES: usize = 4096;
pub(crate) const PTY_READ_BUFFER_BYTES: usize = 8192;
pub(crate) const TERMINAL_OUTPUT_FLUSH_MS: u64 = 16;
pub(crate) const TERMINAL_OUTPUT_MAX_BYTES: usize = 16 * 1024;
pub(crate) const BROWSER_TMUX_TERM: &str = "xterm-256color";
pub(crate) const SCROLLBACK_HOSTILE_SEQUENCES: &[&[u8]] = &[
    b"\x1b[?47h",
    b"\x1b[?47l",
    b"\x1b[?1047h",
    b"\x1b[?1047l",
    b"\x1b[?1049h",
    b"\x1b[?1049l",
    b"\x1b[?1000h",
    b"\x1b[?1000l",
    b"\x1b[?1001h",
    b"\x1b[?1001l",
    b"\x1b[?1002h",
    b"\x1b[?1002l",
    b"\x1b[?1003h",
    b"\x1b[?1003l",
    b"\x1b[?1004h",
    b"\x1b[?1004l",
    b"\x1b[?1005h",
    b"\x1b[?1005l",
    b"\x1b[?1006h",
    b"\x1b[?1006l",
    b"\x1b[?1007h",
    b"\x1b[?1007l",
    b"\x1b[3J",
];

pub(crate) trait TerminalChild {
    fn kill_child(&mut self) -> std::io::Result<()>;
    fn wait_child(&mut self) -> std::io::Result<()>;
}

impl TerminalChild for Box<dyn Child + Send + Sync> {
    fn kill_child(&mut self) -> std::io::Result<()> {
        self.kill()
    }

    fn wait_child(&mut self) -> std::io::Result<()> {
        self.wait().map(|_| ())
    }
}

pub(crate) fn cleanup_spawned_child<C: TerminalChild>(mut child: C) {
    let _ = child.kill_child();
    let _ = child.wait_child();
}

pub(crate) async fn cleanup_spawned_child_async<C: TerminalChild + Send + 'static>(child: C) {
    cleanup_spawned_child_async_with_timeout(child, TERMINAL_CHILD_CLEANUP_WAIT_TIMEOUT).await;
}

pub(crate) async fn cleanup_spawned_child_async_with_timeout<C: TerminalChild + Send + 'static>(
    child: C,
    wait_timeout: Duration,
) {
    let wait_task = tokio::task::spawn_blocking(move || cleanup_spawned_child(child));
    match tokio::time::timeout(wait_timeout, wait_task).await {
        Ok(Ok(())) => {}
        Ok(Err(join_error)) => {
            eprintln!("Ajax web terminal child cleanup task failed: {join_error}");
        }
        Err(_) => {
            eprintln!(
                "Ajax web terminal child cleanup timed out after {wait_timeout:?}; continuing websocket close"
            );
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TmuxAttachCommandPlan {
    pub program: String,
    pub args: Vec<String>,
}

pub fn tmux_attach_target(session: &str, task_window: &str) -> String {
    format!("{session}:{task_window}")
}

pub(crate) fn task_window_probe_command(ephemeral_session: &str, task_window: &str) -> TmuxCommand {
    let target = tmux_attach_target(ephemeral_session, task_window);
    TmuxCommand::new(["display-message", "-p", "-t", &target, "#{window_id}"])
}

pub fn build_tmux_attach_command_plan(plan: &TerminalAttachPlan) -> TmuxAttachCommandPlan {
    let target = tmux_attach_target(&plan.tmux_session, &plan.task_window);
    TmuxAttachCommandPlan {
        program: "tmux".to_string(),
        args: vec!["attach-session".to_string(), "-t".to_string(), target],
    }
}

pub(crate) fn build_tmux_attach_command(command_plan: &TmuxAttachCommandPlan) -> CommandBuilder {
    let mut command = CommandBuilder::new(&command_plan.program);
    for arg in &command_plan.args {
        command.arg(arg);
    }
    command.env("TERM", BROWSER_TMUX_TERM);
    command
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TmuxCommand {
    pub program: String,
    pub args: Vec<String>,
}

impl TmuxCommand {
    pub(crate) fn new<const N: usize>(args: [&str; N]) -> Self {
        TmuxCommand {
            program: "tmux".to_string(),
            args: args.iter().map(|arg| arg.to_string()).collect(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IsolatedAttachPlan {
    pub ephemeral_session: String,
    pub setup: Vec<TmuxCommand>,
    pub history: TmuxCommand,
    pub attach: TmuxAttachCommandPlan,
    pub teardown: Vec<TmuxCommand>,
}

pub const EPHEMERAL_SESSION_INFIX: &str = "-m";

pub fn build_isolated_attach_plan(plan: &TerminalAttachPlan) -> IsolatedAttachPlan {
    let mut isolated = build_isolated_attach_plan_with_token(plan, &random_session_token());
    isolated.teardown = destroy_ephemeral_session_commands(&isolated.ephemeral_session);
    isolated
}

pub fn build_isolated_attach_plan_for_client(
    plan: &TerminalAttachPlan,
    client_id: &str,
) -> IsolatedAttachPlan {
    build_isolated_attach_plan_with_token(plan, &ephemeral_client_token(client_id))
}

pub fn ephemeral_client_token(client_id: &str) -> String {
    let trimmed = client_id.trim();
    if trimmed.is_empty() {
        return random_session_token();
    }
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in trimmed.as_bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    let full = format!("{hash:016x}");
    full[..12].to_string()
}

pub fn destroy_ephemeral_session_commands(ephemeral_session: &str) -> Vec<TmuxCommand> {
    vec![TmuxCommand::new(["kill-session", "-t", ephemeral_session])]
}

pub(crate) fn should_ignore_setup_failure(command: &TmuxCommand, stderr: &str) -> bool {
    command.args.first().map(String::as_str) == Some("new-session")
        && stderr.contains("duplicate session")
}

pub(crate) fn build_isolated_attach_plan_with_token(
    plan: &TerminalAttachPlan,
    token: &str,
) -> IsolatedAttachPlan {
    let ephemeral = format!("{}{EPHEMERAL_SESSION_INFIX}{token}", plan.tmux_session);
    let history_target = tmux_attach_target(&ephemeral, &plan.task_window);
    let ephemeral_plan = TerminalAttachPlan {
        qualified_handle: plan.qualified_handle.clone(),
        tmux_session: ephemeral.clone(),
        task_window: plan.task_window.clone(),
    };
    IsolatedAttachPlan {
        setup: vec![
            TmuxCommand::new([
                "new-session",
                "-d",
                "-s",
                &ephemeral,
                "-t",
                &plan.tmux_session,
            ]),
            TmuxCommand::new(["set-option", "-t", &ephemeral, "status-interval", "5"]),
            TmuxCommand::new(["set-option", "-t", &ephemeral, "visual-activity", "off"]),
            TmuxCommand::new(["set-option", "-t", &ephemeral, "visual-bell", "off"]),
        ],
        history: TmuxCommand::new([
            "capture-pane",
            "-p",
            "-e",
            "-t",
            &history_target,
            "-S",
            "-10000",
            "-E",
            "-1",
        ]),
        attach: build_tmux_attach_command_plan(&ephemeral_plan),
        teardown: vec![],
        ephemeral_session: ephemeral,
    }
}

pub(crate) fn random_session_token() -> String {
    let mut bytes = [0_u8; 6];
    if getrandom::fill(&mut bytes).is_err() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0);
        bytes.copy_from_slice(&nanos.to_le_bytes()[..6]);
    }
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn run_tmux_command_blocking(
    command: &TmuxCommand,
) -> std::io::Result<std::process::Output> {
    std::process::Command::new(&command.program)
        .args(&command.args)
        .output()
}

pub fn is_ephemeral_session_name(name: &str) -> bool {
    match name.rfind(EPHEMERAL_SESSION_INFIX) {
        Some(index) if index > 0 => {
            let token = &name[index + EPHEMERAL_SESSION_INFIX.len()..];
            token.len() == 12
                && token
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        }
        _ => false,
    }
}

pub fn ephemeral_sessions_to_reap(names: &[String]) -> Vec<String> {
    names
        .iter()
        .filter(|name| is_ephemeral_session_name(name))
        .cloned()
        .collect()
}

pub fn ephemeral_sessions_to_reap_detached(
    rows: &[(String, u32)],
    exclude: Option<&str>,
) -> Vec<String> {
    rows.iter()
        .filter(|(name, attached)| {
            is_ephemeral_session_name(name) && *attached == 0 && exclude != Some(name.as_str())
        })
        .map(|(name, _)| name.clone())
        .collect()
}

pub fn reap_orphan_terminal_sessions() {
    let listing = match run_tmux_command_blocking(&TmuxCommand::new([
        "list-sessions",
        "-F",
        "#{session_name}",
    ])) {
        Ok(output) if output.status.success() => output.stdout,
        _ => return,
    };
    let names: Vec<String> = String::from_utf8_lossy(&listing)
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();
    for session in ephemeral_sessions_to_reap(&names) {
        let _ = run_tmux_command_blocking(&TmuxCommand::new(["kill-session", "-t", &session]));
    }
}

pub fn reap_detached_ephemeral_terminal_sessions() {
    reap_detached_ephemeral_terminal_sessions_except(None);
}

pub fn reap_detached_ephemeral_terminal_sessions_except(keep: Option<&str>) {
    let listing = match run_tmux_command_blocking(&TmuxCommand::new([
        "list-sessions",
        "-F",
        "#{session_name} #{session_attached}",
    ])) {
        Ok(output) if output.status.success() => output.stdout,
        _ => return,
    };
    let rows: Vec<(String, u32)> = String::from_utf8_lossy(&listing)
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let name = parts.next()?.to_string();
            let attached = parts.next()?.parse().ok()?;
            Some((name, attached))
        })
        .collect();
    for session in ephemeral_sessions_to_reap_detached(&rows, keep) {
        let _ = run_tmux_command_blocking(&TmuxCommand::new(["kill-session", "-t", &session]));
    }
}

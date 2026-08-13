//! PTY-backed tmux attach for the browser task terminal bridge.

/// Transport input for a browser task terminal attach: the task handle, its
/// tmux session, and its task window. Owned by the PTY adapter that consumes it.
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

/// A single tmux invocation used to stand up or tear down the isolated client
/// session. Kept as a plain data plan so the wiring is unit-testable without a
/// live tmux server.
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

/// Attach a mobile client to its *own* grouped tmux session instead of the
/// shared task session.
///
/// `tmux attach-session` sizes a window to the smallest attached client, so a
/// phone in portrait would shrink the agent window for every other client and
/// SIGWINCH-storm the pane on each keyboard open/close. A grouped session
/// (`new-session -t <shared>`) shares the shared session's window set but keeps
/// an independent size, so the phone can be tiny without disturbing anyone. The
/// ephemeral session *lingers in tmux* on disconnect so the same client token
/// can reconnect to its existing viewport; it is only destroyed by the explicit
/// [`destroy_ephemeral_session_commands`] reaper path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IsolatedAttachPlan {
    /// The ephemeral grouped session name, e.g. `ajax-web-fix-login-m1a2b3c4`.
    pub ephemeral_session: String,
    /// Commands to run before attaching (create the grouped session).
    pub setup: Vec<TmuxCommand>,
    /// Existing task-pane history to seed before live PTY output.
    pub history: TmuxCommand,
    /// The attach command spawned inside the outer PTY.
    pub attach: TmuxAttachCommandPlan,
    /// Commands to run on disconnect (remove the grouped session).
    pub teardown: Vec<TmuxCommand>,
}

/// Prefix that marks a session as an ephemeral per-client grouped session.
/// The reaper uses this to distinguish them from real task sessions.
pub const EPHEMERAL_SESSION_INFIX: &str = "-m";

pub fn build_isolated_attach_plan(plan: &TerminalAttachPlan) -> IsolatedAttachPlan {
    // Random tokens cannot reconnect, so destroy on disconnect instead of lingering.
    let mut isolated = build_isolated_attach_plan_with_token(plan, &random_session_token());
    isolated.teardown = destroy_ephemeral_session_commands(&isolated.ephemeral_session);
    isolated
}

/// Build an isolated attach plan keyed to a stable client id. Two calls with the
/// same client id produce the *same* ephemeral session name, so a browser tab
/// reconnects to its existing tmux viewport instead of spinning up a new one.
/// Callers without a client id should keep using [`build_isolated_attach_plan`]
/// (random per call) to stay unique.
pub fn build_isolated_attach_plan_for_client(
    plan: &TerminalAttachPlan,
    client_id: &str,
) -> IsolatedAttachPlan {
    build_isolated_attach_plan_with_token(plan, &ephemeral_client_token(client_id))
}

/// Stable 12 lowercase-hex token for a browser client id. Empty / whitespace-
/// only ids fall back to a fresh random 12-hex token so callers that have no
/// client id stay unique per call rather than all collapsing onto one shared
/// session.
pub fn ephemeral_client_token(client_id: &str) -> String {
    let trimmed = client_id.trim();
    if trimmed.is_empty() {
        return random_session_token();
    }
    // FNV-1a 64-bit fold of the trimmed id bytes -> first 12 hex chars. No new
    // crate dependency; this only needs uniqueness across browser client ids,
    // not cryptographic resistance.
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in trimmed.as_bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    let full = format!("{hash:016x}");
    full[..12].to_string()
}

/// Explicit destroy commands for a lingering ephemeral session, used by the
/// reaper / manual destroy path. The normal disconnect teardown is intentionally
/// empty so reconnects reuse the viewport; this helper is the only thing that
/// kills a grouped session.
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
    // Reuse the shared attach builder against the ephemeral session so the
    // "never attach through the browser handle" and task-window guarantees
    // are preserved for the isolated client too.
    let ephemeral_plan = TerminalAttachPlan {
        qualified_handle: plan.qualified_handle.clone(),
        tmux_session: ephemeral.clone(),
        task_window: plan.task_window.clone(),
    };
    IsolatedAttachPlan {
        setup: vec![
            // Do not use `-A` here: attach-if-exists requires a TTY and breaks
            // reconnect from `run_tmux_command_blocking`. `-d` creates detached;
            // an already-present ephemeral session returns "duplicate session",
            // which setup treats as success.
            TmuxCommand::new([
                "new-session",
                "-d",
                "-s",
                &ephemeral,
                "-t",
                &plan.tmux_session,
            ]),
            // Quieter status redraw on the browser-only grouped session; never
            // touch the shared task session's options.
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
            // ponytail: matches DESKTOP_SCROLLBACK_LINES; raise both caps if deeper history matters.
            "-10000",
            "-E",
            "-1",
        ]),
        attach: build_tmux_attach_command_plan(&ephemeral_plan),
        // Disconnect leaves the ephemeral session in tmux so the same client
        // token can reconnect; the reaper kills it later via
        // `destroy_ephemeral_session_commands`.
        teardown: vec![],
        ephemeral_session: ephemeral,
    }
}

/// 12 lowercase-hex chars of randomness for the ephemeral session suffix.
pub(crate) fn random_session_token() -> String {
    let mut bytes = [0_u8; 6];
    // A failed RNG here only weakens uniqueness of a short-lived session name;
    // fall back to a time-derived token rather than aborting the attach.
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

/// True when `name` looks like an ephemeral per-client grouped session
/// (`<shared>-m<12 lowercase hex>`). Requires the full 12-hex token so real
/// task sessions such as `ajax-web-main` are never matched.
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

/// Select the ephemeral grouped sessions to kill from a list of live session
/// names. A crashed bridge can leave its per-client session behind; the web
/// server reaps them on startup so they don't accumulate.
pub fn ephemeral_sessions_to_reap(names: &[String]) -> Vec<String> {
    names
        .iter()
        .filter(|name| is_ephemeral_session_name(name))
        .cloned()
        .collect()
}

/// Ephemeral sessions with zero attached clients. Safe to kill while the web
/// server is live: active browser bridges keep `session_attached >= 1`.
/// When `exclude` is set, that session name is kept even if detached so a
/// reconnecting client can reattach to its lingered viewport.
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

/// Best-effort: list tmux sessions and kill any orphaned ephemeral grouped
/// sessions. Never fails the caller; if tmux is absent or has no server there
/// is nothing to reap.
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

/// Kill detached ephemeral sessions while the server is running. Call on each
/// terminal connect so remount/reconnect storms cannot accumulate hundreds of
/// `-m*` sessions (linger-by-design without a live reaper).
pub fn reap_detached_ephemeral_terminal_sessions() {
    reap_detached_ephemeral_terminal_sessions_except(None);
}

/// Like [`reap_detached_ephemeral_terminal_sessions`], but keeps one detached
/// ephemeral session (the reconnect target for this bridge connection).
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

use ajax_core::adapters::{CommandMode, CommandOutput, CommandRunner, CommandSpec};
use ajax_core::commands;
use nix::sys::termios::Termios;
use nix::{
    poll::PollFlags,
    pty::Winsize,
    sys::{termios::tcgetattr, wait::WaitStatus},
};
use std::{
    env,
    fs::{File, OpenOptions},
    io::{self, Write},
    os::fd::{AsFd, AsRawFd},
    path::PathBuf,
    time::{Duration, Instant},
};

use crate::{command_error, CliError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TaskInputAction {
    Forward,
    ReturnToCockpit,
    OpenNewTask,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TaskSessionContext {
    pub new_task_repo: Option<String>,
}

impl TaskSessionContext {
    pub(crate) fn from_task_handle(handle: &str) -> Self {
        Self {
            new_task_repo: repo_from_task_handle(handle),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TaskSessionEnd {
    Normal,
    OpenNewTask,
}

pub(crate) fn repo_from_task_handle(handle: &str) -> Option<String> {
    handle
        .split_once('/')
        .map(|(repo, _)| repo.to_string())
        .or_else(|| (!handle.is_empty()).then(|| handle.to_string()))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FilteredTaskInput {
    pub action: TaskInputAction,
    pub bytes: Vec<u8>,
}

pub(super) const STARTUP_INPUT_SUPPRESSION: Duration = Duration::from_millis(50);
pub(super) const TERM_ATTACH_AFTER: Duration = Duration::from_millis(100);
pub(super) const KILL_ATTACH_AFTER: Duration = Duration::from_millis(300);
pub(super) const GIVE_UP_ATTACH_AFTER: Duration = Duration::from_millis(600);
pub(super) const ATTACH_SHUTDOWN_POLL_INTERVAL: Duration = Duration::from_millis(10);
pub(super) const MAX_INTERRUPTED_ATTACH_RETRIES: usize = 3;
pub(super) const ATTACH_RETRY_STABLE_AFTER: Duration = Duration::from_secs(2);
pub(super) const ATTACH_OUTPUT_BUFFER_LIMIT: usize = 8192;
pub(super) const TASK_SESSION_TRACE_ENV: &str = "AJAX_TASK_SESSION_TRACE";
pub(super) const TASK_SCREEN_ENTRY_SEQUENCE: &[u8] =
    b"\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l\x1b[2J\x1b[H";
pub(super) const TASK_SCREEN_EXIT_SEQUENCE: &[u8] = b"\x1b[?25h";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TaskChildShutdownAction {
    Wait,
    Terminate,
    Kill,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TaskPollAction {
    Pump { tty_ready: bool, master_ready: bool },
    Detach,
    Close,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TaskPollErrorAction {
    Retry,
    Fatal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TaskPollAttempt {
    Retry,
    Ready(TaskPollAction),
    Fatal(nix::errno::Errno),
}

pub(super) struct TaskSessionTrace {
    started: Instant,
    file: Option<File>,
}

impl TaskSessionTrace {
    fn from_env() -> Result<Self, CliError> {
        let path = trace_path_from_env(env::var_os(TASK_SESSION_TRACE_ENV));
        Self::from_path(path)
    }

    fn from_path(path: Option<PathBuf>) -> Result<Self, CliError> {
        let file = match path {
            Some(path) => Some(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .map_err(|error| {
                        CliError::CommandFailed(format!(
                            "failed to open task session trace {}: {error}",
                            path.display()
                        ))
                    })?,
            ),
            None => None,
        };
        Ok(Self {
            started: Instant::now(),
            file,
        })
    }

    fn log(&mut self, event: &str, detail: impl AsRef<str>) {
        let Some(file) = self.file.as_mut() else {
            return;
        };
        let line = format_task_session_trace_line(self.started.elapsed(), event, detail.as_ref());
        let _ = file.write_all(line.as_bytes());
        let _ = file.flush();
    }

    #[cfg(test)]
    fn is_enabled(&self) -> bool {
        self.file.is_some()
    }
}

#[derive(Debug)]
pub(super) struct TaskAttachExit {
    output: Vec<u8>,
    status: Option<WaitStatus>,
    attached_for: Duration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TerminalOwnedSequence {
    FocusReport { len: usize },
    CsiReport { len: usize },
    SgrMouse { len: usize },
    X10Mouse { len: usize },
}

impl TerminalOwnedSequence {
    fn len(self) -> usize {
        match self {
            TerminalOwnedSequence::FocusReport { len }
            | TerminalOwnedSequence::CsiReport { len }
            | TerminalOwnedSequence::SgrMouse { len }
            | TerminalOwnedSequence::X10Mouse { len } => len,
        }
    }
}

pub(super) struct TaskPtyForkConfig {
    child_termios: Termios,
    raw_termios: Termios,
    winsize: Winsize,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TaskDetachStep {
    CloseAttachPty,
    SignalAttachChild,
    WaitForAttachChild,
}

pub(crate) trait TaskSessionRunner {
    fn run_task_session(
        &mut self,
        command: &CommandSpec,
        context: &TaskSessionContext,
    ) -> Result<TaskSessionEnd, CliError>;
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum TaskEntryPlanOutcome {
    Completed(Vec<CommandOutput>),
    OpenNewTask,
}

pub(crate) fn execute_task_entry_plan<R: CommandRunner, S: TaskSessionRunner>(
    plan: &commands::CommandPlan,
    runner: &mut R,
    task_session: &mut S,
    session_context: &TaskSessionContext,
) -> Result<TaskEntryPlanOutcome, CliError> {
    let mut setup_plan = commands::CommandPlan::new(plan.title.clone());
    setup_plan.requires_confirmation = plan.requires_confirmation;
    setup_plan.blocked_reasons = plan.blocked_reasons.clone();
    let mut task_command = None;

    for command in &plan.commands {
        match command.mode {
            CommandMode::Capture => setup_plan.commands.push(command.clone()),
            CommandMode::InheritStdio => {
                if task_command.replace(command.clone()).is_some() {
                    return Err(CliError::CommandFailed(
                        "task entry plan contains multiple interactive commands".to_string(),
                    ));
                }
            }
        }
    }

    let outputs = commands::execute_plan(&setup_plan, true, runner).map_err(command_error)?;
    let task_command = task_command.ok_or_else(|| {
        CliError::CommandFailed(
            "task entry plan did not include an interactive command".to_string(),
        )
    })?;
    match task_session.run_task_session(&task_command, session_context)? {
        TaskSessionEnd::Normal => Ok(TaskEntryPlanOutcome::Completed(outputs)),
        TaskSessionEnd::OpenNewTask => Ok(TaskEntryPlanOutcome::OpenNewTask),
    }
}

pub(crate) fn filter_task_input(input: &[u8]) -> FilteredTaskInput {
    let mut bytes = Vec::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        if let Some(len) = terminal_owned_sequence_len(&input[index..]) {
            index += len;
            continue;
        }

        match input[index] {
            0x11 => {
                return FilteredTaskInput {
                    action: TaskInputAction::ReturnToCockpit,
                    bytes,
                };
            }
            0x14 => {
                return FilteredTaskInput {
                    action: TaskInputAction::OpenNewTask,
                    bytes,
                };
            }
            0x13 => {}
            byte => bytes.push(byte),
        }
        index += 1;
    }

    FilteredTaskInput {
        action: TaskInputAction::Forward,
        bytes,
    }
}

pub(super) fn filter_task_input_after_startup_grace_period(
    input: &[u8],
    elapsed_since_attach: Duration,
) -> FilteredTaskInput {
    if elapsed_since_attach < STARTUP_INPUT_SUPPRESSION && is_startup_terminal_probe(input) {
        return FilteredTaskInput {
            action: TaskInputAction::Forward,
            bytes: Vec::new(),
        };
    }
    filter_task_input(input)
}

pub(super) fn is_startup_terminal_probe(input: &[u8]) -> bool {
    terminal_owned_sequence_len(input) == Some(input.len())
}

pub(super) fn terminal_owned_sequence_len(input: &[u8]) -> Option<usize> {
    terminal_owned_sequence(input).map(TerminalOwnedSequence::len)
}

pub(super) fn terminal_owned_sequence(input: &[u8]) -> Option<TerminalOwnedSequence> {
    if input.starts_with(b"\x1b[I") || input.starts_with(b"\x1b[O") {
        return Some(TerminalOwnedSequence::FocusReport { len: 3 });
    }
    if input.starts_with(b"\x1b[?") {
        return terminal_csi_report_len(input).map(|len| TerminalOwnedSequence::CsiReport { len });
    }
    if input.starts_with(b"\x1b[<") {
        return sgr_mouse_sequence(input).and_then(|(button_code, len)| {
            (!is_scroll_mouse_button_code(button_code))
                .then_some(TerminalOwnedSequence::SgrMouse { len })
        });
    }
    if input.starts_with(b"\x1b[M") && input.len() >= 6 {
        let button_code = (input[3] as usize).saturating_sub(32);
        return (!is_scroll_mouse_button_code(button_code))
            .then_some(TerminalOwnedSequence::X10Mouse { len: 6 });
    }
    None
}

pub(super) fn terminal_csi_report_len(input: &[u8]) -> Option<usize> {
    for (offset, byte) in input.iter().enumerate().skip(3) {
        if byte.is_ascii_digit() || *byte == b';' {
            continue;
        }
        return (*byte == b'c' || *byte == b'n').then_some(offset + 1);
    }
    None
}

pub(super) fn sgr_mouse_sequence(input: &[u8]) -> Option<(usize, usize)> {
    let mut offset = 3;
    let mut button_code = 0usize;
    let mut saw_digit = false;
    while let Some(byte) = input.get(offset) {
        if !byte.is_ascii_digit() {
            break;
        }
        saw_digit = true;
        button_code = button_code
            .checked_mul(10)?
            .checked_add((byte - b'0') as usize)?;
        offset += 1;
    }
    if !saw_digit || input.get(offset) != Some(&b';') {
        return None;
    }

    for (offset, byte) in input.iter().enumerate().skip(offset + 1) {
        if byte.is_ascii_digit() || *byte == b';' {
            continue;
        }
        return (*byte == b'M' || *byte == b'm').then_some((button_code, offset + 1));
    }
    None
}

pub(super) fn is_scroll_mouse_button_code(button_code: usize) -> bool {
    button_code & 64 != 0
}

pub(super) fn task_child_shutdown_action(
    elapsed: Duration,
    sent_terminate: bool,
    sent_kill: bool,
) -> TaskChildShutdownAction {
    if elapsed >= KILL_ATTACH_AFTER && !sent_kill {
        return TaskChildShutdownAction::Kill;
    }
    if elapsed >= TERM_ATTACH_AFTER && !sent_terminate {
        return TaskChildShutdownAction::Terminate;
    }
    TaskChildShutdownAction::Wait
}

pub(super) fn classify_task_poll_events(
    tty_flags: PollFlags,
    master_flags: PollFlags,
) -> TaskPollAction {
    if tty_flags.contains(PollFlags::POLLNVAL) {
        return TaskPollAction::Detach;
    }
    if master_flags.contains(PollFlags::POLLNVAL) {
        return TaskPollAction::Close;
    }
    if master_flags.intersects(PollFlags::POLLERR | PollFlags::POLLHUP) {
        return TaskPollAction::Close;
    }

    let transient_tty_event = tty_flags.intersects(PollFlags::POLLERR | PollFlags::POLLHUP);
    TaskPollAction::Pump {
        tty_ready: tty_flags.contains(PollFlags::POLLIN) && !transient_tty_event,
        master_ready: master_flags.contains(PollFlags::POLLIN),
    }
}

pub(super) fn attach_exit_allows_retry(exit: &TaskAttachExit) -> bool {
    !attach_status_succeeded(exit.status.as_ref())
        && attach_output_mentions_interrupted(&exit.output)
}

pub(super) fn attach_status_succeeded(status: Option<&WaitStatus>) -> bool {
    matches!(status, Some(WaitStatus::Exited(_, 0)))
}

pub(super) fn attach_output_mentions_interrupted(output: &[u8]) -> bool {
    let output = String::from_utf8_lossy(output).to_ascii_lowercase();
    output.contains("eintr") || output.contains("interrupted system call")
}

pub(super) fn classify_task_poll_error(error: nix::errno::Errno) -> TaskPollErrorAction {
    if error == nix::errno::Errno::EINTR {
        TaskPollErrorAction::Retry
    } else {
        TaskPollErrorAction::Fatal
    }
}

pub(super) fn classify_task_poll_attempt(
    result: Result<i32, nix::errno::Errno>,
    tty_flags: PollFlags,
    master_flags: PollFlags,
) -> TaskPollAttempt {
    match result {
        Ok(_) => TaskPollAttempt::Ready(classify_task_poll_events(tty_flags, master_flags)),
        Err(error) => match classify_task_poll_error(error) {
            TaskPollErrorAction::Retry => TaskPollAttempt::Retry,
            TaskPollErrorAction::Fatal => TaskPollAttempt::Fatal(error),
        },
    }
}

pub(super) fn trace_path_from_env(value: Option<std::ffi::OsString>) -> Option<PathBuf> {
    value.filter(|path| !path.is_empty()).map(PathBuf::from)
}

pub(super) fn format_task_session_trace_line(
    elapsed: Duration,
    event: &str,
    detail: &str,
) -> String {
    let event = trace_field(event);
    let detail = trace_detail(detail);
    format!(
        "elapsed_ms={} event={} {}\n",
        elapsed.as_millis(),
        event,
        detail
    )
}

pub(super) fn trace_field(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | ':') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

pub(super) fn trace_detail(value: &str) -> String {
    value.replace(['\r', '\n'], "\\n")
}

pub(super) fn command_for_trace(command: &CommandSpec) -> String {
    std::iter::once(command.program.as_str())
        .chain(command.args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
pub(super) fn task_detach_sequence() -> &'static [TaskDetachStep] {
    &[
        TaskDetachStep::CloseAttachPty,
        TaskDetachStep::SignalAttachChild,
        TaskDetachStep::WaitForAttachChild,
    ]
}

#[derive(Default)]
pub(crate) struct PtyTaskSessionRunner;

impl TaskSessionRunner for PtyTaskSessionRunner {
    fn run_task_session(
        &mut self,
        command: &CommandSpec,
        context: &TaskSessionContext,
    ) -> Result<TaskSessionEnd, CliError> {
        run_pty_task_session(command, context)
    }
}

pub(super) fn run_pty_task_session(
    command: &CommandSpec,
    context: &TaskSessionContext,
) -> Result<TaskSessionEnd, CliError> {
    let prepared = PreparedTaskCommand::new(command)?;
    debug_assert_eq!(prepared.argv.len(), prepared.args.len() + 1);
    let stdin = io::stdin();
    let original_termios =
        tcgetattr(stdin.as_fd()).map_err(tty_error("failed to read terminal mode"))?;
    let child_winsize = read_task_terminal_winsize(stdin.as_raw_fd())?;
    let fork_config = task_pty_fork_config(
        &original_termios,
        child_winsize.ws_row,
        child_winsize.ws_col,
    );
    let mut terminal = TaskOperatorTerminal::open()?;
    let _guard = terminal.enter_raw_mode(original_termios, &fork_config.raw_termios)?;
    let _winch_guard = TaskWinchGuard::install()?;
    let _screen_guard = TaskScreenGuard::enter(&mut terminal.output)?;
    let mut trace = TaskSessionTrace::from_env()?;
    trace.log(
        "session_start",
        format!("command={}", command_for_trace(command)),
    );
    let mut consecutive_interrupted_retries = 0;

    loop {
        match run_pty_task_attach(
            &prepared,
            &fork_config,
            &mut terminal.input,
            &mut terminal.output,
            &mut trace,
            context,
        )? {
            PtyAttachResult::Detached {
                open_new_task: false,
            } => {
                trace.log("session_end", "outcome=detached");
                return Ok(TaskSessionEnd::Normal);
            }
            PtyAttachResult::Detached {
                open_new_task: true,
            } => {
                trace.log("session_end", "outcome=detached reason=ctrl_t");
                return Ok(TaskSessionEnd::OpenNewTask);
            }
            PtyAttachResult::ClientExit(exit) => {
                if !attach_exit_allows_retry(&exit) {
                    trace.log(
                        "session_end",
                        format!(
                            "outcome=attach_client_exited retry=false attached_ms={}",
                            exit.attached_for.as_millis()
                        ),
                    );
                    return Ok(TaskSessionEnd::Normal);
                }
                if exit.attached_for >= ATTACH_RETRY_STABLE_AFTER {
                    consecutive_interrupted_retries = 0;
                }
                if consecutive_interrupted_retries >= MAX_INTERRUPTED_ATTACH_RETRIES {
                    trace.log(
                        "session_end",
                        format!("outcome=retry_limit retries={consecutive_interrupted_retries}"),
                    );
                    return Err(CliError::CommandFailed(
                        "task attach client repeatedly exited after interrupted system call"
                            .to_string(),
                    ));
                }
                trace.log(
                    "reattach",
                    format!(
                        "reason=interrupted_attach retries={} attached_ms={}",
                        consecutive_interrupted_retries + 1,
                        exit.attached_for.as_millis()
                    ),
                );
                consecutive_interrupted_retries += 1;
            }
        }
    }
}

mod attach;
use attach::*;

#[cfg(test)]
mod tests;

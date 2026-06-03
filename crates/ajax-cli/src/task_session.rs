use ajax_core::adapters::{CommandMode, CommandOutput, CommandRunner, CommandSpec};
use ajax_core::commands;
use nix::sys::termios::{
    cfmakeraw, InputFlags, LocalFlags, OutputFlags, SpecialCharacterIndices, Termios,
};
use nix::unistd::dup;
use nix::{
    poll::{poll, PollFd, PollFlags, PollTimeout},
    pty::{forkpty, ForkptyResult, Winsize},
    sys::{
        signal::{kill, sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal},
        termios::{tcgetattr, tcsetattr, SetArg},
        wait::{waitpid, WaitPidFlag, WaitStatus},
    },
};
use std::{
    env,
    ffi::CString,
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd},
    os::raw::c_char,
    os::unix::ffi::OsStrExt,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    thread::sleep,
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

const STARTUP_INPUT_SUPPRESSION: Duration = Duration::from_millis(50);
const TERM_ATTACH_AFTER: Duration = Duration::from_millis(100);
const KILL_ATTACH_AFTER: Duration = Duration::from_millis(300);
const GIVE_UP_ATTACH_AFTER: Duration = Duration::from_millis(600);
const ATTACH_SHUTDOWN_POLL_INTERVAL: Duration = Duration::from_millis(10);
const MAX_INTERRUPTED_ATTACH_RETRIES: usize = 3;
const ATTACH_RETRY_STABLE_AFTER: Duration = Duration::from_secs(2);
const ATTACH_OUTPUT_BUFFER_LIMIT: usize = 8192;
const TASK_SESSION_TRACE_ENV: &str = "AJAX_TASK_SESSION_TRACE";
const TASK_SCREEN_ENTRY_SEQUENCE: &[u8] =
    b"\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l\x1b[2J\x1b[H";
const TASK_SCREEN_EXIT_SEQUENCE: &[u8] = b"\x1b[?25h";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TaskChildShutdownAction {
    Wait,
    Terminate,
    Kill,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TaskPollAction {
    Pump { tty_ready: bool, master_ready: bool },
    Detach,
    Close,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TaskPollErrorAction {
    Retry,
    Fatal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TaskPollAttempt {
    Retry,
    Ready(TaskPollAction),
    Fatal(nix::errno::Errno),
}

struct TaskSessionTrace {
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
struct TaskAttachExit {
    output: Vec<u8>,
    status: Option<WaitStatus>,
    attached_for: Duration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalOwnedSequence {
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

struct TaskPtyForkConfig {
    child_termios: Termios,
    raw_termios: Termios,
    winsize: Winsize,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TaskDetachStep {
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

fn filter_task_input_after_startup_grace_period(
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

fn is_startup_terminal_probe(input: &[u8]) -> bool {
    terminal_owned_sequence_len(input) == Some(input.len())
}

fn terminal_owned_sequence_len(input: &[u8]) -> Option<usize> {
    terminal_owned_sequence(input).map(TerminalOwnedSequence::len)
}

fn terminal_owned_sequence(input: &[u8]) -> Option<TerminalOwnedSequence> {
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

fn terminal_csi_report_len(input: &[u8]) -> Option<usize> {
    for (offset, byte) in input.iter().enumerate().skip(3) {
        if byte.is_ascii_digit() || *byte == b';' {
            continue;
        }
        return (*byte == b'c' || *byte == b'n').then_some(offset + 1);
    }
    None
}

fn sgr_mouse_sequence(input: &[u8]) -> Option<(usize, usize)> {
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

fn is_scroll_mouse_button_code(button_code: usize) -> bool {
    button_code & 64 != 0
}

fn task_child_shutdown_action(
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

fn classify_task_poll_events(tty_flags: PollFlags, master_flags: PollFlags) -> TaskPollAction {
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

fn attach_exit_allows_retry(exit: &TaskAttachExit) -> bool {
    !attach_status_succeeded(exit.status.as_ref())
        && attach_output_mentions_interrupted(&exit.output)
}

fn attach_status_succeeded(status: Option<&WaitStatus>) -> bool {
    matches!(status, Some(WaitStatus::Exited(_, 0)))
}

fn attach_output_mentions_interrupted(output: &[u8]) -> bool {
    let output = String::from_utf8_lossy(output).to_ascii_lowercase();
    output.contains("eintr") || output.contains("interrupted system call")
}

fn classify_task_poll_error(error: nix::errno::Errno) -> TaskPollErrorAction {
    if error == nix::errno::Errno::EINTR {
        TaskPollErrorAction::Retry
    } else {
        TaskPollErrorAction::Fatal
    }
}

fn classify_task_poll_attempt(
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

fn trace_path_from_env(value: Option<std::ffi::OsString>) -> Option<PathBuf> {
    value.filter(|path| !path.is_empty()).map(PathBuf::from)
}

fn format_task_session_trace_line(elapsed: Duration, event: &str, detail: &str) -> String {
    let event = trace_field(event);
    let detail = trace_detail(detail);
    format!(
        "elapsed_ms={} event={} {}\n",
        elapsed.as_millis(),
        event,
        detail
    )
}

fn trace_field(value: &str) -> String {
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

fn trace_detail(value: &str) -> String {
    value.replace(['\r', '\n'], "\\n")
}

fn command_for_trace(command: &CommandSpec) -> String {
    std::iter::once(command.program.as_str())
        .chain(command.args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
fn task_detach_sequence() -> &'static [TaskDetachStep] {
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

fn run_pty_task_session(
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

#[derive(Debug)]
enum PtyAttachResult {
    Detached { open_new_task: bool },
    ClientExit(TaskAttachExit),
}

fn run_pty_task_attach(
    prepared: &PreparedTaskCommand,
    fork_config: &TaskPtyForkConfig,
    terminal_input: &mut File,
    terminal_output: &mut File,
    trace: &mut TaskSessionTrace,
    context: &TaskSessionContext,
) -> Result<PtyAttachResult, CliError> {
    // SAFETY: The parent only touches the returned master fd. In the child
    // branch, all fallible setup was prepared before fork, and the process
    // either execs the requested command or exits immediately.
    match unsafe { forkpty(Some(&fork_config.winsize), Some(&fork_config.child_termios)) }
        .map_err(tty_error("failed to fork task PTY"))?
    {
        ForkptyResult::Child => {
            // SAFETY: The env name is a pre-fork CString with a stable nul-terminated pointer.
            unsafe { nix::libc::unsetenv(prepared.tmux_env_name.as_ptr()) };
            if let Some(cwd) = prepared.cwd.as_ref() {
                // SAFETY: cwd is a pre-fork CString with a stable nul-terminated pointer.
                if unsafe { nix::libc::chdir(cwd.as_ptr()) } != 0 {
                    exit_child_after_exec_failure();
                }
            }
            // SAFETY: executable and argv are fully prepared before fork and
            // remain alive in this child branch until execvp replaces the process.
            unsafe { nix::libc::execvp(prepared.executable.as_ptr(), prepared.argv.as_ptr()) };
            exit_child_after_exec_failure();
        }
        ForkptyResult::Parent { child, master } => {
            trace.log("attach_start", format!("child={}", child.as_raw()));
            pump_task_pty(
                terminal_input,
                terminal_output,
                master,
                child,
                trace,
                context,
            )
        }
    }
}

fn task_pty_fork_config(original_termios: &Termios, rows: u16, columns: u16) -> TaskPtyForkConfig {
    TaskPtyForkConfig {
        child_termios: child_task_termios(original_termios),
        raw_termios: ajax_raw_termios(original_termios),
        winsize: task_pty_winsize(rows, columns),
    }
}

fn read_task_terminal_winsize(fd: i32) -> Result<Winsize, CliError> {
    // SAFETY: ioctl writes into the provided winsize struct for a valid terminal fd.
    let mut raw: nix::libc::winsize = unsafe { std::mem::zeroed() };
    let result = unsafe { nix::libc::ioctl(fd, nix::libc::TIOCGWINSZ, &mut raw) };
    if result != 0 {
        return Err(CliError::CommandFailed(format!(
            "failed to read terminal window size: {}",
            io::Error::last_os_error()
        )));
    }
    Ok(task_pty_winsize(raw.ws_row, raw.ws_col))
}

fn task_pty_winsize(rows: u16, columns: u16) -> Winsize {
    Winsize {
        ws_row: rows,
        ws_col: columns,
        ws_xpixel: 0,
        ws_ypixel: 0,
    }
}

struct TaskOperatorTerminal {
    input: File,
    output: File,
}

impl TaskOperatorTerminal {
    fn open() -> Result<Self, CliError> {
        let stdin = io::stdin();
        let stdout = io::stdout();
        let input = duplicate_task_terminal_fd(
            stdin.as_raw_fd(),
            "failed to duplicate task terminal input",
        )?;
        let output = duplicate_task_terminal_fd(
            stdout.as_raw_fd(),
            "failed to duplicate task terminal output",
        )?;
        Ok(Self { input, output })
    }

    fn enter_raw_mode(
        &mut self,
        original: Termios,
        raw: &Termios,
    ) -> Result<TtyTermiosGuard, CliError> {
        let restore_input = self.input.try_clone().map_err(|error| {
            CliError::CommandFailed(format!("failed to clone task terminal input: {error}"))
        })?;
        tcsetattr(&self.input, SetArg::TCSANOW, raw)
            .map_err(tty_error("failed to set raw terminal mode"))?;
        Ok(TtyTermiosGuard {
            tty: restore_input,
            original,
        })
    }
}

/// Set by the SIGWINCH handler whenever the operator terminal is resized.
/// Seeded `true` so the pump syncs the size once on attach, covering any
/// resize that slipped between reading the winsize and forking the PTY.
static WINCH_PENDING: AtomicBool = AtomicBool::new(true);

extern "C" fn handle_winch(_: nix::libc::c_int) {
    // Async-signal-safe: a single relaxed atomic store, nothing more.
    WINCH_PENDING.store(true, Ordering::Relaxed);
}

/// Installs a SIGWINCH handler for the duration of an attach and restores the
/// previous disposition on drop. The handler must exist (not SIG_IGN/SIG_DFL)
/// so the resize interrupts the pump's blocking `poll` with EINTR.
struct TaskWinchGuard {
    previous: SigAction,
}

impl TaskWinchGuard {
    fn install() -> Result<Self, CliError> {
        WINCH_PENDING.store(true, Ordering::Relaxed);
        let action = SigAction::new(
            SigHandler::Handler(handle_winch),
            // No SA_RESTART: we want `poll` interrupted so the loop reacts.
            SaFlags::empty(),
            SigSet::empty(),
        );
        // SAFETY: `handle_winch` is async-signal-safe (one atomic store).
        let previous = unsafe { sigaction(Signal::SIGWINCH, &action) }
            .map_err(tty_error("failed to install resize handler"))?;
        Ok(Self { previous })
    }
}

impl Drop for TaskWinchGuard {
    fn drop(&mut self) {
        // SAFETY: restoring the disposition captured at install time.
        let _ = unsafe { sigaction(Signal::SIGWINCH, &self.previous) };
    }
}

/// Reads the operator terminal's current window size, or `None` if the ioctl
/// fails (e.g. the descriptor is no longer a tty).
fn read_operator_winsize(fd: i32) -> Option<nix::libc::winsize> {
    // SAFETY: ioctl writes into the provided winsize struct for a tty fd.
    let mut raw: nix::libc::winsize = unsafe { std::mem::zeroed() };
    let result = unsafe { nix::libc::ioctl(fd, nix::libc::TIOCGWINSZ, &mut raw) };
    (result == 0).then_some(raw)
}

fn winsize_changed(last: Option<(u16, u16)>, current: (u16, u16)) -> bool {
    last != Some(current)
}

/// Propagates a pending operator resize to the PTY master so the attached
/// client (tmux) re-renders at the live terminal size. No-op unless SIGWINCH
/// fired since the last call and the size actually changed.
fn sync_pending_winsize(
    operator_fd: i32,
    master_fd: i32,
    last: &mut Option<(u16, u16)>,
    trace: &mut TaskSessionTrace,
) {
    if !WINCH_PENDING.swap(false, Ordering::Relaxed) {
        return;
    }
    let Some(raw) = read_operator_winsize(operator_fd) else {
        trace.log("winsize_read_err", "ioctl=TIOCGWINSZ");
        return;
    };
    let current = (raw.ws_row, raw.ws_col);
    if !winsize_changed(*last, current) {
        return;
    }
    // SAFETY: ioctl reads the winsize struct for a valid master fd.
    let result = unsafe { nix::libc::ioctl(master_fd, nix::libc::TIOCSWINSZ, &raw) };
    if result != 0 {
        trace.log(
            "winsize_apply_err",
            format!("error={}", io::Error::last_os_error()),
        );
        return;
    }
    *last = Some(current);
    trace.log(
        "winsize_apply",
        format!("rows={} cols={}", current.0, current.1),
    );
}

struct TaskScreenGuard {
    output: File,
}

impl TaskScreenGuard {
    fn enter(output: &mut File) -> Result<Self, CliError> {
        output
            .write_all(TASK_SCREEN_ENTRY_SEQUENCE)
            .and_then(|_| output.flush())
            .map_err(io_error("failed to enter task screen"))?;
        let output = output.try_clone().map_err(|error| {
            CliError::CommandFailed(format!("failed to clone task screen output: {error}"))
        })?;
        Ok(Self { output })
    }
}

impl Drop for TaskScreenGuard {
    fn drop(&mut self) {
        let _ = self.output.write_all(TASK_SCREEN_EXIT_SEQUENCE);
        let _ = self.output.flush();
    }
}

fn duplicate_task_terminal_fd(fd: i32, context: &'static str) -> Result<File, CliError> {
    let duplicate = dup(fd).map_err(tty_error(context))?;
    // SAFETY: dup returns a fresh owned file descriptor. File takes ownership
    // and closes it when dropped.
    Ok(unsafe { File::from_raw_fd(duplicate) })
}

fn pump_task_pty(
    terminal_input: &mut File,
    terminal_output: &mut File,
    master: OwnedFd,
    child: nix::unistd::Pid,
    trace: &mut TaskSessionTrace,
    context: &TaskSessionContext,
) -> Result<PtyAttachResult, CliError> {
    let mut master = File::from(master);
    let mut tty_input = [0_u8; 4096];
    let mut pty_output = [0_u8; 8192];
    let mut recent_output = Vec::new();
    let mut last_winsize: Option<(u16, u16)> = None;
    let attached_at = Instant::now();

    loop {
        sync_pending_winsize(
            terminal_input.as_raw_fd(),
            master.as_raw_fd(),
            &mut last_winsize,
            trace,
        );

        let poll_action = {
            let mut poll_fds = [
                PollFd::new(terminal_input.as_fd(), PollFlags::POLLIN),
                PollFd::new(
                    master.as_fd(),
                    PollFlags::POLLIN | PollFlags::POLLHUP | PollFlags::POLLERR,
                ),
            ];
            let poll_result = poll(&mut poll_fds, PollTimeout::NONE);
            let tty_flags = poll_fds[0].revents().unwrap_or_else(PollFlags::empty);
            let master_flags = poll_fds[1].revents().unwrap_or_else(PollFlags::empty);
            match classify_task_poll_attempt(poll_result, tty_flags, master_flags) {
                TaskPollAttempt::Retry => {
                    trace.log("poll_interrupted", "action=retry");
                    continue;
                }
                TaskPollAttempt::Fatal(error) => {
                    trace.log("poll_err", format!("error={error}"));
                    return Err(tty_error("failed to poll task PTY")(error));
                }
                TaskPollAttempt::Ready(action) => {
                    trace.log(
                        "poll_flags",
                        format!("tty={tty_flags:?} master={master_flags:?} action={action:?}"),
                    );
                    action
                }
            }
        };

        let (tty_ready, master_ready) = match poll_action {
            TaskPollAction::Pump {
                tty_ready,
                master_ready,
            } => (tty_ready, master_ready),
            TaskPollAction::Detach => {
                trace.log("outcome", "kind=detach reason=tty_poll");
                return detach_task_child(master, child, false);
            }
            TaskPollAction::Close => {
                trace.log("outcome", "kind=attach_exit reason=master_poll");
                return attach_client_exit(child, recent_output, attached_at.elapsed(), trace);
            }
        };

        if tty_ready {
            let count = match terminal_input.read(&mut tty_input) {
                Ok(count) => {
                    trace.log("tty_read", format!("bytes={count}"));
                    count
                }
                Err(error) => {
                    trace.log("tty_read_err", format!("error={error}"));
                    return Err(io_error("failed to read task terminal input")(error));
                }
            };
            if count == 0 {
                trace.log("outcome", "kind=detach reason=tty_eof");
                return detach_task_child(master, child, false);
            }
            let filtered = filter_task_input_after_startup_grace_period(
                &tty_input[..count],
                attached_at.elapsed(),
            );
            if !filtered.bytes.is_empty() {
                if let Err(error) = master.write_all(&filtered.bytes) {
                    trace.log("master_write_err", format!("error={error}"));
                    return Err(io_error("failed to write task PTY")(error));
                }
                trace.log("master_write", format!("bytes={}", filtered.bytes.len()));
            }
            match filtered.action {
                TaskInputAction::ReturnToCockpit => {
                    trace.log("outcome", "kind=detach reason=ctrl_q");
                    return detach_task_child(master, child, false);
                }
                TaskInputAction::OpenNewTask if context.new_task_repo.is_some() => {
                    trace.log("outcome", "kind=detach reason=ctrl_t");
                    return detach_task_child(master, child, true);
                }
                TaskInputAction::Forward | TaskInputAction::OpenNewTask => {}
            }
        }

        if master_ready {
            match master.read(&mut pty_output) {
                Ok(0) => {
                    trace.log("master_read", "bytes=0");
                    trace.log("outcome", "kind=attach_exit reason=master_eof");
                    return attach_client_exit(child, recent_output, attached_at.elapsed(), trace);
                }
                Ok(count) => {
                    trace.log("master_read", format!("bytes={count}"));
                    append_recent_output(&mut recent_output, &pty_output[..count]);
                    if let Err(error) = terminal_output.write_all(&pty_output[..count]) {
                        trace.log("tty_write_err", format!("error={error}"));
                        return Err(io_error("failed to write task terminal output")(error));
                    }
                    trace.log("tty_write", format!("bytes={count}"));
                    if let Err(error) = terminal_output.flush() {
                        trace.log("tty_flush_err", format!("error={error}"));
                        return Err(io_error("failed to flush task terminal output")(error));
                    }
                }
                Err(error) if pty_was_closed(&error) => {
                    trace.log("master_read_closed", format!("error={error}"));
                    trace.log("outcome", "kind=attach_exit reason=master_closed");
                    return attach_client_exit(child, recent_output, attached_at.elapsed(), trace);
                }
                Err(error) => {
                    trace.log("master_read_err", format!("error={error}"));
                    return Err(CliError::CommandFailed(format!(
                        "failed to read task PTY: {error}"
                    )));
                }
            }
        }
    }
}

fn append_recent_output(output: &mut Vec<u8>, bytes: &[u8]) {
    output.extend_from_slice(bytes);
    if output.len() > ATTACH_OUTPUT_BUFFER_LIMIT {
        let excess = output.len() - ATTACH_OUTPUT_BUFFER_LIMIT;
        output.drain(..excess);
    }
}

fn attach_client_exit(
    child: nix::unistd::Pid,
    output: Vec<u8>,
    attached_for: Duration,
    trace: &mut TaskSessionTrace,
) -> Result<PtyAttachResult, CliError> {
    let status = wait_for_attach_child_status(child)?;
    trace.log(
        "child_status",
        format!(
            "status={status:?} attached_ms={} output_bytes={}",
            attached_for.as_millis(),
            output.len()
        ),
    );
    Ok(PtyAttachResult::ClientExit(TaskAttachExit {
        output,
        status,
        attached_for,
    }))
}

fn wait_for_attach_child_status(child: nix::unistd::Pid) -> Result<Option<WaitStatus>, CliError> {
    let started = Instant::now();
    loop {
        match waitpid(child, Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::StillAlive) => {}
            Ok(status) => return Ok(Some(status)),
            Err(nix::errno::Errno::ECHILD) => return Ok(None),
            Err(error) => {
                return Err(CliError::CommandFailed(format!(
                    "failed to wait for task attach client: {error}"
                )));
            }
        }
        if started.elapsed() >= GIVE_UP_ATTACH_AFTER {
            return Ok(None);
        }
        sleep(ATTACH_SHUTDOWN_POLL_INTERVAL);
    }
}

fn detach_task_child(
    master: File,
    child: nix::unistd::Pid,
    open_new_task: bool,
) -> Result<PtyAttachResult, CliError> {
    drop(master);
    request_task_child_exit(child)?;
    Ok(PtyAttachResult::Detached { open_new_task })
}

fn request_task_child_exit(child: nix::unistd::Pid) -> Result<(), CliError> {
    let _ = kill(child, Signal::SIGHUP);
    wait_for_task_child_exit(child)
}

fn wait_for_task_child_exit(child: nix::unistd::Pid) -> Result<(), CliError> {
    let started = Instant::now();
    let mut sent_terminate = false;
    let mut sent_kill = false;

    loop {
        match waitpid(child, Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::Exited(_, _)) | Ok(WaitStatus::Signaled(_, _, _)) => return Ok(()),
            Ok(WaitStatus::StillAlive) => {}
            Ok(_) => {}
            Err(nix::errno::Errno::ECHILD) => {
                return Ok(());
            }
            Err(error) => {
                return Err(CliError::CommandFailed(format!(
                    "failed to wait for task attach client: {error}"
                )));
            }
        }

        let elapsed = started.elapsed();
        if elapsed >= GIVE_UP_ATTACH_AFTER {
            return Err(CliError::CommandFailed(
                "task attach client did not exit after detach".to_string(),
            ));
        }

        match task_child_shutdown_action(elapsed, sent_terminate, sent_kill) {
            TaskChildShutdownAction::Wait => {}
            TaskChildShutdownAction::Terminate => {
                let _ = kill(child, Signal::SIGTERM);
                sent_terminate = true;
            }
            TaskChildShutdownAction::Kill => {
                let _ = kill(child, Signal::SIGKILL);
                sent_kill = true;
            }
        }
        sleep(ATTACH_SHUTDOWN_POLL_INTERVAL);
    }
}

fn ajax_raw_termios(original: &Termios) -> Termios {
    let mut termios = original.clone();
    cfmakeraw(&mut termios);
    termios
        .input_flags
        .remove(InputFlags::IXON | InputFlags::IXOFF | InputFlags::IXANY);
    termios.control_chars[SpecialCharacterIndices::VMIN as usize] = 1;
    termios.control_chars[SpecialCharacterIndices::VTIME as usize] = 0;
    termios
}

fn child_task_termios(original: &Termios) -> Termios {
    let mut termios = original.clone();
    termios.input_flags.insert(InputFlags::ICRNL);
    termios
        .local_flags
        .insert(LocalFlags::ICANON | LocalFlags::ECHO | LocalFlags::ISIG | LocalFlags::IEXTEN);
    termios
        .output_flags
        .insert(OutputFlags::OPOST | OutputFlags::ONLCR);
    termios
}

struct PreparedTaskCommand {
    executable: CString,
    args: Vec<CString>,
    argv: Vec<*const c_char>,
    cwd: Option<CString>,
    tmux_env_name: CString,
}

impl PreparedTaskCommand {
    fn new(command: &CommandSpec) -> Result<Self, CliError> {
        let executable = CString::new(command.program.as_str())
            .map_err(|_| CliError::CommandFailed("task command contains a nul byte".to_string()))?;
        let mut args = Vec::with_capacity(command.args.len() + 1);
        args.push(executable.clone());
        for arg in &command.args {
            args.push(CString::new(arg.as_str()).map_err(|_| {
                CliError::CommandFailed("task command argument contains a nul byte".to_string())
            })?);
        }
        let mut argv = args
            .iter()
            .map(|arg| arg.as_ptr())
            .collect::<Vec<*const c_char>>();
        argv.push(std::ptr::null());
        let cwd = command
            .cwd
            .as_ref()
            .map(|path| {
                CString::new(path.as_os_str().as_bytes()).map_err(|_| {
                    CliError::CommandFailed("task command cwd contains a nul byte".to_string())
                })
            })
            .transpose()?;

        Ok(Self {
            executable,
            args,
            argv,
            cwd,
            tmux_env_name: CString::new("TMUX").map_err(|_| {
                CliError::CommandFailed(
                    "task command environment name contains a nul byte".to_string(),
                )
            })?,
        })
    }
}

struct TtyTermiosGuard {
    tty: File,
    original: Termios,
}

impl Drop for TtyTermiosGuard {
    fn drop(&mut self) {
        let _ = tcsetattr(&self.tty, SetArg::TCSANOW, &self.original);
    }
}

fn tty_error(context: &'static str) -> impl FnOnce(nix::errno::Errno) -> CliError {
    move |error| CliError::CommandFailed(format!("{context}: {error}"))
}

fn io_error(context: &'static str) -> impl FnOnce(io::Error) -> CliError {
    move |error| CliError::CommandFailed(format!("{context}: {error}"))
}

fn pty_was_closed(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::UnexpectedEof || error.raw_os_error() == Some(nix::libc::EIO)
}

fn exit_child_after_exec_failure() -> ! {
    // SAFETY: This is the child branch immediately after fork. Exiting through
    // libc avoids running parent process cleanup paths in the forked process.
    unsafe { nix::libc::_exit(127) }
}

#[cfg(test)]
mod tests {
    use super::{
        execute_task_entry_plan, filter_task_input, winsize_changed, FilteredTaskInput,
        TaskEntryPlanOutcome, TaskInputAction, TaskSessionContext, TaskSessionEnd,
        TaskSessionRunner,
    };
    use ajax_core::{
        adapters::{CommandMode, CommandSpec, RecordingCommandRunner},
        commands::CommandPlan,
    };
    use nix::poll::PollFlags;
    use nix::sys::termios::{
        InputFlags, LocalFlags, OutputFlags, SpecialCharacterIndices, Termios,
    };
    use std::time::Duration;

    #[derive(Default)]
    struct RecordingTaskSessionRunner {
        commands: Vec<CommandSpec>,
    }

    impl TaskSessionRunner for RecordingTaskSessionRunner {
        fn run_task_session(
            &mut self,
            command: &CommandSpec,
            _context: &TaskSessionContext,
        ) -> Result<TaskSessionEnd, crate::CliError> {
            self.commands.push(command.clone());
            Ok(TaskSessionEnd::Normal)
        }
    }

    struct FailingTaskSessionRunner;

    impl TaskSessionRunner for FailingTaskSessionRunner {
        fn run_task_session(
            &mut self,
            _command: &CommandSpec,
            _context: &TaskSessionContext,
        ) -> Result<TaskSessionEnd, crate::CliError> {
            Err(crate::CliError::CommandFailed(
                "task session unavailable".to_string(),
            ))
        }
    }

    fn sample_termios() -> Termios {
        // SAFETY: The test fills the fields that the wrapper mirrors before
        // converting into nix's safe Termios wrapper.
        let mut raw: nix::libc::termios = unsafe { std::mem::zeroed() };
        raw.c_iflag =
            (InputFlags::IXON | InputFlags::IXOFF | InputFlags::IXANY | InputFlags::ICRNL).bits();
        raw.c_oflag = OutputFlags::OPOST.bits();
        raw.c_lflag = (LocalFlags::ICANON | LocalFlags::ECHO).bits();
        Termios::from(raw)
    }

    #[test]
    fn task_input_filter_returns_to_cockpit_on_control_q_without_forwarding_it() {
        assert_eq!(
            filter_task_input(b"abc\x11def"),
            FilteredTaskInput {
                action: TaskInputAction::ReturnToCockpit,
                bytes: b"abc".to_vec(),
            }
        );
    }

    #[test]
    fn task_input_filter_opens_new_task_on_control_t_without_forwarding_it() {
        assert_eq!(
            filter_task_input(b"abc\x14def"),
            FilteredTaskInput {
                action: TaskInputAction::OpenNewTask,
                bytes: b"abc".to_vec(),
            }
        );
    }

    #[test]
    fn repo_from_task_handle_extracts_repo_prefix() {
        assert_eq!(
            super::repo_from_task_handle("web/fix-login").as_deref(),
            Some("web")
        );
        assert_eq!(super::repo_from_task_handle("api").as_deref(), Some("api"));
    }

    #[test]
    fn task_input_filter_keeps_normal_tmux_keys_inside_task_session() {
        assert_eq!(
            filter_task_input(b"\x02?"),
            FilteredTaskInput {
                action: TaskInputAction::Forward,
                bytes: b"\x02?".to_vec(),
            }
        );
    }

    #[test]
    fn task_input_filter_removes_control_s_without_stopping_task_mode() {
        assert_eq!(
            filter_task_input(b"a\x13b"),
            FilteredTaskInput {
                action: TaskInputAction::Forward,
                bytes: b"ab".to_vec(),
            }
        );
    }

    #[test]
    fn task_input_filter_swallows_startup_terminal_probe_bytes() {
        assert_eq!(
            super::filter_task_input_after_startup_grace_period(
                b"\x1b[?62c",
                super::STARTUP_INPUT_SUPPRESSION / 2,
            ),
            FilteredTaskInput {
                action: TaskInputAction::Forward,
                bytes: Vec::new(),
            }
        );
        assert_eq!(
            super::filter_task_input_after_startup_grace_period(
                b"a",
                super::STARTUP_INPUT_SUPPRESSION / 2,
            ),
            FilteredTaskInput {
                action: TaskInputAction::Forward,
                bytes: b"a".to_vec(),
            }
        );
    }

    #[test]
    fn task_input_filter_swallows_terminal_owned_mouse_reports_without_losing_text() {
        assert_eq!(
            filter_task_input(b"a\x1b[<0;10;5Mb"),
            FilteredTaskInput {
                action: TaskInputAction::Forward,
                bytes: b"ab".to_vec(),
            }
        );
        assert_eq!(
            filter_task_input(b"\x1b[I\x1b[O"),
            FilteredTaskInput {
                action: TaskInputAction::Forward,
                bytes: Vec::new(),
            }
        );
    }

    #[test]
    fn task_input_filter_forwards_sgr_scroll_reports() {
        assert_eq!(
            filter_task_input(b"a\x1b[<64;10;5Mb\x1b[<65;10;5Mc"),
            FilteredTaskInput {
                action: TaskInputAction::Forward,
                bytes: b"a\x1b[<64;10;5Mb\x1b[<65;10;5Mc".to_vec(),
            }
        );
    }

    #[test]
    fn task_input_filter_forwards_x10_scroll_reports() {
        assert_eq!(
            filter_task_input(b"a\x1b[M`!!b\x1b[Ma!!c"),
            FilteredTaskInput {
                action: TaskInputAction::Forward,
                bytes: b"a\x1b[M`!!b\x1b[Ma!!c".to_vec(),
            }
        );
    }

    #[test]
    fn terminal_owned_sequence_parser_names_filtered_sequences() {
        assert_eq!(
            super::terminal_owned_sequence(b"\x1b[I"),
            Some(super::TerminalOwnedSequence::FocusReport { len: 3 })
        );
        assert_eq!(
            super::terminal_owned_sequence(b"\x1b[?62c"),
            Some(super::TerminalOwnedSequence::CsiReport { len: 6 })
        );
        assert_eq!(
            super::terminal_owned_sequence(b"\x1b[<0;10;15M"),
            Some(super::TerminalOwnedSequence::SgrMouse { len: 11 })
        );
        assert_eq!(
            super::terminal_owned_sequence(b"\x1b[M !!"),
            Some(super::TerminalOwnedSequence::X10Mouse { len: 6 })
        );
        assert_eq!(super::terminal_owned_sequence(b"\x1b[M`!!"), None);
        assert_eq!(super::terminal_owned_sequence(b"\x1b[A"), None);
    }

    #[test]
    fn task_child_shutdown_policy_escalates_when_attach_client_lingers() {
        assert_eq!(
            super::task_child_shutdown_action(
                super::TERM_ATTACH_AFTER - Duration::from_millis(1),
                false,
                false,
            ),
            super::TaskChildShutdownAction::Wait
        );
        assert_eq!(
            super::task_child_shutdown_action(super::TERM_ATTACH_AFTER, false, false),
            super::TaskChildShutdownAction::Terminate
        );
        assert_eq!(
            super::task_child_shutdown_action(super::KILL_ATTACH_AFTER, true, false),
            super::TaskChildShutdownAction::Kill
        );
        assert_eq!(
            super::task_child_shutdown_action(super::KILL_ATTACH_AFTER, true, true),
            super::TaskChildShutdownAction::Wait
        );
    }

    #[test]
    fn task_poll_classification_does_not_continue_on_invalid_or_error_only_events() {
        assert_eq!(
            super::classify_task_poll_events(PollFlags::POLLNVAL, PollFlags::empty()),
            super::TaskPollAction::Detach
        );
        assert_eq!(
            super::classify_task_poll_events(PollFlags::empty(), PollFlags::POLLNVAL),
            super::TaskPollAction::Close
        );
        assert_eq!(
            super::classify_task_poll_events(PollFlags::empty(), PollFlags::POLLERR),
            super::TaskPollAction::Close
        );
    }

    #[test]
    fn task_poll_classification_keeps_task_open_on_terminal_side_app_switch_hangup() {
        assert_eq!(
            super::classify_task_poll_events(PollFlags::POLLHUP, PollFlags::empty()),
            super::TaskPollAction::Pump {
                tty_ready: false,
                master_ready: false,
            }
        );
        assert_eq!(
            super::classify_task_poll_events(PollFlags::POLLERR, PollFlags::POLLIN),
            super::TaskPollAction::Pump {
                tty_ready: false,
                master_ready: true,
            }
        );
    }

    #[test]
    fn interrupted_task_pty_poll_is_retried_in_same_attach_loop() {
        assert_eq!(
            super::classify_task_poll_error(nix::errno::Errno::EINTR),
            super::TaskPollErrorAction::Retry
        );
        assert_eq!(
            super::classify_task_poll_error(nix::errno::Errno::EBADF),
            super::TaskPollErrorAction::Fatal
        );
    }

    #[test]
    fn interrupted_task_pty_poll_attempt_continues_without_detach_or_fatal_error() {
        assert_eq!(
            super::classify_task_poll_attempt(
                Err(nix::errno::Errno::EINTR),
                PollFlags::empty(),
                PollFlags::empty(),
            ),
            super::TaskPollAttempt::Retry
        );
        assert_eq!(
            super::classify_task_poll_attempt(
                Err(nix::errno::Errno::EBADF),
                PollFlags::empty(),
                PollFlags::empty(),
            ),
            super::TaskPollAttempt::Fatal(nix::errno::Errno::EBADF)
        );
        assert_eq!(
            super::classify_task_poll_attempt(Ok(1), PollFlags::empty(), PollFlags::POLLIN,),
            super::TaskPollAttempt::Ready(super::TaskPollAction::Pump {
                tty_ready: false,
                master_ready: true,
            })
        );
    }

    #[test]
    fn task_session_trace_line_is_compact_and_single_line() {
        assert_eq!(
            super::format_task_session_trace_line(
                Duration::from_millis(42),
                "poll err",
                "error=EINTR\nnext=line",
            ),
            "elapsed_ms=42 event=poll_err error=EINTR\\nnext=line\n"
        );
    }

    #[test]
    fn task_session_trace_is_disabled_without_path() {
        let trace = super::TaskSessionTrace::from_path(None).unwrap();

        assert!(!trace.is_enabled());
        assert!(super::trace_path_from_env(None).is_none());
        assert!(super::trace_path_from_env(Some(std::ffi::OsString::new())).is_none());
    }

    #[test]
    fn interrupted_attach_client_exit_is_retryable() {
        let exit = super::TaskAttachExit {
            output: b"tmux: EINTR service interrupted call\n".to_vec(),
            status: Some(nix::sys::wait::WaitStatus::Exited(
                nix::unistd::Pid::from_raw(42),
                1,
            )),
            attached_for: Duration::from_millis(50),
        };

        assert!(super::attach_exit_allows_retry(&exit));
    }

    #[test]
    fn clean_attach_client_exit_is_not_retryable() {
        let exit = super::TaskAttachExit {
            output: Vec::new(),
            status: Some(nix::sys::wait::WaitStatus::Exited(
                nix::unistd::Pid::from_raw(42),
                0,
            )),
            attached_for: Duration::from_millis(50),
        };

        assert!(!super::attach_exit_allows_retry(&exit));
    }

    #[test]
    fn task_session_bridge_has_no_debug_log_environment_hook() {
        let source = include_str!("task_session.rs");
        let debug_env = ["AJAX", "_TASK_SESSION_LOG"].concat();
        let logger_type = ["Task", "Session", "Logger"].concat();

        assert!(!source.contains(&debug_env));
        assert!(!source.contains(&logger_type));
    }

    #[test]
    fn task_operator_terminal_uses_inherited_stdio_instead_of_reopening_dev_tty() {
        let source = include_str!("task_session.rs");
        let terminal_source_type = ["TaskOperator", "TerminalSource"].concat();
        let terminal_source_fn = ["task_operator", "_terminal_source"].concat();
        let session_outcome_type = ["TaskSession", "Outcome"].concat();
        assert!(!source.contains(&terminal_source_type));
        assert!(!source.contains(&terminal_source_fn));
        assert!(!source.contains(&session_outcome_type));
    }

    #[test]
    fn task_screen_commands_clear_normal_buffer_without_disabling_scrollback() {
        assert_eq!(
            super::TASK_SCREEN_ENTRY_SEQUENCE,
            b"\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l\x1b[2J\x1b[H"
        );
        assert_eq!(super::TASK_SCREEN_EXIT_SEQUENCE, b"\x1b[?25h");
    }

    #[test]
    fn task_session_does_not_keep_screen_sequence_wrappers() {
        let source = include_str!("task_session.rs");

        for helper in ["task_screen_entry_sequence", "task_screen_exit_sequence"] {
            let function_name = ["fn ", helper].concat();
            assert!(!source.contains(&function_name), "{helper}");
        }
    }

    #[test]
    fn task_pty_winsize_uses_operator_rows_and_columns() {
        let winsize = super::task_pty_winsize(37, 79);

        assert_eq!(winsize.ws_row, 37);
        assert_eq!(winsize.ws_col, 79);
        assert_eq!(winsize.ws_xpixel, 0);
        assert_eq!(winsize.ws_ypixel, 0);
    }

    #[test]
    fn winsize_change_detection_tracks_rows_and_columns() {
        // First observation always counts as a change.
        assert!(winsize_changed(None, (24, 80)));
        // Identical size is a no-op so we never spam SIGWINCH at the child.
        assert!(!winsize_changed(Some((24, 80)), (24, 80)));
        // A change in either dimension propagates.
        assert!(winsize_changed(Some((24, 80)), (30, 80)));
        assert!(winsize_changed(Some((24, 80)), (24, 120)));
    }

    fn set_kernel_winsize(fd: i32, rows: u16, cols: u16) {
        let ws = nix::libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: TIOCSWINSZ reads the winsize struct for a valid pty fd.
        let result = unsafe { nix::libc::ioctl(fd, nix::libc::TIOCSWINSZ, &ws) };
        assert_eq!(
            result,
            0,
            "TIOCSWINSZ failed: {}",
            std::io::Error::last_os_error()
        );
    }

    fn kernel_winsize(fd: i32) -> (u16, u16) {
        // SAFETY: TIOCGWINSZ writes the winsize struct for a valid pty fd.
        let mut ws: nix::libc::winsize = unsafe { std::mem::zeroed() };
        let result = unsafe { nix::libc::ioctl(fd, nix::libc::TIOCGWINSZ, &mut ws) };
        assert_eq!(
            result,
            0,
            "TIOCGWINSZ failed: {}",
            std::io::Error::last_os_error()
        );
        (ws.ws_row, ws.ws_col)
    }

    // End-to-end proof against real kernel PTY state: a live SIGWINCH must move
    // the child PTY's window size, and the old (no-sync) path must leave it stale.
    #[test]
    fn live_sigwinch_propagates_operator_size_to_child_pty() {
        use nix::pty::openpty;
        use nix::sys::signal::{raise, Signal};
        use std::os::fd::AsRawFd;

        // "operator" = the terminal the operator looks at (production reads its
        // size from stdin). "child" = the PTY the attached tmux client renders to.
        let operator = openpty(None, None).expect("openpty operator");
        let child = openpty(None, None).expect("openpty child");
        let operator_read_fd = operator.slave.as_raw_fd();
        let child_master_fd = child.master.as_raw_fd();

        // Both start at 24x80, matching a fresh attach.
        set_kernel_winsize(operator.master.as_raw_fd(), 24, 80);
        set_kernel_winsize(child_master_fd, 24, 80);

        let mut trace = super::TaskSessionTrace::from_path(None).unwrap();
        let mut last: Option<(u16, u16)> = None;

        // Install the real handler used in production (seeds a pending sync).
        let _guard = super::TaskWinchGuard::install().unwrap();

        // First pump iteration syncs the current size on attach.
        super::sync_pending_winsize(operator_read_fd, child_master_fd, &mut last, &mut trace);
        assert_eq!(kernel_winsize(child_master_fd), (24, 80));
        assert_eq!(last, Some((24, 80)));
        println!(
            "[attach]            child PTY size = {:?}",
            kernel_winsize(child_master_fd)
        );

        // The operator terminal is resized (e.g. mobile keyboard hides).
        set_kernel_winsize(operator.master.as_raw_fd(), 40, 100);
        println!(
            "[operator resized]  operator size = {:?}, child PTY size = {:?}",
            kernel_winsize(operator_read_fd),
            kernel_winsize(child_master_fd)
        );

        // OLD BEHAVIOR: with no SIGWINCH propagation, the child stays stale —
        // this is exactly the flicker/scroll-jump bug.
        assert_eq!(
            kernel_winsize(child_master_fd),
            (24, 80),
            "child should still be stale until the resize is propagated"
        );

        // A real SIGWINCH is delivered to this thread, running the production
        // handler, which flags a pending sync.
        raise(Signal::SIGWINCH).expect("raise SIGWINCH");

        // NEW BEHAVIOR: the next pump iteration pushes the live size to the child.
        super::sync_pending_winsize(operator_read_fd, child_master_fd, &mut last, &mut trace);

        assert_eq!(
            kernel_winsize(child_master_fd),
            (40, 100),
            "child PTY must reflect the resized operator terminal"
        );
        assert_eq!(last, Some((40, 100)));
        println!(
            "[after SIGWINCH]    child PTY size = {:?}  <- propagated",
            kernel_winsize(child_master_fd)
        );
    }

    #[test]
    fn task_pty_fork_config_uses_operator_size_and_terminal_modes() {
        let config = super::task_pty_fork_config(&sample_termios(), 37, 79);

        assert_eq!(config.winsize.ws_row, 37);
        assert_eq!(config.winsize.ws_col, 79);
        assert!(config
            .child_termios
            .local_flags
            .contains(LocalFlags::ICANON));
        assert!(config.child_termios.input_flags.contains(InputFlags::ICRNL));
        assert!(!config.raw_termios.input_flags.contains(InputFlags::IXON));
    }

    #[test]
    fn task_detach_sequence_closes_attach_pty_before_waiting() {
        assert_eq!(
            super::task_detach_sequence(),
            &[
                super::TaskDetachStep::CloseAttachPty,
                super::TaskDetachStep::SignalAttachChild,
                super::TaskDetachStep::WaitForAttachChild,
            ]
        );
    }

    #[test]
    fn task_entry_plan_runs_setup_then_task_session_without_global_tmux_binding() {
        let mut plan = CommandPlan::new("open task: web/fix-login");
        plan.commands.push(CommandSpec::new(
            "tmux",
            ["select-window", "-t", "ajax-web-fix-login:worktrunk"],
        ));
        plan.commands.push(
            CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio),
        );
        let mut runner = RecordingCommandRunner::default();
        let mut task_session = RecordingTaskSessionRunner::default();

        let context = TaskSessionContext::from_task_handle("web/fix-login");
        let outcome =
            execute_task_entry_plan(&plan, &mut runner, &mut task_session, &context).unwrap();
        assert!(matches!(outcome, TaskEntryPlanOutcome::Completed(_)));

        assert_eq!(
            runner.commands(),
            &[CommandSpec::new(
                "tmux",
                ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
            )]
        );
        assert_eq!(
            task_session.commands,
            vec![
                CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                    .with_mode(CommandMode::InheritStdio)
            ]
        );
    }

    #[test]
    fn task_entry_plan_surfaces_task_session_failure_after_setup() {
        let mut plan = CommandPlan::new("open task: web/fix-login");
        plan.commands.push(CommandSpec::new(
            "tmux",
            ["select-window", "-t", "ajax-web-fix-login:worktrunk"],
        ));
        plan.commands.push(
            CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio),
        );
        let mut runner = RecordingCommandRunner::default();
        let mut task_session = FailingTaskSessionRunner;

        let context = TaskSessionContext::from_task_handle("web/fix-login");
        let error =
            execute_task_entry_plan(&plan, &mut runner, &mut task_session, &context).unwrap_err();

        assert!(matches!(
            error,
            crate::CliError::CommandFailed(message) if message == "task session unavailable"
        ));
        assert_eq!(
            runner.commands(),
            &[CommandSpec::new(
                "tmux",
                ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
            )]
        );
    }

    #[test]
    fn ajax_raw_termios_disables_software_flow_control_and_reads_single_bytes() {
        let termios = super::ajax_raw_termios(&sample_termios());

        assert!(!termios.input_flags.contains(InputFlags::IXON));
        assert!(!termios.input_flags.contains(InputFlags::IXOFF));
        assert!(!termios.input_flags.contains(InputFlags::IXANY));
        assert_eq!(
            termios.control_chars[SpecialCharacterIndices::VMIN as usize],
            1
        );
        assert_eq!(
            termios.control_chars[SpecialCharacterIndices::VTIME as usize],
            0
        );
    }

    #[test]
    fn child_task_termios_keeps_canonical_input_and_cr_to_newline_translation() {
        let ajax_raw = super::ajax_raw_termios(&sample_termios());
        let child = super::child_task_termios(&ajax_raw);

        assert!(child.local_flags.contains(LocalFlags::ICANON));
        assert!(child.local_flags.contains(LocalFlags::ECHO));
        assert!(child.input_flags.contains(InputFlags::ICRNL));
    }

    #[test]
    fn prepared_task_command_builds_exec_argv_before_fork() {
        let command = CommandSpec::new("tmux", ["attach-session", "-t", "a"]);

        let prepared = super::PreparedTaskCommand::new(&command).unwrap();

        assert_eq!(prepared.executable.to_str().unwrap(), "tmux");
        assert_eq!(
            prepared
                .args
                .iter()
                .map(|arg| arg.to_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["tmux", "attach-session", "-t", "a"]
        );
        assert_eq!(prepared.argv.len(), 5);
        assert!(prepared.argv.last().unwrap().is_null());
    }

    #[test]
    fn prepared_task_command_builds_cwd_and_tmux_env_name_before_fork() {
        let command = CommandSpec::new("sh", ["-lc", "pwd"]).with_cwd("/tmp/ajax task");

        let prepared = super::PreparedTaskCommand::new(&command).unwrap();

        assert_eq!(
            prepared.cwd.as_ref().unwrap().to_str().unwrap(),
            "/tmp/ajax task"
        );
        assert_eq!(prepared.tmux_env_name.to_str().unwrap(), "TMUX");
    }
}

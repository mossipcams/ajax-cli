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
        signal::{kill, Signal},
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
    thread::sleep,
    time::{Duration, Instant},
};

use crate::{command_error, CliError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TaskInputAction {
    Forward,
    ReturnToCockpit,
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
const TASK_SESSION_LOG_ENV: &str = "AJAX_TASK_SESSION_LOG";
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
enum TaskOperatorTerminalSource {
    InheritedStdio,
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

struct TaskSessionLogger {
    started: Instant,
    file: Option<File>,
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
    fn run_task_session(&mut self, command: &CommandSpec) -> Result<(), CliError>;
}

pub(crate) fn execute_task_entry_plan<R: CommandRunner, S: TaskSessionRunner>(
    plan: &commands::CommandPlan,
    runner: &mut R,
    task_session: &mut S,
) -> Result<Vec<CommandOutput>, CliError> {
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
    task_session.run_task_session(&task_command)?;
    Ok(outputs)
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
        return sgr_mouse_sequence_len(input).map(|len| TerminalOwnedSequence::SgrMouse { len });
    }
    if input.starts_with(b"\x1b[M") && input.len() >= 6 {
        return Some(TerminalOwnedSequence::X10Mouse { len: 6 });
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

fn sgr_mouse_sequence_len(input: &[u8]) -> Option<usize> {
    for (offset, byte) in input.iter().enumerate().skip(3) {
        if byte.is_ascii_digit() || *byte == b';' {
            continue;
        }
        return (*byte == b'M' || *byte == b'm').then_some(offset + 1);
    }
    None
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
    if tty_flags.intersects(PollFlags::POLLERR | PollFlags::POLLHUP) {
        return TaskPollAction::Detach;
    }
    if master_flags.contains(PollFlags::POLLNVAL) {
        return TaskPollAction::Close;
    }
    if master_flags.intersects(PollFlags::POLLERR | PollFlags::POLLHUP) {
        return TaskPollAction::Close;
    }

    TaskPollAction::Pump {
        tty_ready: tty_flags.contains(PollFlags::POLLIN),
        master_ready: master_flags.contains(PollFlags::POLLIN),
    }
}

#[cfg(test)]
fn task_detach_sequence() -> &'static [TaskDetachStep] {
    &[
        TaskDetachStep::CloseAttachPty,
        TaskDetachStep::SignalAttachChild,
        TaskDetachStep::WaitForAttachChild,
    ]
}

fn format_task_session_log_line(elapsed: Duration, event: &str, detail: &str) -> String {
    format!(
        "elapsed_ms={} event={} {}\n",
        elapsed.as_millis(),
        event,
        detail
    )
}

fn task_screen_entry_sequence() -> &'static [u8] {
    TASK_SCREEN_ENTRY_SEQUENCE
}

fn task_screen_exit_sequence() -> &'static [u8] {
    TASK_SCREEN_EXIT_SEQUENCE
}

impl TaskSessionLogger {
    fn from_env() -> Result<Self, CliError> {
        let file = match env::var_os(TASK_SESSION_LOG_ENV) {
            Some(path) if !path.is_empty() => Some(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .map_err(|error| {
                        CliError::CommandFailed(format!(
                            "failed to open task session log {}: {error}",
                            std::path::Path::new(&path).display()
                        ))
                    })?,
            ),
            _ => None,
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
        let line = format_task_session_log_line(self.started.elapsed(), event, detail.as_ref());
        let _ = file.write_all(line.as_bytes());
        let _ = file.flush();
    }
}

#[derive(Default)]
pub(crate) struct PtyTaskSessionRunner;

impl TaskSessionRunner for PtyTaskSessionRunner {
    fn run_task_session(&mut self, command: &CommandSpec) -> Result<(), CliError> {
        run_pty_task_session(command)
    }
}

fn run_pty_task_session(command: &CommandSpec) -> Result<(), CliError> {
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
            let mut terminal = TaskOperatorTerminal::open()?;
            let _guard = terminal.enter_raw_mode(original_termios, &fork_config.raw_termios)?;
            let mut logger = TaskSessionLogger::from_env()?;
            logger.log(
                "attach_start",
                format_task_attach_start_detail(child.as_raw(), &fork_config.winsize, command),
            );
            let _screen_guard = TaskScreenGuard::enter(&mut terminal.output)?;
            let result = pump_task_pty(
                &mut terminal.input,
                &mut terminal.output,
                master,
                child,
                &mut logger,
            );
            let _ = waitpid(child, Some(WaitPidFlag::WNOHANG));
            result
        }
    }
}

fn format_task_attach_start_detail(child: i32, winsize: &Winsize, command: &CommandSpec) -> String {
    format!(
        "child={} winsize={}x{} command={}",
        child,
        winsize.ws_col,
        winsize.ws_row,
        command_for_log(command)
    )
}

fn task_operator_terminal_source() -> TaskOperatorTerminalSource {
    TaskOperatorTerminalSource::InheritedStdio
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
        match task_operator_terminal_source() {
            TaskOperatorTerminalSource::InheritedStdio => {
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
        }
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

struct TaskScreenGuard {
    output: File,
}

impl TaskScreenGuard {
    fn enter(output: &mut File) -> Result<Self, CliError> {
        output
            .write_all(task_screen_entry_sequence())
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
        let _ = self.output.write_all(task_screen_exit_sequence());
        let _ = self.output.flush();
    }
}

fn duplicate_task_terminal_fd(fd: i32, context: &'static str) -> Result<File, CliError> {
    let duplicate = dup(fd).map_err(tty_error(context))?;
    // SAFETY: dup returns a fresh owned file descriptor. File takes ownership
    // and closes it when dropped.
    Ok(unsafe { File::from_raw_fd(duplicate) })
}

fn command_for_log(command: &CommandSpec) -> String {
    std::iter::once(command.program.as_str())
        .chain(command.args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ")
}

fn pump_task_pty(
    terminal_input: &mut File,
    terminal_output: &mut File,
    master: OwnedFd,
    child: nix::unistd::Pid,
    logger: &mut TaskSessionLogger,
) -> Result<(), CliError> {
    let mut master = File::from(master);
    let mut tty_input = [0_u8; 4096];
    let mut pty_output = [0_u8; 8192];
    let attached_at = Instant::now();

    loop {
        let poll_action = {
            let mut poll_fds = [
                PollFd::new(terminal_input.as_fd(), PollFlags::POLLIN),
                PollFd::new(
                    master.as_fd(),
                    PollFlags::POLLIN | PollFlags::POLLHUP | PollFlags::POLLERR,
                ),
            ];
            poll(&mut poll_fds, PollTimeout::NONE).map_err(tty_error("failed to poll task PTY"))?;
            let tty_flags = poll_fds[0].revents().unwrap_or_else(PollFlags::empty);
            let master_flags = poll_fds[1].revents().unwrap_or_else(PollFlags::empty);
            let action = classify_task_poll_events(tty_flags, master_flags);
            logger.log(
                "poll",
                format!("tty={tty_flags:?} master={master_flags:?} action={action:?}"),
            );
            action
        };

        let (tty_ready, master_ready) = match poll_action {
            TaskPollAction::Pump {
                tty_ready,
                master_ready,
            } => (tty_ready, master_ready),
            TaskPollAction::Detach => {
                logger.log("detach", "reason=tty_poll");
                return detach_task_child(master, child, logger);
            }
            TaskPollAction::Close => {
                logger.log("close", "reason=master_poll");
                return Ok(());
            }
        };

        if tty_ready {
            let count = terminal_input
                .read(&mut tty_input)
                .map_err(io_error("failed to read task terminal input"))?;
            logger.log("tty_read", format!("bytes={count}"));
            if count == 0 {
                logger.log("detach", "reason=tty_eof");
                return detach_task_child(master, child, logger);
            }
            let filtered = filter_task_input_after_startup_grace_period(
                &tty_input[..count],
                attached_at.elapsed(),
            );
            logger.log(
                "tty_filter",
                format!(
                    "action={:?} in_bytes={} out_bytes={}",
                    filtered.action,
                    count,
                    filtered.bytes.len()
                ),
            );
            if !filtered.bytes.is_empty() {
                master
                    .write_all(&filtered.bytes)
                    .map_err(io_error("failed to write task PTY"))?;
                logger.log("pty_write", format!("bytes={}", filtered.bytes.len()));
            }
            if filtered.action == TaskInputAction::ReturnToCockpit {
                logger.log("detach", "reason=ctrl_q");
                return detach_task_child(master, child, logger);
            }
        }

        if master_ready {
            match master.read(&mut pty_output) {
                Ok(0) => {
                    logger.log("pty_read", "bytes=0");
                    return Ok(());
                }
                Ok(count) => {
                    logger.log("pty_read", format!("bytes={count}"));
                    terminal_output
                        .write_all(&pty_output[..count])
                        .map_err(io_error("failed to write task terminal output"))?;
                    terminal_output
                        .flush()
                        .map_err(io_error("failed to flush task terminal output"))?;
                    logger.log("tty_write", format!("bytes={count}"));
                }
                Err(error) if pty_was_closed(&error) => {
                    logger.log("pty_read_closed", format!("error={error}"));
                    return Ok(());
                }
                Err(error) => {
                    logger.log("pty_read_error", format!("error={error}"));
                    return Err(CliError::CommandFailed(format!(
                        "failed to read task PTY: {error}"
                    )));
                }
            }
        }
    }
}

fn detach_task_child(
    master: File,
    child: nix::unistd::Pid,
    logger: &mut TaskSessionLogger,
) -> Result<(), CliError> {
    logger.log("detach_close_pty", format!("child={}", child.as_raw()));
    drop(master);
    request_task_child_exit(child, logger)
}

fn request_task_child_exit(
    child: nix::unistd::Pid,
    logger: &mut TaskSessionLogger,
) -> Result<(), CliError> {
    logger.log(
        "detach_signal",
        format!("child={} signal=SIGHUP", child.as_raw()),
    );
    let _ = kill(child, Signal::SIGHUP);
    wait_for_task_child_exit(child, logger)
}

fn wait_for_task_child_exit(
    child: nix::unistd::Pid,
    logger: &mut TaskSessionLogger,
) -> Result<(), CliError> {
    let started = Instant::now();
    let mut sent_terminate = false;
    let mut sent_kill = false;

    loop {
        match waitpid(child, Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::Exited(_, status)) => {
                logger.log(
                    "detach_child_exit",
                    format!("child={} status={status}", child.as_raw()),
                );
                return Ok(());
            }
            Ok(WaitStatus::Signaled(_, signal, _)) => {
                logger.log(
                    "detach_child_signal",
                    format!("child={} signal={signal:?}", child.as_raw()),
                );
                return Ok(());
            }
            Ok(WaitStatus::StillAlive) => {}
            Ok(status) => {
                logger.log("detach_child_wait", format!("status={status:?}"));
            }
            Err(nix::errno::Errno::ECHILD) => {
                logger.log("detach_child_missing", format!("child={}", child.as_raw()));
                return Ok(());
            }
            Err(error) => {
                logger.log("detach_child_wait_error", format!("error={error}"));
                return Err(CliError::CommandFailed(format!(
                    "failed to wait for task attach client: {error}"
                )));
            }
        }

        let elapsed = started.elapsed();
        if elapsed >= GIVE_UP_ATTACH_AFTER {
            logger.log(
                "detach_child_timeout",
                format!(
                    "child={} elapsed_ms={}",
                    child.as_raw(),
                    elapsed.as_millis()
                ),
            );
            return Err(CliError::CommandFailed(
                "task attach client did not exit after detach".to_string(),
            ));
        }

        match task_child_shutdown_action(elapsed, sent_terminate, sent_kill) {
            TaskChildShutdownAction::Wait => {}
            TaskChildShutdownAction::Terminate => {
                logger.log(
                    "detach_signal",
                    format!("child={} signal=SIGTERM", child.as_raw()),
                );
                let _ = kill(child, Signal::SIGTERM);
                sent_terminate = true;
            }
            TaskChildShutdownAction::Kill => {
                logger.log(
                    "detach_signal",
                    format!("child={} signal=SIGKILL", child.as_raw()),
                );
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
        execute_task_entry_plan, filter_task_input, FilteredTaskInput, TaskInputAction,
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
        fn run_task_session(&mut self, command: &CommandSpec) -> Result<(), crate::CliError> {
            self.commands.push(command.clone());
            Ok(())
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
            super::terminal_owned_sequence(b"\x1b[Mabc"),
            Some(super::TerminalOwnedSequence::X10Mouse { len: 6 })
        );
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
    fn task_session_log_line_includes_elapsed_event_and_detail() {
        assert_eq!(
            super::format_task_session_log_line(
                Duration::from_millis(42),
                "poll",
                "tty=POLLIN master=POLLIN action=pump",
            ),
            "elapsed_ms=42 event=poll tty=POLLIN master=POLLIN action=pump\n"
        );
    }

    #[test]
    fn task_attach_start_detail_includes_child_size_and_command() {
        let command = CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"]);

        assert_eq!(
            super::format_task_attach_start_detail(123, &super::task_pty_winsize(37, 79), &command),
            "child=123 winsize=79x37 command=tmux attach-session -t ajax-web-fix-login"
        );
    }

    #[test]
    fn task_operator_terminal_uses_inherited_stdio_instead_of_reopening_dev_tty() {
        assert_eq!(
            super::task_operator_terminal_source(),
            super::TaskOperatorTerminalSource::InheritedStdio
        );
    }

    #[test]
    fn task_screen_commands_clear_normal_buffer_without_disabling_scrollback() {
        assert_eq!(
            super::task_screen_entry_sequence(),
            b"\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l\x1b[2J\x1b[H"
        );
        assert_eq!(super::task_screen_exit_sequence(), b"\x1b[?25h");
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

        execute_task_entry_plan(&plan, &mut runner, &mut task_session).unwrap();

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

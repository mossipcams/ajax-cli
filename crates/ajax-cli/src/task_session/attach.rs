use super::*;
use crate::CliError;
use ajax_core::adapters::CommandSpec;
use nix::sys::termios::{
    cfmakeraw, InputFlags, LocalFlags, OutputFlags, SpecialCharacterIndices, Termios,
};
use nix::unistd::dup;
use nix::{
    poll::{poll, PollFd, PollFlags, PollTimeout},
    pty::{forkpty, ForkptyResult, Winsize},
    sys::{
        signal::{kill, sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal},
        termios::{tcsetattr, SetArg},
        wait::{waitpid, WaitPidFlag, WaitStatus},
    },
};
use std::{
    ffi::CString,
    fs::File,
    io::{self, Read, Write},
    os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd},
    os::raw::c_char,
    os::unix::ffi::OsStrExt,
    sync::atomic::{AtomicBool, Ordering},
    thread::sleep,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub(super) enum PtyAttachResult {
    Detached { open_new_task: bool },
    ClientExit(TaskAttachExit),
}

pub(super) fn run_pty_task_attach(
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
            if prepared.clear_tmux_env {
                // SAFETY: The env name is a pre-fork CString with a stable nul-terminated pointer.
                unsafe { nix::libc::unsetenv(prepared.tmux_env_name.as_ptr()) };
            }
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

pub(super) fn task_pty_fork_config(
    original_termios: &Termios,
    rows: u16,
    columns: u16,
) -> TaskPtyForkConfig {
    TaskPtyForkConfig {
        child_termios: child_task_termios(original_termios),
        raw_termios: ajax_raw_termios(original_termios),
        winsize: task_pty_winsize(rows, columns),
    }
}

pub(super) fn read_task_terminal_winsize(fd: i32) -> Result<Winsize, CliError> {
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

pub(super) fn task_pty_winsize(rows: u16, columns: u16) -> Winsize {
    Winsize {
        ws_row: rows,
        ws_col: columns,
        ws_xpixel: 0,
        ws_ypixel: 0,
    }
}

pub(super) struct TaskOperatorTerminal {
    pub(super) input: File,
    pub(super) output: File,
}

impl TaskOperatorTerminal {
    pub(super) fn open() -> Result<Self, CliError> {
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

    pub(super) fn enter_raw_mode(
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
pub(super) struct TaskWinchGuard {
    pub(super) previous: SigAction,
}

impl TaskWinchGuard {
    pub(super) fn install() -> Result<Self, CliError> {
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
pub(super) fn read_operator_winsize(fd: i32) -> Option<nix::libc::winsize> {
    // SAFETY: ioctl writes into the provided winsize struct for a tty fd.
    let mut raw: nix::libc::winsize = unsafe { std::mem::zeroed() };
    let result = unsafe { nix::libc::ioctl(fd, nix::libc::TIOCGWINSZ, &mut raw) };
    (result == 0).then_some(raw)
}

pub(super) fn winsize_changed(last: Option<(u16, u16)>, current: (u16, u16)) -> bool {
    last != Some(current)
}

/// Propagates a pending operator resize to the PTY master so the attached
/// client (tmux) re-renders at the live terminal size. No-op unless SIGWINCH
/// fired since the last call and the size actually changed.
pub(super) fn sync_pending_winsize(
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

pub(super) struct TaskScreenGuard {
    pub(super) output: File,
}

impl TaskScreenGuard {
    pub(super) fn enter(output: &mut File) -> Result<Self, CliError> {
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

pub(super) fn duplicate_task_terminal_fd(fd: i32, context: &'static str) -> Result<File, CliError> {
    let duplicate = dup(fd).map_err(tty_error(context))?;
    // SAFETY: dup returns a fresh owned file descriptor. File takes ownership
    // and closes it when dropped.
    Ok(unsafe { File::from_raw_fd(duplicate) })
}

pub(super) fn pump_task_pty(
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

pub(super) fn append_recent_output(output: &mut Vec<u8>, bytes: &[u8]) {
    output.extend_from_slice(bytes);
    if output.len() > ATTACH_OUTPUT_BUFFER_LIMIT {
        let excess = output.len() - ATTACH_OUTPUT_BUFFER_LIMIT;
        output.drain(..excess);
    }
}

pub(super) fn attach_client_exit(
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

pub(super) fn wait_for_attach_child_status(
    child: nix::unistd::Pid,
) -> Result<Option<WaitStatus>, CliError> {
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

pub(super) fn detach_task_child(
    master: File,
    child: nix::unistd::Pid,
    open_new_task: bool,
) -> Result<PtyAttachResult, CliError> {
    drop(master);
    request_task_child_exit(child)?;
    Ok(PtyAttachResult::Detached { open_new_task })
}

pub(super) fn request_task_child_exit(child: nix::unistd::Pid) -> Result<(), CliError> {
    let _ = kill(child, Signal::SIGHUP);
    wait_for_task_child_exit(child)
}

pub(super) fn wait_for_task_child_exit(child: nix::unistd::Pid) -> Result<(), CliError> {
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

pub(super) fn ajax_raw_termios(original: &Termios) -> Termios {
    let mut termios = original.clone();
    cfmakeraw(&mut termios);
    termios
        .input_flags
        .remove(InputFlags::IXON | InputFlags::IXOFF | InputFlags::IXANY);
    termios.control_chars[SpecialCharacterIndices::VMIN as usize] = 1;
    termios.control_chars[SpecialCharacterIndices::VTIME as usize] = 0;
    termios
}

pub(super) fn child_task_termios(original: &Termios) -> Termios {
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

pub(super) struct PreparedTaskCommand {
    pub(super) executable: CString,
    pub(super) args: Vec<CString>,
    pub(super) argv: Vec<*const c_char>,
    pub(super) cwd: Option<CString>,
    pub(super) tmux_env_name: CString,
    pub(super) clear_tmux_env: bool,
}

impl PreparedTaskCommand {
    pub(super) fn new(command: &CommandSpec) -> Result<Self, CliError> {
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

        let clear_tmux_env = command_needs_detached_tmux_environment(command);

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
            clear_tmux_env,
        })
    }
}

pub(super) fn command_needs_detached_tmux_environment(command: &CommandSpec) -> bool {
    command.program == "tmux"
        && command
            .args
            .first()
            .is_some_and(|arg| arg == "attach-session")
}

pub(super) struct TtyTermiosGuard {
    pub(super) tty: File,
    pub(super) original: Termios,
}

impl Drop for TtyTermiosGuard {
    fn drop(&mut self) {
        let _ = tcsetattr(&self.tty, SetArg::TCSANOW, &self.original);
    }
}

pub(super) fn tty_error(context: &'static str) -> impl FnOnce(nix::errno::Errno) -> CliError {
    move |error| CliError::CommandFailed(format!("{context}: {error}"))
}

pub(super) fn io_error(context: &'static str) -> impl FnOnce(io::Error) -> CliError {
    move |error| CliError::CommandFailed(format!("{context}: {error}"))
}

pub(super) fn pty_was_closed(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::UnexpectedEof || error.raw_os_error() == Some(nix::libc::EIO)
}

pub(super) fn exit_child_after_exec_failure() -> ! {
    // SAFETY: This is the child branch immediately after fork. Exiting through
    // libc avoids running parent process cleanup paths in the forked process.
    unsafe { nix::libc::_exit(127) }
}

use ajax_core::{
    adapters::{CommandMode, CommandOutput, CommandRunner, CommandSpec},
    commands,
};
use nix::{
    poll::{poll, PollFd, PollFlags, PollTimeout},
    pty::{forkpty, ForkptyResult},
    sys::{
        signal::{kill, Signal},
        termios::{
            cfmakeraw, tcgetattr, tcsetattr, InputFlags, LocalFlags, OutputFlags, SetArg,
            SpecialCharacterIndices, Termios,
        },
        wait::{waitpid, WaitPidFlag},
    },
    unistd::{chdir, execvp},
};
use std::{
    ffi::{CStr, CString},
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    os::fd::{AsFd, OwnedFd},
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
    for byte in input {
        match *byte {
            0x11 => {
                return FilteredTaskInput {
                    action: TaskInputAction::ReturnToCockpit,
                    bytes,
                };
            }
            0x13 => {}
            byte => bytes.push(byte),
        }
    }

    FilteredTaskInput {
        action: TaskInputAction::Forward,
        bytes,
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
    let executable = CString::new(command.program.as_str())
        .map_err(|_| CliError::CommandFailed("task command contains a nul byte".to_string()))?;
    let argv = command_argv(command, &executable)?;
    let tty = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map_err(|error| CliError::CommandFailed(format!("failed to open /dev/tty: {error}")))?;
    let original_termios = tcgetattr(&tty).map_err(tty_error("failed to read terminal mode"))?;
    let child_termios = child_task_termios(&original_termios);
    let raw_termios = ajax_raw_termios(&original_termios);
    let restore_tty = tty
        .try_clone()
        .map_err(|error| CliError::CommandFailed(format!("failed to clone /dev/tty: {error}")))?;
    let _guard = TtyTermiosGuard {
        tty: restore_tty,
        original: original_termios,
    };
    tcsetattr(&tty, SetArg::TCSANOW, &raw_termios)
        .map_err(tty_error("failed to set raw terminal mode"))?;

    let child_cwd = command.cwd.clone();
    // SAFETY: The parent only touches the returned master fd. In the child
    // branch, all fallible setup was prepared before fork, and the process
    // either execs the requested command or exits immediately.
    match unsafe { forkpty(None, Some(&child_termios)) }
        .map_err(tty_error("failed to fork task PTY"))?
    {
        ForkptyResult::Child => {
            if let Some(cwd) = child_cwd.as_deref() {
                if chdir(cwd).is_err() {
                    exit_child_after_exec_failure();
                }
            }
            let args = argv.iter().map(CString::as_c_str).collect::<Vec<&CStr>>();
            let _ = execvp(executable.as_c_str(), &args);
            exit_child_after_exec_failure();
        }
        ForkptyResult::Parent { child, master } => {
            let result = pump_task_pty(&tty, master, child);
            let _ = waitpid(child, Some(WaitPidFlag::WNOHANG));
            result
        }
    }
}

fn pump_task_pty(tty: &File, master: OwnedFd, child: nix::unistd::Pid) -> Result<(), CliError> {
    let mut master = File::from(master);
    let mut tty = tty;
    let mut tty_input = [0_u8; 4096];
    let mut pty_output = [0_u8; 8192];

    loop {
        let (tty_ready, master_ready, master_closed) = {
            let mut poll_fds = [
                PollFd::new(tty.as_fd(), PollFlags::POLLIN),
                PollFd::new(
                    master.as_fd(),
                    PollFlags::POLLIN | PollFlags::POLLHUP | PollFlags::POLLERR,
                ),
            ];
            poll(&mut poll_fds, PollTimeout::NONE).map_err(tty_error("failed to poll task PTY"))?;
            let tty_flags = poll_fds[0].revents().unwrap_or_else(PollFlags::empty);
            let master_flags = poll_fds[1].revents().unwrap_or_else(PollFlags::empty);
            (
                tty_flags.contains(PollFlags::POLLIN),
                master_flags.contains(PollFlags::POLLIN),
                master_flags.intersects(PollFlags::POLLHUP | PollFlags::POLLERR),
            )
        };

        if tty_ready {
            let count = tty
                .read(&mut tty_input)
                .map_err(io_error("failed to read /dev/tty"))?;
            if count == 0 {
                let _ = kill(child, Signal::SIGHUP);
                return Ok(());
            }
            let filtered = filter_task_input(&tty_input[..count]);
            if !filtered.bytes.is_empty() {
                master
                    .write_all(&filtered.bytes)
                    .map_err(io_error("failed to write task PTY"))?;
            }
            if filtered.action == TaskInputAction::ReturnToCockpit {
                let _ = kill(child, Signal::SIGHUP);
                return Ok(());
            }
        }

        if master_ready {
            match master.read(&mut pty_output) {
                Ok(0) => return Ok(()),
                Ok(count) => {
                    tty.write_all(&pty_output[..count])
                        .map_err(io_error("failed to write /dev/tty"))?;
                    tty.flush().map_err(io_error("failed to flush /dev/tty"))?;
                }
                Err(error) if pty_was_closed(&error) => return Ok(()),
                Err(error) => {
                    return Err(CliError::CommandFailed(format!(
                        "failed to read task PTY: {error}"
                    )))
                }
            }
        }

        if master_closed {
            return Ok(());
        }
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

fn command_argv(command: &CommandSpec, executable: &CString) -> Result<Vec<CString>, CliError> {
    let mut argv = Vec::with_capacity(command.args.len() + 1);
    argv.push(executable.clone());
    for arg in &command.args {
        argv.push(CString::new(arg.as_str()).map_err(|_| {
            CliError::CommandFailed("task command argument contains a nul byte".to_string())
        })?);
    }
    Ok(argv)
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
    use super::{filter_task_input, FilteredTaskInput, TaskInputAction};
    use nix::sys::termios::{
        InputFlags, LocalFlags, OutputFlags, SpecialCharacterIndices, Termios,
    };

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
}

use super::{
    execute_task_entry_plan, filter_task_input, winsize_changed, FilteredTaskInput,
    TaskEntryPlanOutcome, TaskInputAction, TaskSessionContext, TaskSessionEnd, TaskSessionRunner,
};
use ajax_core::{
    adapters::{CommandMode, CommandSpec, RecordingCommandRunner},
    commands::CommandPlan,
};
use nix::poll::PollFlags;
use nix::sys::termios::{InputFlags, LocalFlags, OutputFlags, SpecialCharacterIndices, Termios};
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
fn task_poll_classification_drains_master_output_before_hup_close() {
    assert_eq!(
        super::classify_task_poll_events(
            PollFlags::empty(),
            PollFlags::POLLIN | PollFlags::POLLHUP
        ),
        super::TaskPollAction::Pump {
            tty_ready: false,
            master_ready: true,
        }
    );
    assert_eq!(
        super::classify_task_poll_events(PollFlags::empty(), PollFlags::POLLHUP),
        super::TaskPollAction::Close
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
fn task_screen_commands_clear_normal_buffer_without_disabling_scrollback() {
    assert_eq!(
        super::TASK_SCREEN_ENTRY_SEQUENCE,
        b"\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l\x1b[2J\x1b[H"
    );
    assert_eq!(super::TASK_SCREEN_EXIT_SEQUENCE, b"\x1b[?25h");
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
        ["select-window", "-t", "ajax-web-fix-login:task"],
    ));
    plan.commands.push(
        CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
            .with_mode(CommandMode::InheritStdio),
    );
    let mut runner = RecordingCommandRunner::default();
    let mut task_session = RecordingTaskSessionRunner::default();

    let context = TaskSessionContext::from_task_handle("web/fix-login");
    let outcome = execute_task_entry_plan(&plan, &mut runner, &mut task_session, &context).unwrap();
    assert!(matches!(outcome, TaskEntryPlanOutcome::Completed(_)));

    assert_eq!(
        runner.commands(),
        &[CommandSpec::new(
            "tmux",
            ["select-window", "-t", "ajax-web-fix-login:task"]
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
        ["select-window", "-t", "ajax-web-fix-login:task"],
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
            ["select-window", "-t", "ajax-web-fix-login:task"]
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

#[test]
fn prepared_task_command_preserves_tmux_for_switch_client() {
    let command = CommandSpec::new("tmux", ["switch-client", "-t", "ajax-web-fix-login"]);

    let prepared = super::PreparedTaskCommand::new(&command).unwrap();

    assert!(!prepared.clear_tmux_env);
}

#[test]
fn prepared_task_command_clears_tmux_for_attach_session() {
    let command = CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"]);

    let prepared = super::PreparedTaskCommand::new(&command).unwrap();

    assert!(prepared.clear_tmux_env);
}

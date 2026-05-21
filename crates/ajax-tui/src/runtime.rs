use ajax_core::{
    models::CockpitActionItem,
    output::{InboxResponse, ReposResponse, TaskCard},
};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    Terminal,
};
use std::{
    io,
    time::{Duration, Instant},
};

use crate::{
    cockpit_state::{Origin, Severity},
    input::{handle_cockpit_event, handle_refresh_result, EventLoopAction},
    rendering::render_ui,
    ActionOutcome, App, CockpitEventHandler, PendingAction,
};

const NOTICE_POLL_INTERVAL: Duration = Duration::from_millis(250);
const MAX_IDLE_POLL_INTERVAL: Duration = Duration::from_secs(1);

pub fn run_interactive(
    repos: ReposResponse,
    cards: Vec<TaskCard>,
    inbox: InboxResponse,
    on_action: impl FnMut(&CockpitActionItem) -> io::Result<ActionOutcome>,
) -> io::Result<Option<PendingAction>> {
    run_interactive_with_flash(repos, cards, inbox, None, on_action)
}

pub fn run_interactive_with_flash(
    repos: ReposResponse,
    cards: Vec<TaskCard>,
    inbox: InboxResponse,
    initial_flash: Option<String>,
    on_action: impl FnMut(&CockpitActionItem) -> io::Result<ActionOutcome>,
) -> io::Result<Option<PendingAction>> {
    run_interactive_with_flash_and_refresh(
        repos,
        cards,
        inbox,
        initial_flash,
        Duration::from_secs(1),
        ActionOnly { on_action },
    )
}

pub fn run_interactive_with_flash_and_refresh(
    repos: ReposResponse,
    cards: Vec<TaskCard>,
    inbox: InboxResponse,
    initial_flash: Option<String>,
    refresh_interval: Duration,
    handler: impl CockpitEventHandler,
) -> io::Result<Option<PendingAction>> {
    let mut stdout = io::stdout();
    let mut terminal_mode = TerminalModeGuard::enter(&mut stdout)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(repos, cards, inbox);
    if let Some(message) = initial_flash {
        app.notify_system(message, Severity::Success, Origin::UserAction);
    }
    let result = run_event_loop(&mut terminal, &mut app, handler, refresh_interval);

    terminal_mode.leave(terminal.backend_mut())?;
    terminal.show_cursor()?;

    result
}

struct ActionOnly<F> {
    on_action: F,
}

impl<F> CockpitEventHandler for ActionOnly<F>
where
    F: FnMut(&CockpitActionItem) -> io::Result<ActionOutcome>,
{
    fn on_action(&mut self, item: &CockpitActionItem) -> io::Result<ActionOutcome> {
        (self.on_action)(item)
    }
}

struct TerminalModeGuard {
    active: bool,
}

impl TerminalModeGuard {
    fn enter(output: &mut impl io::Write) -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = enter_terminal_mode(output) {
            let _ = disable_raw_mode();
            return Err(error);
        }

        Ok(Self { active: true })
    }

    fn leave(&mut self, output: &mut impl io::Write) -> io::Result<()> {
        let leave_result = leave_terminal_mode(output);
        let raw_result = disable_raw_mode();
        self.active = false;
        leave_result?;
        raw_result
    }
}

impl Drop for TerminalModeGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = disable_raw_mode();
            let mut stdout = io::stdout();
            let _ = leave_terminal_mode(&mut stdout);
        }
    }
}

fn enter_terminal_mode(output: &mut impl io::Write) -> io::Result<()> {
    execute!(output, EnterAlternateScreen, EnableMouseCapture)
}

fn leave_terminal_mode(output: &mut impl io::Write) -> io::Result<()> {
    execute!(output, LeaveAlternateScreen, DisableMouseCapture)
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TerminalModeCommand {
    EnterAlternateScreen,
    EnableMouseCapture,
    LeaveAlternateScreen,
    DisableMouseCapture,
}

#[cfg(test)]
pub(crate) fn terminal_entry_commands() -> &'static [TerminalModeCommand] {
    &[
        TerminalModeCommand::EnterAlternateScreen,
        TerminalModeCommand::EnableMouseCapture,
    ]
}

#[cfg(test)]
pub(crate) fn terminal_exit_commands() -> &'static [TerminalModeCommand] {
    &[
        TerminalModeCommand::LeaveAlternateScreen,
        TerminalModeCommand::DisableMouseCapture,
    ]
}

fn run_event_loop<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    mut handler: impl CockpitEventHandler,
    refresh_interval: Duration,
) -> io::Result<Option<PendingAction>> {
    let mut last_refresh = Instant::now();
    let mut needs_draw = true;
    loop {
        let height = terminal
            .size()
            .map_err(|_| io::Error::other("terminal backend size error"))?
            .height as usize;
        let feed_height = crate::visible_feed_height(app, height);

        let notices_changed = app.tick_notices();
        let mut refreshed = false;
        if should_refresh(&mut last_refresh, refresh_interval) {
            handle_refresh_result(app, handler.on_refresh())?;
            refreshed = true;
        }
        app.ensure_visible(feed_height);
        if should_draw(needs_draw, refreshed, notices_changed) {
            terminal
                .draw(|f| render_ui(f, app))
                .map_err(|_| io::Error::other("terminal backend draw error"))?;
            needs_draw = false;
        }

        let timeout = poll_timeout(
            Instant::now(),
            last_refresh,
            refresh_interval,
            app.has_transient_notices(),
        );
        if retry_terminal_event_io(|| event::poll(timeout))? {
            match handle_cockpit_event(
                app,
                retry_terminal_event_io(event::read)?,
                height,
                &mut handler,
            )? {
                EventLoopAction::Continue => needs_draw = true,
                EventLoopAction::Quit => return Ok(None),
                EventLoopAction::Pending(pending) => return Ok(Some(pending)),
            }
        }
    }
}

fn should_draw(needs_draw: bool, refreshed: bool, notices_changed: bool) -> bool {
    needs_draw || refreshed || notices_changed
}

fn terminal_event_error_allows_retry(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::Interrupted
}

fn retry_terminal_event_io<T>(mut operation: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    loop {
        match operation() {
            Ok(value) => return Ok(value),
            Err(error) if terminal_event_error_allows_retry(&error) => {}
            Err(error) => return Err(error),
        }
    }
}

fn should_refresh(last_refresh: &mut Instant, refresh_interval: Duration) -> bool {
    if refresh_interval.is_zero() || last_refresh.elapsed() < refresh_interval {
        return false;
    }

    *last_refresh = Instant::now();
    true
}

fn poll_timeout(
    now: Instant,
    last_refresh: Instant,
    refresh_interval: Duration,
    has_transient_notices: bool,
) -> Duration {
    let timeout = if refresh_interval.is_zero() {
        MAX_IDLE_POLL_INTERVAL
    } else {
        refresh_interval
            .saturating_sub(now.duration_since(last_refresh))
            .min(MAX_IDLE_POLL_INTERVAL)
    };

    if has_transient_notices {
        timeout.min(NOTICE_POLL_INTERVAL)
    } else {
        timeout
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{
        poll_timeout, should_draw, should_refresh, terminal_entry_commands, terminal_exit_commands,
        TerminalModeCommand,
    };

    #[test]
    fn terminal_mode_command_contract_lists_entry_and_exit_commands() {
        assert_eq!(
            terminal_entry_commands(),
            &[
                TerminalModeCommand::EnterAlternateScreen,
                TerminalModeCommand::EnableMouseCapture,
            ]
        );
        assert_eq!(
            terminal_exit_commands(),
            &[
                TerminalModeCommand::LeaveAlternateScreen,
                TerminalModeCommand::DisableMouseCapture,
            ]
        );
    }

    #[test]
    fn refresh_timer_waits_for_interval_and_advances_after_refresh() {
        let interval = Duration::from_secs(5);
        let mut recent = Instant::now();
        assert!(!should_refresh(&mut recent, interval));

        let mut due = Instant::now() - interval - Duration::from_millis(1);
        assert!(should_refresh(&mut due, interval));
        assert!(!should_refresh(&mut due, interval));
    }

    #[test]
    fn zero_refresh_interval_never_refreshes() {
        let mut last_refresh = Instant::now() - Duration::from_secs(60);

        assert!(!should_refresh(&mut last_refresh, Duration::ZERO));
    }

    #[test]
    fn interrupted_terminal_event_io_is_retried() {
        let error = std::io::Error::from(std::io::ErrorKind::Interrupted);

        assert!(super::terminal_event_error_allows_retry(&error));
    }

    #[test]
    fn non_interrupted_terminal_event_io_is_not_retried() {
        let error = std::io::Error::from(std::io::ErrorKind::UnexpectedEof);

        assert!(!super::terminal_event_error_allows_retry(&error));
    }

    #[test]
    fn poll_timeout_waits_until_refresh_deadline_when_idle() {
        let now = Instant::now();
        let last_refresh = now - Duration::from_millis(200);

        assert_eq!(
            poll_timeout(now, last_refresh, Duration::from_millis(800), false),
            Duration::from_millis(600)
        );
    }

    #[test]
    fn poll_timeout_uses_short_notice_ticks_for_transient_notices() {
        let now = Instant::now();
        let last_refresh = now - Duration::from_millis(200);

        assert_eq!(
            poll_timeout(now, last_refresh, Duration::from_millis(800), true),
            Duration::from_millis(250)
        );
    }

    #[test]
    fn redraw_scheduler_skips_idle_frames() {
        assert!(!should_draw(false, false, false));
        assert!(should_draw(true, false, false));
        assert!(should_draw(false, true, false));
        assert!(should_draw(false, false, true));
    }
}

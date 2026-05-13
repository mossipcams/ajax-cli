use ajax_core::{
    models::AttentionItem,
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
    input::{handle_cockpit_event, handle_refresh_result, EventLoopAction},
    rendering::render_ui,
    ActionOutcome, App, CockpitEventHandler, PendingAction,
};

pub fn run_interactive(
    repos: ReposResponse,
    cards: Vec<TaskCard>,
    inbox: InboxResponse,
    on_action: impl FnMut(&AttentionItem) -> io::Result<ActionOutcome>,
) -> io::Result<Option<PendingAction>> {
    run_interactive_with_flash(repos, cards, inbox, None, on_action)
}

pub fn run_interactive_with_flash(
    repos: ReposResponse,
    cards: Vec<TaskCard>,
    inbox: InboxResponse,
    initial_flash: Option<String>,
    on_action: impl FnMut(&AttentionItem) -> io::Result<ActionOutcome>,
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
        app.flash(message);
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
    F: FnMut(&AttentionItem) -> io::Result<ActionOutcome>,
{
    fn on_action(&mut self, item: &AttentionItem) -> io::Result<ActionOutcome> {
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
    loop {
        let height = terminal
            .size()
            .map_err(|_| io::Error::other("terminal backend size error"))?
            .height as usize;
        let feed_height = height.saturating_sub(2);

        app.tick_flash();
        if should_refresh(&mut last_refresh, refresh_interval) {
            handle_refresh_result(app, handler.on_refresh())?;
        }
        app.ensure_visible(feed_height);
        terminal
            .draw(|f| render_ui(f, app))
            .map_err(|_| io::Error::other("terminal backend draw error"))?;

        if event::poll(Duration::from_millis(250))? {
            match handle_cockpit_event(app, event::read()?, height, &mut handler)? {
                EventLoopAction::Continue => {}
                EventLoopAction::Quit => return Ok(None),
                EventLoopAction::Pending(pending) => return Ok(Some(pending)),
            }
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

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{
        should_refresh, terminal_entry_commands, terminal_exit_commands, TerminalModeCommand,
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
}

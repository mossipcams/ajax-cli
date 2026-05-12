use ajax_core::output::CockpitResponse;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind};
use std::io;

use crate::{navigation, ActionOutcome, App, CockpitEventHandler, PendingAction};

pub(crate) enum EventLoopAction {
    Continue,
    Quit,
    Pending(PendingAction),
}

pub(crate) fn handle_cockpit_event<H: CockpitEventHandler + ?Sized>(
    app: &mut App,
    event: Event,
    height: usize,
    handler: &mut H,
) -> io::Result<EventLoopAction> {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            handle_key_event(app, key.code, key.modifiers, handler)
        }
        Event::Mouse(mouse) => {
            // Layout: row 0 = header, last row = status bar, feed in between.
            let feed_top: usize = 1;
            let feed_bottom = height.saturating_sub(1);
            match mouse.kind {
                MouseEventKind::ScrollUp => app.select_prev(),
                MouseEventKind::ScrollDown => app.select_next(),
                MouseEventKind::Down(_) | MouseEventKind::Drag(_) => {
                    let mouse_row = mouse.row as usize;
                    if mouse_row >= feed_top && mouse_row < feed_bottom {
                        let feed_row = mouse_row - feed_top + app.viewport_scroll;
                        app.select_at_feed_row(feed_row);
                    }
                }
                _ => {}
            }
            Ok(EventLoopAction::Continue)
        }
        _ => Ok(EventLoopAction::Continue),
    }
}

fn handle_key_event<H: CockpitEventHandler + ?Sized>(
    app: &mut App,
    code: KeyCode,
    modifiers: KeyModifiers,
    handler: &mut H,
) -> io::Result<EventLoopAction> {
    match code {
        code if is_help_key_event(code, modifiers) => {
            app.open_help();
        }
        KeyCode::Enter if app.is_collecting_input() => {
            if let Some(pending) = app.submit_input() {
                return Ok(EventLoopAction::Pending(pending));
            }
        }
        code if app.is_collecting_input() && is_input_delete_key(code, modifiers) => {
            handle_back_key(app);
        }
        KeyCode::Char(character) if app.is_collecting_input() => {
            app.push_input_char(character);
        }
        KeyCode::Char('q') => return Ok(EventLoopAction::Quit),
        code if is_back_key_event(code, modifiers) => {
            handle_back_key(app);
        }
        KeyCode::Up | KeyCode::Char('k') => app.select_prev(),
        KeyCode::Down | KeyCode::Char('j') => app.select_next(),
        KeyCode::Enter => {
            if let Some(item) = app.activate_selected() {
                let confirmed = app.has_pending_confirmation(&item);
                let result = if confirmed {
                    app.pending_confirmation = None;
                    handler.on_confirmed_action(&item)
                } else {
                    let result = handler.on_action(&item);
                    if let Ok(ActionOutcome::Confirm(_)) = &result {
                        app.pending_confirmation = Some(item.clone());
                    } else {
                        app.pending_confirmation = None;
                    }
                    result
                };
                if let Some(pending) = handle_action_result(app, result)? {
                    return Ok(EventLoopAction::Pending(pending));
                }
            }
        }
        _ => {}
    }

    Ok(EventLoopAction::Continue)
}

pub(crate) fn handle_refresh_result(
    app: &mut App,
    result: io::Result<Option<CockpitResponse>>,
) -> io::Result<()> {
    match result {
        Ok(Some(snapshot)) => {
            app.apply_refresh(snapshot);
            Ok(())
        }
        Ok(None) => Ok(()),
        Err(error) => {
            app.flash(error.to_string());
            Ok(())
        }
    }
}

pub(crate) fn handle_action_result(
    app: &mut App,
    result: io::Result<ActionOutcome>,
) -> io::Result<Option<PendingAction>> {
    match result {
        Ok(ActionOutcome::Refresh {
            repos,
            tasks,
            inbox,
        }) => {
            app.reload(repos, tasks, inbox);
            Ok(None)
        }
        Ok(ActionOutcome::Defer(pending)) => Ok(Some(pending)),
        Ok(ActionOutcome::Confirm(message)) => {
            app.flash(message);
            Ok(None)
        }
        Ok(ActionOutcome::Message(message)) => {
            app.flash(message);
            Ok(None)
        }
        Err(error) => {
            app.flash(error.to_string());
            Ok(None)
        }
    }
}

pub(crate) fn handle_back_key(app: &mut App) -> bool {
    app.go_back();
    false
}

pub(crate) fn is_back_key_event(code: KeyCode, modifiers: KeyModifiers) -> bool {
    navigation::is_back_key_event(code, modifiers)
}

pub(crate) fn is_help_key_event(code: KeyCode, modifiers: KeyModifiers) -> bool {
    navigation::is_help_key_event(code, modifiers)
}

pub(crate) fn is_input_delete_key(code: KeyCode, modifiers: KeyModifiers) -> bool {
    navigation::is_input_delete_key(code, modifiers)
}

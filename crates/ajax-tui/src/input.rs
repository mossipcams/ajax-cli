use ajax_core::cockpit::CockpitAction;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

use crate::app::CockpitApp;

pub fn action_for_key(app: &CockpitApp, key: KeyEvent) -> Option<CockpitAction> {
    if key.kind != KeyEventKind::Press {
        return None;
    }

    match key.code {
        KeyCode::Char('q') => Some(CockpitAction::Quit),
        KeyCode::Char('r') => Some(CockpitAction::Refresh),
        KeyCode::Enter => app.selected_task().map(|task| CockpitAction::OpenTask {
            task_id: task.id.clone(),
        }),
        KeyCode::Char(' ') => app.selected_task().map(|task| CockpitAction::SelectTask {
            task_id: task.id.clone(),
        }),
        _ => None,
    }
}

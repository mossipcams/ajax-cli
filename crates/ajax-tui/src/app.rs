use ajax_core::cockpit::{CockpitSnapshot, CockpitTaskView};

#[derive(Debug, Clone)]
pub struct CockpitApp {
    snapshot: CockpitSnapshot,
    selected_task: usize,
}

impl CockpitApp {
    pub fn new(snapshot: CockpitSnapshot) -> Self {
        Self {
            snapshot,
            selected_task: 0,
        }
    }

    pub fn snapshot(&self) -> &CockpitSnapshot {
        &self.snapshot
    }

    pub fn selected_task(&self) -> Option<&CockpitTaskView> {
        self.snapshot.tasks.get(self.selected_task)
    }

    pub fn select_next_task(&mut self) {
        let max = self.snapshot.tasks.len().saturating_sub(1);
        self.selected_task = (self.selected_task + 1).min(max);
    }

    pub fn select_previous_task(&mut self) {
        self.selected_task = self.selected_task.saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use ajax_core::{
        cockpit::{CockpitAction, CockpitSnapshot, CockpitTaskView, TaskStatus},
        models::TaskId,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    use crate::{app::CockpitApp, input::action_for_key};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        }
    }

    fn snapshot() -> CockpitSnapshot {
        CockpitSnapshot {
            repos: Vec::new(),
            tasks: vec![CockpitTaskView {
                id: TaskId::new("task-1"),
                repo: "web".to_string(),
                handle: "fix-login".to_string(),
                title: "Fix login".to_string(),
                status: TaskStatus::Active,
                needs_attention: false,
                session: None,
            }],
            attention: Vec::new(),
            live: Vec::new(),
        }
    }

    #[test]
    fn keyboard_input_maps_to_typed_cockpit_action() {
        let app = CockpitApp::new(snapshot());

        assert_eq!(
            action_for_key(&app, key(KeyCode::Enter)),
            Some(CockpitAction::OpenTask {
                task_id: TaskId::new("task-1")
            })
        );
        assert_eq!(
            action_for_key(&app, key(KeyCode::Char('q'))),
            Some(CockpitAction::Quit)
        );
    }
}

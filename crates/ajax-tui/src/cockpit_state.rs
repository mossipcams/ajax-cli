use ajax_core::{
    models::{AttentionItem, RecommendedAction, TaskId},
    output::{InboxResponse, RepoSummary, ReposResponse, TaskCard},
    ui_state::UiState,
};
use std::collections::HashSet;

use crate::PendingAction;

#[derive(Clone)]
pub(crate) enum SelectableKind {
    Project(RepoSummary),
    /// Synthetic "+ new task" row, only shown inside a project.
    NewTask {
        repo: String,
    },
    Inbox(AttentionItem),
    Task(TaskCard),
    /// Action row inside the per-task action menu.
    TaskAction {
        task: TaskCard,
        recommended_action: String,
    },
}

#[derive(Clone)]
pub(crate) enum AppView {
    Projects,
    Project {
        repo: String,
    },
    /// Per-task action menu reached by selecting a task and pressing Enter.
    TaskActions {
        task: TaskCard,
        parent: Box<AppView>,
    },
    NewTaskInput {
        repo: String,
        title: String,
    },
    Help {
        previous: Box<AppView>,
    },
}

impl SelectableKind {
    /// Synthesize an `AttentionItem` for the dispatch callback. Inbox items
    /// pass through unchanged; task rows get the default open action.
    /// The CLI dispatcher decides whether an action is navigational or should
    /// point the operator at an explicit executable command.
    pub(crate) fn as_action(&self) -> AttentionItem {
        match self {
            SelectableKind::Project(repo) => AttentionItem {
                task_id: TaskId::new(format!("__project__{}", repo.name)),
                task_handle: repo.name.clone(),
                reason: "project".to_string(),
                priority: 0,
                recommended_action: RecommendedAction::SelectProject.as_str().to_string(),
            },
            SelectableKind::NewTask { repo } => AttentionItem {
                task_id: TaskId::new(format!("__new_task__{repo}")),
                task_handle: repo.clone(),
                reason: "create a new task".to_string(),
                priority: 0,
                recommended_action: RecommendedAction::NewTask.as_str().to_string(),
            },
            SelectableKind::Inbox(item) => item.clone(),
            SelectableKind::Task(card) => AttentionItem {
                task_id: card.id.clone(),
                task_handle: card.qualified_handle.clone(),
                reason: card.action_reason.clone(),
                priority: 50,
                recommended_action: RecommendedAction::OpenTask.as_str().to_string(),
            },
            SelectableKind::TaskAction {
                task,
                recommended_action,
            } => AttentionItem {
                task_id: task.id.clone(),
                task_handle: task.qualified_handle.clone(),
                reason: task.action_reason.clone(),
                priority: 50,
                recommended_action: recommended_action.clone(),
            },
        }
    }
}

fn build_selectables(
    view: &AppView,
    repos: &ReposResponse,
    inbox: &InboxResponse,
    cards: &[TaskCard],
) -> Vec<SelectableKind> {
    let mut out = Vec::new();
    match view {
        AppView::Projects => {
            let inbox_task_ids = inbox_task_ids(inbox.items.iter());
            out.extend(inbox.items.iter().cloned().map(SelectableKind::Inbox));
            out.extend(repos.repos.iter().cloned().map(SelectableKind::Project));
            out.extend(
                cards
                    .iter()
                    .filter(|card| !inbox_task_ids.contains(&card.id))
                    .filter(|card| !matches!(card.ui_state, UiState::Archived))
                    .cloned()
                    .map(SelectableKind::Task),
            );
        }
        AppView::Project { repo } => {
            let repo_inbox_items = inbox
                .items
                .iter()
                .filter(|item| task_handle_repo(&item.task_handle) == Some(repo.as_str()));
            let inbox_task_ids = inbox_task_ids(repo_inbox_items.clone());

            out.push(SelectableKind::NewTask { repo: repo.clone() });
            out.extend(repo_inbox_items.cloned().map(SelectableKind::Inbox));
            out.extend(
                cards
                    .iter()
                    .filter(|card| card_repo(card) == Some(repo.as_str()))
                    .filter(|card| !inbox_task_ids.contains(&card.id))
                    .filter(|card| !matches!(card.ui_state, UiState::Archived))
                    .cloned()
                    .map(SelectableKind::Task),
            );
        }
        AppView::TaskActions { task, .. } => {
            out.extend(
                task.available_actions
                    .iter()
                    .map(|action| SelectableKind::TaskAction {
                        task: task.clone(),
                        recommended_action: action.as_str().to_string(),
                    }),
            );
        }
        AppView::NewTaskInput { .. } => {}
        AppView::Help { .. } => {}
    }
    out
}

fn inbox_task_ids<'a>(items: impl Iterator<Item = &'a AttentionItem>) -> HashSet<TaskId> {
    items.map(|item| item.task_id.clone()).collect()
}

pub struct App {
    pub(crate) repos: ReposResponse,
    pub(crate) cards: Vec<TaskCard>,
    pub(crate) inbox: InboxResponse,
    pub(crate) view: AppView,
    pub(crate) selectables: Vec<SelectableKind>,
    pub(crate) selected: usize,
    pub(crate) viewport_scroll: usize,
    pub(crate) flash: Option<(String, u8)>,
    pub(crate) pending_confirmation: Option<AttentionItem>,
}

pub(crate) const FLASH_TICKS: u8 = 8; // ~2 s at 250 ms poll

impl App {
    pub fn new(repos: ReposResponse, cards: Vec<TaskCard>, inbox: InboxResponse) -> Self {
        let view = AppView::Projects;
        let selectables = build_selectables(&view, &repos, &inbox, &cards);
        Self {
            repos,
            cards,
            inbox,
            view,
            selectables,
            selected: 0,
            viewport_scroll: 0,
            flash: None,
            pending_confirmation: None,
        }
    }

    pub fn select_prev(&mut self) {
        if self.selectables.is_empty() {
            return;
        }
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn select_next(&mut self) {
        if self.selectables.is_empty() {
            return;
        }
        let max = self.selectables.len() - 1;
        self.selected = (self.selected + 1).min(max);
    }

    /// The action that Enter would dispatch right now, or None if nothing is selectable.
    pub fn selected_action(&self) -> Option<AttentionItem> {
        self.selectables.get(self.selected).map(|s| s.as_action())
    }

    /// Return to the cockpit's main project list. Returns false at the top
    /// level so callers can keep the TUI alive without treating back as quit.
    pub fn go_home(&mut self) -> bool {
        if matches!(self.view, AppView::Projects) {
            return false;
        }

        self.view = AppView::Projects;
        self.selected = 0;
        self.viewport_scroll = 0;
        self.pending_confirmation = None;
        self.rebuild_selectables();
        true
    }

    /// Erase editable input, then return to the cockpit's main project list.
    /// Returns false at the top level so back never exits the TUI.
    pub fn go_back(&mut self) -> bool {
        if let AppView::Help { previous } = &self.view {
            self.view = *previous.clone();
            self.selected = 0;
            self.viewport_scroll = 0;
            self.pending_confirmation = None;
            self.rebuild_selectables();
            return true;
        }

        if let AppView::TaskActions { parent, .. } = &self.view {
            self.view = *parent.clone();
            self.selected = 0;
            self.viewport_scroll = 0;
            self.pending_confirmation = None;
            self.rebuild_selectables();
            return true;
        }

        if let AppView::NewTaskInput { title, .. } = &mut self.view {
            if !title.is_empty() {
                title.pop();
                return true;
            }
        }

        self.go_home()
    }

    pub fn open_help(&mut self) {
        if matches!(self.view, AppView::Help { .. }) {
            return;
        }

        self.view = AppView::Help {
            previous: Box::new(self.view.clone()),
        };
        self.selected = 0;
        self.viewport_scroll = 0;
        self.flash = None;
        self.pending_confirmation = None;
        self.rebuild_selectables();
    }

    pub fn activate_selected(&mut self) -> Option<AttentionItem> {
        match self.selectables.get(self.selected).cloned()? {
            SelectableKind::Project(repo) => {
                self.view = AppView::Project { repo: repo.name };
                self.selected = 0;
                self.viewport_scroll = 0;
                self.pending_confirmation = None;
                self.rebuild_selectables();
                None
            }
            SelectableKind::NewTask { repo } => {
                self.view = AppView::NewTaskInput {
                    repo,
                    title: String::new(),
                };
                self.selected = 0;
                self.viewport_scroll = 0;
                self.flash = None;
                self.pending_confirmation = None;
                self.rebuild_selectables();
                None
            }
            SelectableKind::Task(card) => {
                self.view = AppView::TaskActions {
                    task: card,
                    parent: Box::new(self.view.clone()),
                };
                self.selected = 0;
                self.viewport_scroll = 0;
                self.flash = None;
                self.pending_confirmation = None;
                self.rebuild_selectables();
                None
            }
            SelectableKind::Inbox(item) => {
                if let Some(card) = self.find_card_for_task(&item.task_id) {
                    let preselected = card
                        .available_actions
                        .iter()
                        .position(|action| action.as_str() == item.recommended_action.as_str())
                        .unwrap_or(0);
                    self.view = AppView::TaskActions {
                        task: card,
                        parent: Box::new(self.view.clone()),
                    };
                    self.selected = preselected;
                    self.viewport_scroll = 0;
                    self.flash = None;
                    self.pending_confirmation = None;
                    self.rebuild_selectables();
                    None
                } else {
                    Some(SelectableKind::Inbox(item).as_action())
                }
            }
            selectable => Some(selectable.as_action()),
        }
    }

    fn find_card_for_task(&self, task_id: &TaskId) -> Option<TaskCard> {
        self.cards.iter().find(|card| &card.id == task_id).cloned()
    }

    pub fn push_input_char(&mut self, character: char) {
        if let AppView::NewTaskInput { title, .. } = &mut self.view {
            title.push(character);
        }
    }

    pub fn submit_input(&mut self) -> Option<PendingAction> {
        let AppView::NewTaskInput { repo, title } = &self.view else {
            return None;
        };
        let title = title.trim();
        if title.is_empty() {
            self.flash("task name required".to_string());
            return None;
        }

        Some(PendingAction {
            task_handle: repo.clone(),
            recommended_action: RecommendedAction::NewTask.as_str().to_string(),
            task_title: Some(title.to_string()),
        })
    }

    pub fn apply_refresh(&mut self, snapshot: CockpitSnapshot) {
        self.reload(snapshot.repos, snapshot.cards, snapshot.inbox);
    }

    pub(crate) fn is_collecting_input(&self) -> bool {
        matches!(self.view, AppView::NewTaskInput { .. })
    }

    pub(crate) fn reload(
        &mut self,
        repos: ReposResponse,
        cards: Vec<TaskCard>,
        inbox: InboxResponse,
    ) {
        let missing_task_after_refresh = match &self.view {
            AppView::TaskActions { task, .. } => {
                !cards.iter().any(|candidate| candidate.id == task.id)
            }
            _ => false,
        };
        self.repos = repos;
        self.cards = cards;
        self.inbox = inbox;
        self.pending_confirmation = None;
        if missing_task_after_refresh {
            self.view = AppView::Projects;
            self.selected = 0;
            self.viewport_scroll = 0;
        }
        self.rebuild_selectables();
        let max = self.selectables.len().saturating_sub(1);
        self.selected = self.selected.min(max);
    }

    fn rebuild_selectables(&mut self) {
        self.selectables = build_selectables(&self.view, &self.repos, &self.inbox, &self.cards);
    }

    pub(crate) fn flash(&mut self, msg: String) {
        self.flash = Some((msg, FLASH_TICKS));
    }

    pub(crate) fn has_pending_confirmation(&self, item: &AttentionItem) -> bool {
        self.pending_confirmation.as_ref() == Some(item)
    }

    pub(crate) fn tick_flash(&mut self) {
        if let Some((_, ticks)) = &mut self.flash {
            if *ticks == 0 {
                self.flash = None;
            } else {
                *ticks -= 1;
            }
        }
    }
}

/// Snapshot of cockpit state passed into the TUI's refresh path.
#[derive(Clone, Debug)]
pub struct CockpitSnapshot {
    pub repos: ReposResponse,
    pub cards: Vec<TaskCard>,
    pub inbox: InboxResponse,
}

pub(crate) fn task_handle_repo(handle: &str) -> Option<&str> {
    handle.split_once('/').map(|(repo, _)| repo)
}

pub(crate) fn card_repo(card: &TaskCard) -> Option<&str> {
    task_handle_repo(&card.qualified_handle)
}

use ajax_core::{
    models::{AttentionItem, LifecycleStatus, RecommendedAction, TaskId},
    output::{InboxResponse, RepoSummary, ReposResponse, TaskCard},
    ui_state::UiState,
};
use std::collections::{HashMap, HashSet};

use crate::PendingAction;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Hint,
    Success,
    Error,
    Confirm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    BackgroundEvent,
    UserAction,
}

#[derive(Clone, Debug)]
pub struct Notice {
    pub msg: String,
    pub severity: Severity,
    pub origin: Origin,
    pub ticks_remaining: u8,
}

pub(crate) const NOTICE_TICKS_HINT: u8 = 4;
pub(crate) const NOTICE_TICKS_SUCCESS: u8 = 8;
pub(crate) const NOTICE_TICKS_ERROR: u8 = 20;
pub(crate) const NOTICE_TICKS_CONFIRM: u8 = u8::MAX;

pub(crate) fn lifetime_for(severity: Severity) -> u8 {
    match severity {
        Severity::Hint => NOTICE_TICKS_HINT,
        Severity::Success => NOTICE_TICKS_SUCCESS,
        Severity::Error => NOTICE_TICKS_ERROR,
        Severity::Confirm => NOTICE_TICKS_CONFIRM,
    }
}

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
    pub(crate) notices: HashMap<TaskId, Notice>,
    pub(crate) system_notice: Option<Notice>,
    pub(crate) pending_confirmation: Option<AttentionItem>,
}

#[cfg(test)]
pub(crate) const FLASH_TICKS: u8 = NOTICE_TICKS_SUCCESS;

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
            notices: HashMap::new(),
            system_notice: None,
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
        self.invalidate_pending_confirmation();
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
            self.invalidate_pending_confirmation();
            self.rebuild_selectables();
            return true;
        }

        if let AppView::TaskActions { parent, .. } = &self.view {
            self.view = *parent.clone();
            self.selected = 0;
            self.viewport_scroll = 0;
            self.invalidate_pending_confirmation();
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
        self.system_notice = None;
        self.invalidate_pending_confirmation();
        self.rebuild_selectables();
    }

    pub fn activate_selected(&mut self) -> Option<AttentionItem> {
        match self.selectables.get(self.selected).cloned()? {
            SelectableKind::Project(repo) => {
                self.view = AppView::Project { repo: repo.name };
                self.selected = 0;
                self.viewport_scroll = 0;
                self.invalidate_pending_confirmation();
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
                self.system_notice = None;
                self.invalidate_pending_confirmation();
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
                self.system_notice = None;
                self.invalidate_pending_confirmation();
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
                    self.system_notice = None;
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
            self.notify_system(
                "task name required".to_string(),
                Severity::Hint,
                Origin::UserAction,
            );
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
        let prior_lifecycles: HashMap<TaskId, LifecycleStatus> = self
            .cards
            .iter()
            .map(|card| (card.id.clone(), card.lifecycle))
            .collect();
        self.repos = repos;
        self.cards = cards;
        self.inbox = inbox;
        self.pending_confirmation = None;
        self.prune_notices_for_vanished_tasks();
        self.prune_background_error_notices();
        self.prune_stale_lifecycle_notices(&prior_lifecycles);
        self.clear_system_background_error();
        if missing_task_after_refresh {
            self.view = AppView::Projects;
            self.selected = 0;
            self.viewport_scroll = 0;
        }
        self.rebuild_selectables();
        let max = self.selectables.len().saturating_sub(1);
        self.selected = self.selected.min(max);
    }

    fn prune_notices_for_vanished_tasks(&mut self) {
        let live_ids: HashSet<&TaskId> = self.cards.iter().map(|card| &card.id).collect();
        self.notices.retain(|task_id, _| live_ids.contains(task_id));
    }

    fn prune_background_error_notices(&mut self) {
        self.notices.retain(|_, notice| {
            !(notice.severity == Severity::Error && notice.origin == Origin::BackgroundEvent)
        });
    }

    fn clear_system_background_error(&mut self) {
        if let Some(notice) = &self.system_notice {
            if notice.severity == Severity::Error && notice.origin == Origin::BackgroundEvent {
                self.system_notice = None;
            }
        }
    }

    fn prune_stale_lifecycle_notices(&mut self, prior: &HashMap<TaskId, LifecycleStatus>) {
        let current: HashMap<&TaskId, LifecycleStatus> = self
            .cards
            .iter()
            .map(|card| (&card.id, card.lifecycle))
            .collect();
        self.notices.retain(|task_id, notice| {
            let stale_by_severity = matches!(notice.severity, Severity::Success | Severity::Hint);
            if !stale_by_severity {
                return true;
            }
            !matches!(
                (prior.get(task_id), current.get(task_id)),
                (Some(old), Some(new)) if old != new
            )
        });
    }

    fn rebuild_selectables(&mut self) {
        self.selectables = build_selectables(&self.view, &self.repos, &self.inbox, &self.cards);
    }

    fn invalidate_pending_confirmation(&mut self) {
        let Some(item) = self.pending_confirmation.take() else {
            return;
        };
        if let Some(notice) = self.notices.get(&item.task_id) {
            if notice.severity == Severity::Confirm {
                self.notices.remove(&item.task_id);
            }
        }
        self.notify_system(
            "confirm again — context changed".to_string(),
            Severity::Hint,
            Origin::UserAction,
        );
    }

    pub(crate) fn notify_task(
        &mut self,
        task_id: TaskId,
        msg: String,
        severity: Severity,
        origin: Origin,
    ) {
        let new = Notice {
            msg,
            severity,
            origin,
            ticks_remaining: lifetime_for(severity),
        };
        match self.notices.get(&task_id) {
            None => {
                self.notices.insert(task_id, new);
            }
            Some(existing) => {
                if existing.msg == new.msg && existing.severity == new.severity {
                    let mut updated = existing.clone();
                    updated.ticks_remaining = lifetime_for(new.severity);
                    updated.origin = new.origin;
                    self.notices.insert(task_id, updated);
                    return;
                }
                if new.severity > existing.severity
                    || (new.severity == existing.severity
                        && existing.origin == Origin::BackgroundEvent
                        && new.origin == Origin::UserAction)
                {
                    self.notices.insert(task_id, new);
                }
            }
        }
    }

    pub(crate) fn notify_system(&mut self, msg: String, severity: Severity, origin: Origin) {
        let new = Notice {
            msg,
            severity,
            origin,
            ticks_remaining: lifetime_for(severity),
        };
        match &self.system_notice {
            None => {
                self.system_notice = Some(new);
            }
            Some(existing) => {
                if existing.msg == new.msg && existing.severity == new.severity {
                    let mut updated = existing.clone();
                    updated.ticks_remaining = lifetime_for(new.severity);
                    updated.origin = new.origin;
                    self.system_notice = Some(updated);
                    return;
                }
                if new.severity > existing.severity
                    || (new.severity == existing.severity
                        && existing.origin == Origin::BackgroundEvent
                        && new.origin == Origin::UserAction)
                {
                    self.system_notice = Some(new);
                }
            }
        }
    }

    pub(crate) fn current_notice(&self) -> Option<&Notice> {
        if let Some(item) = &self.pending_confirmation {
            if let Some(notice) = self.notices.get(&item.task_id) {
                if notice.severity == Severity::Confirm {
                    return Some(notice);
                }
            }
        }
        if let Some(task_id) = self.selected_task_id() {
            if let Some(notice) = self.notices.get(task_id) {
                return Some(notice);
            }
        }
        self.system_notice.as_ref()
    }

    pub(crate) fn selected_task_id(&self) -> Option<&TaskId> {
        let selectable = self.selectables.get(self.selected)?;
        match selectable {
            SelectableKind::Task(card) => Some(&card.id),
            SelectableKind::Inbox(item) => Some(&item.task_id),
            SelectableKind::TaskAction { task, .. } => Some(&task.id),
            _ => None,
        }
    }

    pub(crate) fn has_pending_confirmation(&self, item: &AttentionItem) -> bool {
        self.pending_confirmation.as_ref() == Some(item)
    }

    pub(crate) fn tick_notices(&mut self) {
        self.notices.retain(|_, notice| {
            if notice.severity == Severity::Confirm {
                true
            } else if notice.ticks_remaining == 0 {
                false
            } else {
                notice.ticks_remaining -= 1;
                true
            }
        });
        if let Some(notice) = &mut self.system_notice {
            if notice.severity != Severity::Confirm {
                if notice.ticks_remaining == 0 {
                    self.system_notice = None;
                } else {
                    notice.ticks_remaining -= 1;
                }
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

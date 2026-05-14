use ajax_core::{
    models::{CockpitActionItem, LifecycleStatus, OperatorAction, TaskId},
    output::{AnnotationItem, InboxResponse, RepoSummary, ReposResponse, TaskCard},
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
    Inbox(AnnotationItem),
    Task(TaskCard),
    /// Action row inside the per-task action menu.
    TaskAction {
        task: TaskCard,
        action: String,
    },
}

#[derive(Clone)]
pub(crate) enum AppView {
    Projects,
    Project { repo: String },
    NewTaskInput { repo: String, title: String },
    Help { previous: Box<AppView> },
}

impl SelectableKind {
    /// Synthesize an action item for the dispatch callback.
    /// The CLI dispatcher decides whether an action is navigational or should
    /// point the operator at an explicit executable command.
    pub(crate) fn as_action(&self) -> CockpitActionItem {
        match self {
            SelectableKind::Project(repo) => CockpitActionItem {
                task_id: TaskId::new(format!("__project__{}", repo.name)),
                task_handle: repo.name.clone(),
                reason: "project".to_string(),
                priority: 0,
                action: "status".to_string(),
            },
            SelectableKind::NewTask { repo } => CockpitActionItem {
                task_id: TaskId::new(format!("__new_task__{repo}")),
                task_handle: repo.clone(),
                reason: "create a new task".to_string(),
                priority: 0,
                action: OperatorAction::Start.as_str().to_string(),
            },
            SelectableKind::Inbox(item) => CockpitActionItem {
                task_id: item.task_id.clone(),
                task_handle: item.task_handle.clone(),
                reason: item.reason.clone(),
                priority: item.severity,
                action: item.action.as_str().to_string(),
            },
            SelectableKind::Task(card) => CockpitActionItem {
                task_id: card.id.clone(),
                task_handle: card.qualified_handle.clone(),
                reason: card_action_reason(card),
                priority: 50,
                action: card.primary_action.as_str().to_string(),
            },
            SelectableKind::TaskAction { task, action } => CockpitActionItem {
                task_id: task.id.clone(),
                task_handle: task.qualified_handle.clone(),
                reason: card_action_reason(task),
                priority: 50,
                action: action.clone(),
            },
        }
    }
}

fn card_action_reason(card: &TaskCard) -> String {
    card.annotations
        .first()
        .map(|annotation| crate::evidence_label(&annotation.evidence).to_string())
        .or_else(|| card.live_summary.clone())
        .unwrap_or_else(|| card.ui_state.as_str().to_string())
}

fn build_selectables(
    view: &AppView,
    repos: &ReposResponse,
    inbox: &InboxResponse,
    cards: &[TaskCard],
    expanded_task: &Option<TaskId>,
) -> Vec<SelectableKind> {
    let mut out = Vec::new();
    let push_with_drawer =
        |out: &mut Vec<SelectableKind>, base: SelectableKind, drawer_task: &TaskCard| {
            out.push(base);
            if expanded_task.as_ref() == Some(&drawer_task.id) {
                for action in &drawer_task.available_actions {
                    out.push(SelectableKind::TaskAction {
                        task: drawer_task.clone(),
                        action: action.as_str().to_string(),
                    });
                }
            }
        };
    match view {
        AppView::Projects => {
            let annotation_items = annotation_inbox_items(cards, inbox);
            let inbox_task_ids = inbox_task_ids(annotation_items.iter());
            for item in annotation_items {
                let drawer_card = cards.iter().find(|c| c.id == item.task_id).cloned();
                let base = SelectableKind::Inbox(item);
                if let Some(card) = drawer_card {
                    push_with_drawer(&mut out, base, &card);
                } else {
                    out.push(base);
                }
            }
            out.extend(repos.repos.iter().cloned().map(SelectableKind::Project));
            for card in cards
                .iter()
                .filter(|card| !inbox_task_ids.contains(&card.id))
                .filter(|card| !matches!(card.ui_state, UiState::Archived))
            {
                push_with_drawer(&mut out, SelectableKind::Task(card.clone()), card);
            }
        }
        AppView::Project { repo } => {
            out.push(SelectableKind::NewTask { repo: repo.clone() });
            for card in cards
                .iter()
                .filter(|card| card_repo(card) == Some(repo.as_str()))
                .filter(|card| !matches!(card.ui_state, UiState::Archived))
            {
                push_with_drawer(&mut out, SelectableKind::Task(card.clone()), card);
            }
        }
        AppView::NewTaskInput { .. } => {}
        AppView::Help { .. } => {}
    }
    out
}

fn annotation_inbox_items(cards: &[TaskCard], fallback: &InboxResponse) -> Vec<AnnotationItem> {
    let mut items = cards
        .iter()
        .filter_map(|card| {
            let annotation = card
                .annotations
                .iter()
                .min_by_key(|annotation| annotation.severity)?;
            Some(AnnotationItem {
                task_id: card.id.clone(),
                task_handle: card.qualified_handle.clone(),
                reason: crate::evidence_label(&annotation.evidence).to_string(),
                severity: annotation.severity,
                action: annotation.suggests,
            })
        })
        .collect::<Vec<_>>();
    if items.is_empty() {
        items = fallback.items.clone();
    }
    items.sort_by(|left, right| {
        left.severity
            .cmp(&right.severity)
            .then_with(|| left.task_handle.cmp(&right.task_handle))
    });
    items
}

fn inbox_task_ids<'a>(items: impl Iterator<Item = &'a AnnotationItem>) -> HashSet<TaskId> {
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
    pub(crate) pending_confirmation: Option<CockpitActionItem>,
    /// Task whose inline action drawer is currently open. The drawer renders
    /// annotation lines plus selectable action rows under the matching task
    /// or inbox row. `None` keeps the list dense.
    pub(crate) expanded_task: Option<TaskId>,
}

#[cfg(test)]
pub(crate) const FLASH_TICKS: u8 = NOTICE_TICKS_SUCCESS;

impl App {
    pub fn new(repos: ReposResponse, cards: Vec<TaskCard>, inbox: InboxResponse) -> Self {
        let view = AppView::Projects;
        let expanded_task: Option<TaskId> = None;
        let selectables = build_selectables(&view, &repos, &inbox, &cards, &expanded_task);
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
            expanded_task,
        }
    }

    pub fn select_prev(&mut self) {
        if self.selectables.is_empty() {
            return;
        }
        self.selected = self.selected.saturating_sub(1);
        self.collapse_drawer_if_left();
    }

    pub fn select_next(&mut self) {
        if self.selectables.is_empty() {
            return;
        }
        let max = self.selectables.len() - 1;
        self.selected = (self.selected + 1).min(max);
        self.collapse_drawer_if_left();
    }

    fn collapse_drawer_if_left(&mut self) {
        let Some(open) = self.expanded_task.clone() else {
            return;
        };
        let still_inside = self
            .selectables
            .get(self.selected)
            .map(|s| match s {
                SelectableKind::Task(card) => card.id == open,
                SelectableKind::Inbox(item) => item.task_id == open,
                SelectableKind::TaskAction { task, .. } => task.id == open,
                _ => false,
            })
            .unwrap_or(false);
        if !still_inside {
            self.expanded_task = None;
            self.invalidate_pending_confirmation();
            // Save selection position by remembering selectable identity-ish.
            let was_idx = self.selected;
            self.rebuild_selectables();
            self.selected = was_idx.min(self.selectables.len().saturating_sub(1));
        }
    }

    /// The action that Enter would dispatch right now, or None if nothing is selectable.
    pub fn selected_action(&self) -> Option<CockpitActionItem> {
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
        if self.expanded_task.is_some() {
            self.collapse_drawer();
            return true;
        }
        if let AppView::Help { previous } = &self.view {
            self.view = *previous.clone();
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

    pub fn activate_selected(&mut self) -> Option<CockpitActionItem> {
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
                self.expand_task_drawer(card.id.clone(), card.primary_action);
                None
            }
            SelectableKind::Inbox(item) => {
                if let Some(card) = self.find_card_for_task(&item.task_id) {
                    self.expand_task_drawer(card.id.clone(), item.action);
                    None
                } else {
                    Some(SelectableKind::Inbox(item).as_action())
                }
            }
            selectable => Some(selectable.as_action()),
        }
    }

    fn expand_task_drawer(&mut self, task_id: TaskId, preferred: OperatorAction) {
        self.expanded_task = Some(task_id.clone());
        self.system_notice = None;
        self.invalidate_pending_confirmation();
        self.rebuild_selectables();
        if let Some(idx) = self.selectables.iter().position(|s| match s {
            SelectableKind::TaskAction { task, action } => {
                task.id == task_id
                    && OperatorAction::from_label(action)
                        .map(|a| a == preferred)
                        .unwrap_or(false)
            }
            _ => false,
        }) {
            self.selected = idx;
        } else if let Some(idx) = self.selectables.iter().position(|s| {
            matches!(s,
            SelectableKind::TaskAction { task, .. } if task.id == task_id)
        }) {
            self.selected = idx;
        }
    }

    fn collapse_drawer(&mut self) -> bool {
        if self.expanded_task.is_none() {
            return false;
        }
        let task_id = self.expanded_task.take();
        self.invalidate_pending_confirmation();
        self.rebuild_selectables();
        if let Some(id) = task_id {
            if let Some(idx) = self.selectables.iter().position(|s| match s {
                SelectableKind::Task(card) => card.id == id,
                SelectableKind::Inbox(item) => item.task_id == id,
                _ => false,
            }) {
                self.selected = idx;
            }
        }
        true
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
            action: OperatorAction::Start.as_str().to_string(),
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
        let missing_task_after_refresh = match (&self.view, &self.expanded_task) {
            (AppView::Project { .. }, Some(task_id)) => {
                !cards.iter().any(|candidate| candidate.id == *task_id)
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
        self.selectables = build_selectables(
            &self.view,
            &self.repos,
            &self.inbox,
            &self.cards,
            &self.expanded_task,
        );
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

    pub(crate) fn has_pending_confirmation(&self, item: &CockpitActionItem) -> bool {
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

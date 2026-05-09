#![deny(unsafe_op_in_unsafe_fn)]

use ajax_core::{
    models::{AttentionItem, TaskId},
    output::{InboxResponse, RepoSummary, ReposResponse, TaskSummary, TasksResponse},
};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
        MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use std::{io, ops::Range, time::Duration};

// ── Text renderer (watch mode) ────────────────────────────────────────────────

pub fn render_cockpit(
    repos: &ReposResponse,
    tasks: &TasksResponse,
    review: &TasksResponse,
    inbox: &InboxResponse,
) -> String {
    let mut lines = vec![
        "Ajax Cockpit".to_string(),
        format!("Repos: {}", repos.repos.len()),
        format!("Tasks: {}", tasks.tasks.len()),
        format!("Review: {}", review.tasks.len()),
        "Inbox".to_string(),
    ];

    if inbox.items.is_empty() {
        lines.push("no tasks need attention".to_string());
    } else {
        lines.extend(inbox.items.iter().map(|item| {
            format!(
                "{}: {} -> {}",
                item.task_handle, item.reason, item.recommended_action
            )
        }));
    }

    lines.join("\n")
}

// ── Public types ──────────────────────────────────────────────────────────────

/// Returned when the TUI exits with a deferred action (e.g. open → tmux attach).
pub struct PendingAction {
    pub task_handle: String,
    pub recommended_action: String,
    pub task_title: Option<String>,
}

/// What the `on_action` callback returns to tell the TUI what to do next.
pub enum ActionOutcome {
    /// Reload the TUI with fresh data.
    Refresh {
        repos: ReposResponse,
        tasks: TasksResponse,
        review: TasksResponse,
        inbox: InboxResponse,
    },
    /// Exit the TUI — the CLI will run the deferred action.
    Defer(PendingAction),
    /// Show a brief status message then stay in the TUI.
    Message(String),
}

// ── Selectable items ──────────────────────────────────────────────────────────

#[derive(Clone)]
enum SelectableKind {
    Project(RepoSummary),
    ProjectAction {
        repo: String,
        label: String,
        recommended_action: String,
    },
    TaskAction {
        task: TaskSummary,
        recommended_action: String,
    },
    /// Synthetic top-of-feed entry. Dispatched as a "new task" action.
    NewTask,
    Inbox(AttentionItem),
    Task(TaskSummary),
    Review(TaskSummary),
}

#[derive(Clone)]
enum AppView {
    Projects,
    Project {
        repo: String,
    },
    TaskPicker {
        repo: String,
        label: String,
        recommended_action: String,
    },
    NewTaskInput {
        repo: String,
        title: String,
    },
}

impl SelectableKind {
    /// Synthesize an `AttentionItem` for the dispatch callback. Inbox items
    /// pass through unchanged; tasks and review entries get default actions.
    /// The CLI dispatcher decides whether an action is navigational or should
    /// point the operator at an explicit executable command.
    fn as_action(&self) -> AttentionItem {
        match self {
            SelectableKind::Project(repo) => AttentionItem {
                task_id: TaskId::new(format!("__project__{}", repo.name)),
                task_handle: repo.name.clone(),
                reason: "project".to_string(),
                priority: 0,
                recommended_action: "select project".to_string(),
            },
            SelectableKind::ProjectAction {
                repo,
                label,
                recommended_action,
            } => AttentionItem {
                task_id: TaskId::new(format!("__project_action__{repo}__{recommended_action}")),
                task_handle: repo.clone(),
                reason: label.clone(),
                priority: 0,
                recommended_action: recommended_action.clone(),
            },
            SelectableKind::TaskAction {
                task,
                recommended_action,
            } => AttentionItem {
                task_id: TaskId::new(task.id.clone()),
                task_handle: task.qualified_handle.clone(),
                reason: task.lifecycle_status.clone(),
                priority: 50,
                recommended_action: recommended_action.clone(),
            },
            SelectableKind::NewTask => AttentionItem {
                task_id: TaskId::new("__new_task__"),
                task_handle: String::new(),
                reason: "create a new task".to_string(),
                priority: 0,
                recommended_action: "new task".to_string(),
            },
            SelectableKind::Inbox(item) => item.clone(),
            SelectableKind::Task(t) => AttentionItem {
                task_id: TaskId::new(t.id.clone()),
                task_handle: t.qualified_handle.clone(),
                reason: t.lifecycle_status.clone(),
                priority: 50,
                recommended_action: "open task".to_string(),
            },
            SelectableKind::Review(t) => AttentionItem {
                task_id: TaskId::new(t.id.clone()),
                task_handle: t.qualified_handle.clone(),
                reason: t.lifecycle_status.clone(),
                priority: 50,
                recommended_action: "review branch".to_string(),
            },
        }
    }
}

fn build_selectables(
    view: &AppView,
    repos: &ReposResponse,
    inbox: &InboxResponse,
    tasks: &TasksResponse,
    review: &TasksResponse,
) -> Vec<SelectableKind> {
    let mut out = Vec::new();
    match view {
        AppView::Projects => {
            out.extend(inbox.items.iter().cloned().map(SelectableKind::Inbox));
            out.extend(repos.repos.iter().cloned().map(SelectableKind::Project));
            out.push(SelectableKind::NewTask);
        }
        AppView::Project { repo } => {
            out.extend(
                inbox
                    .items
                    .iter()
                    .filter(|item| task_handle_repo(&item.task_handle) == Some(repo.as_str()))
                    .cloned()
                    .map(SelectableKind::Inbox),
            );
            out.extend(
                tasks
                    .tasks
                    .iter()
                    .filter(|task| task_summary_repo(task) == Some(repo.as_str()))
                    .cloned()
                    .map(SelectableKind::Task),
            );
            out.extend(
                review
                    .tasks
                    .iter()
                    .filter(|task| task_summary_repo(task) == Some(repo.as_str()))
                    .cloned()
                    .map(SelectableKind::Review),
            );
            out.extend(project_action_selectables(repo));
        }
        AppView::TaskPicker {
            repo,
            recommended_action,
            ..
        } => {
            let source_tasks = if recommended_action == "review branch" {
                &review.tasks
            } else {
                &tasks.tasks
            };
            out.extend(
                source_tasks
                    .iter()
                    .filter(|task| task_summary_repo(task) == Some(repo.as_str()))
                    .cloned()
                    .map(|task| SelectableKind::TaskAction {
                        task,
                        recommended_action: recommended_action.clone(),
                    }),
            );
        }
        AppView::NewTaskInput { .. } => {}
    }
    out
}

fn project_action_selectables(repo: &str) -> Vec<SelectableKind> {
    [
        ("new task", "new task"),
        ("open task", "open task"),
        ("review branch", "review branch"),
        ("merge task", "merge task"),
        ("diff task", "diff task"),
        ("check task", "check task"),
        ("clean task", "clean task"),
        ("repair task", "repair task"),
        ("reconcile", "reconcile"),
        ("status", "status"),
    ]
    .into_iter()
    .map(
        |(label, recommended_action)| SelectableKind::ProjectAction {
            repo: repo.to_string(),
            label: label.to_string(),
            recommended_action: recommended_action.to_string(),
        },
    )
    .collect()
}

fn task_scoped_action(action: &str) -> bool {
    matches!(
        action,
        "open task"
            | "review branch"
            | "check task"
            | "diff task"
            | "merge task"
            | "clean task"
            | "repair task"
    )
}

// ── App state ─────────────────────────────────────────────────────────────────

pub struct App {
    repos: ReposResponse,
    tasks: TasksResponse,
    review: TasksResponse,
    inbox: InboxResponse,
    view: AppView,
    selectables: Vec<SelectableKind>,
    selected: usize,
    viewport_scroll: usize,
    flash: Option<(String, u8)>,
}

const FLASH_TICKS: u8 = 8; // ~2 s at 250 ms poll

impl App {
    pub fn new(
        repos: ReposResponse,
        tasks: TasksResponse,
        review: TasksResponse,
        inbox: InboxResponse,
    ) -> Self {
        let view = AppView::Projects;
        let selectables = build_selectables(&view, &repos, &inbox, &tasks, &review);
        Self {
            repos,
            tasks,
            review,
            inbox,
            view,
            selectables,
            selected: 0,
            viewport_scroll: 0,
            flash: None,
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

    /// Select whichever selectable occupies the given absolute feed row.
    /// No-op if the row falls on a section header / placeholder.
    pub fn select_at_feed_row(&mut self, feed_row: usize) {
        let layout = selectable_row_layout(self);
        if let Some((idx, _)) = layout
            .iter()
            .enumerate()
            .find(|(_, r)| r.contains(&feed_row))
        {
            self.selected = idx;
        }
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
        self.rebuild_selectables();
        true
    }

    /// Erase editable input, then return to the cockpit's main project list.
    /// Returns false at the top level so back never exits the TUI.
    pub fn go_back(&mut self) -> bool {
        if let AppView::NewTaskInput { title, .. } = &mut self.view {
            if !title.is_empty() {
                title.pop();
                return true;
            }
        }

        self.go_home()
    }

    pub fn activate_selected(&mut self) -> Option<AttentionItem> {
        match self.selectables.get(self.selected).cloned()? {
            SelectableKind::Project(repo) => {
                self.view = AppView::Project { repo: repo.name };
                self.selected = 0;
                self.viewport_scroll = 0;
                self.rebuild_selectables();
                None
            }
            SelectableKind::ProjectAction {
                repo,
                label,
                recommended_action,
            } if recommended_action == "new task" => {
                self.view = AppView::NewTaskInput {
                    repo,
                    title: String::new(),
                };
                self.selected = 0;
                self.viewport_scroll = 0;
                self.flash = None;
                self.rebuild_selectables();
                let _ = label;
                None
            }
            SelectableKind::ProjectAction {
                repo,
                label,
                recommended_action,
            } if task_scoped_action(&recommended_action) => {
                self.view = AppView::TaskPicker {
                    repo,
                    label,
                    recommended_action,
                };
                self.selected = 0;
                self.viewport_scroll = 0;
                self.rebuild_selectables();
                None
            }
            SelectableKind::NewTask => {
                if let [repo] = self.repos.repos.as_slice() {
                    self.view = AppView::NewTaskInput {
                        repo: repo.name.clone(),
                        title: String::new(),
                    };
                    self.selected = 0;
                    self.viewport_scroll = 0;
                    self.flash = None;
                    self.rebuild_selectables();
                } else {
                    self.flash("select a project first".to_string());
                }
                None
            }
            selectable => Some(selectable.as_action()),
        }
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
            recommended_action: "new task".to_string(),
            task_title: Some(title.to_string()),
        })
    }

    fn is_collecting_input(&self) -> bool {
        matches!(self.view, AppView::NewTaskInput { .. })
    }

    fn reload(
        &mut self,
        repos: ReposResponse,
        tasks: TasksResponse,
        review: TasksResponse,
        inbox: InboxResponse,
    ) {
        self.repos = repos;
        self.tasks = tasks;
        self.review = review;
        self.inbox = inbox;
        self.rebuild_selectables();
        let max = self.selectables.len().saturating_sub(1);
        self.selected = self.selected.min(max);
    }

    fn rebuild_selectables(&mut self) {
        self.selectables = build_selectables(
            &self.view,
            &self.repos,
            &self.inbox,
            &self.tasks,
            &self.review,
        );
    }

    fn flash(&mut self, msg: String) {
        self.flash = Some((msg, FLASH_TICKS));
    }

    fn tick_flash(&mut self) {
        if let Some((_, ticks)) = &mut self.flash {
            if *ticks == 0 {
                self.flash = None;
            } else {
                *ticks -= 1;
            }
        }
    }

    /// Adjust viewport so the selected item is visible within `viewport_h` rows.
    fn ensure_visible(&mut self, viewport_h: usize) {
        if viewport_h == 0 {
            return;
        }
        let layout = selectable_row_layout(self);
        let Some(range) = layout.get(self.selected).cloned() else {
            return;
        };
        if range.start < self.viewport_scroll {
            self.viewport_scroll = range.start;
        }
        let bottom = self.viewport_scroll + viewport_h;
        if range.end > bottom {
            self.viewport_scroll = range.end.saturating_sub(viewport_h);
        }
    }
}

/// Compute the row range each selectable occupies in the rendered feed,
/// in the same order as `app.selectables`. Must stay in sync with `build_feed`.
fn selectable_row_layout(app: &App) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut row: usize = 1; // top breathing-space row
    let mut prev_group: Option<&'static str> = None;
    for selectable in &app.selectables {
        let group = group_of(selectable);
        if let Some(prev) = prev_group {
            if prev != group {
                row += 1; // blank separator between groups
            }
        }
        out.push(row..row + 1);
        row += 1;
        prev_group = Some(group);
    }
    out
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn run_interactive(
    repos: ReposResponse,
    tasks: TasksResponse,
    review: TasksResponse,
    inbox: InboxResponse,
    on_action: impl FnMut(&AttentionItem) -> io::Result<ActionOutcome>,
) -> io::Result<Option<PendingAction>> {
    let mut stdout = io::stdout();
    let mut terminal_mode = TerminalModeGuard::enter(&mut stdout)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(repos, tasks, review, inbox);
    let result = run_event_loop(&mut terminal, &mut app, on_action);

    terminal_mode.leave(terminal.backend_mut())?;
    terminal.show_cursor()?;

    result
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
enum TerminalModeCommand {
    EnterAlternateScreen,
    EnableMouseCapture,
    LeaveAlternateScreen,
    DisableMouseCapture,
}

#[cfg(test)]
fn terminal_entry_commands() -> &'static [TerminalModeCommand] {
    &[
        TerminalModeCommand::EnterAlternateScreen,
        TerminalModeCommand::EnableMouseCapture,
    ]
}

#[cfg(test)]
fn terminal_exit_commands() -> &'static [TerminalModeCommand] {
    &[
        TerminalModeCommand::LeaveAlternateScreen,
        TerminalModeCommand::DisableMouseCapture,
    ]
}

// ── Event loop ────────────────────────────────────────────────────────────────

fn run_event_loop<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    mut on_action: impl FnMut(&AttentionItem) -> io::Result<ActionOutcome>,
) -> io::Result<Option<PendingAction>> {
    loop {
        let height = terminal
            .size()
            .map_err(|_| io::Error::other("terminal backend size error"))?
            .height as usize;
        let feed_height = height.saturating_sub(2);

        app.tick_flash();
        app.ensure_visible(feed_height);
        terminal
            .draw(|f| render_ui(f, app))
            .map_err(|_| io::Error::other("terminal backend draw error"))?;

        if event::poll(Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Esc => {
                        app.go_home();
                    }
                    KeyCode::Enter if app.is_collecting_input() => {
                        if let Some(pending) = app.submit_input() {
                            return Ok(Some(pending));
                        }
                    }
                    code if app.is_collecting_input()
                        && is_input_delete_key(code, key.modifiers) =>
                    {
                        handle_back_key(app);
                    }
                    KeyCode::Char(character) if app.is_collecting_input() => {
                        app.push_input_char(character);
                    }
                    KeyCode::Left if app.is_collecting_input() => {
                        app.go_back();
                    }
                    KeyCode::Char('q') => return Ok(None),
                    code if is_back_key_event(code, key.modifiers) => {
                        handle_back_key(app);
                    }
                    KeyCode::Up | KeyCode::Char('k') => app.select_prev(),
                    KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                    KeyCode::Enter => {
                        if let Some(item) = app.activate_selected() {
                            match on_action(&item)? {
                                ActionOutcome::Refresh {
                                    repos,
                                    tasks,
                                    review,
                                    inbox,
                                } => app.reload(repos, tasks, review, inbox),
                                ActionOutcome::Defer(pending) => return Ok(Some(pending)),
                                ActionOutcome::Message(msg) => app.flash(msg),
                            }
                        }
                    }
                    _ => {}
                },
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
                }
                _ => {}
            }
        }
    }
}

fn handle_back_key(app: &mut App) -> bool {
    app.go_back();
    false
}

fn is_back_key_event(code: KeyCode, modifiers: KeyModifiers) -> bool {
    matches!(code, KeyCode::Esc | KeyCode::Left | KeyCode::Char('h'))
        || is_navigation_backspace_key(code, modifiers)
}

fn is_navigation_backspace_key(code: KeyCode, modifiers: KeyModifiers) -> bool {
    matches!(
        code,
        KeyCode::Backspace | KeyCode::Char('\u{8}') | KeyCode::Char('\u{7f}')
    ) || matches!(code, KeyCode::Char('h') if modifiers.contains(KeyModifiers::CONTROL))
}

fn is_input_delete_key(code: KeyCode, modifiers: KeyModifiers) -> bool {
    matches!(
        code,
        KeyCode::Backspace | KeyCode::Delete | KeyCode::Char('\u{8}') | KeyCode::Char('\u{7f}')
    ) || matches!(code, KeyCode::Char('h') if modifiers.contains(KeyModifiers::CONTROL))
}

// ── Rendering ─────────────────────────────────────────────────────────────────

fn render_ui(frame: &mut Frame, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(frame.area());

    render_header(frame, app, chunks[0]);
    render_feed(frame, app, chunks[1]);
    render_status_bar(frame, app, chunks[2]);
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let mut parts = vec![Span::styled(
        " Ajax",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )];

    let crumb_sep = || Span::styled(" › ", Style::default().fg(Color::DarkGray));
    let dot_sep = || Span::styled(" · ", Style::default().fg(Color::DarkGray));

    match &app.view {
        AppView::Projects => {
            parts.push(dot_sep());
            parts.push(Span::styled(
                format!("{} repos", app.repos.repos.len()),
                Style::default().fg(Color::White),
            ));
            parts.push(dot_sep());
            parts.push(Span::styled(
                format!("{} tasks", app.tasks.tasks.len()),
                Style::default().fg(Color::White),
            ));
            if !app.review.tasks.is_empty() {
                parts.push(dot_sep());
                parts.push(Span::styled(
                    format!("{} review", app.review.tasks.len()),
                    Style::default().fg(Color::Yellow),
                ));
            }
            if !app.inbox.items.is_empty() {
                parts.push(dot_sep());
                parts.push(Span::styled(
                    format!("{} inbox", app.inbox.items.len()),
                    Style::default().fg(Color::Red),
                ));
            }
        }
        AppView::Project { repo } => {
            parts.push(crumb_sep());
            parts.push(Span::styled(
                repo.clone(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        AppView::TaskPicker { repo, label, .. } => {
            parts.push(crumb_sep());
            parts.push(Span::styled(
                repo.clone(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ));
            parts.push(crumb_sep());
            parts.push(Span::styled(
                label.clone(),
                Style::default().fg(Color::Cyan),
            ));
        }
        AppView::NewTaskInput { repo, .. } => {
            parts.push(crumb_sep());
            parts.push(Span::styled(
                repo.clone(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ));
            parts.push(crumb_sep());
            parts.push(Span::styled("new task", Style::default().fg(Color::Cyan)));
        }
    }

    frame.render_widget(Paragraph::new(Line::from(parts)), area);
}

fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let content = if let Some((msg, _)) = &app.flash {
        Line::from(vec![Span::styled(
            format!(" {msg}"),
            Style::default().fg(Color::Green),
        )])
    } else {
        let mut parts: Vec<Span<'static>> = vec![Span::raw(" ")];
        let push_hint = |parts: &mut Vec<Span<'static>>, key: &str, label: &str, last: bool| {
            parts.push(Span::styled(
                key.to_string(),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));
            parts.push(Span::styled(
                format!(" {label}"),
                Style::default().fg(Color::DarkGray),
            ));
            if !last {
                parts.push(Span::styled(
                    "   ".to_string(),
                    Style::default().fg(Color::DarkGray),
                ));
            }
        };
        let enter_label = match &app.view {
            AppView::Projects => "open",
            AppView::Project { .. } => "act",
            AppView::TaskPicker { .. } => "run",
            AppView::NewTaskInput { .. } => "create",
        };
        let nested = !matches!(app.view, AppView::Projects);
        push_hint(&mut parts, "↑↓", "select", false);
        push_hint(&mut parts, "↵", enter_label, false);
        if nested {
            let back_label = if matches!(app.view, AppView::NewTaskInput { .. }) {
                "erase/back"
            } else {
                "back"
            };
            push_hint(&mut parts, "esc/←/h", back_label, false);
        }
        push_hint(&mut parts, "q", "quit", true);
        Line::from(parts)
    };
    frame.render_widget(Paragraph::new(content), area);
}

fn selected_highlight() -> Style {
    Style::default()
        .bg(Color::Indexed(237))
        .add_modifier(Modifier::BOLD)
}

fn empty_state(text: &str) -> ListItem<'static> {
    ListItem::new(Line::from(vec![Span::styled(
        format!("   {text}"),
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
    )]))
}

fn blank_row() -> ListItem<'static> {
    ListItem::new(Line::from(""))
}

fn task_handle_repo(handle: &str) -> Option<&str> {
    handle.split_once('/').map(|(repo, _)| repo)
}

fn task_summary_repo(task: &TaskSummary) -> Option<&str> {
    task_handle_repo(&task.qualified_handle)
}

fn group_of(kind: &SelectableKind) -> &'static str {
    match kind {
        SelectableKind::Inbox(_) => "hot",
        SelectableKind::Project(_) => "projects",
        SelectableKind::Task(_) | SelectableKind::Review(_) | SelectableKind::TaskAction { .. } => {
            "tasks"
        }
        SelectableKind::NewTask | SelectableKind::ProjectAction { .. } => "actions",
    }
}

fn task_glyph(status: &str, needs_attention: bool) -> Span<'static> {
    if needs_attention {
        return Span::styled("⚡", Style::default().fg(Color::Red));
    }
    if status.contains("Active") {
        Span::styled("●", Style::default().fg(Color::Green))
    } else if status.contains("Reviewable") || status.contains("Mergeable") {
        Span::styled("✓", Style::default().fg(Color::Yellow))
    } else if status.contains("Error") || status.contains("Orphaned") {
        Span::styled("⚠", Style::default().fg(Color::Red))
    } else if status.contains("Waiting") {
        Span::styled("◔", Style::default().fg(Color::Blue))
    } else {
        Span::styled("○", Style::default().fg(Color::DarkGray))
    }
}

fn task_status_label(task: &TaskSummary) -> String {
    task.live_status
        .as_ref()
        .map(|status| status.summary.clone())
        .unwrap_or_else(|| task.lifecycle_status.clone())
}

fn project_glyph(repo: &RepoSummary) -> Span<'static> {
    if repo.broken_tasks > 0 {
        Span::styled("⚠", Style::default().fg(Color::Red))
    } else if repo.reviewable_tasks > 0 {
        Span::styled("✓", Style::default().fg(Color::Yellow))
    } else if repo.active_tasks > 0 {
        Span::styled("●", Style::default().fg(Color::Green))
    } else {
        Span::styled("○", Style::default().fg(Color::DarkGray))
    }
}

fn inbox_glyph(priority: u32) -> Span<'static> {
    let color = if priority < 20 {
        Color::Red
    } else if priority < 50 {
        Color::Yellow
    } else {
        Color::Cyan
    };
    Span::styled("⚡", Style::default().fg(color))
}

fn action_glyph(recommended_action: &str) -> Span<'static> {
    let (glyph, color) = match recommended_action {
        "new task" => ("+", Color::Green),
        "open task" => ("▸", Color::Cyan),
        "review branch" => ("✓", Color::Yellow),
        "merge task" => ("⇄", Color::Yellow),
        "diff task" => ("◇", Color::DarkGray),
        "check task" => ("⊙", Color::DarkGray),
        "clean task" => ("✕", Color::DarkGray),
        "repair task" => ("⚒", Color::DarkGray),
        "reconcile" => ("⟳", Color::DarkGray),
        "status" => ("?", Color::DarkGray),
        _ => ("·", Color::DarkGray),
    };
    Span::styled(glyph, Style::default().fg(color))
}

fn priority_accent(priority: u32) -> Color {
    if priority < 20 {
        Color::Red
    } else if priority < 50 {
        Color::Yellow
    } else {
        Color::White
    }
}

fn project_subtitle(repo: &RepoSummary) -> String {
    let mut parts = Vec::new();
    if repo.active_tasks > 0 {
        parts.push(format!("{} active", repo.active_tasks));
    }
    if repo.reviewable_tasks > 0 {
        parts.push(format!("{} review", repo.reviewable_tasks));
    }
    if repo.broken_tasks > 0 {
        parts.push(format!("{} broken", repo.broken_tasks));
    }
    if parts.is_empty() {
        "idle".to_string()
    } else {
        parts.join(" · ")
    }
}

fn render_row(glyph: Span<'static>, mut spans: Vec<Span<'static>>) -> ListItem<'static> {
    let mut all = vec![Span::raw("   "), glyph, Span::raw("  ")];
    all.append(&mut spans);
    ListItem::new(Line::from(all))
}

fn render_selectable(s: &SelectableKind) -> ListItem<'static> {
    let bold_white = Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(Color::DarkGray);
    let cyan = Style::default().fg(Color::Cyan);
    match s {
        SelectableKind::Inbox(item) => render_row(
            inbox_glyph(item.priority),
            vec![
                Span::styled(format!("{:<22}", item.task_handle), bold_white),
                Span::styled(
                    item.reason.clone(),
                    Style::default().fg(priority_accent(item.priority)),
                ),
                Span::styled("  →  ", dim),
                Span::styled(item.recommended_action.clone(), cyan),
            ],
        ),
        SelectableKind::Project(repo) => render_row(
            project_glyph(repo),
            vec![
                Span::styled(format!("{:<20}", repo.name), bold_white),
                Span::styled(project_subtitle(repo), dim),
            ],
        ),
        SelectableKind::NewTask => render_row(
            action_glyph("new task"),
            vec![Span::styled(
                "start a new task",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )],
        ),
        SelectableKind::ProjectAction {
            label,
            recommended_action,
            ..
        } => {
            let primary = matches!(
                recommended_action.as_str(),
                "new task" | "open task" | "review branch" | "merge task"
            );
            let label_style = if primary {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            render_row(
                action_glyph(recommended_action),
                vec![Span::styled(label.clone(), label_style)],
            )
        }
        SelectableKind::TaskAction {
            task,
            recommended_action,
        } => render_row(
            task_glyph(&task.lifecycle_status, false),
            vec![
                Span::styled(
                    format!("{:<28}", task.qualified_handle),
                    Style::default().fg(Color::White),
                ),
                Span::styled(recommended_action.clone(), cyan),
            ],
        ),
        SelectableKind::Task(t) => render_row(
            task_glyph(&t.lifecycle_status, t.needs_attention),
            vec![
                Span::styled(
                    format!("{:<28}", t.qualified_handle),
                    Style::default().fg(Color::White),
                ),
                Span::styled(task_status_label(t), dim),
            ],
        ),
        SelectableKind::Review(t) => render_row(
            task_glyph(&t.lifecycle_status, false),
            vec![
                Span::styled(
                    format!("{:<28}", t.qualified_handle),
                    Style::default().fg(Color::White),
                ),
                Span::styled(task_status_label(t), dim),
            ],
        ),
    }
}

fn build_feed(app: &App, _width: usize) -> (Vec<ListItem<'static>>, Vec<usize>) {
    let mut rows: Vec<ListItem<'static>> = Vec::new();
    let mut sel_to_row: Vec<usize> = Vec::new();

    rows.push(blank_row());

    if let AppView::NewTaskInput { title, .. } = &app.view {
        let display_title = if title.is_empty() {
            "<type a task name>".to_string()
        } else {
            title.clone()
        };
        rows.push(render_row(
            action_glyph("new task"),
            vec![
                Span::styled(
                    "Task name  ",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(display_title, Style::default().fg(Color::Cyan)),
            ],
        ));
        return (rows, sel_to_row);
    }

    if app.selectables.is_empty() {
        let msg = match &app.view {
            AppView::Projects => "no projects yet · edit ~/.config/ajax/config.toml to add one",
            AppView::Project { .. } => "nothing here yet · esc/←/h to go back",
            AppView::TaskPicker { .. } => "no matching tasks · esc/←/h to go back",
            AppView::NewTaskInput { .. } => "enter a task name",
        };
        rows.push(empty_state(msg));
        return (rows, sel_to_row);
    }

    let mut prev_group: Option<&'static str> = None;
    for selectable in &app.selectables {
        let group = group_of(selectable);
        if let Some(prev) = prev_group {
            if prev != group {
                rows.push(blank_row());
            }
        }
        sel_to_row.push(rows.len());
        rows.push(render_selectable(selectable));
        prev_group = Some(group);
    }

    (rows, sel_to_row)
}

fn render_feed(frame: &mut Frame, app: &App, area: Rect) {
    let width = area.width as usize;
    let (items, sel_to_row) = build_feed(app, width);
    let visible: Vec<ListItem> = items.into_iter().skip(app.viewport_scroll).collect();

    let mut state = ListState::default();
    if let Some(&row) = sel_to_row.get(app.selected) {
        if row >= app.viewport_scroll {
            state.select(Some(row - app.viewport_scroll));
        }
    }

    let list = List::new(visible)
        .block(Block::default())
        .highlight_style(selected_highlight());
    frame.render_stateful_widget(list, area, &mut state);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        render_cockpit, render_ui, selectable_row_layout, App, AppView, TerminalModeCommand,
    };
    use ajax_core::{
        models::{AttentionItem, LiveObservation, LiveStatusKind, TaskId},
        output::{InboxResponse, RepoSummary, ReposResponse, TaskSummary, TasksResponse},
    };
    use crossterm::event::{KeyCode, KeyModifiers};
    use ratatui::{backend::TestBackend, Terminal};

    fn sample_repos() -> ReposResponse {
        ReposResponse {
            repos: vec![RepoSummary {
                name: "web".to_string(),
                path: "/Users/matt/projects/web".to_string(),
                active_tasks: 1,
                reviewable_tasks: 1,
                cleanable_tasks: 0,
                broken_tasks: 0,
            }],
        }
    }

    fn sample_tasks() -> TasksResponse {
        TasksResponse {
            tasks: vec![TaskSummary {
                id: "task-1".to_string(),
                qualified_handle: "web/fix-login".to_string(),
                title: "Fix login".to_string(),
                lifecycle_status: "Active".to_string(),
                needs_attention: true,
                live_status: None,
            }],
        }
    }

    fn sample_inbox() -> InboxResponse {
        InboxResponse {
            items: vec![AttentionItem {
                task_id: TaskId::new("task-1"),
                task_handle: "web/fix-login".to_string(),
                reason: "agent needs input".to_string(),
                priority: 10,
                recommended_action: "open task".to_string(),
            }],
        }
    }

    #[test]
    fn task_rows_render_live_status_when_present() {
        let mut tasks = sample_tasks();
        tasks.tasks[0].live_status = Some(LiveObservation::new(
            LiveStatusKind::WaitingForApproval,
            "waiting for approval",
        ));
        let mut app = App::new(
            sample_repos(),
            tasks,
            TasksResponse { tasks: vec![] },
            InboxResponse { items: vec![] },
        );
        app.activate_selected();

        let content = render_to_string(80, 30, &app);

        assert!(content.contains("web/fix-login"));
        assert!(content.contains("waiting for approval"));
        assert!(!content.contains("Active"));
    }

    fn render_to_string(width: u16, height: u16, app: &App) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render_ui(f, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn cockpit_renders_backend_snapshot() {
        let repos = sample_repos();
        let tasks = sample_tasks();
        let inbox = sample_inbox();
        let rendered = render_cockpit(&repos, &tasks, &tasks, &inbox);
        assert!(rendered.contains("Ajax Cockpit"));
        assert!(rendered.contains("Repos: 1"));
        assert!(rendered.contains("Review: 1"));
        assert!(rendered.contains("web/fix-login: agent needs input -> open task"));
    }

    #[test]
    fn feed_inbox_appears_before_tasks() {
        // In the Project view, inbox rows precede task rows.
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        // Projects view: [inbox, project, NewTask]. Drill into the project.
        app.select_next();
        app.activate_selected();
        let content = render_to_string(80, 30, &app);
        let inbox_pos = content.find("agent needs input").unwrap();
        let task_pos = content.find("Active").unwrap();
        assert!(inbox_pos < task_pos);
    }

    #[test]
    fn feed_starts_with_inbox_then_projects() {
        let repos = ReposResponse {
            repos: vec![
                RepoSummary {
                    name: "autodoctor".to_string(),
                    path: "/Users/matt/Desktop/Projects/autodoctor".to_string(),
                    active_tasks: 1,
                    reviewable_tasks: 0,
                    cleanable_tasks: 0,
                    broken_tasks: 0,
                },
                RepoSummary {
                    name: "autosnooze".to_string(),
                    path: "/Users/matt/Desktop/Projects/autosnooze".to_string(),
                    active_tasks: 0,
                    reviewable_tasks: 1,
                    cleanable_tasks: 0,
                    broken_tasks: 0,
                },
            ],
        };
        let app = App::new(repos, sample_tasks(), sample_tasks(), sample_inbox());

        let content = render_to_string(80, 30, &app);
        let inbox_pos = content.find("agent needs input").unwrap();
        let autodoctor_pos = content.find("autodoctor").unwrap();
        let autosnooze_pos = content.find("autosnooze").unwrap();

        // Inbox precedes both projects.
        assert!(inbox_pos < autodoctor_pos);
        assert!(inbox_pos < autosnooze_pos);
        // Initial selection is the inbox item.
        assert_eq!(
            app.selected_action().unwrap().recommended_action,
            "open task"
        );
    }

    #[test]
    fn activating_project_opens_project_workflow() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        // Projects view: [inbox, project, NewTask]. Skip the inbox to reach the project.
        app.select_next();
        assert!(app.activate_selected().is_none());

        let content = render_to_string(80, 30, &app);
        // Header now shows a breadcrumb instead of a "Project: web" title.
        assert!(content.contains("› web"));
        assert!(content.contains("web/fix-login"));
    }

    #[test]
    fn top_level_back_stays_in_cockpit() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );

        assert!(!super::handle_back_key(&mut app));
        let content = render_to_string(80, 30, &app);
        assert!(content.contains("Ajax"));
        assert!(content.contains("web"));
    }

    #[test]
    fn top_level_backspace_stays_in_cockpit() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );

        assert!(super::is_back_key_event(
            KeyCode::Backspace,
            KeyModifiers::NONE
        ));
        assert!(!super::handle_back_key(&mut app));
        let content = render_to_string(80, 30, &app);
        assert!(content.contains("Ajax"));
        assert!(content.contains("web"));
    }

    #[test]
    fn top_level_back_variants_stay_in_cockpit() {
        for (code, modifiers) in [
            (KeyCode::Backspace, KeyModifiers::NONE),
            (KeyCode::Char('\u{8}'), KeyModifiers::NONE),
            (KeyCode::Char('\u{7f}'), KeyModifiers::NONE),
            (KeyCode::Char('h'), KeyModifiers::CONTROL),
        ] {
            let mut app = App::new(
                sample_repos(),
                sample_tasks(),
                sample_tasks(),
                sample_inbox(),
            );

            assert!(super::is_back_key_event(code, modifiers));
            assert!(!super::handle_back_key(&mut app));
            let content = render_to_string(80, 30, &app);
            assert!(content.contains("Ajax"));
            assert!(content.contains("web"));
        }
    }

    #[test]
    fn terminal_entry_uses_only_unambiguous_tui_modes() {
        assert_eq!(
            super::terminal_entry_commands(),
            &[
                TerminalModeCommand::EnterAlternateScreen,
                TerminalModeCommand::EnableMouseCapture
            ]
        );
    }

    #[test]
    fn terminal_exit_restores_tui_modes() {
        assert_eq!(
            super::terminal_exit_commands(),
            &[
                TerminalModeCommand::LeaveAlternateScreen,
                TerminalModeCommand::DisableMouseCapture
            ]
        );
    }

    #[test]
    fn nested_back_returns_to_parent_without_exit() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        app.select_next();
        app.activate_selected();

        assert!(!super::handle_back_key(&mut app));
        let content = render_to_string(80, 30, &app);
        assert!(!content.contains("› web"));
    }

    #[test]
    fn task_picker_back_returns_to_ajax_main_menu() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        app.select_next();
        app.activate_selected();
        app.select_next();
        app.select_next();
        app.select_next();
        app.select_next();
        app.activate_selected();

        assert!(!super::handle_back_key(&mut app));
        let content = render_to_string(80, 30, &app);
        assert!(content.contains("Ajax"));
        assert!(content.contains("start a new task"));
        assert!(!content.contains("› web"));
        assert!(!content.contains("› open task"));
    }

    #[test]
    fn nested_backspace_returns_to_parent_without_exit() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        app.select_next();
        app.activate_selected();

        assert!(super::is_back_key_event(
            KeyCode::Backspace,
            KeyModifiers::NONE
        ));
        assert!(!super::handle_back_key(&mut app));
        let content = render_to_string(80, 30, &app);
        assert!(!content.contains("› web"));
    }

    #[test]
    fn immediate_back_keys_do_not_depend_on_escape() {
        for key in [
            KeyCode::Left,
            KeyCode::Backspace,
            KeyCode::Char('h'),
            KeyCode::Esc,
        ] {
            assert!(
                super::is_back_key_event(key, KeyModifiers::NONE),
                "{key:?} should navigate back"
            );
        }
    }

    #[test]
    fn navigation_back_accepts_common_terminal_encodings() {
        for (code, modifiers) in [
            (KeyCode::Left, KeyModifiers::NONE),
            (KeyCode::Backspace, KeyModifiers::NONE),
            (KeyCode::Esc, KeyModifiers::NONE),
            (KeyCode::Char('\u{8}'), KeyModifiers::NONE),
            (KeyCode::Char('\u{7f}'), KeyModifiers::NONE),
            (KeyCode::Char('h'), KeyModifiers::NONE),
            (KeyCode::Char('h'), KeyModifiers::CONTROL),
        ] {
            assert!(
                super::is_back_key_event(code, modifiers),
                "{code:?} with {modifiers:?} should navigate back"
            );
        }

        assert!(!super::is_back_key_event(
            KeyCode::Char('x'),
            KeyModifiers::NONE
        ));
    }

    #[test]
    fn delete_is_not_a_cockpit_navigation_key() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        app.select_next();
        app.activate_selected();
        assert!(matches!(&app.view, AppView::Project { repo } if repo == "web"));
        let selected_before = app.selected;
        let before = render_to_string(80, 30, &app);

        assert!(!super::is_back_key_event(
            KeyCode::Delete,
            KeyModifiers::NONE
        ));
        assert!(before.contains("› web"));

        let after = render_to_string(80, 30, &app);
        assert_eq!(before, after);
        assert!(matches!(&app.view, AppView::Project { repo } if repo == "web"));
        assert_eq!(app.selected, selected_before);
        assert!(after.contains("› web"));
    }

    #[test]
    fn delete_on_top_level_is_ignored_by_navigation() {
        let app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        let before = render_to_string(80, 30, &app);

        assert!(!super::is_back_key_event(
            KeyCode::Delete,
            KeyModifiers::NONE
        ));

        let after = render_to_string(80, 30, &app);
        assert_eq!(before, after);
        assert!(after.contains("Ajax"));
        assert!(after.contains("start a new task"));
    }

    #[test]
    fn input_delete_accepts_common_terminal_encodings() {
        for (code, modifiers) in [
            (KeyCode::Backspace, KeyModifiers::NONE),
            (KeyCode::Delete, KeyModifiers::NONE),
            (KeyCode::Char('\u{8}'), KeyModifiers::NONE),
            (KeyCode::Char('\u{7f}'), KeyModifiers::NONE),
            (KeyCode::Char('h'), KeyModifiers::CONTROL),
        ] {
            assert!(
                super::is_input_delete_key(code, modifiers),
                "{code:?} with {modifiers:?} should erase input"
            );
        }

        assert!(!super::is_input_delete_key(
            KeyCode::Char('h'),
            KeyModifiers::NONE
        ));
    }

    #[test]
    fn delete_in_task_title_input_erases_without_closing_ajax() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        app.select_next();
        app.activate_selected();
        app.select_next();
        app.select_next();
        app.select_next();
        app.activate_selected();
        app.push_input_char('x');
        assert!(
            matches!(
                &app.view,
                AppView::NewTaskInput { repo, title } if repo == "web" && title == "x"
            ),
            "Delete regression setup should be editing a web task title"
        );

        assert!(super::is_input_delete_key(
            KeyCode::Delete,
            KeyModifiers::NONE
        ));
        assert!(!super::handle_back_key(&mut app));
        assert!(
            matches!(
                &app.view,
                AppView::NewTaskInput { repo, title } if repo == "web" && title.is_empty()
            ),
            "Delete should erase editable text without leaving task title input"
        );

        let content = render_to_string(80, 30, &app);
        assert!(content.contains("› new task"));
        assert!(content.contains("Task name"));
        assert!(content.contains("<type a task name>"));
        assert!(!content.contains("Task name  x"));
    }

    #[test]
    fn nested_views_advertise_immediate_back_keys() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        app.select_next();
        app.activate_selected();

        let content = render_to_string(80, 30, &app);
        assert!(content.contains("esc/←/h back"));
    }

    #[test]
    fn project_workflow_shows_action_menu() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        // Drill into the project (inbox → project → NewTask, so step past inbox).
        app.select_next();
        app.activate_selected();

        let content = render_to_string(80, 30, &app);
        for expected in [
            "new task",
            "open task",
            "review branch",
            "merge task",
            "diff task",
            "check task",
            "clean task",
            "repair task",
            "reconcile",
            "status",
        ] {
            assert!(content.contains(expected), "missing {expected}");
        }
        // No "Back" entry in the new design — esc handles back navigation.
        assert!(!content.contains("Back"));
        // First selectable on the Project view is the (filtered) inbox row.
        let item = app.selected_action().unwrap();
        assert_eq!(item.task_handle, "web/fix-login");
        assert_eq!(item.recommended_action, "open task");
    }

    #[test]
    fn selected_project_only_shows_that_projects_tasks() {
        let repos = ReposResponse {
            repos: vec![
                RepoSummary {
                    name: "web".to_string(),
                    path: "/Users/matt/Desktop/Projects/web".to_string(),
                    active_tasks: 1,
                    reviewable_tasks: 0,
                    cleanable_tasks: 0,
                    broken_tasks: 0,
                },
                RepoSummary {
                    name: "api".to_string(),
                    path: "/Users/matt/Desktop/Projects/api".to_string(),
                    active_tasks: 1,
                    reviewable_tasks: 0,
                    cleanable_tasks: 0,
                    broken_tasks: 0,
                },
            ],
        };
        let tasks = TasksResponse {
            tasks: vec![
                TaskSummary {
                    id: "task-1".to_string(),
                    qualified_handle: "web/fix-login".to_string(),
                    title: "Fix login".to_string(),
                    lifecycle_status: "Active".to_string(),
                    needs_attention: true,
                    live_status: None,
                },
                TaskSummary {
                    id: "task-2".to_string(),
                    qualified_handle: "api/add-cache".to_string(),
                    title: "Add cache".to_string(),
                    lifecycle_status: "Active".to_string(),
                    needs_attention: false,
                    live_status: None,
                },
            ],
        };
        let inbox = InboxResponse {
            items: vec![
                AttentionItem {
                    task_id: TaskId::new("task-1"),
                    task_handle: "web/fix-login".to_string(),
                    reason: "agent needs input".to_string(),
                    priority: 10,
                    recommended_action: "open task".to_string(),
                },
                AttentionItem {
                    task_id: TaskId::new("task-2"),
                    task_handle: "api/add-cache".to_string(),
                    reason: "stale task".to_string(),
                    priority: 60,
                    recommended_action: "open task".to_string(),
                },
            ],
        };
        let mut app = App::new(repos, tasks.clone(), tasks, inbox);
        // Selectables: [inbox web, inbox api, project web, project api, NewTask].
        // Step past both inbox rows and the web project to land on the api project.
        app.select_next();
        app.select_next();
        app.select_next();
        app.activate_selected();

        let content = render_to_string(100, 50, &app);
        assert!(content.contains("› api"));
        assert!(content.contains("api/add-cache"));
        assert!(!content.contains("web/fix-login"));
        assert!(!content.contains("agent needs input"));
    }

    #[test]
    fn project_task_action_opens_scoped_task_picker() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        // Projects view: [inbox, project, NewTask]. Drill into the project.
        app.select_next();
        app.activate_selected();
        // Project view: [inbox, task, review, new task, open task, ...].
        // Walk to the "open task" project action (index 4).
        app.select_next();
        app.select_next();
        app.select_next();
        app.select_next();

        assert!(app.activate_selected().is_none());

        let content = render_to_string(80, 30, &app);
        // Breadcrumb now reads "Ajax › web › open task".
        assert!(content.contains("› open task"));
        assert!(content.contains("web/fix-login"));
        let item = app.selected_action().unwrap();
        assert_eq!(item.task_handle, "web/fix-login");
        assert_eq!(item.recommended_action, "open task");
    }

    #[test]
    fn project_new_task_action_opens_title_input() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        app.select_next();
        app.activate_selected();
        app.select_next();
        app.select_next();
        app.select_next();

        assert!(app.activate_selected().is_none());

        let content = render_to_string(80, 30, &app);
        assert!(content.contains("› new task"));
        assert!(content.contains("Task name"));
    }

    #[test]
    fn top_level_new_task_action_opens_title_input_for_single_repo() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        app.select_next();
        app.select_next();

        assert!(app.activate_selected().is_none());
        assert!(
            matches!(
                &app.view,
                AppView::NewTaskInput { repo, title } if repo == "web" && title.is_empty()
            ),
            "top-level new task should collect a task name when there is one repo"
        );

        let content = render_to_string(80, 30, &app);
        assert!(content.contains("› new task"));
        assert!(content.contains("Task name"));
    }

    #[test]
    fn new_task_title_input_collects_text_before_pending_action() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        app.select_next();
        app.activate_selected();
        app.select_next();
        app.select_next();
        app.select_next();
        app.activate_selected();

        assert!(app.submit_input().is_none());
        app.push_input_char('F');
        app.push_input_char('i');
        app.push_input_char('x');

        let pending = app.submit_input().unwrap();

        assert_eq!(pending.task_handle, "web");
        assert_eq!(pending.recommended_action, "new task");
        assert_eq!(pending.task_title.as_deref(), Some("Fix"));
    }

    #[test]
    fn new_task_title_backspace_edits_then_returns_to_main_menu() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        app.select_next();
        app.activate_selected();
        app.select_next();
        app.select_next();
        app.select_next();
        app.activate_selected();

        app.push_input_char('x');
        assert!(!super::handle_back_key(&mut app));
        assert!(
            matches!(
                &app.view,
                AppView::NewTaskInput { repo, title } if repo == "web" && title.is_empty()
            ),
            "first backspace should edit the task title without leaving input"
        );
        assert!(render_to_string(80, 30, &app).contains("Task name"));
        assert!(!super::handle_back_key(&mut app));
        assert!(matches!(app.view, AppView::Projects));
        assert_eq!(app.selected, 0);

        let content = render_to_string(80, 30, &app);
        assert!(content.contains("Ajax"));
        assert!(content.contains("start a new task"));
        assert!(!content.contains("› web"));
        assert!(!content.contains("› new task"));
    }

    #[test]
    fn escape_from_new_task_input_returns_to_ajax_main_menu() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        app.select_next();
        app.activate_selected();
        app.select_next();
        app.select_next();
        app.select_next();
        app.activate_selected();
        app.push_input_char('x');

        assert!(app.go_home());

        let content = render_to_string(80, 30, &app);
        assert!(content.contains("Ajax"));
        assert!(content.contains("start a new task"));
        assert!(!content.contains("› web"));
        assert!(!content.contains("› new task"));
        assert!(!content.contains("Task name"));
    }

    #[test]
    fn feed_inbox_items_render_handle_reason_and_action() {
        let app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        let content = render_to_string(80, 30, &app);
        assert!(content.contains("web/fix-login"));
        assert!(content.contains("agent needs input"));
        assert!(content.contains("open task"));
    }

    #[test]
    fn interactive_cockpit_renders_to_narrow_buffer() {
        let app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        let content = render_to_string(50, 24, &app);
        assert!(content.contains("Ajax"));
        assert!(content.contains("web/fix-login"));
        assert!(content.contains("agent needs input"));
    }

    #[test]
    fn select_prev_clamps_at_zero() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        app.select_prev();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn select_next_walks_inbox_project_newtask() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        // Projects view: [inbox, project, NewTask] = 3 selectables.
        assert_eq!(app.selected, 0);
        app.select_next();
        assert_eq!(app.selected, 1);
        app.select_next();
        assert_eq!(app.selected, 2);
        // clamps at last
        app.select_next();
        assert_eq!(app.selected, 2);
    }

    #[test]
    fn select_at_feed_row_lands_on_correct_selectable() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        // Layout on Projects view (no headers, blank row between groups):
        //   0 blank (top breathing space)
        //   1 inbox     ← selectable 0
        //   2 blank (hot → projects)
        //   3 project   ← selectable 1
        //   4 blank (projects → actions)
        //   5 NewTask   ← selectable 2
        app.select_at_feed_row(1);
        assert_eq!(app.selected, 0);
        app.select_at_feed_row(3);
        assert_eq!(app.selected, 1);
        app.select_at_feed_row(5);
        assert_eq!(app.selected, 2);
        // blank separator row → no change
        app.select_at_feed_row(2);
        assert_eq!(app.selected, 2);
    }

    #[test]
    fn new_task_is_always_present_even_when_other_sections_empty() {
        let mut app = App::new(
            sample_repos(),
            TasksResponse { tasks: vec![] },
            TasksResponse { tasks: vec![] },
            InboxResponse { items: vec![] },
        );
        app.select_next();
        let item = app.selected_action().unwrap();
        assert_eq!(item.recommended_action, "new task");
    }

    #[test]
    fn selected_action_for_inbox_uses_recommended_action() {
        let app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        // Projects view: [inbox, project, NewTask] — inbox is the initial selection.
        let item = app.selected_action().unwrap();
        assert_eq!(item.task_handle, "web/fix-login");
        assert_eq!(item.recommended_action, "open task");
    }

    #[test]
    fn selected_action_for_task_synthesizes_open_task() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            InboxResponse { items: vec![] },
        );
        // Projects view (no inbox): [project, NewTask]. Drill into the project.
        app.activate_selected();
        // Project view (no inbox): [task, review, ...10 actions].
        let item = app.selected_action().unwrap();
        assert_eq!(item.task_handle, "web/fix-login");
        assert_eq!(item.recommended_action, "open task");
        app.select_next();
        let item = app.selected_action().unwrap();
        assert_eq!(item.recommended_action, "review branch");
    }

    #[test]
    fn reload_updates_app_data_and_clamps_selection() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        app.selected = 99;
        app.reload(
            sample_repos(),
            TasksResponse { tasks: vec![] },
            TasksResponse { tasks: vec![] },
            InboxResponse { items: vec![] },
        );
        // project and NewTask remain → clamped to NewTask.
        assert_eq!(app.selected, 1);
        assert_eq!(
            app.selected_action().unwrap().recommended_action,
            "new task"
        );
    }

    #[test]
    fn ensure_visible_scrolls_viewport_to_selected() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        // Projects view: [inbox, project, NewTask] — walk to the bottom selectable.
        app.select_next();
        app.select_next();
        app.ensure_visible(2);
        let layout = selectable_row_layout(&app);
        let range = layout[app.selected].clone();
        assert!(app.viewport_scroll <= range.start);
        assert!(range.end <= app.viewport_scroll + 2);
    }

    #[test]
    fn on_action_message_outcome_sets_flash() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        app.flash("done".to_string());
        assert!(app.flash.is_some());
    }
}

#![deny(unsafe_op_in_unsafe_fn)]

use ajax_core::{
    models::{AttentionItem, TaskId},
    output::{InboxResponse, RepoSummary, ReposResponse, TaskSummary, TasksResponse},
};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseEventKind,
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
}

impl SelectableKind {
    /// Synthesize an `AttentionItem` for the dispatch callback. Inbox items
    /// pass through unchanged; tasks and review entries get default actions
    /// that the CLI dispatcher already handles ("open task", "review branch").
    /// The synthetic NewTask entry uses the "new task" action, which the CLI
    /// dispatcher matches on to invoke `commands::new_task_plan`.
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
            out.extend(repos.repos.iter().cloned().map(SelectableKind::Project));
            out.push(SelectableKind::NewTask);
            out.extend(inbox.items.iter().cloned().map(SelectableKind::Inbox));
            out.extend(tasks.tasks.iter().cloned().map(SelectableKind::Task));
            out.extend(review.tasks.iter().cloned().map(SelectableKind::Review));
        }
        AppView::Project { repo } => {
            out.extend(project_action_selectables(repo));
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
    }
    out
}

fn project_action_selectables(repo: &str) -> Vec<SelectableKind> {
    [
        ("+ New task", "new task"),
        ("Open task", "open task"),
        ("Review branch", "review branch"),
        ("Check task", "check task"),
        ("Diff task", "diff task"),
        ("Merge task", "merge task"),
        ("Clean task", "clean task"),
        ("Repair task", "repair task"),
        ("Reconcile", "reconcile"),
        ("Status", "status"),
        ("Back", "back"),
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

    /// Pop one level of view nesting. Returns true when the view changed,
    /// false at the top level (Projects) so the caller can decide whether to quit.
    pub fn go_back(&mut self) -> bool {
        match &self.view {
            AppView::Projects => false,
            AppView::Project { .. } => {
                self.view = AppView::Projects;
                self.selected = 0;
                self.viewport_scroll = 0;
                self.rebuild_selectables();
                true
            }
            AppView::TaskPicker { repo, .. } => {
                let repo = repo.clone();
                self.view = AppView::Project { repo };
                self.selected = 0;
                self.viewport_scroll = 0;
                self.rebuild_selectables();
                true
            }
        }
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
                recommended_action, ..
            } if recommended_action == "back" => {
                self.view = AppView::Projects;
                self.selected = 0;
                self.viewport_scroll = 0;
                self.rebuild_selectables();
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
            selectable => Some(selectable.as_action()),
        }
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
    let mut row: usize = 0;

    match &app.view {
        AppView::Projects => {
            row += 1; // header
            for _ in &app.repos.repos {
                out.push(row..row + 1);
                row += 1;
            }
        }
        AppView::Project { .. } => {}
        AppView::TaskPicker { .. } => {
            row += 1; // header
            for _ in &app.selectables {
                out.push(row..row + 1);
                row += 1;
            }
            return out;
        }
    }

    row += 1; // header
    let action_count = match &app.view {
        AppView::Projects => 1,
        AppView::Project { repo } => project_action_selectables(repo).len(),
        AppView::TaskPicker { .. } => 0,
    };
    for _ in 0..action_count {
        out.push(row..row + 1);
        row += 1;
    }

    // Inbox
    row += 1; // header
    let inbox_items = visible_inbox_items(app);
    if inbox_items.is_empty() {
        row += 1; // placeholder
    } else {
        for _ in inbox_items {
            out.push(row..row + 1);
            row += 1;
        }
    }

    // Tasks
    row += 1;
    let tasks = visible_tasks(app);
    if tasks.is_empty() {
        row += 1;
    } else {
        for _ in tasks {
            out.push(row..row + 1);
            row += 1;
        }
    }

    // Review
    row += 1;
    for _ in visible_review_tasks(app) {
        out.push(row..row + 1);
        row += 1;
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
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(repos, tasks, review, inbox);
    let result = run_event_loop(&mut terminal, &mut app, on_action);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

// ── Event loop ────────────────────────────────────────────────────────────────

fn run_event_loop<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    mut on_action: impl FnMut(&AttentionItem) -> io::Result<ActionOutcome>,
) -> io::Result<Option<PendingAction>> {
    loop {
        let height = terminal.size()?.height as usize;
        let feed_height = height.saturating_sub(2);

        app.tick_flash();
        app.ensure_visible(feed_height);
        terminal.draw(|f| render_ui(f, app))?;

        if event::poll(Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char('q') => return Ok(None),
                    KeyCode::Esc => {
                        app.go_back();
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
        };
        let nested = !matches!(app.view, AppView::Projects);
        push_hint(&mut parts, "↑↓", "select", false);
        push_hint(&mut parts, "↵", enter_label, false);
        if nested {
            push_hint(&mut parts, "esc", "back", false);
        }
        push_hint(&mut parts, "q", "quit", true);
        Line::from(parts)
    };
    frame.render_widget(Paragraph::new(content), area);
}

fn section_header(label: &str, width: usize) -> ListItem<'static> {
    let prefix = format!("── {label} ");
    let prefix_chars = prefix.chars().count();
    let pad = width.saturating_sub(prefix_chars);
    let mut line = prefix;
    line.extend(std::iter::repeat_n('─', pad));
    ListItem::new(Line::from(vec![Span::styled(
        line,
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::DIM),
    )]))
}

fn selected_highlight() -> Style {
    Style::default()
        .bg(Color::Indexed(237))
        .add_modifier(Modifier::BOLD)
}

fn empty_state(text: &str) -> ListItem<'static> {
    ListItem::new(Line::from(vec![Span::styled(
        format!("  {text}"),
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
    )]))
}

fn lifecycle_color(status: &str) -> Color {
    if status.contains("Active") {
        Color::Green
    } else if status.contains("Reviewable") || status.contains("Mergeable") {
        Color::Yellow
    } else if status.contains("Error") || status.contains("Orphaned") {
        Color::Red
    } else if status.contains("Waiting") {
        Color::Blue
    } else {
        Color::DarkGray
    }
}

fn priority_color(priority: u32) -> Color {
    if priority < 20 {
        Color::Red
    } else if priority < 50 {
        Color::Yellow
    } else {
        Color::White
    }
}

/// Two-character left gutter: a colored bar when selected, blank otherwise.
fn selection_gutter(is_selected: bool, accent: Color) -> Span<'static> {
    if is_selected {
        Span::styled(
            "▌ ",
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw("  ")
    }
}

/// Tap-shortcut badge `[1]`–`[9]` for the first 9 selectables, blank padding past 9.
/// Keeping width fixed at 4 chars so handles align across rows.
fn item_badge(sel_idx: usize) -> Span<'static> {
    let n = sel_idx + 1;
    if n <= 9 {
        Span::styled(
            format!("[{n}] "),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw("    ")
    }
}

fn task_handle_repo(handle: &str) -> Option<&str> {
    handle.split_once('/').map(|(repo, _)| repo)
}

fn task_summary_repo(task: &TaskSummary) -> Option<&str> {
    task_handle_repo(&task.qualified_handle)
}

fn visible_inbox_items(app: &App) -> Vec<&AttentionItem> {
    app.inbox
        .items
        .iter()
        .filter(|item| match &app.view {
            AppView::Projects => true,
            AppView::Project { repo } => task_handle_repo(&item.task_handle) == Some(repo.as_str()),
            AppView::TaskPicker { .. } => false,
        })
        .collect()
}

fn visible_tasks(app: &App) -> Vec<&TaskSummary> {
    app.tasks
        .tasks
        .iter()
        .filter(|task| match &app.view {
            AppView::Projects => true,
            AppView::Project { repo } => task_summary_repo(task) == Some(repo.as_str()),
            AppView::TaskPicker { .. } => false,
        })
        .collect()
}

fn visible_review_tasks(app: &App) -> Vec<&TaskSummary> {
    app.review
        .tasks
        .iter()
        .filter(|task| match &app.view {
            AppView::Projects => true,
            AppView::Project { repo } => task_summary_repo(task) == Some(repo.as_str()),
            AppView::TaskPicker { .. } => false,
        })
        .collect()
}

fn action_row(sel_idx: usize, is_selected: bool, label: &str) -> ListItem<'static> {
    ListItem::new(Line::from(vec![
        item_badge(sel_idx),
        selection_gutter(is_selected, Color::Green),
        Span::styled(
            label.to_string(),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
    ]))
}

fn build_feed(app: &App, width: usize) -> (Vec<ListItem<'static>>, Vec<usize>) {
    let mut rows: Vec<ListItem<'static>> = Vec::new();
    let mut sel_to_row: Vec<usize> = Vec::new();
    let mut sel_idx: usize = 0;

    if let AppView::TaskPicker {
        label,
        recommended_action,
        ..
    } = &app.view
    {
        rows.push(section_header(&format!("Choose task · {label}"), width));
        if app.selectables.is_empty() {
            rows.push(empty_state("no matching tasks · esc to go back"));
        } else {
            for selectable in &app.selectables {
                let SelectableKind::TaskAction { task, .. } = selectable else {
                    continue;
                };
                let is_selected = app.selected == sel_idx;
                let color = lifecycle_color(&task.lifecycle_status);
                sel_to_row.push(rows.len());
                rows.push(ListItem::new(Line::from(vec![
                    item_badge(sel_idx),
                    selection_gutter(is_selected, color),
                    Span::styled(
                        format!("{:<28}", task.qualified_handle),
                        Style::default().fg(Color::White),
                    ),
                    Span::styled(recommended_action.clone(), Style::default().fg(Color::Cyan)),
                ])));
                sel_idx += 1;
            }
        }
        return (rows, sel_to_row);
    }

    // ── Projects ─────────────────────────────────────────────────────────────
    match &app.view {
        AppView::Projects => {
            rows.push(section_header(
                &format!("Projects ({})", app.repos.repos.len()),
                width,
            ));

            if app.repos.repos.is_empty() {
                rows.push(empty_state(
                    "no projects configured · edit ~/.config/ajax/config.toml to add one",
                ));
            } else {
                for repo in &app.repos.repos {
                    let is_selected = app.selected == sel_idx;
                    let accent = if repo.broken_tasks > 0 {
                        Color::Red
                    } else if repo.reviewable_tasks > 0 {
                        Color::Yellow
                    } else {
                        Color::Cyan
                    };
                    sel_to_row.push(rows.len());
                    rows.push(ListItem::new(Line::from(vec![
                        item_badge(sel_idx),
                        selection_gutter(is_selected, accent),
                        Span::styled(
                            format!("{:<22}", repo.name),
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!(
                                "{} active  {} review  {} broken",
                                repo.active_tasks, repo.reviewable_tasks, repo.broken_tasks
                            ),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ])));
                    sel_idx += 1;
                }
            }
        }
        AppView::Project { repo } => {
            rows.push(section_header(&format!("Project: {repo}"), width));
        }
        AppView::TaskPicker { .. } => {}
    }

    rows.push(section_header("Actions", width));
    match &app.view {
        AppView::Projects => {
            sel_to_row.push(rows.len());
            rows.push(action_row(sel_idx, app.selected == sel_idx, "+ New task"));
            sel_idx += 1;
        }
        AppView::Project { repo } => {
            for action in project_action_selectables(repo) {
                let SelectableKind::ProjectAction { label, .. } = action else {
                    continue;
                };
                sel_to_row.push(rows.len());
                rows.push(action_row(sel_idx, app.selected == sel_idx, &label));
                sel_idx += 1;
            }
        }
        AppView::TaskPicker { .. } => {}
    }

    // ── Inbox ────────────────────────────────────────────────────────────────
    let inbox_items = visible_inbox_items(app);
    rows.push(section_header(
        &format!("Inbox ({})", inbox_items.len()),
        width,
    ));

    if inbox_items.is_empty() {
        rows.push(empty_state("all caught up · no tasks need attention"));
    } else {
        for item in inbox_items {
            let is_selected = app.selected == sel_idx;
            let accent = priority_color(item.priority);
            sel_to_row.push(rows.len());
            rows.push(ListItem::new(Line::from(vec![
                item_badge(sel_idx),
                selection_gutter(is_selected, accent),
                Span::styled(
                    format!("{:<22}", item.task_handle),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(item.reason.clone(), Style::default().fg(accent)),
                Span::styled("  →  ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    item.recommended_action.clone(),
                    Style::default().fg(Color::Cyan),
                ),
            ])));
            sel_idx += 1;
        }
    }

    // ── Tasks ─────────────────────────────────────────────────────────────────
    let tasks = visible_tasks(app);
    rows.push(section_header(&format!("Tasks ({})", tasks.len()), width));

    if tasks.is_empty() {
        rows.push(empty_state(
            "no active tasks · pick + New task above to start one",
        ));
    } else {
        for t in tasks {
            let is_selected = app.selected == sel_idx;
            let color = lifecycle_color(&t.lifecycle_status);
            let flag = if t.needs_attention { " ⚑" } else { "" };
            sel_to_row.push(rows.len());
            rows.push(ListItem::new(Line::from(vec![
                item_badge(sel_idx),
                selection_gutter(is_selected, color),
                Span::styled(
                    format!("{:<28}", t.qualified_handle),
                    Style::default().fg(Color::White),
                ),
                Span::styled(t.lifecycle_status.clone(), Style::default().fg(color)),
                Span::styled(flag.to_string(), Style::default().fg(Color::Red)),
            ])));
            sel_idx += 1;
        }
    }

    // ── Review ────────────────────────────────────────────────────────────────
    let review_tasks = visible_review_tasks(app);
    rows.push(section_header(
        &format!("Review ({})", review_tasks.len()),
        width,
    ));

    if review_tasks.is_empty() {
        rows.push(empty_state(
            "nothing waiting for review · finished tasks land here",
        ));
    } else {
        for t in review_tasks {
            let is_selected = app.selected == sel_idx;
            let color = lifecycle_color(&t.lifecycle_status);
            sel_to_row.push(rows.len());
            rows.push(ListItem::new(Line::from(vec![
                item_badge(sel_idx),
                selection_gutter(is_selected, color),
                Span::styled(
                    format!("{:<28}", t.qualified_handle),
                    Style::default().fg(Color::White),
                ),
                Span::styled(t.lifecycle_status.clone(), Style::default().fg(color)),
            ])));
            sel_idx += 1;
        }
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
    use super::{render_cockpit, render_ui, selectable_row_layout, App};
    use ajax_core::{
        models::{AttentionItem, TaskId},
        output::{InboxResponse, RepoSummary, ReposResponse, TaskSummary, TasksResponse},
    };
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
        let app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        let content = render_to_string(60, 30, &app);
        let inbox_pos = content.find("Inbox").unwrap();
        let tasks_pos = content.find("Tasks").unwrap();
        assert!(inbox_pos < tasks_pos);
    }

    #[test]
    fn feed_starts_with_configured_projects() {
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
        let projects_pos = content.find("Projects").unwrap();
        let inbox_pos = content.find("Inbox").unwrap();
        let autodoctor_pos = content.find("autodoctor").unwrap();
        let autosnooze_pos = content.find("autosnooze").unwrap();

        assert!(projects_pos < inbox_pos);
        assert!(autodoctor_pos < inbox_pos);
        assert!(autosnooze_pos < inbox_pos);
        assert_eq!(
            app.selected_action().unwrap().recommended_action,
            "select project"
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

        assert!(app.activate_selected().is_none());

        let content = render_to_string(80, 30, &app);
        assert!(content.contains("Project: web"));
        assert!(content.contains("web/fix-login"));
    }

    #[test]
    fn project_workflow_shows_gum_style_action_menu() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        app.activate_selected();

        let content = render_to_string(80, 30, &app);
        for expected in [
            "+ New task",
            "Open task",
            "Review branch",
            "Check task",
            "Diff task",
            "Merge task",
            "Clean task",
            "Repair task",
            "Reconcile",
            "Status",
            "Back",
        ] {
            assert!(content.contains(expected), "missing {expected}");
        }
        let item = app.selected_action().unwrap();
        assert_eq!(item.task_handle, "web");
        assert_eq!(item.recommended_action, "new task");
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
                },
                TaskSummary {
                    id: "task-2".to_string(),
                    qualified_handle: "api/add-cache".to_string(),
                    title: "Add cache".to_string(),
                    lifecycle_status: "Active".to_string(),
                    needs_attention: false,
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
        app.select_next();
        app.activate_selected();

        let content = render_to_string(100, 50, &app);
        assert!(content.contains("Project: api"));
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
        app.activate_selected();
        app.select_next();

        assert!(app.activate_selected().is_none());

        let content = render_to_string(80, 30, &app);
        assert!(content.contains("Choose task · Open task"));
        assert!(content.contains("web/fix-login"));
        let item = app.selected_action().unwrap();
        assert_eq!(item.task_handle, "web/fix-login");
        assert_eq!(item.recommended_action, "open task");
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
        assert!(content.contains("Inbox"));
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
    fn select_next_walks_projects_actions_inbox_tasks_review() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        // [project, NewTask, inbox, task, review] = 5 selectables.
        assert_eq!(app.selected, 0);
        app.select_next();
        assert_eq!(app.selected, 1);
        app.select_next();
        assert_eq!(app.selected, 2);
        app.select_next();
        assert_eq!(app.selected, 3);
        app.select_next();
        assert_eq!(app.selected, 4);
        // clamps at last
        app.select_next();
        assert_eq!(app.selected, 4);
    }

    #[test]
    fn select_at_feed_row_lands_on_correct_selectable() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        // Layout (rows):
        //   0 Projects header
        //   1 project             ← selectable 0
        //   2 Actions header
        //   3 + New task          ← selectable 1
        //   4 Inbox header
        //   5 inbox               ← selectable 2
        //   6 Tasks header
        //   7 task                ← selectable 3
        //   8 Review header
        //   9 review              ← selectable 4
        app.select_at_feed_row(1);
        assert_eq!(app.selected, 0);
        app.select_at_feed_row(5);
        assert_eq!(app.selected, 2);
        app.select_at_feed_row(7);
        assert_eq!(app.selected, 3);
        app.select_at_feed_row(9);
        assert_eq!(app.selected, 4);
        // header row → no change
        app.select_at_feed_row(6);
        assert_eq!(app.selected, 4);
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
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        // skip the project and NewTask selectables
        app.select_next();
        app.select_next();
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
        // selectables: [project, NewTask, task, review]
        app.select_next();
        app.select_next();
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
        // walk down past project, NewTask, inbox, task, onto review
        app.select_next();
        app.select_next();
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

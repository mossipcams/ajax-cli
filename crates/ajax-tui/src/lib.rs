#![deny(unsafe_op_in_unsafe_fn)]

use ajax_core::{
    models::{AttentionItem, TaskId},
    output::{InboxResponse, ReposResponse, TaskSummary, TasksResponse},
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
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, Paragraph},
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
    /// Synthetic top-of-feed entry. Dispatched as a "new task" action.
    NewTask,
    Inbox(AttentionItem),
    Task(TaskSummary),
    Review(TaskSummary),
}

impl SelectableKind {
    /// Synthesize an `AttentionItem` for the dispatch callback. Inbox items
    /// pass through unchanged; tasks and review entries get default actions
    /// that the CLI dispatcher already handles ("open task", "review branch").
    /// The synthetic NewTask entry uses the "new task" action, which the CLI
    /// dispatcher matches on to invoke `commands::new_task_plan`.
    fn as_action(&self) -> AttentionItem {
        match self {
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
    inbox: &InboxResponse,
    tasks: &TasksResponse,
    review: &TasksResponse,
) -> Vec<SelectableKind> {
    let mut out = Vec::new();
    out.push(SelectableKind::NewTask);
    out.extend(inbox.items.iter().cloned().map(SelectableKind::Inbox));
    out.extend(tasks.tasks.iter().cloned().map(SelectableKind::Task));
    out.extend(review.tasks.iter().cloned().map(SelectableKind::Review));
    out
}

// ── App state ─────────────────────────────────────────────────────────────────

pub struct App {
    repos: ReposResponse,
    tasks: TasksResponse,
    review: TasksResponse,
    inbox: InboxResponse,
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
        let selectables = build_selectables(&inbox, &tasks, &review);
        Self {
            repos,
            tasks,
            review,
            inbox,
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
        self.selectables = build_selectables(&self.inbox, &self.tasks, &self.review);
        let max = self.selectables.len().saturating_sub(1);
        self.selected = self.selected.min(max);
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

    // Actions (always one synthetic NewTask entry)
    row += 1; // header
    out.push(row..row + 1);
    row += 1;

    // Inbox
    row += 1; // header
    if app.inbox.items.is_empty() {
        row += 1; // placeholder
    } else {
        for _ in &app.inbox.items {
            out.push(row..row + 2);
            row += 2;
        }
    }

    // Tasks
    row += 1;
    if app.tasks.tasks.is_empty() {
        row += 1;
    } else {
        for _ in &app.tasks.tasks {
            out.push(row..row + 1);
            row += 1;
        }
    }

    // Review
    row += 1;
    for _ in &app.review.tasks {
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
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(None),
                    KeyCode::Up | KeyCode::Char('k') => app.select_prev(),
                    KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                    KeyCode::Enter => {
                        if let Some(item) = app.selected_action() {
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
                                let feed_row =
                                    mouse_row - feed_top + app.viewport_scroll;
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

fn render_header(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let mut parts = vec![
        Span::styled(
            " Ajax",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" · ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{} repos", app.repos.repos.len()),
            Style::default().fg(Color::White),
        ),
        Span::styled(" · ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{} tasks", app.tasks.tasks.len()),
            Style::default().fg(Color::White),
        ),
    ];
    if !app.review.tasks.is_empty() {
        parts.push(Span::styled(" · ", Style::default().fg(Color::DarkGray)));
        parts.push(Span::styled(
            format!("{} review", app.review.tasks.len()),
            Style::default().fg(Color::Yellow),
        ));
    }
    if !app.inbox.items.is_empty() {
        parts.push(Span::styled(" · ", Style::default().fg(Color::DarkGray)));
        parts.push(Span::styled(
            format!("{} inbox", app.inbox.items.len()),
            Style::default().fg(Color::Red),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(parts)), area);
}

fn render_status_bar(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let content = if let Some((msg, _)) = &app.flash {
        Line::from(vec![Span::styled(
            format!(" {msg}"),
            Style::default().fg(Color::Green),
        )])
    } else {
        Line::from(vec![
            Span::styled(" tap", Style::default().fg(Color::Yellow)),
            Span::raw(":select  "),
            Span::styled("↵", Style::default().fg(Color::Yellow)),
            Span::raw(":act  "),
            Span::styled("q", Style::default().fg(Color::Yellow)),
            Span::raw(":quit"),
        ])
    };
    frame.render_widget(Paragraph::new(content), area);
}

fn section_header(label: &str) -> ListItem<'static> {
    let label = label.to_owned();
    ListItem::new(Line::from(vec![Span::styled(
        format!("── {label} "),
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::DIM),
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
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw("    ")
    }
}

fn continuation_pad() -> Span<'static> {
    Span::raw("    ")
}

fn build_feed(app: &App) -> Vec<ListItem<'static>> {
    let mut rows: Vec<ListItem<'static>> = Vec::new();
    let mut sel_idx: usize = 0;

    // ── Actions ──────────────────────────────────────────────────────────────
    rows.push(section_header("Actions"));
    {
        let is_selected = app.selected == sel_idx;
        let label_style = if is_selected {
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        };
        rows.push(ListItem::new(Line::from(vec![
            item_badge(sel_idx),
            selection_gutter(is_selected, Color::Green),
            Span::styled("+ New task", label_style),
        ])));
        sel_idx += 1;
    }

    // ── Inbox ────────────────────────────────────────────────────────────────
    rows.push(section_header(&format!(
        "Inbox ({})",
        app.inbox.items.len()
    )));

    if app.inbox.items.is_empty() {
        rows.push(ListItem::new(Line::from(vec![Span::styled(
            "  no tasks need attention",
            Style::default().fg(Color::DarkGray),
        )])));
    } else {
        for item in &app.inbox.items {
            let is_selected = app.selected == sel_idx;
            let accent = priority_color(item.priority);
            let handle_style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            };
            rows.push(ListItem::new(Line::from(vec![
                item_badge(sel_idx),
                selection_gutter(is_selected, accent),
                Span::styled(item.task_handle.clone(), handle_style),
            ])));
            rows.push(ListItem::new(Line::from(vec![
                continuation_pad(),
                selection_gutter(is_selected, accent),
                Span::raw("  "),
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
    rows.push(section_header(&format!(
        "Tasks ({})",
        app.tasks.tasks.len()
    )));

    if app.tasks.tasks.is_empty() {
        rows.push(ListItem::new(Line::from(vec![Span::styled(
            "  no active tasks",
            Style::default().fg(Color::DarkGray),
        )])));
    } else {
        for t in &app.tasks.tasks {
            let is_selected = app.selected == sel_idx;
            let color = lifecycle_color(&t.lifecycle_status);
            let flag = if t.needs_attention { " ⚑" } else { "" };
            let handle_style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::REVERSED)
            } else {
                Style::default().fg(Color::White)
            };
            rows.push(ListItem::new(Line::from(vec![
                item_badge(sel_idx),
                selection_gutter(is_selected, color),
                Span::styled(format!("{:<28}", t.qualified_handle), handle_style),
                Span::styled(t.lifecycle_status.clone(), Style::default().fg(color)),
                Span::styled(flag.to_string(), Style::default().fg(Color::Red)),
            ])));
            sel_idx += 1;
        }
    }

    // ── Review ────────────────────────────────────────────────────────────────
    rows.push(section_header(&format!(
        "Review ({})",
        app.review.tasks.len()
    )));

    if app.review.tasks.is_empty() {
        rows.push(ListItem::new(Line::from(vec![Span::styled(
            "  no tasks ready for review",
            Style::default().fg(Color::DarkGray),
        )])));
    } else {
        for t in &app.review.tasks {
            let is_selected = app.selected == sel_idx;
            let color = lifecycle_color(&t.lifecycle_status);
            let handle_style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::REVERSED)
            } else {
                Style::default().fg(Color::White)
            };
            rows.push(ListItem::new(Line::from(vec![
                item_badge(sel_idx),
                selection_gutter(is_selected, color),
                Span::styled(format!("{:<28}", t.qualified_handle), handle_style),
                Span::styled(t.lifecycle_status.clone(), Style::default().fg(color)),
            ])));
            sel_idx += 1;
        }
    }

    rows
}

fn render_feed(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let visible: Vec<ListItem> = build_feed(app)
        .into_iter()
        .skip(app.viewport_scroll)
        .collect();
    let list = List::new(visible).block(Block::default());
    frame.render_widget(list, area);
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
    fn feed_inbox_items_rendered_as_two_rows() {
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
    fn select_next_walks_actions_inbox_tasks_review() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        // [NewTask, inbox, task, review] = 4 selectables, NewTask at idx 0
        assert_eq!(app.selected, 0);
        app.select_next();
        assert_eq!(app.selected, 1);
        app.select_next();
        assert_eq!(app.selected, 2);
        app.select_next();
        assert_eq!(app.selected, 3);
        // clamps at last
        app.select_next();
        assert_eq!(app.selected, 3);
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
        //   0 Actions header
        //   1 + New task          ← selectable 0
        //   2 Inbox header
        //   3 inbox line 1        ← selectable 1
        //   4 inbox line 2          (same selectable 1)
        //   5 Tasks header
        //   6 task                ← selectable 2
        //   7 Review header
        //   8 review              ← selectable 3
        app.select_at_feed_row(1);
        assert_eq!(app.selected, 0);
        app.select_at_feed_row(4); // inbox second line
        assert_eq!(app.selected, 1);
        app.select_at_feed_row(6);
        assert_eq!(app.selected, 2);
        app.select_at_feed_row(8);
        assert_eq!(app.selected, 3);
        // header row → no change
        app.select_at_feed_row(5);
        assert_eq!(app.selected, 3);
    }

    #[test]
    fn new_task_is_always_present_even_when_other_sections_empty() {
        let app = App::new(
            sample_repos(),
            TasksResponse { tasks: vec![] },
            TasksResponse { tasks: vec![] },
            InboxResponse { items: vec![] },
        );
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
        // skip the NewTask selectable
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
        // selectables: [NewTask, task, review]
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
        // only NewTask remains → clamped to 0
        assert_eq!(app.selected, 0);
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
        // walk down past NewTask, inbox, task, onto review
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

#![deny(unsafe_op_in_unsafe_fn)]

use ajax_core::{
    models::AttentionItem,
    output::{InboxResponse, ReposResponse, TasksResponse},
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
use std::{io, time::Duration};

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

// ── App state ─────────────────────────────────────────────────────────────────

pub struct App {
    repos: ReposResponse,
    tasks: TasksResponse,
    review: TasksResponse,
    inbox: InboxResponse,
    scroll: usize,
    flash: Option<(String, u8)>, // (message, ticks remaining)
}

const FLASH_TICKS: u8 = 8; // ~2 s at 250 ms poll

impl App {
    pub fn new(
        repos: ReposResponse,
        tasks: TasksResponse,
        review: TasksResponse,
        inbox: InboxResponse,
    ) -> Self {
        Self {
            repos,
            tasks,
            review,
            inbox,
            scroll: 0,
            flash: None,
        }
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn scroll_down(&mut self, viewport_rows: usize) {
        let total = self.feed_len();
        if total > viewport_rows {
            self.scroll = (self.scroll + 1).min(total - viewport_rows);
        }
    }

    /// Which inbox item is "selected" given the current scroll position.
    /// Inbox item i occupies rows (1 + i*2) and (2 + i*2) in the feed;
    /// row 0 is the Inbox section header.
    pub fn selected_inbox(&self) -> Option<usize> {
        if self.inbox.items.is_empty() {
            return None;
        }
        let idx = self.scroll.saturating_sub(1) / 2;
        Some(idx.min(self.inbox.items.len() - 1))
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
        // clamp scroll after reload in case data shrank
        let max = self.feed_len().saturating_sub(1);
        self.scroll = self.scroll.min(max);
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

    fn feed_len(&self) -> usize {
        let inbox_rows = 1 + if self.inbox.items.is_empty() {
            1
        } else {
            self.inbox.items.len() * 2
        };
        let tasks_rows = 1 + self.tasks.tasks.len().max(1);
        let review_rows = 1 + self.review.tasks.len().max(1);
        inbox_rows + tasks_rows + review_rows
    }
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
        terminal.draw(|f| render_ui(f, app))?;

        if event::poll(Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(None),
                    KeyCode::Up | KeyCode::Char('k') => app.scroll_up(),
                    KeyCode::Down | KeyCode::Char('j') => app.scroll_down(feed_height),
                    KeyCode::Enter => {
                        if let Some(idx) = app.selected_inbox() {
                            let item = app.inbox.items[idx].clone();
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
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::ScrollUp => app.scroll_up(),
                    MouseEventKind::ScrollDown => app.scroll_down(feed_height),
                    _ => {}
                },
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
            Span::styled(" scroll", Style::default().fg(Color::Yellow)),
            Span::raw(":navigate  "),
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

fn build_feed(app: &App) -> Vec<ListItem<'static>> {
    let mut rows: Vec<ListItem<'static>> = Vec::new();
    let selected = app.selected_inbox();

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
        for (i, item) in app.inbox.items.iter().enumerate() {
            let is_selected = selected == Some(i);
            let bullet = if is_selected { "●" } else { "○" };
            let priority_color = if item.priority < 20 {
                Color::Red
            } else if item.priority < 50 {
                Color::Yellow
            } else {
                Color::White
            };
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
            // Line 1: bullet + handle (reversed when selected)
            rows.push(ListItem::new(Line::from(vec![
                Span::styled(format!(" {bullet} "), Style::default().fg(priority_color)),
                Span::styled(item.task_handle.clone(), handle_style),
            ])));
            // Line 2: indented reason → action
            rows.push(ListItem::new(Line::from(vec![
                Span::raw("     "),
                Span::styled(item.reason.clone(), Style::default().fg(priority_color)),
                Span::styled("  →  ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    item.recommended_action.clone(),
                    Style::default().fg(Color::Cyan),
                ),
            ])));
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
            let flag = if t.needs_attention { " ⚑" } else { "" };
            let color = lifecycle_color(&t.lifecycle_status);
            rows.push(ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{:<28}", t.qualified_handle),
                    Style::default().fg(Color::White),
                ),
                Span::styled(t.lifecycle_status.clone(), Style::default().fg(color)),
                Span::styled(flag.to_string(), Style::default().fg(Color::Red)),
            ])));
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
            let color = lifecycle_color(&t.lifecycle_status);
            rows.push(ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{:<28}", t.qualified_handle),
                    Style::default().fg(Color::White),
                ),
                Span::styled(t.lifecycle_status.clone(), Style::default().fg(color)),
            ])));
        }
    }

    rows
}

fn render_feed(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let visible: Vec<ListItem> = build_feed(app).into_iter().skip(app.scroll).collect();
    let list = List::new(visible).block(Block::default());
    frame.render_widget(list, area);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{render_cockpit, render_ui, App};
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
    fn scroll_up_clamps_at_zero() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        app.scroll_up();
        assert_eq!(app.scroll, 0);
    }

    #[test]
    fn scroll_down_advances_when_feed_exceeds_viewport() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        app.scroll_down(3);
        assert!(app.scroll > 0);
    }

    #[test]
    fn scroll_down_does_not_advance_when_feed_fits() {
        let mut app = App::new(
            sample_repos(),
            TasksResponse { tasks: vec![] },
            TasksResponse { tasks: vec![] },
            InboxResponse { items: vec![] },
        );
        let before = app.scroll;
        app.scroll_down(100);
        assert_eq!(app.scroll, before);
    }

    #[test]
    fn selected_inbox_tracks_scroll_position() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            InboxResponse {
                items: vec![
                    AttentionItem {
                        task_id: TaskId::new("t1"),
                        task_handle: "web/a".to_string(),
                        reason: "r".to_string(),
                        priority: 10,
                        recommended_action: "open task".to_string(),
                    },
                    AttentionItem {
                        task_id: TaskId::new("t2"),
                        task_handle: "web/b".to_string(),
                        reason: "r".to_string(),
                        priority: 10,
                        recommended_action: "clean task".to_string(),
                    },
                ],
            },
        );
        assert_eq!(app.selected_inbox(), Some(0));
        // scroll to item 1's handle row (row 3 in the feed)
        app.scroll = 3;
        assert_eq!(app.selected_inbox(), Some(1));
    }

    #[test]
    fn reload_updates_app_data_and_clamps_scroll() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        app.scroll = 99;
        app.reload(
            sample_repos(),
            TasksResponse { tasks: vec![] },
            TasksResponse { tasks: vec![] },
            InboxResponse { items: vec![] },
        );
        // feed shrank, scroll must be clamped
        assert!(app.scroll < 99);
    }

    #[test]
    fn on_action_message_outcome_sets_flash() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        // simulate what the event loop does with a Message outcome
        app.flash("done".to_string());
        assert!(app.flash.is_some());
    }
}

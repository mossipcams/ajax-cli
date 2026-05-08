#![deny(unsafe_op_in_unsafe_fn)]

use ajax_core::output::{InboxResponse, ReposResponse, TasksResponse};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
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

// ── Text renderer (watch mode and tests) ─────────────────────────────────────

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

// ── Interactive TUI ───────────────────────────────────────────────────────────

pub struct App {
    repos: ReposResponse,
    tasks: TasksResponse,
    review: TasksResponse,
    inbox: InboxResponse,
    scroll: usize,
}

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
        }
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn scroll_down(&mut self, max_rows: usize) {
        let total = self.feed_len();
        if total > max_rows {
            self.scroll = (self.scroll + 1).min(total - max_rows);
        }
    }

    // Total number of rows the feed produces (for scroll clamping).
    fn feed_len(&self) -> usize {
        // section header + items (inbox items are 2 rows each)
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

pub fn run_interactive(
    repos: ReposResponse,
    tasks: TasksResponse,
    review: TasksResponse,
    inbox: InboxResponse,
) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(repos, tasks, review, inbox);
    let result = run_event_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

fn run_event_loop<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> io::Result<()> {
    loop {
        let height = terminal.size()?.height as usize;
        // subtract header (1) + status bar (1)
        let feed_height = height.saturating_sub(2);

        terminal.draw(|f| render_ui(f, app))?;

        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Up | KeyCode::Char('k') => app.scroll_up(),
                        KeyCode::Down | KeyCode::Char('j') => app.scroll_down(feed_height),
                        _ => {}
                    }
                }
            }
        }
    }
}

fn render_ui(frame: &mut Frame, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(frame.area());

    render_header(frame, app, chunks[0]);
    render_feed(frame, app, chunks[1]);
    render_status_bar(frame, chunks[2]);
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

fn render_status_bar(frame: &mut Frame, area: ratatui::layout::Rect) {
    let bar = Paragraph::new(Line::from(vec![
        Span::styled(" j/k", Style::default().fg(Color::Yellow)),
        Span::raw(":scroll  "),
        Span::styled("q", Style::default().fg(Color::Yellow)),
        Span::raw(":quit"),
    ]));
    frame.render_widget(bar, area);
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
            let bullet = if i == app.scroll { "●" } else { "○" };
            let priority_color = if item.priority < 20 {
                Color::Red
            } else if item.priority < 50 {
                Color::Yellow
            } else {
                Color::White
            };
            // Line 1: bullet + handle
            rows.push(ListItem::new(Line::from(vec![
                Span::styled(format!(" {} ", bullet), Style::default().fg(priority_color)),
                Span::styled(
                    item.task_handle.clone(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
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
        assert!(
            inbox_pos < tasks_pos,
            "Inbox section should appear before Tasks section"
        );
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

        assert!(
            content.contains("web/fix-login"),
            "inbox handle should appear in feed"
        );
        assert!(
            content.contains("agent needs input"),
            "inbox reason should appear in feed"
        );
        assert!(
            content.contains("open task"),
            "inbox action should appear in feed"
        );
    }

    #[test]
    fn interactive_cockpit_renders_to_narrow_buffer() {
        let app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        // Simulate a narrow mobile terminal (50 cols)
        let backend = TestBackend::new(50, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|f| render_ui(f, &app)).unwrap();

        let content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();

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
        // feed_len > 3, so scrolling with a 3-row viewport should advance
        app.scroll_down(3);
        assert!(app.scroll > 0, "scroll should advance when feed > viewport");
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
        // feed is short — scrolling with a large viewport should be a no-op
        app.scroll_down(100);
        assert_eq!(app.scroll, before);
    }
}

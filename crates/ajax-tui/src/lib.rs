#![deny(unsafe_op_in_unsafe_fn)]

use ajax_core::output::{InboxResponse, ReposResponse, TasksResponse};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Layout, Rect},
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Panel {
    Tasks,
    Review,
    Inbox,
}

pub struct App {
    repos: ReposResponse,
    tasks: TasksResponse,
    review: TasksResponse,
    inbox: InboxResponse,
    focused: Panel,
    task_scroll: usize,
    review_scroll: usize,
    inbox_scroll: usize,
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
            focused: Panel::Tasks,
            task_scroll: 0,
            review_scroll: 0,
            inbox_scroll: 0,
        }
    }

    pub fn cycle_focus(&mut self) {
        self.focused = match self.focused {
            Panel::Tasks => Panel::Review,
            Panel::Review => Panel::Inbox,
            Panel::Inbox => Panel::Tasks,
        };
    }

    pub fn scroll_up(&mut self) {
        let scroll = self.focused_scroll_mut();
        *scroll = scroll.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        let len = self.focused_len();
        let scroll = self.focused_scroll_mut();
        *scroll = (*scroll + 1).min(len.saturating_sub(1));
    }

    fn focused_scroll_mut(&mut self) -> &mut usize {
        match self.focused {
            Panel::Tasks => &mut self.task_scroll,
            Panel::Review => &mut self.review_scroll,
            Panel::Inbox => &mut self.inbox_scroll,
        }
    }

    fn focused_len(&self) -> usize {
        match self.focused {
            Panel::Tasks => self.tasks.tasks.len(),
            Panel::Review => self.review.tasks.len(),
            Panel::Inbox => self.inbox.items.len(),
        }
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
        terminal.draw(|f| render_ui(f, app))?;

        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Tab => app.cycle_focus(),
                        KeyCode::Up | KeyCode::Char('k') => app.scroll_up(),
                        KeyCode::Down | KeyCode::Char('j') => app.scroll_down(),
                        _ => {}
                    }
                }
            }
        }
    }
}

fn render_ui(frame: &mut Frame, app: &App) {
    let outer = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(frame.area());

    render_title(frame, outer[0]);
    render_content(frame, app, outer[1]);
    render_status_bar(frame, outer[2]);
}

fn render_title(frame: &mut Frame, area: Rect) {
    let title = Paragraph::new(Line::from(vec![Span::styled(
        " Ajax Cockpit",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )]));
    frame.render_widget(title, area);
}

fn render_status_bar(frame: &mut Frame, area: Rect) {
    let bar = Paragraph::new(Line::from(vec![
        Span::styled(" Tab", Style::default().fg(Color::Yellow)),
        Span::raw(":panel  "),
        Span::styled("↑↓", Style::default().fg(Color::Yellow)),
        Span::raw(":scroll  "),
        Span::styled("q", Style::default().fg(Color::Yellow)),
        Span::raw(":quit"),
    ]));
    frame.render_widget(bar, area);
}

fn render_content(frame: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Percentage(40),
        Constraint::Percentage(20),
        Constraint::Percentage(40),
    ])
    .split(area);

    let top_cols =
        Layout::horizontal([Constraint::Percentage(35), Constraint::Percentage(65)]).split(rows[0]);

    render_repos(frame, app, top_cols[0]);
    render_tasks(frame, app, top_cols[1]);
    render_review(frame, app, rows[1]);
    render_inbox(frame, app, rows[2]);
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

fn focus_border_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn render_repos(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .repos
        .repos
        .iter()
        .map(|r| {
            let mut spans = vec![Span::styled(
                format!(" {:<14}", r.name),
                Style::default().fg(Color::White),
            )];
            if r.active_tasks > 0 {
                spans.push(Span::styled(
                    format!("{} active", r.active_tasks),
                    Style::default().fg(Color::Green),
                ));
            }
            if r.reviewable_tasks > 0 {
                spans.push(Span::styled(
                    format!("  {} review", r.reviewable_tasks),
                    Style::default().fg(Color::Yellow),
                ));
            }
            if r.broken_tasks > 0 {
                spans.push(Span::styled(
                    format!("  {} broken", r.broken_tasks),
                    Style::default().fg(Color::Red),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let block = Block::bordered()
        .title(format!(" Repos ({}) ", app.repos.repos.len()))
        .border_style(Style::default().fg(Color::DarkGray));
    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn render_tasks(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focused == Panel::Tasks;
    let items: Vec<ListItem> = app
        .tasks
        .tasks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let bullet = if i == app.task_scroll && focused {
                "▶"
            } else {
                " "
            };
            let flag = if t.needs_attention { " ⚑" } else { "" };
            let color = lifecycle_color(&t.lifecycle_status);
            ListItem::new(Line::from(vec![
                Span::raw(format!("{} ", bullet)),
                Span::styled(
                    format!("{:<27}", t.qualified_handle),
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    format!("{:<12}", t.lifecycle_status),
                    Style::default().fg(color),
                ),
                Span::styled(flag.to_string(), Style::default().fg(Color::Red)),
            ]))
        })
        .collect();

    let block = Block::bordered()
        .title(format!(" Tasks ({}) ", app.tasks.tasks.len()))
        .border_style(focus_border_style(focused));
    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn render_review(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focused == Panel::Review;
    let title = format!(" Review Queue ({}) ", app.review.tasks.len());
    let block = Block::bordered()
        .title(title)
        .border_style(focus_border_style(focused));

    if app.review.tasks.is_empty() {
        let p = Paragraph::new(" no tasks ready for review")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        frame.render_widget(p, area);
        return;
    }

    let items: Vec<ListItem> = app
        .review
        .tasks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let bullet = if i == app.review_scroll && focused {
                "▶"
            } else {
                " "
            };
            let color = lifecycle_color(&t.lifecycle_status);
            ListItem::new(Line::from(vec![
                Span::raw(format!("{} ", bullet)),
                Span::styled(
                    format!("{:<27}", t.qualified_handle),
                    Style::default().fg(Color::White),
                ),
                Span::styled(t.lifecycle_status.clone(), Style::default().fg(color)),
            ]))
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn render_inbox(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focused == Panel::Inbox;
    let title = format!(" Inbox ({}) ", app.inbox.items.len());
    let block = Block::bordered()
        .title(title)
        .border_style(focus_border_style(focused));

    if app.inbox.items.is_empty() {
        let p = Paragraph::new(" no tasks need attention")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        frame.render_widget(p, area);
        return;
    }

    let items: Vec<ListItem> = app
        .inbox
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let bullet = if i == app.inbox_scroll && focused {
                "●"
            } else {
                "○"
            };
            let priority_color = if item.priority < 20 {
                Color::Red
            } else if item.priority < 50 {
                Color::Yellow
            } else {
                Color::White
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", bullet), Style::default().fg(priority_color)),
                Span::styled(
                    format!("{:<24}", item.task_handle),
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    format!("{:<30}", item.reason),
                    Style::default().fg(priority_color),
                ),
                Span::styled(" → ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    item.recommended_action.clone(),
                    Style::default().fg(Color::Cyan),
                ),
            ]))
        })
        .collect();

    let list = List::new(items).block(block);
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

    #[test]
    fn interactive_cockpit_renders_title_and_task_to_buffer() {
        let app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|f| render_ui(f, &app)).unwrap();

        let content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();

        assert!(content.contains("Ajax Cockpit"));
        assert!(content.contains("web/fix-login"));
        assert!(content.contains("Active"));
        assert!(content.contains("Inbox"));
        assert!(content.contains("agent needs input"));
    }

    #[test]
    fn app_cycle_focus_rotates_through_panels() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        use super::Panel;
        assert_eq!(app.focused, Panel::Tasks);
        app.cycle_focus();
        assert_eq!(app.focused, Panel::Review);
        app.cycle_focus();
        assert_eq!(app.focused, Panel::Inbox);
        app.cycle_focus();
        assert_eq!(app.focused, Panel::Tasks);
    }

    #[test]
    fn app_scroll_bounded_by_item_count() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            sample_tasks(),
            sample_inbox(),
        );
        // tasks panel has 1 item — scroll should not exceed 0
        app.scroll_down();
        assert_eq!(app.task_scroll, 0);
        app.scroll_up();
        assert_eq!(app.task_scroll, 0);
    }

    #[test]
    fn app_scroll_advances_within_bounds() {
        let mut app = App::new(
            sample_repos(),
            TasksResponse {
                tasks: vec![
                    TaskSummary {
                        id: "t1".to_string(),
                        qualified_handle: "web/a".to_string(),
                        title: "A".to_string(),
                        lifecycle_status: "Active".to_string(),
                        needs_attention: false,
                    },
                    TaskSummary {
                        id: "t2".to_string(),
                        qualified_handle: "web/b".to_string(),
                        title: "B".to_string(),
                        lifecycle_status: "Active".to_string(),
                        needs_attention: false,
                    },
                ],
            },
            TasksResponse { tasks: vec![] },
            InboxResponse { items: vec![] },
        );
        assert_eq!(app.task_scroll, 0);
        app.scroll_down();
        assert_eq!(app.task_scroll, 1);
        app.scroll_down();
        assert_eq!(app.task_scroll, 1); // capped at len-1
        app.scroll_up();
        assert_eq!(app.task_scroll, 0);
    }
}

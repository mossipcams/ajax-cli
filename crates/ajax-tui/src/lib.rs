#![deny(unsafe_op_in_unsafe_fn)]

mod actions;
mod cockpit_state;
mod input;
mod layout;
mod navigation;
mod rendering;
mod runtime;

use ajax_core::{
    models::{AttentionItem, LiveStatusKind},
    output::{
        CockpitResponse, InboxResponse, RepoSummary, ReposResponse, TaskSummary, TasksResponse,
    },
};
pub use cockpit_state::App;
#[cfg(test)]
pub(crate) use cockpit_state::FLASH_TICKS;
use cockpit_state::{is_waiting_for_input, task_summary_repo, AppView, SelectableKind};
#[cfg(test)]
use input::{
    handle_action_result, handle_back_key, handle_cockpit_event, is_back_key_event,
    is_help_key_event, is_input_delete_key, EventLoopAction,
};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph},
    Frame,
};
#[cfg(test)]
use rendering::render_ui;
use rendering::StatusBucket;
pub use runtime::{
    run_interactive, run_interactive_with_flash, run_interactive_with_flash_and_refresh,
};
#[cfg(test)]
use runtime::{terminal_entry_commands, terminal_exit_commands, TerminalModeCommand};
use std::{io, ops::Range};

// ── Text renderer (watch mode) ────────────────────────────────────────────────

pub fn render_cockpit(
    repos: &ReposResponse,
    tasks: &TasksResponse,
    inbox: &InboxResponse,
) -> String {
    let mut lines = vec![
        "Ajax Cockpit".to_string(),
        format!("Repos: {}", repos.repos.len()),
        format!("Tasks: {}", tasks.tasks.len()),
        "Task Statuses".to_string(),
    ];

    if tasks.tasks.is_empty() {
        lines.push("no active tasks".to_string());
    } else {
        lines.extend(tasks.tasks.iter().map(|task| {
            format!(
                "{}\t{}\t{}",
                task.qualified_handle,
                task_status_label(task),
                task.title
            )
        }));
    }

    lines.push("Inbox".to_string());

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
        inbox: InboxResponse,
    },
    /// Exit the TUI — the CLI will run the deferred action.
    Defer(PendingAction),
    /// Ask for a second explicit activation before running a risky action.
    Confirm(String),
    /// Show a brief status message then stay in the TUI.
    Message(String),
}

pub trait CockpitEventHandler {
    fn on_action(&mut self, item: &AttentionItem) -> io::Result<ActionOutcome>;

    fn on_confirmed_action(&mut self, item: &AttentionItem) -> io::Result<ActionOutcome> {
        self.on_action(item)
    }

    fn on_refresh(&mut self) -> io::Result<Option<CockpitResponse>> {
        Ok(None)
    }
}

// ── Layout-coupled state helpers ──────────────────────────────────────────────

impl App {
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
    layout::selectable_row_ranges(selectable_feed_rows(app))
}

// ── Rendering ─────────────────────────────────────────────────────────────────

fn primary_accent() -> Color {
    bucket_color(StatusBucket::Active)
}

fn secondary_accent() -> Color {
    bucket_color(StatusBucket::NeedsYou)
}

fn danger_accent() -> Color {
    bucket_color(StatusBucket::Stuck)
}

fn muted_text() -> Color {
    bucket_color(StatusBucket::Idle)
}

fn subtle_text() -> Color {
    Color::Indexed(240)
}

fn bucket_color(bucket: StatusBucket) -> Color {
    rendering::bucket_color(bucket)
}

fn bucket_glyph(bucket: StatusBucket) -> &'static str {
    rendering::bucket_glyph(bucket)
}

fn live_bucket(kind: &LiveStatusKind) -> StatusBucket {
    match kind {
        LiveStatusKind::AgentRunning
        | LiveStatusKind::CommandRunning
        | LiveStatusKind::TestsRunning => StatusBucket::Active,
        LiveStatusKind::WaitingForApproval
        | LiveStatusKind::WaitingForInput
        | LiveStatusKind::AuthRequired => StatusBucket::NeedsYou,
        LiveStatusKind::Blocked
        | LiveStatusKind::MergeConflict
        | LiveStatusKind::CiFailed
        | LiveStatusKind::CommandFailed
        | LiveStatusKind::RateLimited
        | LiveStatusKind::ContextLimit => StatusBucket::Stuck,
        LiveStatusKind::Done => StatusBucket::Done,
        LiveStatusKind::ShellIdle | LiveStatusKind::Unknown => StatusBucket::Idle,
        LiveStatusKind::WorktreeMissing
        | LiveStatusKind::TmuxMissing
        | LiveStatusKind::WorktrunkMissing => StatusBucket::Missing,
    }
}

fn lifecycle_bucket(lifecycle: &str) -> StatusBucket {
    if lifecycle.contains("Error") || lifecycle.contains("Orphaned") {
        StatusBucket::Stuck
    } else if lifecycle.contains("Reviewable")
        || lifecycle.contains("Mergeable")
        || lifecycle.contains("Waiting")
    {
        StatusBucket::NeedsYou
    } else if lifecycle.contains("Merged") || lifecycle.contains("Cleanable") {
        StatusBucket::Done
    } else if lifecycle.contains("Active") || lifecycle.contains("Provisioning") {
        StatusBucket::Active
    } else {
        StatusBucket::Idle
    }
}

fn task_bucket(task: &TaskSummary) -> StatusBucket {
    let primary = task
        .live_status
        .as_ref()
        .map(|obs| live_bucket(&obs.kind))
        .unwrap_or_else(|| lifecycle_bucket(&task.lifecycle_status));
    match (primary, task.needs_attention) {
        (StatusBucket::Idle | StatusBucket::Active | StatusBucket::Done, true) => {
            StatusBucket::NeedsYou
        }
        (bucket, _) => bucket,
    }
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let mut parts = vec![Span::styled(
        " Ajax",
        Style::default()
            .fg(primary_accent())
            .add_modifier(Modifier::BOLD),
    )];

    let crumb_sep = || Span::styled(" > ", Style::default().fg(subtle_text()));
    let dot_sep = || Span::styled(" - ", Style::default().fg(subtle_text()));
    let crumb_style = Style::default()
        .fg(primary_accent())
        .add_modifier(Modifier::BOLD);

    match &app.view {
        AppView::Projects => {
            parts.push(dot_sep());
            parts.push(Span::styled(
                format!("{} repos", app.repos.repos.len()),
                Style::default().fg(secondary_accent()),
            ));
            parts.push(dot_sep());
            parts.push(Span::styled(
                format!("{} tasks", app.tasks.tasks.len()),
                Style::default().fg(primary_accent()),
            ));
            if !app.inbox.items.is_empty() {
                parts.push(dot_sep());
                parts.push(Span::styled(
                    format!("{} inbox", app.inbox.items.len()),
                    Style::default()
                        .fg(danger_accent())
                        .add_modifier(Modifier::BOLD),
                ));
                if let Some(next) = app.inbox.items.first() {
                    parts.push(dot_sep());
                    parts.push(Span::styled(
                        format!("next {}", next.task_handle),
                        Style::default()
                            .fg(danger_accent())
                            .add_modifier(Modifier::BOLD),
                    ));
                }
            }
            let reviewable_tasks: u32 = app
                .repos
                .repos
                .iter()
                .map(|repo| repo.reviewable_tasks)
                .sum();
            if reviewable_tasks > 0 {
                parts.push(dot_sep());
                parts.push(Span::styled(
                    format!("{reviewable_tasks} review"),
                    Style::default()
                        .fg(secondary_accent())
                        .add_modifier(Modifier::BOLD),
                ));
            }
            let cleanable_tasks: u32 = app
                .repos
                .repos
                .iter()
                .map(|repo| repo.cleanable_tasks)
                .sum();
            if cleanable_tasks > 0 {
                parts.push(dot_sep());
                parts.push(Span::styled(
                    format!("{cleanable_tasks} clean"),
                    Style::default()
                        .fg(danger_accent())
                        .add_modifier(Modifier::BOLD),
                ));
            }
        }
        AppView::Project { repo } => {
            parts.push(crumb_sep());
            parts.push(Span::styled(repo.clone(), crumb_style));
        }
        AppView::TaskActions { task, .. } => {
            if let Some(repo) = task_summary_repo(task) {
                parts.push(crumb_sep());
                parts.push(Span::styled(repo.to_string(), crumb_style));
            }
            parts.push(crumb_sep());
            parts.push(Span::styled(
                task.qualified_handle.clone(),
                Style::default()
                    .fg(primary_accent())
                    .add_modifier(Modifier::BOLD),
            ));
        }
        AppView::NewTaskInput { repo, .. } => {
            parts.push(crumb_sep());
            parts.push(Span::styled(repo.clone(), crumb_style));
            parts.push(crumb_sep());
            parts.push(Span::styled(
                "new task",
                Style::default().fg(primary_accent()),
            ));
        }
        AppView::Help { .. } => {
            parts.push(crumb_sep());
            parts.push(Span::styled(
                "help",
                Style::default().fg(secondary_accent()),
            ));
        }
    }

    if show_brand(&app.view) {
        let brand = ajax_brand_spans();
        let brand_width: u16 = brand.iter().map(|s| s.content.chars().count() as u16).sum();
        let chunks =
            Layout::horizontal([Constraint::Min(0), Constraint::Length(brand_width)]).split(area);
        frame.render_widget(Paragraph::new(Line::from(parts)), chunks[0]);
        frame.render_widget(Paragraph::new(Line::from(brand).right_aligned()), chunks[1]);
    } else {
        frame.render_widget(Paragraph::new(Line::from(parts)), area);
    }
}

fn show_brand(view: &AppView) -> bool {
    matches!(
        view,
        AppView::Projects | AppView::Project { .. } | AppView::TaskActions { .. }
    )
}

fn ajax_brand_spans() -> Vec<Span<'static>> {
    let bracket = Style::default().fg(subtle_text());
    let brand = Style::default()
        .fg(primary_accent())
        .add_modifier(Modifier::BOLD);
    vec![
        Span::raw(" "),
        Span::styled("[", bracket),
        Span::styled("AJAX", brand),
        Span::styled("]", bracket),
        Span::raw(" "),
    ]
}

fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let content = if let Some((msg, _)) = &app.flash {
        Line::from(vec![Span::styled(
            format!(" {msg}"),
            Style::default()
                .fg(primary_accent())
                .add_modifier(Modifier::BOLD),
        )])
    } else {
        let mut parts: Vec<Span<'static>> = vec![Span::raw(" ")];
        let push_hint = |parts: &mut Vec<Span<'static>>, key: &str, label: &str, last: bool| {
            parts.push(Span::styled(
                key.to_string(),
                Style::default()
                    .fg(secondary_accent())
                    .add_modifier(Modifier::BOLD),
            ));
            parts.push(Span::styled(
                format!(" {label}"),
                Style::default().fg(subtle_text()),
            ));
            if !last {
                parts.push(Span::styled(
                    "   ".to_string(),
                    Style::default().fg(subtle_text()),
                ));
            }
        };
        let enter_label = match &app.view {
            AppView::Projects => "open",
            AppView::Project { .. } => "open",
            AppView::TaskActions { .. } => "run",
            AppView::NewTaskInput { .. } => "create",
            AppView::Help { .. } => "back",
        };
        let nested = !matches!(app.view, AppView::Projects);
        push_hint(&mut parts, "up/down", "select", false);
        push_hint(&mut parts, "enter", enter_label, false);
        push_hint(&mut parts, "?", "help", false);
        if nested {
            let back_label = if matches!(app.view, AppView::NewTaskInput { .. }) {
                "erase/back"
            } else {
                "back"
            };
            push_hint(&mut parts, "esc/h", back_label, false);
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
            .fg(subtle_text())
            .add_modifier(Modifier::ITALIC),
    )]))
}

fn blank_row() -> ListItem<'static> {
    ListItem::new(Line::from(""))
}

fn group_of(kind: &SelectableKind) -> &'static str {
    match kind {
        SelectableKind::NewTask { .. } => "create",
        SelectableKind::Inbox(_) => "hot",
        SelectableKind::Project(_) => "projects",
        SelectableKind::Task(_) => "tasks",
        SelectableKind::TaskAction { .. } => "task-actions",
    }
}

fn task_glyph(task: &TaskSummary) -> Span<'static> {
    let bucket = task_bucket(task);
    Span::styled(
        bucket_glyph(bucket),
        Style::default()
            .fg(bucket_color(bucket))
            .add_modifier(Modifier::BOLD),
    )
}

fn task_handle_color(task: &TaskSummary) -> Color {
    bucket_color(task_bucket(task))
}

fn task_status_label(task: &TaskSummary) -> String {
    task.live_status
        .as_ref()
        .map(|status| status.summary.clone())
        .unwrap_or_else(|| task.lifecycle_status.clone())
}

fn project_glyph(repo: &RepoSummary) -> Span<'static> {
    if repo.active_tasks > 0 {
        Span::styled(
            "*",
            Style::default()
                .fg(primary_accent())
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(".", Style::default().fg(subtle_text()))
    }
}

fn project_name_color(repo: &RepoSummary) -> Color {
    if repo.active_tasks > 0 {
        primary_accent()
    } else {
        muted_text()
    }
}

fn inbox_glyph(color: Color) -> Span<'static> {
    Span::styled("!", Style::default().fg(color).add_modifier(Modifier::BOLD))
}

fn inbox_item_accent(item: &AttentionItem) -> Color {
    if is_waiting_for_input(&item.reason) {
        return secondary_accent();
    }
    priority_accent(item.priority)
}

fn priority_accent(priority: u32) -> Color {
    if priority < 20 {
        danger_accent()
    } else if priority < 50 {
        secondary_accent()
    } else {
        primary_accent()
    }
}

fn action_chrome(recommended_action: &str) -> actions::ActionChrome {
    actions::action_chrome(recommended_action)
}

fn action_glyph(recommended_action: &str) -> Span<'static> {
    let chrome = action_chrome(recommended_action);
    Span::styled(chrome.glyph, chrome.glyph_style())
}

fn action_label_style(recommended_action: &str) -> Style {
    action_chrome(recommended_action).label_style()
}

fn project_subtitle(repo: &RepoSummary) -> String {
    let mut parts = Vec::new();
    if repo.active_tasks > 0 {
        parts.push(format!("{} active", repo.active_tasks));
    }
    if repo.attention_items > 0 {
        parts.push(format!("{} attention", repo.attention_items));
    }
    if repo.reviewable_tasks > 0 {
        parts.push(format!("{} review", repo.reviewable_tasks));
    }
    if repo.cleanable_tasks > 0 {
        parts.push(format!("{} clean", repo.cleanable_tasks));
    }
    if parts.is_empty() {
        "idle".to_string()
    } else {
        parts.join(" - ")
    }
}

fn render_row(glyph: Span<'static>, mut spans: Vec<Span<'static>>) -> ListItem<'static> {
    let mut all = vec![Span::raw("   "), glyph, Span::raw("  ")];
    all.append(&mut spans);
    ListItem::new(Line::from(all))
}

fn render_selectable(s: &SelectableKind) -> ListItem<'static> {
    let bold = Modifier::BOLD;
    let dim = Style::default().fg(subtle_text());
    let arrow = Style::default().fg(subtle_text());
    match s {
        SelectableKind::Inbox(item) => {
            let accent = inbox_item_accent(item);
            render_row(
                inbox_glyph(accent),
                vec![
                    Span::styled(
                        format!("{:<22}", item.task_handle),
                        Style::default().fg(accent).add_modifier(bold),
                    ),
                    Span::styled(item.reason.clone(), Style::default().fg(accent)),
                    Span::styled("  ->  ", arrow),
                    Span::styled(
                        item.recommended_action.clone(),
                        Style::default().fg(primary_accent()).add_modifier(bold),
                    ),
                ],
            )
        }
        SelectableKind::Project(repo) => render_row(
            project_glyph(repo),
            vec![
                Span::styled(
                    format!("{:<20}", repo.name),
                    Style::default()
                        .fg(project_name_color(repo))
                        .add_modifier(bold),
                ),
                Span::styled(project_subtitle(repo), dim),
            ],
        ),
        SelectableKind::NewTask { .. } => render_row(
            action_glyph("new task"),
            vec![Span::styled(
                "start a new task",
                Style::default().fg(primary_accent()).add_modifier(bold),
            )],
        ),
        SelectableKind::TaskAction {
            recommended_action, ..
        } => render_row(
            action_glyph(recommended_action),
            vec![Span::styled(
                recommended_action.clone(),
                action_label_style(recommended_action),
            )],
        ),
        SelectableKind::Task(t) => render_row(
            task_glyph(t),
            vec![
                Span::styled(
                    format!("{:<28}", t.qualified_handle),
                    Style::default().fg(task_handle_color(t)).add_modifier(bold),
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
                Span::styled(display_title, Style::default().fg(primary_accent())),
            ],
        ));
        return (rows, sel_to_row);
    }

    if matches!(app.view, AppView::Help { .. }) {
        rows.push(render_row(
            action_glyph("help"),
            vec![Span::styled(
                "Keyboard shortcuts",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )],
        ));
        for (key, label) in [
            ("up/down", "select the previous or next row"),
            ("j/k", "select the next or previous row"),
            ("enter", "open or run the selected row"),
            ("?", "show this help page"),
            ("esc/h/backspace", "go back to the previous view"),
            ("q", "quit the cockpit"),
            ("mouse scroll", "move the selection"),
            ("mouse click", "select a visible row"),
            (
                "new task input",
                "type a title; backspace erases before going back",
            ),
        ] {
            rows.push(render_row(
                Span::styled(".", Style::default().fg(subtle_text())),
                vec![
                    Span::styled(format!("{key:<18}"), Style::default().fg(Color::Yellow)),
                    Span::styled(label.to_string(), Style::default().fg(subtle_text())),
                ],
            ));
        }
        return (rows, sel_to_row);
    }

    if app.selectables.is_empty() {
        let msg = match &app.view {
            AppView::Projects => "no projects yet - edit ~/.config/ajax/config.toml to add one",
            AppView::Project { .. } => "nothing here yet - esc/h to go back",
            AppView::TaskActions { .. } => "no actions available",
            AppView::NewTaskInput { .. } => "enter a task name",
            AppView::Help { .. } => "keyboard shortcuts",
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

fn selectable_feed_rows(app: &App) -> Vec<usize> {
    let (_, selectable_rows) = build_feed(app, 0);
    selectable_rows
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
        action_chrome, action_glyph, action_label_style, bucket_color, bucket_glyph, danger_accent,
        handle_cockpit_event, inbox_glyph, inbox_item_accent, lifecycle_bucket, muted_text,
        primary_accent, priority_accent, project_glyph, project_name_color, project_subtitle,
        render_cockpit, render_ui, secondary_accent, selectable_feed_rows, selectable_row_layout,
        selected_highlight, show_brand, subtle_text, task_bucket, task_glyph, task_handle_color,
        ActionOutcome, App, AppView, CockpitEventHandler, EventLoopAction, PendingAction,
        SelectableKind, StatusBucket, TerminalModeCommand, FLASH_TICKS,
    };
    use ajax_core::{
        models::{AttentionItem, LiveObservation, LiveStatusKind, RecommendedAction, TaskId},
        output::{
            CockpitResponse, CockpitSummary, InboxResponse, RepoSummary, ReposResponse,
            TaskSummary, TasksResponse,
        },
    };
    use crossterm::event::{
        Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton,
        MouseEvent, MouseEventKind,
    };
    use ratatui::{
        backend::TestBackend,
        style::{Color, Modifier, Style},
        Terminal,
    };
    use rstest::rstest;

    fn sample_repos() -> ReposResponse {
        ReposResponse {
            repos: vec![RepoSummary {
                name: "web".to_string(),
                path: "/Users/matt/projects/web".to_string(),
                active_tasks: 1,
                attention_items: 1,
                reviewable_tasks: 1,
                cleanable_tasks: 0,
            }],
        }
    }

    #[test]
    fn active_tui_api_does_not_export_legacy_cockpit_facades() {
        let lib = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"),
        )
        .unwrap();

        for legacy_module in ["app", "input", "render"] {
            let legacy_export = ["pub mod ", legacy_module, ";"].concat();
            assert!(!lib.contains(&legacy_export));
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
                actions: vec![RecommendedAction::OpenTask.as_str().to_string()],
            }],
        }
    }

    fn sample_tasks_with_count(count: usize) -> TasksResponse {
        TasksResponse {
            tasks: (0..count)
                .map(|idx| TaskSummary {
                    id: format!("task-{idx}"),
                    qualified_handle: format!("web/task-{idx}"),
                    title: format!("Task {idx}"),
                    lifecycle_status: "Active".to_string(),
                    needs_attention: false,
                    live_status: None,
                    actions: vec![RecommendedAction::OpenTask.as_str().to_string()],
                })
                .collect(),
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
    fn cockpit_palette_maps_accents_to_status_buckets() {
        assert_eq!(primary_accent(), bucket_color(StatusBucket::Active));
        assert_eq!(secondary_accent(), bucket_color(StatusBucket::NeedsYou));
        assert_eq!(danger_accent(), bucket_color(StatusBucket::Stuck));
        assert_eq!(muted_text(), bucket_color(StatusBucket::Idle));
        assert_eq!(subtle_text(), Color::Indexed(240));
    }

    #[rstest]
    #[case(StatusBucket::Active, "▸")]
    #[case(StatusBucket::NeedsYou, "?")]
    #[case(StatusBucket::Stuck, "!")]
    #[case(StatusBucket::Done, "✓")]
    #[case(StatusBucket::Idle, "·")]
    #[case(StatusBucket::Missing, "×")]
    fn status_buckets_have_stable_glyphs(#[case] bucket: StatusBucket, #[case] glyph: &str) {
        assert_eq!(bucket_glyph(bucket), glyph);
        assert_eq!(crate::rendering::bucket_glyph(bucket), glyph);
    }

    #[rstest]
    #[case("Error", StatusBucket::Stuck)]
    #[case("Orphaned", StatusBucket::Stuck)]
    #[case("Reviewable", StatusBucket::NeedsYou)]
    #[case("Mergeable", StatusBucket::NeedsYou)]
    #[case("Waiting", StatusBucket::NeedsYou)]
    #[case("Merged", StatusBucket::Done)]
    #[case("Cleanable", StatusBucket::Done)]
    #[case("Active", StatusBucket::Active)]
    #[case("Provisioning", StatusBucket::Active)]
    #[case("Removed", StatusBucket::Idle)]
    fn lifecycle_labels_map_to_status_buckets(
        #[case] lifecycle: &str,
        #[case] bucket: StatusBucket,
    ) {
        assert_eq!(lifecycle_bucket(lifecycle), bucket);
    }

    #[test]
    fn row_chrome_helpers_preserve_visible_glyphs_and_styles() {
        let active_repo = RepoSummary {
            name: "web".to_string(),
            path: "/repo".to_string(),
            active_tasks: 1,
            attention_items: 0,
            reviewable_tasks: 0,
            cleanable_tasks: 0,
        };
        let idle_repo = RepoSummary {
            active_tasks: 0,
            ..active_repo.clone()
        };
        let urgent_item = AttentionItem {
            task_id: TaskId::new("task-1"),
            task_handle: "web/fix".to_string(),
            reason: "waiting for input".to_string(),
            priority: 10,
            recommended_action: "open task".to_string(),
        };

        assert_eq!(project_glyph(&active_repo).content.as_ref(), "*");
        assert_eq!(project_glyph(&idle_repo).content.as_ref(), ".");
        assert_eq!(project_name_color(&active_repo), primary_accent());
        assert_eq!(project_name_color(&idle_repo), muted_text());
        assert_eq!(inbox_glyph(danger_accent()).content.as_ref(), "!");
        assert_eq!(inbox_item_accent(&urgent_item), secondary_accent());
        assert_eq!(action_glyph("help").content.as_ref(), "?");
        assert_eq!(
            action_glyph("help").style,
            Style::default()
                .fg(Color::LightYellow)
                .add_modifier(Modifier::BOLD)
        );
        assert_eq!(action_glyph("unknown").content.as_ref(), ".");
        assert_eq!(
            action_label_style("help"),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        );
        assert_eq!(
            action_label_style("unknown"),
            Style::default().fg(muted_text())
        );
    }

    #[rstest]
    #[case(0, danger_accent())]
    #[case(19, danger_accent())]
    #[case(20, secondary_accent())]
    #[case(49, secondary_accent())]
    #[case(50, primary_accent())]
    fn priority_boundaries_map_to_expected_accents(#[case] priority: u32, #[case] color: Color) {
        assert_eq!(priority_accent(priority), color);
    }

    #[test]
    fn project_subtitle_includes_only_nonzero_counts() {
        let idle = RepoSummary {
            name: "web".to_string(),
            path: "/repo".to_string(),
            active_tasks: 0,
            attention_items: 0,
            reviewable_tasks: 0,
            cleanable_tasks: 0,
        };
        let busy = RepoSummary {
            active_tasks: 1,
            attention_items: 2,
            reviewable_tasks: 3,
            cleanable_tasks: 4,
            ..idle.clone()
        };

        assert_eq!(project_subtitle(&idle), "idle");
        assert_eq!(
            project_subtitle(&busy),
            "1 active - 2 attention - 3 review - 4 clean"
        );
    }

    #[rstest]
    #[case(AppView::Projects, true)]
    #[case(AppView::Project { repo: "web".to_string() }, true)]
    #[case(
        AppView::TaskActions {
            task: TaskSummary {
                id: "task-1".to_string(),
                qualified_handle: "web/fix-login".to_string(),
                title: "Fix login".to_string(),
                lifecycle_status: "Active".to_string(),
                needs_attention: false,
                live_status: None,
                actions: vec![RecommendedAction::OpenTask.as_str().to_string()],
            },
            parent: Box::new(AppView::Projects),
        },
        true
    )]
    #[case(
        AppView::NewTaskInput {
            repo: "web".to_string(),
            title: String::new(),
        },
        false
    )]
    #[case(
        AppView::Help {
            previous: Box::new(AppView::Projects),
        },
        false
    )]
    fn brand_visibility_matches_primary_operator_views(
        #[case] view: AppView,
        #[case] expected: bool,
    ) {
        assert_eq!(show_brand(&view), expected);
    }

    #[test]
    fn selected_rows_use_highlight_style() {
        assert_eq!(
            selected_highlight(),
            Style::default()
                .bg(Color::Indexed(237))
                .add_modifier(Modifier::BOLD)
        );
    }

    #[test]
    fn top_level_status_bar_does_not_advertise_nested_back_action() {
        let app = App::new(sample_repos(), sample_tasks(), sample_inbox());

        let content = render_to_string(80, 30, &app);

        assert!(content.contains("q quit"));
        assert!(!content.contains("esc/h back"));
        assert!(!content.contains("esc/h erase/back"));
    }

    #[test]
    fn scrolled_feed_highlights_selected_row_at_viewport_offset() {
        let mut app = app_in_project_view_with_task_count(8);
        let target = 5;
        let target_feed_row = selectable_row_layout(&app)[target].start;
        app.selected = target;
        app.viewport_scroll = 2;
        let selected_screen_row = 1 + target_feed_row - app.viewport_scroll;
        let backend = TestBackend::new(80, 12);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| render_ui(frame, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let selected_row_has_highlight = (0..buffer.area.width).any(|x| {
            let cell = &buffer[(x, selected_screen_row as u16)];
            cell.bg == Color::Indexed(237) && cell.modifier.contains(Modifier::BOLD)
        });
        assert!(selected_row_has_highlight);
    }

    #[test]
    fn cockpit_text_renderer_does_not_show_review_lane() {
        let content = render_cockpit(
            &sample_repos(),
            &sample_tasks(),
            &InboxResponse { items: vec![] },
        );

        assert!(!content.contains("Review:"));
        assert!(!content.contains("review"));
        assert!(content.contains("web/fix-login"));
    }

    #[test]
    fn task_rows_render_live_status_when_present() {
        let mut tasks = sample_tasks();
        tasks.tasks[0].live_status = Some(LiveObservation::new(
            LiveStatusKind::WaitingForApproval,
            "waiting for approval",
        ));
        let mut app = App::new(sample_repos(), tasks, InboxResponse { items: vec![] });
        app.activate_selected();

        let content = render_to_string(80, 30, &app);

        assert!(content.contains("web/fix-login"));
        assert!(content.contains("waiting for approval"));
        assert!(!content.contains("Active"));
    }

    #[test]
    fn waiting_for_input_task_attention_uses_needs_you_chrome() {
        let mut tasks = sample_tasks();
        tasks.tasks[0].live_status = Some(LiveObservation::new(
            LiveStatusKind::WaitingForInput,
            "waiting for input",
        ));
        tasks.tasks[0].needs_attention = true;
        let task = &tasks.tasks[0];

        assert_eq!(task_bucket(task), StatusBucket::NeedsYou);
        assert_eq!(
            task_glyph(task).style.fg,
            Some(bucket_color(StatusBucket::NeedsYou))
        );
        assert_eq!(
            task_handle_color(task),
            bucket_color(StatusBucket::NeedsYou)
        );
    }

    #[test]
    fn cockpit_header_summarizes_review_and_cleanup_pressure() {
        let mut repos = sample_repos();
        repos.repos[0].cleanable_tasks = 1;
        let app = App::new(repos, sample_tasks(), sample_inbox());

        let content = render_to_string(80, 30, &app);

        assert!(content.contains("1 review"));
        assert!(content.contains("1 clean"));
    }

    #[test]
    fn project_rows_summarize_operator_work_by_project() {
        let mut repos = sample_repos();
        repos.repos[0].cleanable_tasks = 1;
        let app = App::new(repos, sample_tasks(), sample_inbox());

        let content = render_to_string(80, 30, &app);

        assert!(content.contains("1 active - 1 attention - 1 review - 1 clean"));
    }

    #[test]
    fn cockpit_header_names_next_attention_item() {
        let app = App::new(sample_repos(), sample_tasks(), sample_inbox());

        let content = render_to_string(80, 30, &app);

        assert!(content.contains("next web/fix-login"));
    }

    #[test]
    fn refresh_snapshot_updates_live_status_and_preserves_selection() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            InboxResponse { items: vec![] },
        );
        app.select_next();
        let selected_before = app.selected;
        let mut refreshed_tasks = sample_tasks();
        refreshed_tasks.tasks[0].live_status = Some(LiveObservation::new(
            LiveStatusKind::WaitingForApproval,
            "waiting for approval",
        ));

        app.apply_refresh(CockpitResponse {
            summary: CockpitSummary {
                repos: 1,
                tasks: 1,
                active_tasks: 1,
                attention_items: 0,
                reviewable_tasks: 1,
                cleanable_tasks: 0,
            },
            repos: sample_repos(),
            tasks: refreshed_tasks,
            review: TasksResponse { tasks: vec![] },
            inbox: InboxResponse { items: vec![] },
            next: ajax_core::output::NextResponse { item: None },
        });

        assert_eq!(app.selected, selected_before);
        let content = render_to_string(80, 30, &app);
        assert!(content.contains("web/fix-login"));
        assert!(content.contains("waiting for approval"));
        assert!(!content.contains("Active"));
    }

    #[test]
    fn cockpit_render_uses_single_cell_symbols() {
        let app = App::new(sample_repos(), sample_tasks(), sample_inbox());
        let backend = TestBackend::new(80, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|f| render_ui(f, &app)).unwrap();

        let empty_cells = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .filter(|cell| cell.symbol().is_empty())
            .count();

        assert_eq!(empty_cells, 0);
    }

    #[test]
    fn cockpit_brand_renders_at_header_right_edge() {
        let app = App::new(sample_repos(), sample_tasks(), sample_inbox());
        let backend = TestBackend::new(80, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|f| render_ui(f, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let brand = (73..79)
            .map(|x| buffer[(x, 0)].symbol())
            .collect::<String>();
        assert_eq!(brand, "[AJAX]");
        assert_eq!(buffer[(79, 0)].symbol(), " ");
    }

    #[test]
    fn cockpit_render_uses_orange_yellow_palette() {
        let mut app = app_in_project_view();
        app.select_next();
        app.select_next();
        app.activate_selected();
        let backend = TestBackend::new(80, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|f| render_ui(f, &app)).unwrap();

        let colors = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.fg)
            .collect::<Vec<_>>();
        assert!(colors.contains(&primary_accent()));
        assert!(colors.contains(&secondary_accent()));
        for bad_color in [
            Color::LightCyan,
            Color::LightGreen,
            Color::LightBlue,
            Color::LightMagenta,
        ] {
            assert!(
                !colors.contains(&bad_color),
                "cockpit palette should not render old accent color {bad_color:?}"
            );
        }
    }

    #[test]
    fn cockpit_render_uses_ascii_chrome_for_tmux_copy() {
        let app = App::new(sample_repos(), sample_tasks(), sample_inbox());

        let content = render_to_string(80, 30, &app);

        assert!(
            content.is_ascii(),
            "cockpit chrome should avoid wide glyph artifacts in tmux"
        );
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

    fn app_in_project_view() -> App {
        let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
        app.select_next();
        assert!(app.activate_selected().is_none());
        app
    }

    fn app_in_project_view_with_task_count(count: usize) -> App {
        let mut app = App::new(
            sample_repos(),
            sample_tasks_with_count(count),
            InboxResponse { items: vec![] },
        );
        app.activate_selected();
        app
    }

    fn app_in_empty_new_task_input() -> App {
        let mut app = app_in_project_view();
        assert!(app.activate_selected().is_none());
        assert!(matches!(
            &app.view,
            AppView::NewTaskInput { repo, title } if repo == "web" && title.is_empty()
        ));
        app
    }

    struct NoopHandler;

    impl CockpitEventHandler for NoopHandler {
        fn on_action(&mut self, _: &AttentionItem) -> std::io::Result<ActionOutcome> {
            Ok(ActionOutcome::Message("ignored".to_string()))
        }
    }

    struct DeferHandler;

    impl CockpitEventHandler for DeferHandler {
        fn on_action(&mut self, item: &AttentionItem) -> std::io::Result<ActionOutcome> {
            Ok(ActionOutcome::Defer(PendingAction {
                task_handle: item.task_handle.clone(),
                recommended_action: item.recommended_action.clone(),
                task_title: None,
            }))
        }
    }

    #[derive(Default)]
    struct ConfirmHandler {
        asked: usize,
        confirmed: usize,
    }

    impl CockpitEventHandler for ConfirmHandler {
        fn on_action(&mut self, _: &AttentionItem) -> std::io::Result<ActionOutcome> {
            self.asked += 1;
            Ok(ActionOutcome::Confirm(
                "press enter again to confirm".to_string(),
            ))
        }

        fn on_confirmed_action(&mut self, _: &AttentionItem) -> std::io::Result<ActionOutcome> {
            self.confirmed += 1;
            Ok(ActionOutcome::Message("confirmed".to_string()))
        }
    }

    fn handle_with_noop(app: &mut App, event: Event, height: usize) -> EventLoopAction {
        let mut handler = NoopHandler;
        handle_cockpit_event(app, event, height, &mut handler).unwrap()
    }

    #[rstest]
    #[case(0, 0)]
    #[case(1, 0)]
    #[case(2, 1)]
    fn select_prev_saturates_at_first_row(#[case] start_steps: usize, #[case] expected: usize) {
        let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
        for _ in 0..start_steps {
            app.select_next();
        }

        app.select_prev();

        assert_eq!(app.selected, expected);
    }

    #[rstest]
    #[case::projects(AppView::Projects, false)]
    #[case::project(AppView::Project { repo: String::new() }, false)]
    #[case::new_task(
        AppView::NewTaskInput {
            repo: String::new(),
            title: String::new()
        },
        true
    )]
    #[case::help(
        AppView::Help {
            previous: Box::new(AppView::Projects)
        },
        false
    )]
    fn collecting_input_matches_only_new_task_view(#[case] view: AppView, #[case] expected: bool) {
        let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
        app.view = view;

        assert_eq!(app.is_collecting_input(), expected);
    }

    #[rstest]
    #[case(0, Some(FLASH_TICKS))]
    #[case(1, Some(FLASH_TICKS - 1))]
    #[case(FLASH_TICKS, Some(0))]
    #[case(FLASH_TICKS + 1, None)]
    fn flash_expires_after_final_visible_tick(
        #[case] ticks: u8,
        #[case] expected_remaining: Option<u8>,
    ) {
        let mut app = app_in_empty_new_task_input();
        assert!(app.submit_input().is_none());

        for _ in 0..ticks {
            app.tick_flash();
        }

        assert_eq!(
            app.flash.as_ref().map(|(_, ticks)| *ticks),
            expected_remaining
        );
    }

    #[test]
    fn ensure_visible_leaves_exact_bottom_boundary_stable() {
        let mut app = app_in_project_view();
        app.selected = 2;
        app.viewport_scroll = selectable_row_layout(&app)[0].start;
        let selected_range = selectable_row_layout(&app)[app.selected].clone();
        let viewport_height = selected_range.end - app.viewport_scroll;

        app.ensure_visible(viewport_height);

        assert_eq!(app.viewport_scroll, selectable_row_layout(&app)[0].start);
        assert_eq!(selected_range.end, app.viewport_scroll + viewport_height);
    }

    #[test]
    fn ensure_visible_scrolls_up_and_down_to_selected_row() {
        let mut app = app_in_project_view();
        app.selected = 0;
        app.viewport_scroll = selectable_row_layout(&app)[2].start;

        app.ensure_visible(1);

        assert_eq!(app.viewport_scroll, selectable_row_layout(&app)[0].start);

        app.selected = 2;
        app.ensure_visible(1);

        let selected_range = selectable_row_layout(&app)[app.selected].clone();
        assert_eq!(app.viewport_scroll, selected_range.end - 1);
    }

    #[test]
    fn ensure_visible_uses_addition_for_viewport_bottom() {
        let mut app = app_in_project_view_with_task_count(6);
        let layout = selectable_row_layout(&app);
        let (selected, selected_range) = layout
            .iter()
            .cloned()
            .enumerate()
            .find(|(_, range)| range.end == 6)
            .expect("fixture should have a selectable ending at feed row 6");
        app.selected = selected;
        app.viewport_scroll = 3;

        app.ensure_visible(2);

        assert_eq!(app.viewport_scroll, selected_range.end - 2);
    }

    #[test]
    fn ensure_visible_zero_height_never_scrolls() {
        let mut app = app_in_project_view_with_task_count(6);
        app.selected = app.selectables.len() - 1;
        app.viewport_scroll = 3;

        app.ensure_visible(0);

        assert_eq!(app.viewport_scroll, 3);
    }

    #[test]
    fn non_press_key_events_do_not_mutate_input_state() {
        let mut app = app_in_empty_new_task_input();
        let event = Event::Key(KeyEvent {
            code: KeyCode::Char('x'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
            state: KeyEventState::NONE,
        });

        let mut handler = NoopHandler;
        let action = handle_cockpit_event(&mut app, event, 10, &mut handler).unwrap();

        assert!(matches!(action, EventLoopAction::Continue));
        assert!(
            matches!(&app.view, AppView::NewTaskInput { title, .. } if title.is_empty()),
            "release events must not append editable input"
        );
    }

    #[rstest]
    #[case(KeyCode::Enter, None, true)]
    #[case(KeyCode::Char('x'), None, false)]
    #[case(KeyCode::Backspace, Some("a"), false)]
    #[case(KeyCode::Delete, Some("a"), false)]
    fn input_mode_keys_use_input_branches(
        #[case] code: KeyCode,
        #[case] initial_title: Option<&str>,
        #[case] flashes_for_empty_submit: bool,
    ) {
        let mut app = app_in_empty_new_task_input();
        if let Some(title) = initial_title {
            for character in title.chars() {
                app.push_input_char(character);
            }
        }

        let mut handler = NoopHandler;
        let action = handle_cockpit_event(
            &mut app,
            Event::Key(KeyEvent::new(code, KeyModifiers::NONE)),
            10,
            &mut handler,
        )
        .unwrap();

        assert!(matches!(action, EventLoopAction::Continue));
        assert_eq!(app.flash.is_some(), flashes_for_empty_submit);
        match code {
            KeyCode::Char('x') => {
                assert!(matches!(&app.view, AppView::NewTaskInput { title, .. } if title == "x"));
            }
            KeyCode::Backspace | KeyCode::Delete => {
                assert!(
                    matches!(&app.view, AppView::NewTaskInput { title, .. } if title.is_empty())
                );
            }
            KeyCode::Enter => {}
            _ => unreachable!(),
        }
    }

    #[rstest]
    #[case(KeyCode::Char('?'), KeyModifiers::NONE)]
    #[case(KeyCode::Char('/'), KeyModifiers::SHIFT)]
    fn help_keys_open_help_view(#[case] code: KeyCode, #[case] modifiers: KeyModifiers) {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            InboxResponse { items: vec![] },
        );

        let action = handle_with_noop(&mut app, Event::Key(KeyEvent::new(code, modifiers)), 10);

        assert!(matches!(action, EventLoopAction::Continue));
        assert!(matches!(app.view, AppView::Help { .. }));
    }

    #[test]
    fn escape_key_returns_to_parent_view() {
        let mut app = app_in_project_view();

        let action = handle_with_noop(
            &mut app,
            Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            10,
        );

        assert!(matches!(action, EventLoopAction::Continue));
        assert!(matches!(app.view, AppView::Projects));
    }

    #[test]
    fn quit_key_requests_event_loop_exit() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            InboxResponse { items: vec![] },
        );

        let action = handle_with_noop(
            &mut app,
            Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
            10,
        );

        assert!(matches!(action, EventLoopAction::Quit));
    }

    #[rstest]
    #[case(KeyCode::Down, 0, 1)]
    #[case(KeyCode::Char('j'), 0, 1)]
    #[case(KeyCode::Up, 1, 0)]
    #[case(KeyCode::Char('k'), 1, 0)]
    fn navigation_keys_update_selection(
        #[case] code: KeyCode,
        #[case] start: usize,
        #[case] expected: usize,
    ) {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            InboxResponse { items: vec![] },
        );
        app.selected = start;

        let action = handle_with_noop(
            &mut app,
            Event::Key(KeyEvent::new(code, KeyModifiers::NONE)),
            10,
        );

        assert!(matches!(action, EventLoopAction::Continue));
        assert_eq!(app.selected, expected);
    }

    #[rstest]
    #[case(KeyCode::Char('h'))]
    #[case(KeyCode::Left)]
    fn back_keys_return_to_parent_view(#[case] code: KeyCode) {
        let mut app = app_in_project_view();

        let action = handle_with_noop(
            &mut app,
            Event::Key(KeyEvent::new(code, KeyModifiers::NONE)),
            10,
        );

        assert!(matches!(action, EventLoopAction::Continue));
        assert!(matches!(app.view, AppView::Projects));
    }

    #[test]
    fn enter_on_task_action_delegates_to_handler() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            InboxResponse { items: vec![] },
        );
        app.select_next();
        assert!(app.activate_selected().is_none());
        let mut handler = DeferHandler;

        let action = handle_cockpit_event(
            &mut app,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            10,
            &mut handler,
        )
        .unwrap();

        assert!(matches!(
            action,
            EventLoopAction::Pending(PendingAction {
                recommended_action,
                ..
            }) if recommended_action == "open task"
        ));
    }

    #[rstest]
    #[case(MouseEventKind::ScrollDown, 2)]
    #[case(MouseEventKind::ScrollUp, 0)]
    fn mouse_scroll_updates_selection(#[case] kind: MouseEventKind, #[case] expected: usize) {
        let mut app = app_in_project_view();
        app.selected = 1;

        let action = handle_with_noop(
            &mut app,
            Event::Mouse(MouseEvent {
                kind,
                column: 0,
                row: 1,
                modifiers: KeyModifiers::NONE,
            }),
            10,
        );

        assert!(matches!(action, EventLoopAction::Continue));
        assert_eq!(app.selected, expected);
    }

    #[test]
    fn mouse_click_selects_feed_row_inside_feed_bounds() {
        let mut app = app_in_project_view();
        let target = 2;
        let target_feed_row = selectable_row_layout(&app)[target].start;

        let action = handle_with_noop(
            &mut app,
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 2,
                row: (target_feed_row + 1) as u16,
                modifiers: KeyModifiers::NONE,
            }),
            10,
        );

        assert!(matches!(action, EventLoopAction::Continue));
        assert_eq!(app.selected, target);
    }

    #[test]
    fn mouse_click_accounts_for_viewport_scroll_offset() {
        let mut app = app_in_project_view_with_task_count(8);
        let target = 5;
        let target_feed_row = selectable_row_layout(&app)[target].start;
        app.viewport_scroll = 2;
        let mouse_row = (target_feed_row - app.viewport_scroll + 1) as u16;

        let action = handle_with_noop(
            &mut app,
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 2,
                row: mouse_row,
                modifiers: KeyModifiers::NONE,
            }),
            10,
        );

        assert!(matches!(action, EventLoopAction::Continue));
        assert_eq!(app.selected, target);
    }

    #[rstest]
    #[case(0)]
    #[case(9)]
    fn mouse_click_outside_feed_bounds_is_ignored(#[case] row: u16) {
        let mut app = app_in_project_view();
        app.selected = 1;

        let action = handle_with_noop(
            &mut app,
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 2,
                row,
                modifiers: KeyModifiers::NONE,
            }),
            10,
        );

        assert!(matches!(action, EventLoopAction::Continue));
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn mouse_click_on_status_bar_is_ignored_even_when_scrolled() {
        let mut app = app_in_project_view_with_task_count(12);
        app.selected = 1;
        app.viewport_scroll = 2;

        let action = handle_with_noop(
            &mut app,
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 2,
                row: 9,
                modifiers: KeyModifiers::NONE,
            }),
            10,
        );

        assert!(matches!(action, EventLoopAction::Continue));
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn cockpit_renders_backend_snapshot() {
        let repos = sample_repos();
        let tasks = sample_tasks();
        let inbox = sample_inbox();
        let rendered = render_cockpit(&repos, &tasks, &inbox);
        assert!(rendered.contains("Ajax Cockpit"));
        assert!(rendered.contains("Repos: 1"));
        assert!(!rendered.contains("Review:"));
        assert!(rendered.contains("web/fix-login: agent needs input -> open task"));
    }

    #[test]
    fn feed_inbox_appears_before_tasks() {
        // In the Project view, inbox rows precede task rows.
        let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
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
                    attention_items: 0,
                    reviewable_tasks: 0,
                    cleanable_tasks: 0,
                },
                RepoSummary {
                    name: "autosnooze".to_string(),
                    path: "/Users/matt/Desktop/Projects/autosnooze".to_string(),
                    active_tasks: 0,
                    attention_items: 0,
                    reviewable_tasks: 1,
                    cleanable_tasks: 0,
                },
            ],
        };
        let app = App::new(repos, sample_tasks(), sample_inbox());

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
    fn main_page_renders_task_statuses_without_opening_project() {
        let app = App::new(
            sample_repos(),
            sample_tasks(),
            InboxResponse { items: vec![] },
        );

        let content = render_to_string(80, 30, &app);

        assert!(content.contains("web/fix-login"));
        assert!(content.contains("Active"));
        assert!(!content.contains("> web"));
    }

    #[test]
    fn main_page_task_row_enters_open_task_action() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            InboxResponse { items: vec![] },
        );

        for _ in 0..app.selectables.len() {
            if matches!(
                app.selectables.get(app.selected),
                Some(SelectableKind::Task(_))
            ) {
                break;
            }
            app.select_next();
        }
        // Enter on a Task opens the per-task action menu (default first row = "open task").
        assert!(app.activate_selected().is_none());
        assert!(matches!(&app.view, AppView::TaskActions { .. }));

        let item = app.activate_selected().unwrap();
        assert_eq!(item.task_handle, "web/fix-login");
        assert_eq!(item.recommended_action, "open task");
    }

    #[test]
    fn main_page_deduplicates_tasks_already_shown_in_inbox() {
        let app = App::new(
            sample_repos(),
            sample_tasks(),
            InboxResponse {
                items: vec![AttentionItem {
                    task_id: TaskId::new("task-1"),
                    task_handle: "web/fix-login".to_string(),
                    reason: "waiting for input".to_string(),
                    priority: 6,
                    recommended_action: "open task".to_string(),
                }],
            },
        );

        let task_rows = app
            .selectables
            .iter()
            .filter(|selectable| {
                matches!(
                    selectable,
                    SelectableKind::Task(task) if task.qualified_handle == "web/fix-login"
                )
            })
            .count();
        let inbox_rows = app
            .selectables
            .iter()
            .filter(|selectable| {
                matches!(
                    selectable,
                    SelectableKind::Inbox(item) if item.task_handle == "web/fix-login"
                )
            })
            .count();

        assert_eq!(inbox_rows, 1);
        assert_eq!(task_rows, 0);
    }

    #[test]
    fn project_page_deduplicates_tasks_already_shown_in_inbox() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            InboxResponse {
                items: vec![AttentionItem {
                    task_id: TaskId::new("task-1"),
                    task_handle: "web/fix-login".to_string(),
                    reason: "waiting for input".to_string(),
                    priority: 6,
                    recommended_action: "open task".to_string(),
                }],
            },
        );

        app.select_next();
        app.activate_selected();

        let task_rows = app
            .selectables
            .iter()
            .filter(|selectable| {
                matches!(
                    selectable,
                    SelectableKind::Task(task) if task.qualified_handle == "web/fix-login"
                )
            })
            .count();
        let inbox_rows = app
            .selectables
            .iter()
            .filter(|selectable| {
                matches!(
                    selectable,
                    SelectableKind::Inbox(item) if item.task_handle == "web/fix-login"
                )
            })
            .count();

        assert_eq!(inbox_rows, 1);
        assert_eq!(task_rows, 0);
    }

    #[test]
    fn activating_project_opens_project_workflow() {
        let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
        // Projects view: [inbox, project, NewTask]. Skip the inbox to reach the project.
        app.select_next();
        assert!(app.activate_selected().is_none());

        let content = render_to_string(80, 30, &app);
        // Header now shows a breadcrumb instead of a "Project: web" title.
        assert!(content.contains("> web"));
        assert!(content.contains("web/fix-login"));
    }

    #[test]
    fn top_level_back_stays_in_cockpit() {
        let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());

        assert!(!super::handle_back_key(&mut app));
        let content = render_to_string(80, 30, &app);
        assert!(content.contains("Ajax"));
        assert!(content.contains("web"));
    }

    #[test]
    fn top_level_backspace_stays_in_cockpit() {
        let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());

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
            let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());

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
    fn runtime_module_exposes_terminal_mode_commands() {
        assert_eq!(
            crate::runtime::terminal_entry_commands(),
            super::terminal_entry_commands()
        );
        assert_eq!(
            crate::runtime::terminal_exit_commands(),
            super::terminal_exit_commands()
        );
    }

    #[test]
    fn nested_back_returns_to_parent_without_exit() {
        let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
        app.select_next();
        app.activate_selected();

        assert!(!super::handle_back_key(&mut app));
        let content = render_to_string(80, 30, &app);
        assert!(!content.contains("> web"));
    }

    #[test]
    fn nested_backspace_returns_to_parent_without_exit() {
        let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
        app.select_next();
        app.activate_selected();

        assert!(super::is_back_key_event(
            KeyCode::Backspace,
            KeyModifiers::NONE
        ));
        assert!(!super::handle_back_key(&mut app));
        let content = render_to_string(80, 30, &app);
        assert!(!content.contains("> web"));
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
        let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
        app.select_next();
        app.activate_selected();
        assert!(matches!(&app.view, AppView::Project { repo } if repo == "web"));
        let selected_before = app.selected;
        let before = render_to_string(80, 30, &app);

        assert!(!super::is_back_key_event(
            KeyCode::Delete,
            KeyModifiers::NONE
        ));
        assert!(before.contains("> web"));

        let after = render_to_string(80, 30, &app);
        assert_eq!(before, after);
        assert!(matches!(&app.view, AppView::Project { repo } if repo == "web"));
        assert_eq!(app.selected, selected_before);
        assert!(after.contains("> web"));
    }

    #[test]
    fn delete_on_top_level_is_ignored_by_navigation() {
        let app = App::new(sample_repos(), sample_tasks(), sample_inbox());
        let before = render_to_string(80, 30, &app);

        assert!(!super::is_back_key_event(
            KeyCode::Delete,
            KeyModifiers::NONE
        ));

        let after = render_to_string(80, 30, &app);
        assert_eq!(before, after);
        assert!(after.contains("Ajax"));
        assert!(after.contains("web"));
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
        let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
        // Projects = [inbox, project, task]. Drill into project, then activate NewTask.
        app.select_next();
        app.activate_selected();
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
        assert!(content.contains("> new task"));
        assert!(content.contains("Task name"));
        assert!(content.contains("<type a task name>"));
        assert!(!content.contains("Task name  x"));
    }

    #[test]
    fn nested_views_advertise_immediate_back_keys() {
        let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
        app.select_next();
        app.activate_selected();

        let content = render_to_string(80, 30, &app);
        assert!(content.contains("esc/h back"));
    }

    #[test]
    fn help_page_lists_cockpit_shortcuts() {
        let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());

        app.open_help();

        let content = render_to_string(80, 30, &app);
        for expected in [
            "> help",
            "Keyboard shortcuts",
            "up/down",
            "j/k",
            "enter",
            "?",
            "esc/h/backspace",
            "q",
            "mouse scroll",
            "mouse click",
            "new task input",
        ] {
            assert!(content.contains(expected), "missing {expected}");
        }
    }

    #[test]
    fn question_mark_is_the_help_shortcut() {
        assert!(super::is_help_key_event(
            KeyCode::Char('?'),
            KeyModifiers::NONE
        ));
        assert!(super::is_help_key_event(
            KeyCode::Char('/'),
            KeyModifiers::SHIFT
        ));
        assert!(!super::is_help_key_event(
            KeyCode::Char('/'),
            KeyModifiers::NONE
        ));
    }

    #[test]
    fn help_back_returns_to_previous_view() {
        let mut app = app_in_project_view();
        assert!(matches!(&app.view, AppView::Project { repo } if repo == "web"));

        app.open_help();
        assert!(matches!(app.view, AppView::Help { .. }));
        assert!(!super::handle_back_key(&mut app));

        let content = render_to_string(80, 30, &app);
        assert!(matches!(&app.view, AppView::Project { repo } if repo == "web"));
        assert!(content.contains("> web"));
        assert!(!content.contains("Keyboard shortcuts"));
    }

    #[test]
    fn help_escape_returns_to_previous_view() {
        let mut app = app_in_project_view();
        assert!(matches!(&app.view, AppView::Project { repo } if repo == "web"));

        app.open_help();
        assert!(matches!(app.view, AppView::Help { .. }));
        let action = handle_with_noop(
            &mut app,
            Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            10,
        );

        assert!(matches!(action, EventLoopAction::Continue));
        let content = render_to_string(80, 30, &app);
        assert!(matches!(&app.view, AppView::Project { repo } if repo == "web"));
        assert!(content.contains("> web"));
        assert!(!content.contains("Keyboard shortcuts"));
    }

    #[test]
    fn project_view_lists_new_task_first_then_tasks() {
        let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
        // Projects view: [inbox, project, task]. Drill into the project.
        app.select_next();
        app.activate_selected();

        // Project view should be: [NewTask, inbox, task].
        assert!(matches!(
            app.selectables.first(),
            Some(SelectableKind::NewTask { .. })
        ));
        // No action wall — only one task-style row in the middle is dispatched
        // on Enter and that's a Task or Review (not a project-action verb).
        for s in &app.selectables {
            assert!(
                !matches!(s, SelectableKind::TaskAction { .. }),
                "project view must not contain TaskAction rows"
            );
        }

        let content = render_to_string(80, 30, &app);
        assert!(content.contains("start a new task"));
        assert!(!content.contains("reconcile"));
    }

    #[test]
    fn project_view_shows_one_status_row_for_review_task() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            InboxResponse { items: vec![] },
        );
        app.activate_selected();

        let task_rows = app
            .selectables
            .iter()
            .filter(|selectable| {
                matches!(
                    selectable,
                    SelectableKind::Task(task) if task.qualified_handle == "web/fix-login"
                )
            })
            .count();

        assert_eq!(task_rows, 1);
        assert!(
            app.selectables
                .iter()
                .any(|selectable| matches!(selectable, SelectableKind::Task(task) if task.qualified_handle == "web/fix-login"))
        );
        app.select_next();
        let item = app.selected_action().unwrap();
        assert_eq!(item.task_handle, "web/fix-login");
        assert_eq!(item.recommended_action, "open task");
    }

    #[test]
    fn enter_on_task_opens_task_actions_menu() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            InboxResponse { items: vec![] },
        );
        // Projects view (no inbox, no review): [project, task]. Walk to the task.
        app.select_next();
        app.select_next();
        assert!(matches!(
            app.selectables.get(app.selected),
            Some(SelectableKind::Task(_))
        ));

        // Enter opens the per-task action menu, doesn't dispatch directly.
        assert!(app.activate_selected().is_none());
        assert!(matches!(
            &app.view,
            AppView::TaskActions { task, .. }
                if task.qualified_handle == "web/fix-login"
        ));

        // First action in a non-review menu is "open task".
        let item = app.selected_action().unwrap();
        assert_eq!(item.task_handle, "web/fix-login");
        assert_eq!(item.recommended_action, "open task");

        let content = render_to_string(80, 30, &app);
        assert!(content.contains("> web/fix-login"));
        assert!(content.contains("open task"));
        assert!(!content.contains("merge task"));
        assert!(!content.contains("clean task"));
        for hidden_entry in [
            "diff task",
            "check task",
            "review branch",
            "open worktrunk",
            "inspect task",
        ] {
            assert!(
                !content.contains(hidden_entry),
                "menu should not render low-value task action {hidden_entry}"
            );
        }
    }

    #[test]
    fn empty_task_list_does_not_create_task_rows() {
        let mut app = App::new(
            sample_repos(),
            TasksResponse { tasks: vec![] },
            InboxResponse { items: vec![] },
        );
        app.activate_selected();

        assert!(app
            .selectables
            .iter()
            .all(|selectable| !matches!(selectable, SelectableKind::Task(_))));
    }

    #[test]
    fn task_actions_back_returns_to_parent_view() {
        let mut app = app_in_project_view();
        // Project view: [NewTask, inbox, task].
        // Step past NewTask + inbox to the task status row.
        app.select_next();
        app.select_next();
        assert!(matches!(
            app.selectables.get(app.selected),
            Some(SelectableKind::Task(_))
        ));
        app.activate_selected();
        assert!(matches!(app.view, AppView::TaskActions { .. }));

        super::handle_back_key(&mut app);
        assert!(matches!(&app.view, AppView::Project { repo } if repo == "web"));
    }

    #[test]
    fn task_action_dispatches_recommended_action_on_enter() {
        let mut app = app_in_project_view();
        app.select_next();
        app.select_next();
        app.activate_selected(); // open TaskActions menu

        // All task status rows open the same task action menu.
        let item = app.activate_selected().unwrap();
        assert_eq!(item.task_handle, "web/fix-login");
        assert_eq!(item.recommended_action, "open task");
    }

    #[test]
    fn task_action_menu_uses_core_action_catalog_labels() {
        let labels = RecommendedAction::task_picker_menu()
            .iter()
            .map(|action| action.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec![
                RecommendedAction::OpenTask.as_str(),
                RecommendedAction::MergeTask.as_str(),
                RecommendedAction::CleanTask.as_str(),
                RecommendedAction::RemoveTask.as_str(),
            ]
        );
    }

    #[test]
    fn task_action_menu_uses_only_product_task_actions() {
        let product_task_actions = RecommendedAction::cockpit_product_actions()
            .iter()
            .copied()
            .filter(|action| {
                matches!(
                    action,
                    RecommendedAction::OpenTask
                        | RecommendedAction::MergeTask
                        | RecommendedAction::CleanTask
                        | RecommendedAction::RemoveTask
                )
            })
            .map(|action| action.as_str())
            .collect::<Vec<_>>();
        let task_menu_actions = RecommendedAction::task_picker_menu()
            .iter()
            .map(|action| action.as_str())
            .collect::<Vec<_>>();

        assert_eq!(task_menu_actions, product_task_actions);
        assert!(!task_menu_actions.contains(&RecommendedAction::OpenTrunk.as_str()));
        assert!(!task_menu_actions.contains(&"check task"));
        assert!(!task_menu_actions.contains(&"diff task"));
    }

    #[test]
    fn task_picker_actions_have_dedicated_render_metadata() {
        for action in RecommendedAction::task_picker_menu() {
            let chrome = action_chrome(action.as_str());
            assert_ne!(chrome.glyph, ".", "{action:?}");
        }

        let open = action_chrome(RecommendedAction::OpenTask.as_str());
        assert_eq!(open.glyph_color, primary_accent());
        assert_eq!(open.label_color, primary_accent());

        let action = RecommendedAction::MergeTask;
        let chrome = action_chrome(action.as_str());
        assert_eq!(chrome.glyph_color, secondary_accent(), "{action:?}");
        assert_eq!(chrome.label_color, secondary_accent(), "{action:?}");
    }

    #[test]
    fn current_core_actions_have_dedicated_render_metadata() {
        for action in RecommendedAction::all() {
            let chrome = action_chrome(action.as_str());

            assert_ne!(chrome.glyph, ".", "{action:?}");
        }
    }

    #[test]
    fn actions_module_exposes_typed_action_chrome() {
        let chrome = crate::actions::recommended_action_chrome(RecommendedAction::OpenTask);

        assert_eq!(chrome.glyph, ">");
        assert_eq!(chrome.label_color, primary_accent());
    }

    #[test]
    fn cockpit_state_module_exposes_state_transitions() {
        let mut app =
            crate::cockpit_state::App::new(sample_repos(), sample_tasks(), sample_inbox());

        app.select_next();
        assert!(app.activate_selected().is_none());
        assert!(matches!(
            &app.view,
            crate::cockpit_state::AppView::Project { repo } if repo == "web"
        ));

        app.open_help();
        assert!(matches!(
            app.view,
            crate::cockpit_state::AppView::Help { .. }
        ));
    }

    #[test]
    fn navigation_module_classifies_back_keys() {
        assert!(crate::navigation::is_back_key_event(
            KeyCode::Esc,
            KeyModifiers::NONE
        ));
        assert!(!crate::navigation::is_back_key_event(
            KeyCode::Char('x'),
            KeyModifiers::NONE
        ));
    }

    #[test]
    fn input_module_handles_navigation_events() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            InboxResponse { items: vec![] },
        );

        let action = crate::input::handle_cockpit_event(
            &mut app,
            Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            10,
            &mut NoopHandler,
        )
        .unwrap();

        assert!(matches!(action, EventLoopAction::Continue));
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn layout_module_exposes_selectable_row_ranges() {
        assert_eq!(
            crate::layout::selectable_row_ranges([1, 3, 5]),
            vec![1..2, 3..4, 5..6]
        );
    }

    #[test]
    fn rendering_module_exposes_status_palette() {
        assert_eq!(
            crate::rendering::bucket_color(crate::rendering::StatusBucket::Active),
            primary_accent()
        );
    }

    #[test]
    fn rendering_module_exposes_screen_renderer() {
        let app = App::new(sample_repos(), sample_tasks(), sample_inbox());
        let backend = TestBackend::new(80, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| crate::rendering::render_ui(frame, &app))
            .unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Ajax"));
    }

    #[test]
    fn enter_on_inbox_row_opens_task_actions_with_recommendation_preselected() {
        let inbox = InboxResponse {
            items: vec![AttentionItem {
                task_id: TaskId::new("task-1"),
                task_handle: "web/fix-login".to_string(),
                reason: "agent is running".to_string(),
                priority: 90,
                recommended_action: "open task".to_string(),
            }],
        };
        let mut app = App::new(sample_repos(), sample_tasks(), inbox);
        // Top-level Projects view: [inbox row, project, task]. Default selection is the inbox.
        assert!(matches!(
            app.selectables.get(app.selected),
            Some(SelectableKind::Inbox(_))
        ));

        assert!(app.activate_selected().is_none());
        assert!(matches!(&app.view, AppView::TaskActions { .. }));

        let inbox = InboxResponse {
            items: vec![AttentionItem {
                task_id: TaskId::new("task-1"),
                task_handle: "web/fix-login".to_string(),
                reason: "review ready".to_string(),
                priority: 30,
                recommended_action: "merge task".to_string(),
            }],
        };
        let mut tasks = sample_tasks();
        tasks.tasks[0].lifecycle_status = "Reviewable".to_string();
        tasks.tasks[0].actions = vec![
            RecommendedAction::OpenTask.as_str().to_string(),
            RecommendedAction::MergeTask.as_str().to_string(),
        ];
        let mut app = App::new(sample_repos(), tasks, inbox);
        assert!(app.activate_selected().is_none());
        let item = app.selected_action().unwrap();
        assert_eq!(item.recommended_action, "merge task");
    }

    #[test]
    fn project_view_has_no_reconcile_action() {
        let app = app_in_project_view();

        assert!(app
            .selectables
            .iter()
            .all(|selectable| !matches!(selectable, SelectableKind::TaskAction { .. })));
        assert!(render_to_string(80, 30, &app).contains("start a new task"));
        assert!(!render_to_string(80, 30, &app).contains("reconcile"));
    }

    #[test]
    fn selected_project_only_shows_that_projects_tasks() {
        let repos = ReposResponse {
            repos: vec![
                RepoSummary {
                    name: "web".to_string(),
                    path: "/Users/matt/Desktop/Projects/web".to_string(),
                    active_tasks: 1,
                    attention_items: 1,
                    reviewable_tasks: 0,
                    cleanable_tasks: 0,
                },
                RepoSummary {
                    name: "api".to_string(),
                    path: "/Users/matt/Desktop/Projects/api".to_string(),
                    active_tasks: 1,
                    attention_items: 0,
                    reviewable_tasks: 0,
                    cleanable_tasks: 0,
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
                    actions: vec![RecommendedAction::OpenTask.as_str().to_string()],
                },
                TaskSummary {
                    id: "task-2".to_string(),
                    qualified_handle: "api/add-cache".to_string(),
                    title: "Add cache".to_string(),
                    lifecycle_status: "Active".to_string(),
                    needs_attention: false,
                    live_status: None,
                    actions: vec![RecommendedAction::OpenTask.as_str().to_string()],
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
        let mut app = App::new(repos, tasks.clone(), inbox);
        // Selectables: [inbox web, inbox api, project web, project api, NewTask].
        // Step past both inbox rows and the web project to land on the api project.
        app.select_next();
        app.select_next();
        app.select_next();
        app.activate_selected();

        let content = render_to_string(100, 50, &app);
        assert!(content.contains("> api"));
        assert!(content.contains("api/add-cache"));
        assert!(!content.contains("web/fix-login"));
        assert!(!content.contains("agent needs input"));
    }

    #[test]
    fn project_new_task_row_opens_title_input() {
        // NewTask is the first selectable inside Project view.
        let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
        // Projects view: [inbox, project, task]. Drill into project.
        app.select_next();
        app.activate_selected();
        // Project view, selected = 0 = NewTask.
        assert!(matches!(
            app.selectables.first(),
            Some(SelectableKind::NewTask { .. })
        ));
        assert!(app.activate_selected().is_none());

        let content = render_to_string(80, 30, &app);
        assert!(content.contains("> new task"));
        assert!(content.contains("Task name"));
    }

    #[test]
    fn new_task_title_input_collects_text_before_pending_action() {
        let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
        // Projects → drill into project; NewTask is selected = 0 in Project view.
        app.select_next();
        app.activate_selected();
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
        let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
        // Projects = [inbox, project, task]. Drill into project, then activate NewTask.
        app.select_next();
        app.activate_selected();
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
        assert!(content.contains("web"));
        assert!(!content.contains("> web"));
        assert!(!content.contains("> new task"));
    }

    #[test]
    fn escape_from_new_task_input_returns_to_ajax_main_menu() {
        let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
        // Projects = [inbox, project, task]. Drill into project, then activate NewTask.
        app.select_next();
        app.activate_selected();
        app.activate_selected();
        app.push_input_char('x');

        assert!(app.go_home());

        let content = render_to_string(80, 30, &app);
        assert!(content.contains("Ajax"));
        assert!(content.contains("web"));
        assert!(!content.contains("> web"));
        assert!(!content.contains("> new task"));
        assert!(!content.contains("Task name"));
    }

    #[test]
    fn feed_inbox_items_render_handle_reason_and_action() {
        let app = App::new(sample_repos(), sample_tasks(), sample_inbox());
        let content = render_to_string(80, 30, &app);
        assert!(content.contains("web/fix-login"));
        assert!(content.contains("agent needs input"));
        assert!(content.contains("open task"));
    }

    #[test]
    fn waiting_for_input_inbox_items_use_yellow_chrome() {
        let item = AttentionItem {
            task_id: TaskId::new("task-1"),
            task_handle: "web/fix-login".to_string(),
            reason: "waiting for input".to_string(),
            priority: 6,
            recommended_action: "open task".to_string(),
        };

        assert_eq!(inbox_item_accent(&item), secondary_accent());
    }

    #[test]
    fn interactive_cockpit_renders_to_narrow_buffer() {
        let app = App::new(sample_repos(), sample_tasks(), sample_inbox());
        let content = render_to_string(50, 24, &app);
        assert!(content.contains("Ajax"));
        assert!(content.contains("web/fix-login"));
        assert!(content.contains("agent needs input"));
    }

    #[test]
    fn select_prev_clamps_at_zero() {
        let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
        app.select_prev();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn select_next_walks_inbox_project_newtask_status() {
        let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
        // Projects view: [inbox, project, task].
        assert_eq!(app.selected, 0);
        app.select_next();
        assert_eq!(app.selected, 1);
        app.select_next();
        assert_eq!(app.selected, 2);
        assert_eq!(
            app.selected_action().unwrap().recommended_action,
            "open task"
        );
        // clamps at last
        app.select_next();
        assert_eq!(app.selected, 2);
    }

    #[test]
    fn select_at_feed_row_lands_on_correct_selectable() {
        let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
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
    fn selectable_row_layout_comes_from_rendered_feed_rows() {
        let mut app = app_in_project_view();
        app.select_next();
        app.activate_selected();

        let expected = selectable_feed_rows(&app)
            .into_iter()
            .map(|row| row..row + 1)
            .collect::<Vec<_>>();

        assert_eq!(selectable_row_layout(&app), expected);
    }

    #[test]
    fn new_task_is_always_present_even_when_other_sections_empty() {
        let mut app = App::new(
            sample_repos(),
            TasksResponse { tasks: vec![] },
            InboxResponse { items: vec![] },
        );
        // Top-level holds only the project; drilling in always shows NewTask first.
        app.activate_selected();
        assert!(matches!(
            app.selectables.first(),
            Some(SelectableKind::NewTask { .. })
        ));
        let item = app.selected_action().unwrap();
        assert_eq!(item.recommended_action, "new task");
    }

    #[test]
    fn selected_action_for_inbox_uses_recommended_action() {
        let app = App::new(sample_repos(), sample_tasks(), sample_inbox());
        // Projects view: [inbox, project, NewTask] — inbox is the initial selection.
        let item = app.selected_action().unwrap();
        assert_eq!(item.task_handle, "web/fix-login");
        assert_eq!(item.recommended_action, "open task");
    }

    #[test]
    fn selected_action_for_task_uses_single_open_row() {
        let mut app = App::new(
            sample_repos(),
            sample_tasks(),
            InboxResponse { items: vec![] },
        );
        // Projects view (no inbox): [project, task]. Drill into the project.
        app.activate_selected();
        // Project view (no inbox): [NewTask, task]. Step past NewTask.
        app.select_next();
        let item = app.selected_action().unwrap();
        assert_eq!(item.task_handle, "web/fix-login");
        assert_eq!(item.recommended_action, "open task");
    }

    #[test]
    fn reload_updates_app_data_and_clamps_selection() {
        let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
        app.selected = 99;
        app.reload(
            sample_repos(),
            TasksResponse { tasks: vec![] },
            InboxResponse { items: vec![] },
        );
        // Only the project row remains at top level → clamps to it.
        assert_eq!(app.selected, 0);
        assert_eq!(
            app.selected_action().unwrap().recommended_action,
            "select project"
        );
    }

    #[test]
    fn refresh_after_removed_task_returns_to_main_page() {
        let mut app = app_in_project_view();
        app.select_next();
        app.activate_selected();
        assert!(matches!(&app.view, AppView::TaskActions { .. }));

        super::handle_action_result(
            &mut app,
            Ok(ActionOutcome::Refresh {
                repos: sample_repos(),
                tasks: TasksResponse { tasks: vec![] },
                inbox: InboxResponse { items: vec![] },
            }),
        )
        .unwrap();

        assert!(matches!(&app.view, AppView::Projects));
        assert_eq!(
            app.selected_action().unwrap().recommended_action,
            "select project"
        );
    }

    #[test]
    fn ensure_visible_scrolls_viewport_to_selected() {
        let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
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
        let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
        app.flash("done".to_string());
        assert!(app.flash.is_some());
    }

    #[test]
    fn action_errors_set_flash_and_stay_in_ajax() {
        let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());

        let pending = super::handle_action_result(
            &mut app,
            Err(std::io::Error::other("git exited with status 42")),
        )
        .unwrap();

        assert!(pending.is_none());
        assert_eq!(
            app.flash.as_ref().map(|(message, _)| message.as_str()),
            Some("git exited with status 42")
        );
    }

    #[test]
    fn task_action_confirmation_requires_second_activation() {
        let mut app = app_in_project_view();
        app.select_next();
        app.select_next();
        app.activate_selected();
        let mut handler = ConfirmHandler::default();

        let first = handle_cockpit_event(
            &mut app,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            10,
            &mut handler,
        )
        .unwrap();
        assert!(matches!(first, EventLoopAction::Continue));
        assert_eq!(handler.asked, 1);
        assert_eq!(handler.confirmed, 0);
        assert_eq!(
            app.flash.as_ref().map(|(message, _)| message.as_str()),
            Some("press enter again to confirm")
        );

        let second = handle_cockpit_event(
            &mut app,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            10,
            &mut handler,
        )
        .unwrap();
        assert!(matches!(second, EventLoopAction::Continue));
        assert_eq!(handler.asked, 1);
        assert_eq!(handler.confirmed, 1);
    }
}

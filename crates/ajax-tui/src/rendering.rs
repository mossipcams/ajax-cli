use ajax_core::{
    models::Annotation,
    output::{RepoSummary, TaskCard},
    ui_state::UiState,
};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::{
    actions,
    cockpit_state::{AppView, SelectableKind, Severity},
    palette::{
        accent_danger as danger_accent, accent_primary as primary_accent, accent_success,
        accent_warning as secondary_accent, selected_highlight, text_chrome as subtle_text,
        text_data as muted_text,
    },
    App,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusBucket {
    Active,
    NeedsYou,
    Stuck,
    Done,
    Idle,
}

pub(crate) fn bucket_color(bucket: StatusBucket) -> Color {
    match bucket {
        StatusBucket::Active => primary_accent(),
        StatusBucket::NeedsYou => secondary_accent(),
        StatusBucket::Stuck => danger_accent(),
        StatusBucket::Done => accent_success(),
        StatusBucket::Idle => muted_text(),
    }
}

pub(crate) fn bucket_glyph(bucket: StatusBucket) -> &'static str {
    match bucket {
        StatusBucket::Active => "▸",
        StatusBucket::NeedsYou => "?",
        StatusBucket::Stuck => "!",
        StatusBucket::Done => "✓",
        StatusBucket::Idle => "·",
    }
}

pub(crate) fn render_ui(frame: &mut Frame, app: &App) {
    let show_notice = app.current_notice().is_some();
    let mut constraints: Vec<Constraint> = vec![Constraint::Length(1)];
    constraints.push(Constraint::Min(0));
    if show_notice {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Length(1));
    let chunks = Layout::vertical(constraints).split(frame.area());

    let mut idx = 0;
    render_header(frame, app, chunks[idx]);
    idx += 1;
    render_feed(frame, app, chunks[idx]);
    idx += 1;
    if show_notice {
        render_notice_row(frame, app, chunks[idx]);
        idx += 1;
    }
    render_status_bar(frame, app, chunks[idx]);
}

pub(crate) fn ui_state_bucket(state: UiState) -> StatusBucket {
    match state {
        UiState::Blocked => StatusBucket::NeedsYou,
        UiState::NeedsInput => StatusBucket::NeedsYou,
        UiState::Running => StatusBucket::Active,
        UiState::ReviewReady => StatusBucket::NeedsYou,
        UiState::SafeMerge => StatusBucket::Done,
        UiState::Cleanable => StatusBucket::Done,
        UiState::Failed => StatusBucket::Stuck,
        UiState::Idle => StatusBucket::Idle,
        UiState::Archived => StatusBucket::Idle,
    }
}

pub(crate) fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let mut parts = vec![Span::styled(
        " Ajax",
        Style::default()
            .fg(primary_accent())
            .add_modifier(Modifier::BOLD),
    )];

    let crumb_sep = || Span::styled(" > ", Style::default().fg(subtle_text()));
    let crumb_style = Style::default()
        .fg(primary_accent())
        .add_modifier(Modifier::BOLD);

    match &app.view {
        AppView::Projects => {}
        AppView::Project { repo } => {
            parts.push(crumb_sep());
            parts.push(Span::styled(repo.clone(), crumb_style));
        }
        AppView::NewTaskInput { repo, .. } => {
            parts.push(crumb_sep());
            parts.push(Span::styled(repo.clone(), crumb_style));
            parts.push(crumb_sep());
            parts.push(Span::styled("start", Style::default().fg(primary_accent())));
        }
        AppView::Help { .. } => {
            parts.push(crumb_sep());
            parts.push(Span::styled(
                "help",
                Style::default().fg(secondary_accent()),
            ));
        }
    }

    if matches!(app.view, AppView::Projects) {
        let right_text = format!("{} repos", app.repos.repos.len());
        let left_width: usize = parts.iter().map(|s| s.content.chars().count()).sum();
        let right_width = right_text.chars().count();
        let pad = (area.width as usize)
            .saturating_sub(left_width + right_width)
            .saturating_sub(1);
        parts.push(Span::raw(" ".repeat(pad)));
        parts.push(Span::styled(
            right_text,
            Style::default().fg(secondary_accent()),
        ));
    }

    frame.render_widget(Paragraph::new(Line::from(parts)), area);
}

fn notice_glyph(severity: Severity) -> &'static str {
    match severity {
        Severity::Confirm => ">",
        Severity::Error => "!",
        Severity::Success => ".",
        Severity::Hint => "-",
    }
}

fn notice_color(severity: Severity) -> Color {
    match severity {
        Severity::Confirm => primary_accent(),
        Severity::Error => danger_accent(),
        Severity::Success => secondary_accent(),
        Severity::Hint => subtle_text(),
    }
}

fn render_notice_row(frame: &mut Frame, app: &App, area: Rect) {
    let Some(notice) = app.current_notice() else {
        return;
    };
    let style = Style::default()
        .fg(notice_color(notice.severity))
        .add_modifier(Modifier::BOLD);
    let text = format!(" {} {}", notice_glyph(notice.severity), notice.msg);
    frame.render_widget(Paragraph::new(Line::from(Span::styled(text, style))), area);
}

pub(crate) fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
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
    frame.render_widget(Paragraph::new(Line::from(parts)), area);
}

pub(crate) fn task_glyph(card: &TaskCard) -> Span<'static> {
    let bucket = ui_state_bucket(card.ui_state);
    Span::styled(
        bucket_glyph(bucket),
        Style::default()
            .fg(bucket_color(bucket))
            .add_modifier(Modifier::BOLD),
    )
}

pub(crate) fn priority_accent(priority: u32) -> Color {
    if priority < 20 {
        danger_accent()
    } else if priority < 50 {
        secondary_accent()
    } else {
        primary_accent()
    }
}

pub(crate) fn action_glyph(action: &str) -> Span<'static> {
    let chrome = actions::action_chrome(action);
    Span::styled(chrome.glyph, chrome.glyph_style)
}

pub(crate) fn project_subtitle(repo: &RepoSummary) -> String {
    let mut parts = Vec::new();
    if repo.active_tasks > 0 {
        parts.push(format!("{} active", repo.active_tasks));
    }
    if repo.attention_items > 0 {
        let verb = if repo.attention_items == 1 {
            "needs"
        } else {
            "need"
        };
        parts.push(format!("{} {verb} you", repo.attention_items));
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

fn column_separator() -> Span<'static> {
    Span::styled("|", Style::default().fg(subtle_text()))
}

fn task_row_spans(t: &TaskCard) -> Vec<Span<'static>> {
    let bold = Modifier::BOLD;
    let mut action_chars = t.primary_action.as_str().chars();
    let action_label = match action_chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + action_chars.as_str(),
        None => String::new(),
    };
    let chrome = crate::actions::operator_action_chrome(t.primary_action);
    vec![
        Span::styled(
            t.qualified_handle.clone(),
            Style::default()
                .fg(bucket_color(ui_state_bucket(t.ui_state)))
                .add_modifier(bold),
        ),
        column_separator(),
        Span::styled(t.status_label.clone(), Style::default().fg(muted_text())),
        column_separator(),
        Span::styled(action_label, chrome.label_style),
        Span::raw(" "),
        Span::styled(chrome.glyph.to_string(), chrome.glyph_style),
    ]
}

fn render_row(
    is_selected: bool,
    glyph: Span<'static>,
    mut spans: Vec<Span<'static>>,
) -> ListItem<'static> {
    let prefix = if is_selected {
        Span::styled(
            ">",
            Style::default()
                .fg(primary_accent())
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw(" ")
    };
    let mut all = vec![prefix, glyph, Span::raw(" ")];
    all.append(&mut spans);
    ListItem::new(Line::from(all))
}

pub(crate) fn render_selectable(s: &SelectableKind, is_selected: bool) -> ListItem<'static> {
    let bold = Modifier::BOLD;
    let dim = Style::default().fg(muted_text());
    match s {
        SelectableKind::Inbox(item) => {
            let accent = priority_accent(item.severity);
            let (repo, task_id) = item
                .task_handle
                .split_once('/')
                .unwrap_or((item.task_handle.as_str(), ""));
            render_row(
                is_selected,
                Span::styled(
                    "!",
                    Style::default().fg(accent).add_modifier(Modifier::BOLD),
                ),
                vec![
                    Span::styled(
                        repo.to_string(),
                        Style::default().fg(accent).add_modifier(bold),
                    ),
                    column_separator(),
                    Span::styled(
                        task_id.to_string(),
                        Style::default().fg(accent).add_modifier(bold),
                    ),
                    column_separator(),
                    Span::styled(item.reason.clone(), Style::default().fg(accent)),
                ],
            )
        }
        SelectableKind::Project(repo) => {
            let (glyph, name_color) = if repo.active_tasks > 0 {
                (
                    Span::styled(
                        "*",
                        Style::default()
                            .fg(primary_accent())
                            .add_modifier(Modifier::BOLD),
                    ),
                    primary_accent(),
                )
            } else {
                (Span::raw(" "), muted_text())
            };
            render_row(
                is_selected,
                glyph,
                vec![
                    Span::styled(
                        repo.name.clone(),
                        Style::default().fg(name_color).add_modifier(bold),
                    ),
                    column_separator(),
                    Span::styled(project_subtitle(repo), dim),
                ],
            )
        }
        SelectableKind::NewTask { .. } => render_row(
            is_selected,
            action_glyph("start"),
            vec![Span::styled(
                "start a new task",
                Style::default().fg(primary_accent()).add_modifier(bold),
            )],
        ),
        SelectableKind::TaskAction { action, .. } => render_row(
            is_selected,
            action_glyph(action),
            vec![Span::styled(
                action.clone(),
                actions::action_chrome(action).label_style,
            )],
        ),
        SelectableKind::Task(t) => render_row(is_selected, task_glyph(t), task_row_spans(t)),
    }
}

pub(crate) fn build_feed(app: &App, _width: usize) -> (Vec<ListItem<'static>>, Vec<usize>) {
    let mut rows: Vec<ListItem<'static>> = Vec::new();
    let mut sel_to_row: Vec<usize> = Vec::new();

    rows.push(ListItem::new(Line::from("")));

    if let AppView::NewTaskInput { title, .. } = &app.view {
        let display_title = if title.is_empty() {
            "<type a task name>".to_string()
        } else {
            title.clone()
        };
        rows.push(render_row(
            false,
            action_glyph("start"),
            vec![
                Span::styled(
                    "Task name  ",
                    Style::default()
                        .fg(primary_accent())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(display_title, Style::default().fg(primary_accent())),
            ],
        ));
        return (rows, sel_to_row);
    }

    if matches!(app.view, AppView::Help { .. }) {
        rows.push(render_row(
            false,
            action_glyph("help"),
            vec![Span::styled(
                "Keyboard shortcuts",
                Style::default()
                    .fg(primary_accent())
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
                "start input",
                "type a title; backspace erases before going back",
            ),
        ] {
            rows.push(render_row(
                false,
                Span::styled(".", Style::default().fg(subtle_text())),
                vec![
                    Span::styled(
                        format!("{key:<18}"),
                        Style::default().fg(secondary_accent()),
                    ),
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
            AppView::NewTaskInput { .. } => "enter a task name",
            AppView::Help { .. } => "keyboard shortcuts",
        };
        rows.push(ListItem::new(Line::from(vec![Span::styled(
            format!("   {msg}"),
            Style::default()
                .fg(subtle_text())
                .add_modifier(Modifier::ITALIC),
        )])));
        return (rows, sel_to_row);
    }

    let mut prev_group: Option<&'static str> = None;
    for (idx, selectable) in app.selectables.iter().enumerate() {
        let group = match selectable {
            SelectableKind::NewTask { .. } => "create",
            SelectableKind::Inbox(_) => "hot",
            SelectableKind::Project(_) => "projects",
            SelectableKind::Task(_) => "tasks",
            SelectableKind::TaskAction { .. } => "task-actions",
        };
        if prev_group != Some(group) && !matches!(selectable, SelectableKind::TaskAction { .. }) {
            let label = match group {
                "hot" => "inbox",
                "create" => "start",
                "projects" => "projects",
                "tasks" => "tasks",
                _ => "",
            };
            let count_suffix = if group == "hot" {
                format!(" ({})", app.inbox.items.len())
            } else {
                String::new()
            };
            let style = if group == "hot" {
                Style::default()
                    .fg(secondary_accent())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(subtle_text())
            };
            rows.push(ListItem::new(Line::from(vec![Span::styled(
                format!("   -- {label}{count_suffix} --"),
                style,
            )])));
        }
        sel_to_row.push(rows.len());
        rows.push(render_selectable(selectable, app.selected == idx));
        if let Some((card, row_reason)) = expanded_card_for(selectable, app) {
            for annotation in &card.annotations {
                if Some(annotation.row_label()) == row_reason {
                    continue;
                }
                rows.push(render_annotation_line(annotation));
            }
        }
        prev_group = Some(group);
    }

    (rows, sel_to_row)
}

fn expanded_card_for<'a>(
    s: &SelectableKind,
    app: &'a App,
) -> Option<(&'a TaskCard, Option<String>)> {
    let open = app.expanded_task.as_ref()?;
    let (task_id, row_reason) = match s {
        SelectableKind::Task(card) if &card.id == open => {
            (&card.id, Some(card.status_label.clone()))
        }
        SelectableKind::Inbox(item) if &item.task_id == open => {
            (&item.task_id, Some(item.reason.clone()))
        }
        _ => return None,
    };
    app.cards
        .iter()
        .find(|c| &c.id == task_id)
        .map(|card| (card, row_reason))
}

fn render_annotation_line(annotation: &Annotation) -> ListItem<'static> {
    let chrome = crate::actions::annotation_chrome(annotation.kind);
    let prefix = Span::raw("      ");
    let connector = Span::styled("├─ ".to_string(), Style::default().fg(subtle_text()));
    let glyph = Span::styled(format!("{} ", chrome.glyph), chrome.glyph_style);
    let label = Span::styled(annotation.row_label(), Style::default().fg(muted_text()));
    ListItem::new(Line::from(vec![prefix, connector, glyph, label]))
}

pub(crate) fn render_feed(frame: &mut Frame, app: &App, area: Rect) {
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

#[cfg(test)]
mod tests {
    #[test]
    fn rendering_does_not_keep_trivial_forwarders() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/rendering.rs"),
        )
        .unwrap();

        for forwarder in [
            "primary_accent",
            "secondary_accent",
            "danger_accent",
            "muted_text",
            "subtle_text",
            "selected_highlight",
            "empty_state",
            "action_chrome",
            "action_label_style",
            "card_bucket",
            "inbox_glyph",
            "inbox_item_accent",
            "group_of",
            "project_glyph",
            "project_name_color",
            "task_handle_color",
            "task_status_label",
            "task_row_label",
            "title_case",
            "selectable_feed_rows",
            "blank_row",
            "section_header_label",
            "section_header_row",
        ] {
            let function_name = ["fn ", forwarder].concat();
            assert!(!source.contains(&function_name), "{forwarder}");
        }
    }
}

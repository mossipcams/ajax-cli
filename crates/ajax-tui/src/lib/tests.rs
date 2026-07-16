use crate::palette::{
    accent_danger as danger_accent, accent_primary as primary_accent, accent_success,
    accent_warning as secondary_accent, selected_highlight, text_chrome as subtle_text,
    text_data as muted_text,
};

use super::{
    action_glyph, bucket_color, bucket_glyph, feed_top_row, handle_cockpit_event, priority_accent,
    project_subtitle, render_cockpit, render_ui, selectable_row_layout, task_glyph, ActionOutcome,
    App, AppView, CockpitEventHandler, CockpitSnapshot, EventLoopAction, PendingAction,
    SelectableKind, StatusBucket,
};
use ajax_core::{
    models::{
        Annotation, AnnotationKind, CockpitActionItem, Evidence, LifecycleStatus, OperatorAction,
        TaskId,
    },
    output::{AnnotationItem, InboxResponse, RepoSummary, ReposResponse, TaskCard},
    ui_state::TaskStatus,
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
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

fn sample_card(
    id: &str,
    handle: &str,
    title: &str,
    status: TaskStatus,
    lifecycle: LifecycleStatus,
) -> TaskCard {
    TaskCard {
        id: TaskId::new(id),
        qualified_handle: handle.to_string(),
        title: title.to_string(),
        status,
        status_explanation: Some(status.as_str().to_string()),
        lifecycle,
        last_activity_at: std::time::UNIX_EPOCH,
        annotations: Vec::new(),
        primary_action: OperatorAction::Resume,
        available_actions: vec![OperatorAction::Resume],
        remediations: Vec::new(),
    }
}

fn sample_tasks() -> Vec<TaskCard> {
    vec![sample_card(
        "task-1",
        "web/fix-login",
        "Fix login",
        TaskStatus::Waiting,
        LifecycleStatus::Active,
    )]
}

fn sample_tasks_with_count(count: usize) -> Vec<TaskCard> {
    (0..count)
        .map(|idx| {
            sample_card(
                &format!("task-{idx}"),
                &format!("web/task-{idx}"),
                &format!("Task {idx}"),
                TaskStatus::Idle,
                LifecycleStatus::Active,
            )
        })
        .collect()
}

fn sample_inbox() -> InboxResponse {
    InboxResponse {
        items: vec![AnnotationItem {
            task_id: TaskId::new("task-99"),
            task_handle: "web/fix-login".to_string(),
            reason: "needs_input".to_string(),
            severity: 30,
            action: OperatorAction::Resume,
        }],
    }
}

#[test]
fn cockpit_palette_maps_accents_to_status_buckets() {
    assert_eq!(primary_accent(), bucket_color(StatusBucket::Active));
    assert_eq!(secondary_accent(), bucket_color(StatusBucket::NeedsYou));
    assert_eq!(danger_accent(), bucket_color(StatusBucket::Stuck));
    assert_eq!(muted_text(), bucket_color(StatusBucket::Idle));
    assert_eq!(subtle_text(), Color::Indexed(244));
}

#[test]
fn palette_has_no_other_hardcoded_colors_in_production() {
    let banned = [
        "Color::White",
        "Color::Yellow",
        "Color::LightYellow",
        "Color::LightCyan",
        "Color::LightGreen",
        "Color::LightBlue",
        "Color::LightMagenta",
        "Color::Red",
        "Color::Green",
        "Color::Blue",
        "Color::Cyan",
        "Color::Magenta",
        "Color::Black",
        "Color::Gray",
        "Color::DarkGray",
    ];
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for relative in ["src/lib.rs", "src/actions.rs", "src/rendering.rs"] {
        let path = manifest_dir.join(relative);
        let source = std::fs::read_to_string(&path).unwrap();
        let cutoff = source.find("mod tests {").unwrap_or(source.len());
        let production = &source[..cutoff];
        for substring in banned {
            assert!(
                !production.contains(substring),
                "{relative} production should not contain `{substring}` — route through palette.rs"
            );
        }
    }
}

#[test]
fn palette_surface_is_four_accents_plus_two_greys() {
    let entries = [
        ("accent_primary", primary_accent(), Color::Indexed(110)),
        ("accent_warning", secondary_accent(), Color::Indexed(179)),
        ("accent_danger", danger_accent(), Color::Indexed(174)),
        ("accent_success", accent_success(), Color::Indexed(108)),
        ("text_data", muted_text(), Color::Indexed(248)),
        ("text_chrome", subtle_text(), Color::Indexed(244)),
    ];
    for (name, actual, expected) in entries {
        assert_eq!(actual, expected, "palette::{name} drift");
    }
    let colors: Vec<Color> = entries.iter().map(|(_, c, _)| *c).collect();
    for (i, a) in colors.iter().enumerate() {
        for b in &colors[i + 1..] {
            assert_ne!(a, b, "palette entries should be distinct");
        }
    }
    assert_eq!(
        selected_highlight(),
        Style::default().add_modifier(Modifier::BOLD)
    );
}

#[test]
fn palette_tiers_are_legible_on_dark_terminals() {
    let subtle = match subtle_text() {
        Color::Indexed(v) => v,
        other => panic!("subtle_text should be Indexed; got {other:?}"),
    };
    let muted = match muted_text() {
        Color::Indexed(v) => v,
        other => panic!("muted_text should be Indexed; got {other:?}"),
    };
    assert!(
        subtle > 240,
        "subtle_text {subtle} must clear the near-black greyscale band"
    );
    assert!(
        muted > subtle,
        "muted_text {muted} must stay brighter than subtle_text {subtle} so quiet data outranks chrome"
    );
}

#[rstest]
#[case(StatusBucket::Active, "▸")]
#[case(StatusBucket::NeedsYou, "?")]
#[case(StatusBucket::Stuck, "!")]
#[case(StatusBucket::Done, "✓")]
#[case(StatusBucket::Idle, "·")]
fn status_buckets_have_stable_glyphs(#[case] bucket: StatusBucket, #[case] glyph: &str) {
    assert_eq!(bucket_glyph(bucket), glyph);
    assert_eq!(crate::rendering::bucket_glyph(bucket), glyph);
}

#[test]
fn row_chrome_helpers_preserve_visible_glyphs_and_styles() {
    let urgent_item = AnnotationItem {
        task_id: TaskId::new("task-1"),
        task_handle: "web/fix".to_string(),
        reason: "waiting for input".to_string(),
        severity: 30,
        action: OperatorAction::Resume,
    };

    assert_eq!(priority_accent(urgent_item.severity), secondary_accent());
    assert_eq!(action_glyph("help").content.as_ref(), "?");
    assert_eq!(
        action_glyph("help").style,
        Style::default()
            .fg(secondary_accent())
            .add_modifier(Modifier::BOLD)
    );
    assert_eq!(action_glyph("unknown").content.as_ref(), ".");
    assert_eq!(
        crate::actions::action_chrome("help").label_style,
        Style::default()
            .fg(primary_accent())
            .add_modifier(Modifier::BOLD)
    );
    assert_eq!(
        crate::actions::action_chrome("unknown").label_style,
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
        "1 active - 2 need you - 3 review - 4 clean"
    );
}

#[test]
fn selected_rows_use_highlight_style() {
    assert_eq!(
        selected_highlight(),
        Style::default().add_modifier(Modifier::BOLD)
    );
}

fn row_text_finder<'a>(buffer: &'a ratatui::buffer::Buffer) -> impl Fn(u16) -> String + 'a {
    let width = buffer.area.width;
    move |y: u16| (0..width).map(|x| buffer[(x, y)].symbol()).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedTaskRow {
    handle: String,
    status: String,
    action: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedInboxRow {
    repo: String,
    task_id: String,
    reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedProjectRow {
    name: String,
    subtitle: String,
}

fn parse_task_row(row: &str) -> Option<ParsedTaskRow> {
    let trimmed = row.trim();
    let slash = trimmed.find('/')?;
    let handle_start = trimmed[..slash]
        .char_indices()
        .rev()
        .find(|(_, c)| !matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_'))
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    let rest = trimmed[handle_start..].trim_start();
    let mut parts = rest.splitn(3, '|');
    let handle = parts.next()?.trim().to_string();
    if !handle.contains('/') {
        return None;
    }
    let status = parts.next()?.trim().to_string();
    let action_raw = parts.next()?.trim().to_string();
    let action = action_raw
        .split_whitespace()
        .next()
        .unwrap_or(action_raw.as_str())
        .to_string();
    Some(ParsedTaskRow {
        handle,
        status,
        action,
    })
}

fn parse_inbox_row(row: &str) -> Option<ParsedInboxRow> {
    let trimmed = row.trim();
    let bang = trimmed.find('!')?;
    let after = trimmed[bang + 1..].trim_start();
    let mut parts = after.splitn(3, '|');
    let repo = parts.next()?.trim().to_string();
    let task_id = parts.next()?.trim().to_string();
    let reason = parts.next()?.trim().to_string();
    if repo.is_empty() || task_id.is_empty() || reason.is_empty() {
        return None;
    }
    Some(ParsedInboxRow {
        repo,
        task_id,
        reason,
    })
}

fn parse_project_row(row: &str) -> Option<ParsedProjectRow> {
    let trimmed = row.trim();
    let pipe = trimmed.find('|')?;
    let name = trimmed[..pipe].split_whitespace().next_back()?.to_string();
    let subtitle = trimmed[pipe + 1..].trim().to_string();
    Some(ParsedProjectRow { name, subtitle })
}

fn buffer_rows(buffer: &ratatui::buffer::Buffer) -> Vec<String> {
    let row_text = row_text_finder(buffer);
    (0..buffer.area.height).map(row_text).collect()
}

fn find_buffer_row(
    buffer: &ratatui::buffer::Buffer,
    predicate: impl Fn(&str) -> bool,
) -> Option<String> {
    buffer_rows(buffer).into_iter().find(|row| predicate(row))
}

fn section_header_row(buffer: &ratatui::buffer::Buffer, section: &str) -> Option<String> {
    find_buffer_row(buffer, |row| {
        let trimmed = row.trim();
        trimmed.starts_with("-- ") && trimmed.contains(section)
    })
}

fn status_bar_line(buffer: &ratatui::buffer::Buffer) -> String {
    let y = buffer.area.height.saturating_sub(1);
    row_text_finder(buffer)(y)
}

fn task_rows_from_buffer(buffer: &ratatui::buffer::Buffer) -> Vec<ParsedTaskRow> {
    buffer_rows(buffer)
        .iter()
        .filter_map(|row| parse_task_row(row))
        .collect()
}

#[allow(dead_code)]
fn task_rows_from_content(content: &str, width: u16) -> Vec<ParsedTaskRow> {
    content
        .as_bytes()
        .chunks(width as usize)
        .map(|chunk| std::str::from_utf8(chunk).unwrap_or(""))
        .filter_map(parse_task_row)
        .collect()
}

fn render_buffer(width: u16, height: u16, app: &App) -> ratatui::buffer::Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| render_ui(f, app)).unwrap();
    terminal.backend().buffer().clone()
}

#[test]
fn task_row_separates_columns_with_pipe() {
    let app = app_in_project_view_with_task_count(1);
    let buffer = render_buffer(80, 20, &app);

    let parsed = task_rows_from_buffer(&buffer)
        .into_iter()
        .find(|row| row.handle == "web/task-0")
        .expect("task row should render");
    assert_eq!(
        parsed,
        ParsedTaskRow {
            handle: "web/task-0".to_string(),
            status: "Idle - Idle".to_string(),
            action: "Resume".to_string(),
        }
    );
}

#[test]
fn task_rows_have_no_trailing_padding_when_handle_lengths_vary() {
    let mut cards = sample_tasks_with_count(2);
    cards[0].qualified_handle = "web/x".to_string();
    cards[1].qualified_handle = "web/very-long-handle-name".to_string();
    let app = App::new(
        sample_repos(),
        cards.clone(),
        InboxResponse { items: vec![] },
    );
    let mut app = app;
    app.activate_selected();
    let buffer = render_buffer(120, 20, &app);
    let row_text = row_text_finder(&buffer);

    for card in &cards {
        let row = (0..buffer.area.height)
            .map(&row_text)
            .find(|t| t.contains(&card.qualified_handle))
            .unwrap_or_else(|| panic!("row for {} should render", card.qualified_handle));
        let after_handle = row
            .split(card.qualified_handle.as_str())
            .nth(1)
            .expect("handle present");
        let leading = after_handle.chars().take_while(|c| *c == ' ').count();
        assert_eq!(
            leading, 0,
            "row for {} should follow handle immediately with '|' (no leading space), got {leading}: {row:?}",
            card.qualified_handle
        );
    }
}

#[test]
fn inbox_row_separates_columns_with_pipe() {
    let app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    let buffer = render_buffer(80, 20, &app);
    let row_text = row_text_finder(&buffer);

    let inbox_row = (0..buffer.area.height)
        .map(&row_text)
        .find(|t| t.contains("fix-login") && t.contains("needs_input"))
        .expect("inbox row should render");
    let pipe_count = inbox_row.matches('|').count();
    assert_eq!(
        pipe_count, 2,
        "inbox row should separate repo, task_id, and reason with two pipes: {inbox_row:?}"
    );
}

#[test]
fn project_row_separates_name_and_subtitle_with_pipe() {
    let app = App::new(
        sample_repos(),
        sample_tasks(),
        InboxResponse { items: vec![] },
    );
    let buffer = render_buffer(80, 20, &app);
    let row_text = row_text_finder(&buffer);

    let project_row = (0..buffer.area.height)
        .map(&row_text)
        .find(|t| t.contains("web") && t.contains("active"))
        .expect("project row should render");
    let pipe_count = project_row.matches('|').count();
    assert!(
        pipe_count >= 1,
        "project row should use a pipe between name and subtitle: {project_row:?}"
    );
}

#[test]
fn inbox_section_header_styled_as_attention() {
    let app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    let backend = TestBackend::new(80, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| render_ui(f, &app)).unwrap();

    let buffer = terminal.backend().buffer();
    let width = buffer.area.width;
    let height = buffer.area.height;

    let row_text = |y: u16| -> String { (0..width).map(|x| buffer[(x, y)].symbol()).collect() };

    let inbox_y = (0..height)
        .find(|&y| row_text(y).contains("-- inbox"))
        .expect("inbox section header should render in the Projects view");
    let inbox_styled = (0..width).any(|x| {
        let cell = &buffer[(x, inbox_y)];
        cell.fg == secondary_accent() && cell.modifier.contains(Modifier::BOLD)
    });
    assert!(
        inbox_styled,
        "inbox header should render in secondary_accent + BOLD so it owns inbox prominence"
    );

    let projects_y = (0..height)
        .find(|&y| row_text(y).contains("-- projects"))
        .expect("projects section header should render in the Projects view");
    let projects_uses_subtle = (0..width).any(|x| {
        let cell = &buffer[(x, projects_y)];
        cell.fg == subtle_text() && !cell.modifier.contains(Modifier::BOLD)
    });
    assert!(
        projects_uses_subtle,
        "non-hot section headers should stay in subtle chrome (no BOLD)"
    );
}

#[test]
fn top_level_status_bar_does_not_advertise_nested_back_action() {
    let app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    let buffer = render_buffer(80, 30, &app);
    let status_bar = status_bar_line(&buffer);

    assert_eq!(
        status_bar.trim_end(),
        " up/down select   enter open   ^T new task   ? help   q quit"
    );
    assert_eq!(status_bar.find("esc/h back"), None);
    assert_eq!(status_bar.find("esc/h erase/back"), None);
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
        cell.modifier.contains(Modifier::BOLD)
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

    // Only the lane header is forbidden; task titles may legitimately
    // contain the word "review".
    assert!(
        !content.lines().any(|line| line.starts_with("Review:")),
        "{content}"
    );
    assert_eq!(
        content
            .lines()
            .find(|line| line.starts_with("web/fix-login"))
            .map(str::to_string),
        Some("web/fix-login\tWaiting - Waiting\tFix login".to_string())
    );
}

#[test]
fn task_rows_render_live_status_when_present() {
    let mut tasks = sample_tasks();
    tasks[0].status = ajax_core::ui_state::TaskStatus::Waiting;
    tasks[0].status_explanation = Some("waiting for approval".to_string());
    let mut app = App::new(sample_repos(), tasks, InboxResponse { items: vec![] });
    app.activate_selected();

    let buffer = render_buffer(80, 30, &app);
    let parsed = task_rows_from_buffer(&buffer)
        .into_iter()
        .find(|row| row.handle == "web/fix-login")
        .expect("task row should render");
    assert_eq!(
        parsed,
        ParsedTaskRow {
            handle: "web/fix-login".to_string(),
            status: "Waiting - waiting for approval".to_string(),
            action: "Resume".to_string(),
        }
    );
}

#[test]
fn waiting_for_input_task_attention_uses_needs_you_chrome() {
    let mut tasks = sample_tasks();
    tasks[0].status = ajax_core::ui_state::TaskStatus::Waiting;
    tasks[0].annotations = vec![Annotation::new(
        AnnotationKind::NeedsMe,
        Evidence::LiveStatus(ajax_core::models::LiveStatusKind::WaitingForInput),
    )];
    let card = &tasks[0];

    assert_eq!(
        crate::rendering::task_card_bucket(card),
        StatusBucket::NeedsYou
    );
    assert_eq!(
        task_glyph(card).style.fg,
        Some(bucket_color(StatusBucket::NeedsYou))
    );
}

#[test]
fn app_starts_with_no_expanded_task() {
    let app = App::new(sample_repos(), sample_tasks(), sample_inbox());

    assert!(app.expanded_task.is_none());
}

#[test]
fn project_selectables_include_drawer_when_task_expanded() {
    let mut tasks = sample_tasks();
    tasks[0].available_actions = vec![OperatorAction::Resume, OperatorAction::Review];
    let mut app = App::new(sample_repos(), tasks, InboxResponse { items: vec![] });
    app.activate_selected();
    let task_idx = app
        .selectables
        .iter()
        .position(|s| matches!(s, SelectableKind::Task(_)))
        .unwrap();
    app.selected = task_idx;
    app.activate_selected();

    let action_count = app
        .selectables
        .iter()
        .filter(|s| matches!(s, SelectableKind::TaskAction { .. }))
        .count();
    assert_eq!(action_count, 2);
}

#[test]
fn selecting_different_task_collapses_drawer() {
    let mut tasks = sample_tasks_with_count(2);
    tasks[0].available_actions = vec![OperatorAction::Resume];
    tasks[1].available_actions = vec![OperatorAction::Resume];
    let mut app = App::new(sample_repos(), tasks, InboxResponse { items: vec![] });
    app.activate_selected();
    let first = app
        .selectables
        .iter()
        .position(|s| matches!(s, SelectableKind::Task(_)))
        .unwrap();
    app.selected = first;
    app.activate_selected();
    assert!(app.expanded_task.is_some());

    // Step past the expanded task's drawer rows onto the next task.
    loop {
        app.select_next();
        if matches!(app.selectables[app.selected], SelectableKind::Task(_)) {
            let cur_id = app.selected_task_id().cloned();
            if cur_id.as_ref() != app.expanded_task.as_ref() {
                break;
            }
        }
        if app.selected + 1 >= app.selectables.len() {
            break;
        }
    }

    // Drawer should collapse once cursor moves off the expanded task.
    // (Implementation collapses via the navigation hook.)
    assert!(app.expanded_task.is_none());
}

#[test]
fn esc_collapses_drawer_keeps_view() {
    let mut tasks = sample_tasks();
    tasks[0].available_actions = vec![OperatorAction::Resume];
    let mut app = App::new(sample_repos(), tasks, InboxResponse { items: vec![] });
    app.activate_selected();
    let idx = app
        .selectables
        .iter()
        .position(|s| matches!(s, SelectableKind::Task(_)))
        .unwrap();
    app.selected = idx;
    app.activate_selected();
    assert!(app.expanded_task.is_some());

    let kept = app.go_back();

    assert!(kept);
    assert!(app.expanded_task.is_none());
    assert!(matches!(app.view, AppView::Project { .. }));
}

#[test]
fn enter_on_task_in_project_toggles_drawer() {
    let mut tasks = sample_tasks();
    tasks[0].available_actions = vec![OperatorAction::Resume, OperatorAction::Review];
    let mut app = App::new(sample_repos(), tasks, InboxResponse { items: vec![] });

    // Drill into Project view for "web".
    app.activate_selected();
    assert!(matches!(app.view, AppView::Project { .. }));

    // Locate the task selectable.
    let task_idx = app
        .selectables
        .iter()
        .position(|s| matches!(s, SelectableKind::Task(_)))
        .expect("task selectable exists");
    app.selected = task_idx;
    let task_id = match &app.selectables[task_idx] {
        SelectableKind::Task(card) => card.id.clone(),
        _ => unreachable!(),
    };
    assert!(app.expanded_task.is_none());

    let dispatched = app.activate_selected();

    assert!(
        dispatched.is_none(),
        "first Enter should expand, not dispatch"
    );
    assert!(matches!(app.view, AppView::Project { .. }));
    assert_eq!(app.expanded_task.as_ref(), Some(&task_id));
}

#[test]
fn cockpit_row_uses_canonical_status_instead_of_annotation_label() {
    let mut tasks = sample_tasks();
    tasks[0].annotations = vec![Annotation::new(
        AnnotationKind::NeedsMe,
        Evidence::LiveStatus(ajax_core::models::LiveStatusKind::WaitingForInput),
    )];

    let content = render_cockpit(&sample_repos(), &tasks, &InboxResponse { items: vec![] });

    assert_eq!(
        content
            .lines()
            .find(|line| line.starts_with("web/fix-login"))
            .map(str::to_string),
        Some("web/fix-login\tWaiting - Waiting\tFix login".to_string()),
        "{content}"
    );
    for forbidden in ["blocked", "needs input", "LiveStatus"] {
        assert_eq!(
            content
                .lines()
                .map(|line| line.find(forbidden))
                .filter(Option::is_some)
                .count(),
            0,
            "unexpected {forbidden:?} in {content}"
        );
    }
}

#[test]
fn cockpit_row_renders_probe_failure_status_verbatim() {
    let mut tasks = sample_tasks();
    tasks[0].status = ajax_core::ui_state::TaskStatus::Error;
    tasks[0].status_explanation = Some("status unavailable: tmux server unavailable".to_string());

    let content = render_cockpit(&sample_repos(), &tasks, &InboxResponse { items: vec![] });

    assert_eq!(
        content
            .lines()
            .find(|line| line.starts_with("web/fix-login"))
            .map(str::to_string),
        Some(
            "web/fix-login\tError - status unavailable: tmux server unavailable\tFix login"
                .to_string()
        ),
        "{content}"
    );
    assert!(
        content
            .lines()
            .all(|line| !line.split_whitespace().any(|word| word == "unknown")),
        "{content}"
    );
}

#[rstest]
#[case(Evidence::SideFlag(ajax_core::models::SideFlag::NeedsInput))]
#[case(Evidence::LiveStatus(ajax_core::models::LiveStatusKind::WaitingForInput))]
fn evidence_label_collapses_needs_input_variants(#[case] evidence: Evidence) {
    assert_eq!(evidence.attention_label(), "needs input");
}

#[test]
fn inbox_section_renders_labeled_header_with_count() {
    let inbox = InboxResponse {
        items: (0..3)
            .map(|i| AnnotationItem {
                task_id: TaskId::new(format!("t{i}").as_str()),
                task_handle: format!("web/task-{i}"),
                reason: "needs input".to_string(),
                severity: 30,
                action: OperatorAction::Resume,
            })
            .collect(),
    };
    let app = App::new(sample_repos(), sample_tasks(), inbox);
    let buffer = render_buffer(80, 30, &app);

    assert_eq!(
        section_header_row(&buffer, "inbox").map(|row| row.trim().to_string()),
        Some("-- inbox (3) --".to_string())
    );
}

#[test]
fn task_row_renders_primary_action_label_and_chrome() {
    let mut tasks = sample_tasks();
    tasks[0].primary_action = OperatorAction::Review;
    tasks[0].status = ajax_core::ui_state::TaskStatus::Waiting;
    tasks[0].status_explanation = Some("review ready".to_string());
    tasks[0].annotations = vec![Annotation::new(
        AnnotationKind::Reviewable,
        Evidence::Lifecycle(LifecycleStatus::Reviewable),
    )];
    let mut app = App::new(sample_repos(), tasks, InboxResponse { items: vec![] });
    // Drill into the project so the task row is unambiguously visible.
    while !matches!(app.view, AppView::Project { .. }) {
        if matches!(
            app.selectables.get(app.selected),
            Some(SelectableKind::Project(_))
        ) {
            app.activate_selected();
        } else {
            app.select_next();
        }
    }

    let buffer = render_buffer(80, 30, &app);
    let parsed = task_rows_from_buffer(&buffer)
        .into_iter()
        .find(|row| row.handle == "web/fix-login")
        .expect("task row should render");
    assert_eq!(
        parsed,
        ParsedTaskRow {
            handle: "web/fix-login".to_string(),
            status: "Waiting - review ready".to_string(),
            action: "Review".to_string(),
        }
    );
}

#[test]
fn cockpit_inbox_lists_supplied_items_in_order() {
    let reviewable = sample_card(
        "task-review",
        "web/review",
        "Review task",
        TaskStatus::Waiting,
        LifecycleStatus::Reviewable,
    );
    let needs_me = sample_card(
        "task-needs-me",
        "web/needs-me",
        "Needs me",
        TaskStatus::Waiting,
        LifecycleStatus::Active,
    );

    let app = App::new(
        sample_repos(),
        vec![reviewable, needs_me],
        InboxResponse {
            items: vec![
                AnnotationItem {
                    task_id: TaskId::new("task-needs-me"),
                    task_handle: "web/needs-me".to_string(),
                    reason: "waiting for input".to_string(),
                    severity: 1,
                    action: OperatorAction::Resume,
                },
                AnnotationItem {
                    task_id: TaskId::new("task-review"),
                    task_handle: "web/review".to_string(),
                    reason: "review ready".to_string(),
                    severity: 3,
                    action: OperatorAction::Review,
                },
            ],
        },
    );

    assert!(matches!(
        app.selectables.first(),
        Some(SelectableKind::Inbox(item)) if item.task_handle == "web/needs-me"
    ));
    assert!(matches!(
        app.selectables.get(1),
        Some(SelectableKind::Inbox(item)) if item.task_handle == "web/review"
    ));
}

#[test]
fn cockpit_header_summarizes_review_and_cleanup_pressure() {
    let mut repos = sample_repos();
    repos.repos[0].cleanable_tasks = 1;
    let app = App::new(repos, sample_tasks(), sample_inbox());
    let buffer = render_buffer(80, 30, &app);

    let project = parse_project_row(
        &find_buffer_row(&buffer, |row| {
            row.contains("web|") && row.contains("active") && row.contains("review")
        })
        .expect("project row should render"),
    )
    .expect("project row should parse");
    assert_eq!(
        project.subtitle,
        "1 active - 1 needs you - 1 review - 1 clean"
    );
}

#[test]
fn project_rows_summarize_operator_work_by_project() {
    let mut repos = sample_repos();
    repos.repos[0].cleanable_tasks = 1;
    let app = App::new(repos, sample_tasks(), sample_inbox());
    let buffer = render_buffer(80, 30, &app);

    let project = parse_project_row(
        &find_buffer_row(&buffer, |row| {
            row.contains("web|") && row.contains("active") && row.contains("review")
        })
        .expect("project row should render"),
    )
    .expect("project row should parse");
    assert_eq!(
        project,
        ParsedProjectRow {
            name: "web".to_string(),
            subtitle: "1 active - 1 needs you - 1 review - 1 clean".to_string(),
        }
    );
}

#[rstest]
#[case(1, "1 needs you")]
#[case(2, "2 need you")]
fn project_subtitle_pluralizes_attention_count(#[case] attention: u32, #[case] expected: &str) {
    let repo = RepoSummary {
        name: "web".to_string(),
        path: "/repo".to_string(),
        active_tasks: 0,
        attention_items: attention,
        reviewable_tasks: 0,
        cleanable_tasks: 0,
    };
    let subtitle = project_subtitle(&repo);
    assert_eq!(subtitle, expected);
}

#[test]
fn inbox_row_uses_two_column_repo_task_layout() {
    let inbox = InboxResponse {
        items: vec![AnnotationItem {
            task_id: TaskId::new("t-1"),
            task_handle: "autosnooze/open-deps".to_string(),
            reason: "needs input".to_string(),
            severity: 30,
            action: OperatorAction::Resume,
        }],
    };
    let app = App::new(sample_repos(), Vec::<TaskCard>::new(), inbox);
    let buffer = render_buffer(120, 30, &app);

    let inbox_row =
        find_buffer_row(&buffer, |row| row.contains('!')).expect("inbox row should render");
    assert_eq!(
        parse_inbox_row(&inbox_row),
        Some(ParsedInboxRow {
            repo: "autosnooze".to_string(),
            task_id: "open-deps".to_string(),
            reason: "needs input".to_string(),
        })
    );
    assert_eq!(
        buffer_rows(&buffer).iter().find(|row| row
            .split_whitespace()
            .any(|word| word == "autosnooze/open-deps")),
        None,
        "repo and task-id should render as separate columns"
    );
    assert_eq!(
        buffer_rows(&buffer).iter().find(|row| {
            let trimmed = row.trim();
            trimmed.split_whitespace().any(|word| word == "Resume")
                && trimmed.split_whitespace().any(|word| word == "!")
        }),
        None,
        "right-side action chrome should not render on inbox rows"
    );
}

#[test]
fn header_shows_repo_count_right_aligned() {
    let mut repos = sample_repos();
    repos.repos.push(RepoSummary {
        name: "api".to_string(),
        path: "/repo/api".to_string(),
        active_tasks: 0,
        attention_items: 0,
        reviewable_tasks: 0,
        cleanable_tasks: 0,
    });
    let app = App::new(repos, sample_tasks(), sample_inbox());
    let backend = TestBackend::new(80, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| render_ui(f, &app)).unwrap();
    let buffer = terminal.backend().buffer();
    let row0: String = (0..buffer.area.width)
        .map(|x| buffer[(x, 0)].symbol())
        .collect();

    assert_eq!(
        row0.split_whitespace().next(),
        Some("Ajax"),
        "row 0 keeps the breadcrumb: {row0:?}"
    );
    assert_eq!(
        row0.split_whitespace().rev().take(2).collect::<Vec<_>>(),
        vec!["repos", "2"],
        "repo count should sit at the right edge of the header: {row0:?}"
    );
}

#[test]
fn projects_view_omits_attention_banner_when_inbox_visible() {
    let app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    let buffer = render_buffer(120, 30, &app);

    assert!(
        !buffer_rows(&buffer)
            .iter()
            .any(|row| row.trim() == "! fix-login: needs_input"),
        "attention banner should be gone"
    );
    assert_eq!(
        section_header_row(&buffer, "inbox").map(|row| row.trim().to_string()),
        Some("-- inbox (1) --".to_string())
    );
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
    refreshed_tasks[0].status = ajax_core::ui_state::TaskStatus::Waiting;
    refreshed_tasks[0].status_explanation = Some("waiting for approval".to_string());

    app.apply_refresh(CockpitSnapshot {
        repos: sample_repos(),
        cards: refreshed_tasks,
        inbox: InboxResponse { items: vec![] },
    });

    assert_eq!(app.selected, selected_before);
    let buffer = render_buffer(80, 30, &app);
    let row = task_rows_from_buffer(&buffer)
        .into_iter()
        .find(|row| row.handle == "web/fix-login")
        .expect("fix-login row should render");
    assert_eq!(row.status, "Waiting - waiting for approval");
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
fn cockpit_brand_does_not_render_in_header() {
    let app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    let buffer = render_buffer(80, 30, &app);
    let header = row_text_finder(&buffer)(0);

    assert_eq!(header.split_whitespace().next(), Some("Ajax"));
    assert_eq!(
        header.find("[AJAX]"),
        None,
        "brand marker should no longer render"
    );
}

#[test]
fn feed_top_row_is_breadcrumb_row_only() {
    let projects_app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    assert_eq!(feed_top_row(&projects_app), 1);

    let mut subview_app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    subview_app.select_next();
    subview_app.activate_selected();
    assert_eq!(feed_top_row(&subview_app), 1);
}

#[test]
fn feed_screen_rows_match_rendered_layout_with_and_without_notice() {
    let mut app = app_in_project_view_with_task_count(3);

    assert_eq!(super::feed_screen_rows(&app, 10), 1..9);

    app.notify_system(
        "done".to_string(),
        super::Severity::Success,
        super::cockpit_state::Origin::UserAction,
    );

    assert_eq!(super::feed_screen_rows(&app, 10), 1..8);
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
    assert_eq!(
        colors
            .iter()
            .copied()
            .filter(|color| *color == primary_accent() || *color == secondary_accent())
            .collect::<std::collections::HashSet<_>>()
            .len(),
        2
    );
    for bad_color in [
        Color::LightCyan,
        Color::LightGreen,
        Color::LightBlue,
        Color::LightMagenta,
    ] {
        assert_eq!(
            colors.iter().filter(|&&c| c == bad_color).count(),
            0,
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
    fn on_action(&mut self, _: &CockpitActionItem) -> std::io::Result<ActionOutcome> {
        Ok(ActionOutcome::Message("ignored".to_string()))
    }
}

struct DeferHandler;

impl CockpitEventHandler for DeferHandler {
    fn on_action(&mut self, item: &CockpitActionItem) -> std::io::Result<ActionOutcome> {
        Ok(ActionOutcome::Defer(PendingAction {
            task_handle: item.task_handle.clone(),
            action: item.action.clone(),
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
    fn on_action(&mut self, _: &CockpitActionItem) -> std::io::Result<ActionOutcome> {
        self.asked += 1;
        Ok(ActionOutcome::Confirm(
            "press enter again to confirm".to_string(),
        ))
    }

    fn on_confirmed_action(&mut self, _: &CockpitActionItem) -> std::io::Result<ActionOutcome> {
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
#[case(0, Some(crate::cockpit_state::NOTICE_TICKS_HINT))]
#[case(1, Some(crate::cockpit_state::NOTICE_TICKS_HINT - 1))]
#[case(crate::cockpit_state::NOTICE_TICKS_HINT, Some(0))]
#[case(crate::cockpit_state::NOTICE_TICKS_HINT + 1, None)]
fn flash_expires_after_final_visible_tick(
    #[case] ticks: u8,
    #[case] expected_remaining: Option<u8>,
) {
    let mut app = app_in_empty_new_task_input();
    assert!(app.submit_input().is_none());

    for _ in 0..ticks {
        app.tick_notices();
    }

    assert_eq!(
        app.current_notice().map(|n| n.ticks_remaining),
        expected_remaining
    );
}

#[test]
fn ensure_visible_leaves_exact_bottom_boundary_stable() {
    let mut app = app_in_project_view_with_task_count(3);
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
    let mut app = app_in_project_view_with_task_count(3);
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
    assert_eq!(app.current_notice().is_some(), flashes_for_empty_submit);
    match code {
        KeyCode::Char('x') => {
            assert!(matches!(&app.view, AppView::NewTaskInput { title, .. } if title == "x"));
        }
        KeyCode::Backspace | KeyCode::Delete => {
            assert!(matches!(&app.view, AppView::NewTaskInput { title, .. } if title.is_empty()));
        }
        KeyCode::Enter => {}
        _ => unreachable!(),
    }
}

#[test]
fn paste_appends_text_while_collecting_task_input() {
    let mut app = app_in_empty_new_task_input();

    let action = handle_with_noop(&mut app, Event::Paste("Fix keyboard gaps".to_string()), 10);

    assert!(matches!(action, EventLoopAction::Continue));
    assert!(
        matches!(&app.view, AppView::NewTaskInput { title, .. } if title == "Fix keyboard gaps")
    );
}

#[test]
fn paste_outside_task_input_is_ignored() {
    let mut app = app_in_project_view_with_task_count(3);
    let selected_before = app.selected;

    let action = handle_with_noop(&mut app, Event::Paste("ignored".to_string()), 10);

    assert!(matches!(action, EventLoopAction::Continue));
    assert_eq!(app.selected, selected_before);
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

#[test]
fn control_q_does_not_quit_cockpit() {
    let mut app = App::new(
        sample_repos(),
        sample_tasks(),
        InboxResponse { items: vec![] },
    );

    let action = handle_with_noop(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL)),
        10,
    );

    assert!(matches!(action, EventLoopAction::Continue));
}

#[test]
fn control_q_returns_to_ajax_main_menu() {
    let mut app = app_in_project_view();

    let action = handle_with_noop(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL)),
        10,
    );

    assert!(matches!(action, EventLoopAction::Continue));
    assert!(matches!(app.view, AppView::Projects));
    assert!(app.expanded_task.is_none());
}

#[test]
fn control_t_does_not_quit_cockpit() {
    let mut app = App::new(
        sample_repos(),
        sample_tasks(),
        InboxResponse { items: vec![] },
    );

    let action = handle_with_noop(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL)),
        10,
    );

    assert!(matches!(action, EventLoopAction::Continue));
}

#[test]
fn control_t_opens_new_task_input_in_project_view() {
    let mut app = app_in_project_view();

    let action = handle_with_noop(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL)),
        10,
    );

    assert!(matches!(action, EventLoopAction::Continue));
    assert!(matches!(
        &app.view,
        AppView::NewTaskInput { repo, title } if repo == "web" && title.is_empty()
    ));
}

#[test]
fn control_t_uses_selected_task_repo_on_projects_view() {
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    while !matches!(
        app.selectables.get(app.selected),
        Some(SelectableKind::Task(_))
    ) {
        app.select_next();
    }

    let action = handle_with_noop(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL)),
        10,
    );

    assert!(matches!(action, EventLoopAction::Continue));
    assert!(matches!(
        &app.view,
        AppView::NewTaskInput { repo, title } if repo == "web" && title.is_empty()
    ));
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
#[case(KeyCode::PageDown, 0, 4)]
#[case(KeyCode::PageUp, 8, 4)]
#[case(KeyCode::Home, 8, 0)]
#[case(KeyCode::End, 0, 10)]
fn page_and_edge_navigation_keys_update_selection(
    #[case] code: KeyCode,
    #[case] start: usize,
    #[case] expected: usize,
) {
    let mut app = app_in_project_view_with_task_count(10);
    app.selected = start;

    let action = handle_with_noop(
        &mut app,
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE)),
        6,
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
            action,
            ..
        }) if action == "resume"
    ));
}

#[rstest]
#[case(MouseEventKind::ScrollDown, 2)]
#[case(MouseEventKind::ScrollUp, 0)]
fn mouse_scroll_updates_selection(#[case] kind: MouseEventKind, #[case] expected: usize) {
    let mut app = app_in_project_view_with_task_count(3);
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
    let mut app = app_in_project_view_with_task_count(3);
    let target = 2;
    let target_feed_row = selectable_row_layout(&app)[target].start;
    let feed_top = super::feed_top_row(&app);

    let action = handle_with_noop(
        &mut app,
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 2,
            row: (target_feed_row + feed_top) as u16,
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
    let feed_top = super::feed_top_row(&app);
    let mouse_row = (target_feed_row - app.viewport_scroll + feed_top) as u16;

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
fn mouse_click_on_notice_row_is_ignored_even_when_scrolled() {
    let mut app = app_in_project_view_with_task_count(12);
    app.selected = 1;
    app.viewport_scroll = 2;
    app.notify_system(
        "confirm action".to_string(),
        super::Severity::Confirm,
        super::cockpit_state::Origin::UserAction,
    );

    let action = handle_with_noop(
        &mut app,
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 2,
            row: 8,
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
    let lines: Vec<&str> = rendered.lines().collect();
    assert_eq!(lines[0], "Ajax Cockpit");
    assert_eq!(lines[1], "Repos: 1");
    assert_eq!(lines[3], "Task Statuses");
    assert_eq!(lines[4], "web/fix-login\tWaiting - Waiting\tFix login");
    assert_eq!(lines[5], "Inbox");
    assert_eq!(lines[6], "web/fix-login: needs_input -> resume");
    assert!(
        !lines.iter().any(|line| line.starts_with("Review:")),
        "{rendered}"
    );
}

#[test]
fn project_drill_in_has_no_inbox_section() {
    // Inbox lives only on the Projects (top) view per option A. The
    // Project drill-in shows each repo task exactly once with its
    // annotation chrome on the row.
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    app.select_next();
    app.activate_selected();

    let project_inbox = app
        .selectables
        .iter()
        .filter(|s| matches!(s, SelectableKind::Inbox(_)))
        .count();
    assert_eq!(project_inbox, 0);
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
    let inbox_pos = content.find("needs_input").unwrap();
    let autodoctor_pos = content.find("autodoctor").unwrap();
    let autosnooze_pos = content.find("autosnooze").unwrap();

    // Inbox precedes both projects.
    assert!(inbox_pos < autodoctor_pos);
    assert!(inbox_pos < autosnooze_pos);
    // Initial selection is the inbox item.
    assert_eq!(app.selected_action().unwrap().action, "resume");
}

#[test]
fn main_page_renders_task_statuses_without_opening_project() {
    let app = App::new(
        sample_repos(),
        sample_tasks(),
        InboxResponse { items: vec![] },
    );
    let buffer = render_buffer(80, 30, &app);

    let parsed = task_rows_from_buffer(&buffer)
        .into_iter()
        .find(|row| row.handle == "web/fix-login")
        .expect("task row should render");
    assert_eq!(
        parsed,
        ParsedTaskRow {
            handle: "web/fix-login".to_string(),
            status: "Waiting - Waiting".to_string(),
            action: "Resume".to_string(),
        }
    );
    assert!(matches!(app.view, AppView::Projects));
    assert!(row_text_finder(&buffer)(0).trim_start().starts_with("Ajax"));
}

#[test]
fn main_page_task_row_enter_expands_drawer_then_dispatches() {
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
    // First Enter on a Task expands the drawer (does not dispatch).
    assert!(app.activate_selected().is_none());
    assert!(matches!(&app.view, AppView::Projects));
    assert!(app.expanded_task.is_some());

    // Second Enter on the first drawer action dispatches.
    let item = app.activate_selected().unwrap();
    assert_eq!(item.task_handle, "web/fix-login");
    assert_eq!(item.action, "resume");
}

#[test]
fn main_page_deduplicates_tasks_already_shown_in_inbox() {
    let app = App::new(
        sample_repos(),
        sample_tasks(),
        InboxResponse {
            items: vec![AnnotationItem {
                task_id: TaskId::new("task-1"),
                task_handle: "web/fix-login".to_string(),
                reason: "waiting for input".to_string(),
                severity: 6,
                action: OperatorAction::Resume,
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
fn projects_view_uses_supplied_inbox_instead_of_rebuilding_from_cards() {
    let mut tasks = sample_tasks();
    tasks[0].annotations = vec![Annotation::new(
        AnnotationKind::NeedsMe,
        Evidence::SideFlag(ajax_core::models::SideFlag::NeedsInput),
    )];
    let app = App::new(sample_repos(), tasks, InboxResponse { items: vec![] });

    let inbox_rows = app
        .selectables
        .iter()
        .filter(|selectable| matches!(selectable, SelectableKind::Inbox(_)))
        .count();
    let task_rows = app
        .selectables
        .iter()
        .filter(|selectable| matches!(selectable, SelectableKind::Task(_)))
        .count();

    assert_eq!(inbox_rows, 0);
    assert_eq!(task_rows, 1);
}

#[test]
fn project_page_lists_each_task_once_without_inbox_section() {
    let mut app = App::new(
        sample_repos(),
        sample_tasks(),
        InboxResponse {
            items: vec![AnnotationItem {
                task_id: TaskId::new("task-1"),
                task_handle: "web/fix-login".to_string(),
                reason: "waiting for input".to_string(),
                severity: 6,
                action: OperatorAction::Resume,
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
        .filter(|selectable| matches!(selectable, SelectableKind::Inbox(_)))
        .count();

    assert_eq!(inbox_rows, 0);
    assert_eq!(task_rows, 1);
}

#[test]
fn activating_project_opens_project_workflow() {
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    // Projects view: [inbox, project, NewTask]. Skip the inbox to reach the project.
    app.select_next();
    assert!(app.activate_selected().is_none());

    assert!(matches!(&app.view, AppView::Project { repo } if repo == "web"));
    let buffer = render_buffer(80, 30, &app);
    assert_eq!(
        row_text_finder(&buffer)(0)
            .split_whitespace()
            .collect::<Vec<_>>(),
        vec!["Ajax", ">", "web"]
    );
    assert_eq!(
        task_rows_from_buffer(&buffer)
            .into_iter()
            .map(|row| row.handle)
            .collect::<Vec<_>>(),
        vec!["web/fix-login".to_string()]
    );
}

#[test]
fn top_level_back_stays_in_cockpit() {
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());

    assert!(!app.go_back());
    assert!(matches!(app.view, AppView::Projects));
    assert_eq!(app.selected, 0);
}

#[test]
fn top_level_backspace_stays_in_cockpit() {
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());

    assert!(crate::navigation::is_back_key_event(
        KeyCode::Backspace,
        KeyModifiers::NONE
    ));
    assert!(!app.go_back());
    assert!(matches!(app.view, AppView::Projects));
    assert_eq!(app.selected, 0);
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

        assert!(crate::navigation::is_back_key_event(code, modifiers));
        assert!(!app.go_back());
        assert!(matches!(app.view, AppView::Projects));
        assert_eq!(app.selected, 0);
    }
}

#[test]
fn nested_back_returns_to_parent_without_exit() {
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    app.select_next();
    app.activate_selected();

    assert!(app.go_back());
    assert!(matches!(app.view, AppView::Projects));
    let buffer = render_buffer(80, 30, &app);
    assert_eq!(row_text_finder(&buffer)(0).find("> web"), None);
}

#[test]
fn nested_backspace_returns_to_parent_without_exit() {
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    app.select_next();
    app.activate_selected();

    assert!(crate::navigation::is_back_key_event(
        KeyCode::Backspace,
        KeyModifiers::NONE
    ));
    assert!(app.go_back());
    assert!(matches!(app.view, AppView::Projects));
    let buffer = render_buffer(80, 30, &app);
    assert_eq!(row_text_finder(&buffer)(0).find("> web"), None);
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
            crate::navigation::is_back_key_event(key, KeyModifiers::NONE),
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
            crate::navigation::is_back_key_event(code, modifiers),
            "{code:?} with {modifiers:?} should navigate back"
        );
    }

    assert!(!crate::navigation::is_back_key_event(
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

    assert!(!crate::navigation::is_back_key_event(
        KeyCode::Delete,
        KeyModifiers::NONE
    ));
    let before_buffer = render_buffer(80, 30, &app);
    assert_eq!(
        row_text_finder(&before_buffer)(0)
            .split_whitespace()
            .collect::<Vec<_>>(),
        vec!["Ajax", ">", "web"]
    );

    let after = render_to_string(80, 30, &app);
    assert_eq!(before, after);
    assert!(matches!(&app.view, AppView::Project { repo } if repo == "web"));
    assert_eq!(app.selected, selected_before);
    let after_buffer = render_buffer(80, 30, &app);
    assert_eq!(
        row_text_finder(&after_buffer)(0)
            .split_whitespace()
            .collect::<Vec<_>>(),
        vec!["Ajax", ">", "web"]
    );
}

#[test]
fn delete_on_top_level_is_ignored_by_navigation() {
    let app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    let before = render_to_string(80, 30, &app);

    assert!(!crate::navigation::is_back_key_event(
        KeyCode::Delete,
        KeyModifiers::NONE
    ));

    let after = render_to_string(80, 30, &app);
    assert_eq!(before, after);
    let buffer = render_buffer(80, 30, &app);
    assert!(row_text_finder(&buffer)(0).trim_start().starts_with("Ajax"));
    assert!(
        app.selectables.iter().any(|selectable| matches!(
            selectable,
            SelectableKind::Project(summary) if summary.name == "web"
        )),
        "top-level view should still list the web project"
    );
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
            crate::navigation::is_input_delete_key(code, modifiers),
            "{code:?} with {modifiers:?} should erase input"
        );
    }

    assert!(!crate::navigation::is_input_delete_key(
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

    assert!(crate::navigation::is_input_delete_key(
        KeyCode::Delete,
        KeyModifiers::NONE
    ));
    assert!(app.go_back());
    assert!(
        matches!(
            &app.view,
            AppView::NewTaskInput { repo, title } if repo == "web" && title.is_empty()
        ),
        "Delete should erase editable text without leaving task title input"
    );

    let buffer = render_buffer(80, 30, &app);
    assert!(matches!(
        &app.view,
        AppView::NewTaskInput { repo, title } if repo == "web" && title.is_empty()
    ));
    let status_bar = status_bar_line(&buffer);
    assert_eq!(
        status_bar.trim_end(),
        " up/down select   enter create   ? help   esc/h erase/back   q quit"
    );
    assert_eq!(
        row_text_finder(&buffer)(0)
            .split_whitespace()
            .collect::<Vec<_>>(),
        vec!["Ajax", ">", "web", ">", "start"]
    );
    let input_row = find_buffer_row(&buffer, |row| row.contains("Task name"))
        .expect("new task input row should render")
        .trim()
        .to_string();
    assert_eq!(input_row, "+ Task name  <type a task name>");
}

#[test]
fn nested_views_advertise_immediate_back_keys() {
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    app.select_next();
    app.activate_selected();

    let status_bar = status_bar_line(&render_buffer(80, 30, &app));
    assert_eq!(
        status_bar.trim_end(),
        " up/down select   enter open   ^T new task   ? help   esc/h back   q quit"
    );
}

#[test]
fn help_page_lists_cockpit_shortcuts() {
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());

    app.open_help();
    assert!(matches!(app.view, AppView::Help { .. }));

    let buffer = render_buffer(80, 30, &app);
    let rows: Vec<String> = buffer_rows(&buffer)
        .iter()
        .map(|row| row.trim().to_string())
        .filter(|row| !row.is_empty())
        .collect();
    assert_eq!(
        rows.iter().find(|row| row.ends_with("Keyboard shortcuts")),
        Some(&"? Keyboard shortcuts".to_string())
    );
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
        let expected = format!(". {key:<18}{label}");
        assert_eq!(
            rows.iter().filter(|row| *row == &expected).count(),
            1,
            "missing help entry for {key:?}: {rows:?}"
        );
    }
    assert_eq!(
        row_text_finder(&buffer)(0)
            .split_whitespace()
            .collect::<Vec<_>>(),
        vec!["Ajax", ">", "help"]
    );
}

#[test]
fn question_mark_is_the_help_shortcut() {
    assert!(crate::navigation::is_help_key_event(
        KeyCode::Char('?'),
        KeyModifiers::NONE
    ));
    assert!(crate::navigation::is_help_key_event(
        KeyCode::Char('/'),
        KeyModifiers::SHIFT
    ));
    assert!(!crate::navigation::is_help_key_event(
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
    assert!(app.go_back());

    assert!(matches!(&app.view, AppView::Project { repo } if repo == "web"));
    let buffer = render_buffer(80, 30, &app);
    assert_eq!(
        row_text_finder(&buffer)(0)
            .split_whitespace()
            .collect::<Vec<_>>(),
        vec!["Ajax", ">", "web"]
    );
    assert_eq!(
        buffer_rows(&buffer)
            .iter()
            .find(|row| row.trim().ends_with("Keyboard shortcuts")),
        None
    );
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
    assert!(matches!(&app.view, AppView::Project { repo } if repo == "web"));
    let buffer = render_buffer(80, 30, &app);
    assert_eq!(
        row_text_finder(&buffer)(0)
            .split_whitespace()
            .collect::<Vec<_>>(),
        vec!["Ajax", ">", "web"]
    );
    assert_eq!(
        buffer_rows(&buffer)
            .iter()
            .find(|row| row.trim().ends_with("Keyboard shortcuts")),
        None
    );
}

#[test]
fn project_view_lists_new_task_first_then_tasks() {
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    // Projects view: [inbox, project, task]. Drill into the project.
    app.select_next();
    app.activate_selected();

    // Project view should be: [NewTask, task].
    assert_eq!(
        app.selectables
            .iter()
            .map(|selectable| match selectable {
                SelectableKind::NewTask { .. } => "new_task".to_string(),
                SelectableKind::Task(task) => task.qualified_handle.clone(),
                SelectableKind::Inbox(_) => "inbox".to_string(),
                SelectableKind::Project(_) => "project".to_string(),
                SelectableKind::TaskAction { .. } => "task_action".to_string(),
                SelectableKind::Remediation { .. } => "remediation".to_string(),
            })
            .collect::<Vec<_>>(),
        vec!["new_task".to_string(), "web/fix-login".to_string()]
    );
    // No action wall — only one task-style row in the middle is dispatched
    // on Enter and that's a Task or Review (not a project-action verb).
    for s in &app.selectables {
        assert!(
            !matches!(s, SelectableKind::TaskAction { .. }),
            "project view must not contain TaskAction rows"
        );
    }

    let buffer = render_buffer(80, 30, &app);
    assert_eq!(
        find_buffer_row(&buffer, |row| row.contains("start a new task"))
            .map(|row| row.trim().to_string()),
        Some(">+ start a new task".to_string()),
        "new task row should render first in project view"
    );
    assert_eq!(
        buffer_rows(&buffer)
            .iter()
            .flat_map(|row| row.split_whitespace())
            .filter(|word| *word == "reconcile")
            .collect::<Vec<_>>(),
        Vec::<&str>::new(),
        "project view must not advertise reconcile"
    );
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
    assert_eq!(item.action, "resume");
}

#[test]
fn enter_on_task_expands_drawer_with_primary_action_preselected() {
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

    // Enter expands the drawer in-place, doesn't dispatch.
    assert!(app.activate_selected().is_none());
    assert!(matches!(&app.view, AppView::Projects));
    assert!(app.expanded_task.is_some());

    // Drawer cursor lands on the primary action ("resume").
    let item = app.selected_action().unwrap();
    assert_eq!(item.task_handle, "web/fix-login");
    assert_eq!(item.action, "resume");
}

#[test]
fn empty_task_list_does_not_create_task_rows() {
    let mut app = App::new(
        sample_repos(),
        Vec::<TaskCard>::new(),
        InboxResponse { items: vec![] },
    );
    app.activate_selected();

    assert!(app
        .selectables
        .iter()
        .all(|selectable| !matches!(selectable, SelectableKind::Task(_))));
}

#[test]
fn drawer_back_collapses_keeping_project_view() {
    let mut app = app_in_project_view();
    let task_idx = app
        .selectables
        .iter()
        .position(|s| matches!(s, SelectableKind::Task(_)))
        .expect("project view has at least one task");
    app.selected = task_idx;
    app.activate_selected();
    assert!(app.expanded_task.is_some());

    assert!(app.go_back());
    assert!(app.expanded_task.is_none());
    assert!(matches!(&app.view, AppView::Project { repo } if repo == "web"));
}

#[test]
fn refresh_after_drop_clears_expanded_missing_task() {
    let mut app = app_in_project_view();
    let task_idx = app
        .selectables
        .iter()
        .position(|s| matches!(s, SelectableKind::Task(_)))
        .expect("project view has at least one task");
    app.selected = task_idx;
    app.activate_selected();
    assert!(app.expanded_task.is_some());

    app.apply_refresh(CockpitSnapshot {
        repos: sample_repos(),
        cards: Vec::new(),
        inbox: InboxResponse { items: vec![] },
    });

    assert!(app.expanded_task.is_none());
    assert!(!app.selectables.iter().any(|s| matches!(
        s,
        SelectableKind::Task(_) | SelectableKind::TaskAction { .. }
    )));
}

#[test]
fn refresh_after_archive_clears_expanded_unselectable_task() {
    let mut app = app_in_project_view();
    let task_id = TaskId::new("task-1");
    let task_idx = app
        .selectables
        .iter()
        .position(|s| matches!(s, SelectableKind::Task(task) if task.id == task_id))
        .expect("project view has task");
    app.selected = task_idx;
    app.activate_selected();
    assert_eq!(app.expanded_task.as_ref(), Some(&task_id));

    let mut archived = sample_tasks();
    archived[0].status = ajax_core::ui_state::TaskStatus::Idle;
    archived[0].lifecycle = LifecycleStatus::Removed;
    app.apply_refresh(CockpitSnapshot {
        repos: sample_repos(),
        cards: archived,
        inbox: InboxResponse { items: vec![] },
    });

    assert!(app.expanded_task.is_none());
    assert!(!app.selectables.iter().any(|s| matches!(
        s,
        SelectableKind::Task(task) if task.id == task_id
    )));
    assert!(!app.selectables.iter().any(|s| matches!(
        s,
        SelectableKind::TaskAction { task, .. } if task.id == task_id
    )));
}

#[test]
fn optimistic_drop_removes_task_until_refresh_restores_it() {
    let mut app = app_in_project_view();
    let task_id = TaskId::new("task-1");
    let task_idx = app
        .selectables
        .iter()
        .position(|s| matches!(s, SelectableKind::Task(task) if task.id == task_id))
        .expect("project view has task");
    app.selected = task_idx;
    app.activate_selected();
    assert!(app.expanded_task.is_some());

    app.optimistically_remove_task(&task_id);

    assert!(app.expanded_task.is_none());
    assert!(!app.cards.iter().any(|card| card.id == task_id));
    assert!(!app.inbox.items.iter().any(|item| item.task_id == task_id));
    assert!(!app.selectables.iter().any(|s| matches!(
        s,
        SelectableKind::Task(task) if task.id == task_id
    )));
    assert!(!app.selectables.iter().any(|s| matches!(
        s,
        SelectableKind::TaskAction { task, .. } if task.id == task_id
    )));

    app.apply_refresh(CockpitSnapshot {
        repos: sample_repos(),
        cards: sample_tasks(),
        inbox: sample_inbox(),
    });

    assert!(app.cards.iter().any(|card| card.id == task_id));
    assert!(app.selectables.iter().any(|s| matches!(
        s,
        SelectableKind::Task(task) if task.id == task_id
    )));
}

#[test]
fn drawer_action_dispatches_on_enter() {
    let mut app = app_in_project_view();
    let task_idx = app
        .selectables
        .iter()
        .position(|s| matches!(s, SelectableKind::Task(_)))
        .expect("project view has at least one task");
    app.selected = task_idx;
    app.activate_selected(); // expand drawer

    // Cursor now rests on the first drawer action row.
    let item = app.activate_selected().unwrap();
    assert_eq!(item.task_handle, "web/fix-login");
    assert_eq!(item.action, "resume");
}

#[test]
fn task_picker_actions_have_dedicated_render_metadata() {
    for action in [
        OperatorAction::Resume,
        OperatorAction::Review,
        OperatorAction::Ship,
        OperatorAction::Drop,
    ] {
        let chrome = crate::actions::action_chrome(action.as_str());
        assert_ne!(chrome.glyph, ".", "{action:?}");
    }

    let open = crate::actions::action_chrome(OperatorAction::Resume.as_str());
    let open_style = Style::default()
        .fg(primary_accent())
        .add_modifier(Modifier::BOLD);
    assert_eq!(open.glyph_style, open_style);
    assert_eq!(open.label_style, open_style);

    let action = OperatorAction::Ship;
    let chrome = crate::actions::action_chrome(action.as_str());
    let ship_style = Style::default()
        .fg(secondary_accent())
        .add_modifier(Modifier::BOLD);
    assert_eq!(chrome.glyph_style, ship_style, "{action:?}");
    assert_eq!(chrome.label_style, ship_style, "{action:?}");
}

#[test]
fn current_core_actions_have_dedicated_render_metadata() {
    for action in OperatorAction::all() {
        let chrome = crate::actions::action_chrome(action.as_str());

        assert_ne!(chrome.glyph, ".", "{action:?}");
    }
}

#[test]
fn actions_module_exposes_typed_action_chrome() {
    let chrome = crate::actions::operator_action_chrome(OperatorAction::Resume);

    assert_eq!(chrome.glyph, ">");
    assert_eq!(
        chrome.label_style,
        Style::default()
            .fg(primary_accent())
            .add_modifier(Modifier::BOLD)
    );
}

#[test]
fn cockpit_state_module_exposes_state_transitions() {
    let mut app = crate::cockpit_state::App::new(sample_repos(), sample_tasks(), sample_inbox());

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
fn rendering_module_exposes_status_palette() {
    assert_eq!(
        crate::rendering::bucket_color(crate::rendering::StatusBucket::Active),
        primary_accent()
    );
}

#[test]
fn rendering_module_exposes_screen_renderer() {
    let app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    let buffer = render_buffer(80, 30, &app);

    assert!(row_text_finder(&buffer)(0).trim_start().starts_with("Ajax"));
}

#[test]
fn enter_on_inbox_row_expands_drawer_with_recommendation_preselected() {
    let inbox = InboxResponse {
        items: vec![AnnotationItem {
            task_id: TaskId::new("task-1"),
            task_handle: "web/fix-login".to_string(),
            reason: "agent is running".to_string(),
            severity: 90,
            action: OperatorAction::Resume,
        }],
    };
    let mut app = App::new(sample_repos(), sample_tasks(), inbox);
    // Top-level Projects view: [inbox row, project, task]. Default selection is the inbox.
    assert!(matches!(
        app.selectables.get(app.selected),
        Some(SelectableKind::Inbox(_))
    ));

    assert!(app.activate_selected().is_none());
    assert!(matches!(&app.view, AppView::Projects));
    assert!(app.expanded_task.is_some());

    let inbox = InboxResponse {
        items: vec![AnnotationItem {
            task_id: TaskId::new("task-1"),
            task_handle: "web/fix-login".to_string(),
            reason: "review ready".to_string(),
            severity: 30,
            action: OperatorAction::Ship,
        }],
    };
    let mut tasks = sample_tasks();
    tasks[0].lifecycle = LifecycleStatus::Reviewable;
    tasks[0].available_actions = vec![OperatorAction::Resume, OperatorAction::Ship];
    tasks[0].primary_action = OperatorAction::Ship;
    let mut app = App::new(sample_repos(), tasks, inbox);
    assert!(app.activate_selected().is_none());
    let item = app.selected_action().unwrap();
    assert_eq!(item.action, "ship");
}

#[test]
fn enter_on_inbox_row_includes_recommended_action_missing_from_task_actions() {
    let inbox = InboxResponse {
        items: vec![AnnotationItem {
            task_id: TaskId::new("task-1"),
            task_handle: "web/fix-login".to_string(),
            reason: "cleanable".to_string(),
            severity: 40,
            action: OperatorAction::Drop,
        }],
    };
    let mut app = App::new(sample_repos(), sample_tasks(), inbox);

    assert!(app.activate_selected().is_none());

    assert!(app.selectables.iter().any(|selectable| matches!(
        selectable,
        SelectableKind::TaskAction { action, .. } if action == "drop"
    )));
    let item = app.selected_action().unwrap();
    assert_eq!(item.action, "drop");
}

#[test]
fn project_view_has_no_reconcile_action() {
    let app = app_in_project_view();

    assert!(app
        .selectables
        .iter()
        .all(|selectable| !matches!(selectable, SelectableKind::TaskAction { .. })));
    let buffer = render_buffer(80, 30, &app);
    assert_eq!(
        find_buffer_row(&buffer, |row| row.contains("start a new task"))
            .map(|row| row.trim().to_string()),
        Some(">+ start a new task".to_string())
    );
    assert_eq!(
        buffer_rows(&buffer)
            .iter()
            .flat_map(|row| row.split_whitespace())
            .filter(|word| *word == "reconcile")
            .collect::<Vec<_>>(),
        Vec::<&str>::new()
    );
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
    let tasks = vec![
        sample_card(
            "task-1",
            "web/fix-login",
            "Fix login",
            TaskStatus::Waiting,
            LifecycleStatus::Active,
        ),
        sample_card(
            "task-2",
            "api/add-cache",
            "Add cache",
            TaskStatus::Idle,
            LifecycleStatus::Active,
        ),
    ];
    let inbox = InboxResponse {
        items: vec![
            AnnotationItem {
                task_id: TaskId::new("task-1"),
                task_handle: "web/fix-login".to_string(),
                reason: "needs_input".to_string(),
                severity: 10,
                action: OperatorAction::Resume,
            },
            AnnotationItem {
                task_id: TaskId::new("task-2"),
                task_handle: "api/add-cache".to_string(),
                reason: "stale task".to_string(),
                severity: 60,
                action: OperatorAction::Resume,
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

    assert!(matches!(&app.view, AppView::Project { repo } if repo == "api"));
    let buffer = render_buffer(100, 50, &app);
    assert_eq!(
        row_text_finder(&buffer)(0)
            .split_whitespace()
            .collect::<Vec<_>>(),
        vec!["Ajax", ">", "api"]
    );
    let parsed = task_rows_from_buffer(&buffer)
        .into_iter()
        .find(|row| row.handle == "api/add-cache")
        .expect("api task row should render");
    assert_eq!(parsed.handle, "api/add-cache");
    assert!(!task_rows_from_buffer(&buffer)
        .iter()
        .any(|row| row.handle == "web/fix-login"));
    assert_eq!(
        buffer_rows(&buffer)
            .iter()
            .map(|row| row.find("agent needs input"))
            .filter(Option::is_some)
            .count(),
        0
    );
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
    assert!(matches!(
        &app.view,
        AppView::NewTaskInput { repo, title } if repo == "web" && title.is_empty()
    ));

    let buffer = render_buffer(80, 30, &app);
    assert_eq!(
        row_text_finder(&buffer)(0)
            .split_whitespace()
            .collect::<Vec<_>>(),
        vec!["Ajax", ">", "web", ">", "start"]
    );
    let input_row = find_buffer_row(&buffer, |row| row.contains("Task name"))
        .expect("task title input row should render")
        .trim()
        .to_string();
    assert_eq!(input_row, "+ Task name  <type a task name>");
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
    assert_eq!(pending.action, "start");
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
    assert!(app.go_back());
    assert!(
        matches!(
            &app.view,
            AppView::NewTaskInput { repo, title } if repo == "web" && title.is_empty()
        ),
        "first backspace should edit the task title without leaving input"
    );
    assert!(matches!(
        &app.view,
        AppView::NewTaskInput { repo, title } if repo == "web" && title.is_empty()
    ));
    let title_buffer = render_buffer(80, 30, &app);
    let input_row = find_buffer_row(&title_buffer, |row| row.contains("Task name"))
        .expect("task title input row should render")
        .trim()
        .to_string();
    assert_eq!(input_row, "+ Task name  <type a task name>");
    assert!(app.go_back());
    assert!(matches!(app.view, AppView::Projects));
    assert_eq!(app.selected, 0);

    let buffer = render_buffer(80, 30, &app);
    assert_eq!(
        row_text_finder(&buffer)(0).split_whitespace().next(),
        Some("Ajax")
    );
    for y in 0..buffer.area.height {
        let row = row_text_finder(&buffer)(y);
        assert_eq!(
            row.find("> web"),
            None,
            "unexpected project breadcrumb: {row:?}"
        );
        assert_eq!(
            row.find("> start"),
            None,
            "unexpected new-task breadcrumb: {row:?}"
        );
    }
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

    assert!(matches!(app.view, AppView::Projects));
    let buffer = render_buffer(80, 30, &app);
    assert_eq!(
        row_text_finder(&buffer)(0).split_whitespace().next(),
        Some("Ajax")
    );
    assert_eq!(row_text_finder(&buffer)(0).find("> web"), None);
    assert_eq!(
        buffer_rows(&buffer)
            .iter()
            .filter(|row| row.contains("> start"))
            .count(),
        0
    );
    assert_eq!(
        buffer_rows(&buffer)
            .iter()
            .filter(|row| row.contains("Task name"))
            .count(),
        0
    );
}

#[test]
fn feed_inbox_items_render_handle_reason_and_action() {
    let app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    let buffer = render_buffer(80, 30, &app);
    let inbox_row = find_buffer_row(&buffer, |row| {
        row.contains('!') && row.contains("fix-login")
    })
    .expect("inbox row should render");
    assert_eq!(
        parse_inbox_row(&inbox_row),
        Some(ParsedInboxRow {
            repo: "web".to_string(),
            task_id: "fix-login".to_string(),
            reason: "needs_input".to_string(),
        })
    );
    let task = task_rows_from_buffer(&buffer)
        .into_iter()
        .find(|row| row.handle == "web/fix-login")
        .expect("task row should still render for duplicate inbox handle");
    assert_eq!(task.action, "Resume");
}

#[test]
fn waiting_for_input_inbox_items_use_yellow_chrome() {
    let item = AnnotationItem {
        task_id: TaskId::new("task-1"),
        task_handle: "web/fix-login".to_string(),
        reason: "waiting for input".to_string(),
        severity: 30,
        action: OperatorAction::Resume,
    };

    assert_eq!(priority_accent(item.severity), secondary_accent());
}

#[test]
fn interactive_cockpit_renders_to_narrow_buffer() {
    let app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    let buffer = render_buffer(50, 24, &app);

    assert!(row_text_finder(&buffer)(0).trim_start().starts_with("Ajax"));
    let parsed = task_rows_from_buffer(&buffer)
        .into_iter()
        .find(|row| row.handle == "web/fix-login")
        .expect("task row should render in narrow buffer");
    assert_eq!(parsed.handle, "web/fix-login");
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
    assert_eq!(app.selected_action().unwrap().action, "resume");
    // clamps at last
    app.select_next();
    assert_eq!(app.selected, 2);
}

#[test]
fn select_at_feed_row_lands_on_correct_selectable() {
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    // Layout on Projects view (every group gets a header row):
    //   0 blank (top breathing space)
    //   1 -- inbox (n) --
    //   2 inbox     ← selectable 0
    //   3 -- projects --
    //   4 project   ← selectable 1
    //   5 -- start --
    //   6 NewTask   ← selectable 2
    app.select_at_feed_row(2);
    assert_eq!(app.selected, 0);
    app.select_at_feed_row(4);
    assert_eq!(app.selected, 1);
    app.select_at_feed_row(6);
    assert_eq!(app.selected, 2);
    // header row → no change
    app.select_at_feed_row(3);
    assert_eq!(app.selected, 2);
}

#[test]
fn selectable_row_layout_comes_from_rendered_feed_rows() {
    let mut app = app_in_project_view();
    app.select_next();
    app.activate_selected();

    let (_, selectable_feed_rows) = crate::rendering::build_feed(&app, 0);
    let expected = selectable_feed_rows
        .into_iter()
        .map(|row| row..row + 1)
        .collect::<Vec<_>>();

    assert_eq!(selectable_row_layout(&app), expected);
}

#[test]
fn new_task_is_always_present_even_when_other_sections_empty() {
    let mut app = App::new(
        sample_repos(),
        Vec::<TaskCard>::new(),
        InboxResponse { items: vec![] },
    );
    // Top-level holds only the project; drilling in always shows NewTask first.
    app.activate_selected();
    assert!(matches!(
        app.selectables.first(),
        Some(SelectableKind::NewTask { .. })
    ));
    let item = app.selected_action().unwrap();
    assert_eq!(item.action, "start");
}

#[test]
fn selected_action_for_inbox_uses_action() {
    let app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    // Projects view: [inbox, project, NewTask] — inbox is the initial selection.
    let item = app.selected_action().unwrap();
    assert_eq!(item.task_handle, "web/fix-login");
    assert_eq!(item.action, "resume");
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
    assert_eq!(item.action, "resume");
}

#[test]
fn reload_updates_app_data_and_clamps_selection() {
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    app.selected = 99;
    app.reload(
        sample_repos(),
        Vec::<TaskCard>::new(),
        InboxResponse { items: vec![] },
    );
    // Only the project row remains at top level → clamps to it.
    assert_eq!(app.selected, 0);
    assert_eq!(app.selected_action().unwrap().action, "status");
}

#[test]
fn refresh_after_removed_task_returns_to_main_page() {
    let mut app = app_in_project_view();
    let task_idx = app
        .selectables
        .iter()
        .position(|s| matches!(s, SelectableKind::Task(_)))
        .expect("task row exists");
    app.selected = task_idx;
    let item = app.selected_action().expect("task row selected");
    app.activate_selected();
    assert!(app.expanded_task.is_some());

    super::handle_action_result(
        &mut app,
        &item,
        Ok(ActionOutcome::Refresh(CockpitSnapshot {
            repos: sample_repos(),
            cards: Vec::<TaskCard>::new(),
            inbox: InboxResponse { items: vec![] },
        })),
    )
    .unwrap();

    assert!(matches!(&app.view, AppView::Projects));
    assert_eq!(app.selected_action().unwrap().action, "status");
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
    app.notify_system(
        "done".to_string(),
        super::Severity::Success,
        super::cockpit_state::Origin::UserAction,
    );
    assert!(app.current_notice().is_some());
}

#[test]
fn action_errors_set_flash_and_stay_in_ajax() {
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    // Select an inbox row so the task notice lookup matches the dispatched item.
    let item = app.selected_action().expect("inbox item selected");

    let pending = super::handle_action_result(
        &mut app,
        &item,
        Err(std::io::Error::other("git exited with status 42")),
    )
    .unwrap();

    assert!(pending.is_none());
    assert_eq!(
        app.current_notice().map(|n| n.msg.as_str()),
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
        app.current_notice().map(|n| n.msg.as_str()),
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

#[test]
fn task_action_confirmation_survives_refresh_when_action_remains_available() {
    let mut app = app_in_project_view();
    app.select_next();
    app.select_next();
    app.activate_selected();
    let mut handler = ConfirmHandler::default();

    handle_cockpit_event(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        10,
        &mut handler,
    )
    .unwrap();

    app.apply_refresh(CockpitSnapshot {
        repos: sample_repos(),
        cards: sample_tasks(),
        inbox: sample_inbox(),
    });

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

#[test]
fn task_action_confirmation_survives_refresh_when_task_leaves_inbox() {
    let mut tasks = sample_tasks();
    tasks[0].primary_action = OperatorAction::Drop;
    tasks[0].available_actions = vec![OperatorAction::Drop];
    let inbox = InboxResponse {
        items: vec![AnnotationItem {
            task_id: tasks[0].id.clone(),
            task_handle: tasks[0].qualified_handle.clone(),
            reason: "cleanable".to_string(),
            severity: 40,
            action: OperatorAction::Drop,
        }],
    };
    let mut app = App::new(sample_repos(), tasks.clone(), inbox);
    app.activate_selected();
    let mut handler = ConfirmHandler::default();

    handle_cockpit_event(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        10,
        &mut handler,
    )
    .unwrap();

    app.apply_refresh(CockpitSnapshot {
        repos: sample_repos(),
        cards: tasks,
        inbox: InboxResponse { items: vec![] },
    });

    handle_cockpit_event(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        10,
        &mut handler,
    )
    .unwrap();

    assert_eq!(handler.asked, 1);
    assert_eq!(handler.confirmed, 1);
}

#[test]
fn task_action_confirmation_survives_refresh_when_drawer_actions_reorder() {
    let mut tasks = sample_tasks();
    tasks[0].primary_action = OperatorAction::Drop;
    tasks[0].available_actions = vec![OperatorAction::Resume, OperatorAction::Drop];
    let mut app = App::new(sample_repos(), tasks, InboxResponse { items: vec![] });
    app.select_next();
    app.activate_selected();
    let mut handler = ConfirmHandler::default();

    handle_cockpit_event(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        10,
        &mut handler,
    )
    .unwrap();

    let mut reordered = sample_tasks();
    reordered[0].primary_action = OperatorAction::Drop;
    reordered[0].available_actions = vec![OperatorAction::Drop, OperatorAction::Resume];
    app.apply_refresh(CockpitSnapshot {
        repos: sample_repos(),
        cards: reordered,
        inbox: InboxResponse { items: vec![] },
    });

    handle_cockpit_event(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        10,
        &mut handler,
    )
    .unwrap();

    assert_eq!(handler.asked, 1);
    assert_eq!(handler.confirmed, 1);
}

#[test]
fn task_action_confirmation_is_invalidated_when_refresh_removes_action() {
    let mut app = app_in_project_view();
    app.select_next();
    app.select_next();
    app.activate_selected();
    let mut handler = ConfirmHandler::default();

    handle_cockpit_event(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        10,
        &mut handler,
    )
    .unwrap();

    let mut tasks = sample_tasks();
    tasks[0].primary_action = OperatorAction::Review;
    tasks[0].available_actions = vec![OperatorAction::Review];
    app.apply_refresh(CockpitSnapshot {
        repos: sample_repos(),
        cards: tasks,
        inbox: sample_inbox(),
    });

    handle_cockpit_event(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        10,
        &mut handler,
    )
    .unwrap();

    assert_eq!(handler.asked, 2);
    assert_eq!(handler.confirmed, 0);
}

#[test]
fn notify_task_higher_severity_replaces_lower() {
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    let task_id = TaskId::new("task-1");

    app.notify_task(
        task_id.clone(),
        "saved".to_string(),
        super::Severity::Success,
        super::cockpit_state::Origin::UserAction,
    );
    app.notify_task(
        task_id.clone(),
        "boom".to_string(),
        super::Severity::Error,
        super::cockpit_state::Origin::UserAction,
    );

    let notice = app.notices.get(&task_id).expect("notice present");
    assert_eq!(notice.msg, "boom");
    assert_eq!(notice.severity, super::Severity::Error);
}

#[test]
fn notify_task_lower_severity_dropped() {
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    let task_id = TaskId::new("task-1");

    app.notify_task(
        task_id.clone(),
        "confirm me".to_string(),
        super::Severity::Confirm,
        super::cockpit_state::Origin::UserAction,
    );
    app.notify_task(
        task_id.clone(),
        "later success".to_string(),
        super::Severity::Success,
        super::cockpit_state::Origin::UserAction,
    );

    let notice = app.notices.get(&task_id).expect("notice present");
    assert_eq!(notice.msg, "confirm me");
    assert_eq!(notice.severity, super::Severity::Confirm);
}

#[test]
fn notify_task_identical_message_resets_ticks_remaining() {
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    let task_id = TaskId::new("task-1");

    app.notify_task(
        task_id.clone(),
        "saved".to_string(),
        super::Severity::Success,
        super::cockpit_state::Origin::UserAction,
    );
    let full = super::cockpit_state::NOTICE_TICKS_SUCCESS;
    // Tick the notice down a few steps, but not to zero.
    for _ in 0..3 {
        app.tick_notices();
    }
    assert_eq!(app.notices.get(&task_id).unwrap().ticks_remaining, full - 3);

    // Identical (msg, severity) must reset to full lifetime.
    app.notify_task(
        task_id.clone(),
        "saved".to_string(),
        super::Severity::Success,
        super::cockpit_state::Origin::UserAction,
    );
    assert_eq!(app.notices.get(&task_id).unwrap().ticks_remaining, full);
}

#[test]
fn notify_task_user_action_replaces_background_event_at_equal_severity() {
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    let task_id = TaskId::new("task-1");

    app.notify_task(
        task_id.clone(),
        "background failed".to_string(),
        super::Severity::Error,
        super::cockpit_state::Origin::BackgroundEvent,
    );
    app.notify_task(
        task_id.clone(),
        "user failed".to_string(),
        super::Severity::Error,
        super::cockpit_state::Origin::UserAction,
    );

    let notice = app.notices.get(&task_id).expect("notice present");
    assert_eq!(notice.msg, "user failed");
    assert_eq!(notice.origin, super::cockpit_state::Origin::UserAction,);
}

#[test]
fn notify_task_background_event_does_not_replace_user_action_at_equal_severity() {
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    let task_id = TaskId::new("task-1");

    app.notify_task(
        task_id.clone(),
        "user failed".to_string(),
        super::Severity::Error,
        super::cockpit_state::Origin::UserAction,
    );
    app.notify_task(
        task_id.clone(),
        "background failed".to_string(),
        super::Severity::Error,
        super::cockpit_state::Origin::BackgroundEvent,
    );

    let notice = app.notices.get(&task_id).expect("notice present");
    assert_eq!(notice.msg, "user failed");
    assert_eq!(notice.origin, super::cockpit_state::Origin::UserAction,);
}

#[test]
fn current_notice_prefers_selected_task_notice_over_system_notice() {
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    // selected=0 → inbox item for task-99 (sample_inbox).
    let selected_task_id = app
        .selected_task_id()
        .cloned()
        .expect("inbox row maps to task id");

    app.notify_system(
        "system message".to_string(),
        super::Severity::Error,
        super::cockpit_state::Origin::BackgroundEvent,
    );
    app.notify_task(
        selected_task_id,
        "task message".to_string(),
        super::Severity::Hint,
        super::cockpit_state::Origin::UserAction,
    );

    let notice = app.current_notice().expect("notice present");
    assert_eq!(notice.msg, "task message");
    assert_eq!(notice.severity, super::Severity::Hint);
}

#[test]
fn current_notice_returns_system_notice_when_selected_row_has_none() {
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    // Move selection onto the project row, which does not map to a task id.
    app.select_next();
    assert!(app.selected_task_id().is_none());

    app.notify_system(
        "system message".to_string(),
        super::Severity::Success,
        super::cockpit_state::Origin::UserAction,
    );

    let notice = app.current_notice().expect("notice present");
    assert_eq!(notice.msg, "system message");
    assert_eq!(notice.severity, super::Severity::Success);
}

#[test]
fn current_notice_prefers_pending_confirm_over_selected_task() {
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    let selected_task_id = app
        .selected_task_id()
        .cloned()
        .expect("inbox row maps to task id");

    let confirm_item = CockpitActionItem {
        task_id: TaskId::new("task-1"),
        task_handle: "web/fix-login".to_string(),
        reason: "open".to_string(),
        priority: 50,
        action: "resume".to_string(),
    };

    app.notify_task(
        confirm_item.task_id.clone(),
        "press enter again to confirm".to_string(),
        super::Severity::Confirm,
        super::cockpit_state::Origin::UserAction,
    );
    app.pending_confirmation = Some(confirm_item);

    // A notice on the currently selected row must lose to the pending Confirm.
    app.notify_task(
        selected_task_id,
        "selected message".to_string(),
        super::Severity::Error,
        super::cockpit_state::Origin::UserAction,
    );

    let notice = app.current_notice().expect("notice present");
    assert_eq!(notice.msg, "press enter again to confirm");
    assert_eq!(notice.severity, super::Severity::Confirm);
}

#[test]
fn error_notice_decays_over_error_lifetime() {
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    let task_id = TaskId::new("task-1");
    app.notify_task(
        task_id.clone(),
        "boom".to_string(),
        super::Severity::Error,
        super::cockpit_state::Origin::UserAction,
    );

    let lifetime = super::cockpit_state::NOTICE_TICKS_ERROR;
    // After exactly `lifetime` ticks, the notice is still present at 0.
    for _ in 0..lifetime {
        app.tick_notices();
    }
    let remaining = app.notices.get(&task_id).map(|n| n.ticks_remaining);
    assert_eq!(remaining, Some(0));

    // One more tick prunes it.
    app.tick_notices();
    assert!(!app.notices.contains_key(&task_id));
}

#[test]
fn confirm_notice_does_not_decay() {
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    let task_id = TaskId::new("task-1");
    app.notify_task(
        task_id.clone(),
        "press enter again to confirm".to_string(),
        super::Severity::Confirm,
        super::cockpit_state::Origin::UserAction,
    );

    let initial = app.notices.get(&task_id).unwrap().ticks_remaining;
    assert_eq!(initial, super::cockpit_state::NOTICE_TICKS_CONFIRM);

    // Tick well past any non-sticky lifetime; Confirm must persist unchanged.
    for _ in 0..super::cockpit_state::NOTICE_TICKS_ERROR + 2 {
        app.tick_notices();
    }

    let notice = app.notices.get(&task_id).expect("confirm still present");
    assert_eq!(notice.severity, super::Severity::Confirm);
    assert_eq!(notice.ticks_remaining, initial);
}

#[test]
fn reload_prunes_notices_for_vanished_tasks() {
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    let task_id = TaskId::new("task-1");
    app.notify_task(
        task_id.clone(),
        "saved".to_string(),
        super::Severity::Success,
        super::cockpit_state::Origin::UserAction,
    );
    assert!(app.notices.contains_key(&task_id));

    // Refresh with no cards — task-1 has vanished.
    app.apply_refresh(CockpitSnapshot {
        repos: sample_repos(),
        cards: Vec::<TaskCard>::new(),
        inbox: InboxResponse { items: vec![] },
    });

    assert!(
        !app.notices.contains_key(&task_id),
        "notice for vanished task should be pruned"
    );
}

#[test]
fn reload_clears_background_error_notices_but_keeps_user_action_errors() {
    let cards = vec![
        sample_card(
            "task-1",
            "web/fix-login",
            "Fix login",
            TaskStatus::Waiting,
            LifecycleStatus::Active,
        ),
        sample_card(
            "task-2",
            "web/add-search",
            "Add search",
            TaskStatus::Idle,
            LifecycleStatus::Active,
        ),
    ];
    let inbox = InboxResponse { items: vec![] };
    let mut app = App::new(sample_repos(), cards.clone(), inbox.clone());

    let bg = TaskId::new("task-1");
    let user = TaskId::new("task-2");

    app.notify_task(
        bg.clone(),
        "poll failed".to_string(),
        super::Severity::Error,
        super::cockpit_state::Origin::BackgroundEvent,
    );
    app.notify_task(
        user.clone(),
        "merge failed".to_string(),
        super::Severity::Error,
        super::cockpit_state::Origin::UserAction,
    );

    app.apply_refresh(CockpitSnapshot {
        repos: sample_repos(),
        cards,
        inbox,
    });

    assert!(
        !app.notices.contains_key(&bg),
        "BackgroundEvent error should be cleared on refresh"
    );
    assert!(
        app.notices.contains_key(&user),
        "UserAction error should survive refresh"
    );
}

#[test]
fn reload_drops_success_hint_on_lifecycle_change_keeps_error_confirm() {
    let initial = vec![
        sample_card(
            "task-1",
            "web/a",
            "A",
            TaskStatus::Idle,
            LifecycleStatus::Active,
        ),
        sample_card(
            "task-2",
            "web/b",
            "B",
            TaskStatus::Idle,
            LifecycleStatus::Active,
        ),
        sample_card(
            "task-3",
            "web/c",
            "C",
            TaskStatus::Idle,
            LifecycleStatus::Active,
        ),
        sample_card(
            "task-4",
            "web/d",
            "D",
            TaskStatus::Idle,
            LifecycleStatus::Active,
        ),
    ];
    let inbox = InboxResponse { items: vec![] };
    let mut app = App::new(sample_repos(), initial, inbox.clone());

    app.notify_task(
        TaskId::new("task-1"),
        "success".to_string(),
        super::Severity::Success,
        super::cockpit_state::Origin::UserAction,
    );
    app.notify_task(
        TaskId::new("task-2"),
        "hint".to_string(),
        super::Severity::Hint,
        super::cockpit_state::Origin::UserAction,
    );
    app.notify_task(
        TaskId::new("task-3"),
        "error".to_string(),
        super::Severity::Error,
        super::cockpit_state::Origin::UserAction,
    );
    app.notify_task(
        TaskId::new("task-4"),
        "confirm".to_string(),
        super::Severity::Confirm,
        super::cockpit_state::Origin::UserAction,
    );

    // All four tasks change lifecycle on refresh.
    let refreshed = vec![
        sample_card(
            "task-1",
            "web/a",
            "A",
            TaskStatus::Idle,
            LifecycleStatus::Reviewable,
        ),
        sample_card(
            "task-2",
            "web/b",
            "B",
            TaskStatus::Idle,
            LifecycleStatus::Reviewable,
        ),
        sample_card(
            "task-3",
            "web/c",
            "C",
            TaskStatus::Idle,
            LifecycleStatus::Reviewable,
        ),
        sample_card(
            "task-4",
            "web/d",
            "D",
            TaskStatus::Idle,
            LifecycleStatus::Reviewable,
        ),
    ];
    app.apply_refresh(CockpitSnapshot {
        repos: sample_repos(),
        cards: refreshed,
        inbox,
    });

    assert!(
        !app.notices.contains_key(&TaskId::new("task-1")),
        "Success should be dropped when lifecycle changes"
    );
    assert!(
        !app.notices.contains_key(&TaskId::new("task-2")),
        "Hint should be dropped when lifecycle changes"
    );
    assert!(
        app.notices.contains_key(&TaskId::new("task-3")),
        "Error must survive lifecycle change"
    );
    assert!(
        app.notices.contains_key(&TaskId::new("task-4")),
        "Confirm must survive lifecycle change"
    );
}

#[test]
fn reload_clears_system_background_error_notice() {
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    app.notify_system(
        "poll failed".to_string(),
        super::Severity::Error,
        super::cockpit_state::Origin::BackgroundEvent,
    );
    assert!(app.system_notice.is_some());

    app.apply_refresh(CockpitSnapshot {
        repos: sample_repos(),
        cards: sample_tasks(),
        inbox: sample_inbox(),
    });

    assert!(
        app.system_notice.is_none(),
        "system BackgroundEvent error must clear on successful refresh"
    );
}

#[test]
fn reload_preserves_system_user_action_notice() {
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    app.notify_system(
        "task name required".to_string(),
        super::Severity::Hint,
        super::cockpit_state::Origin::UserAction,
    );

    app.apply_refresh(CockpitSnapshot {
        repos: sample_repos(),
        cards: sample_tasks(),
        inbox: sample_inbox(),
    });

    assert!(
        app.system_notice.is_some(),
        "system UserAction notice must survive successful refresh"
    );
}

#[test]
fn view_change_via_go_home_invalidates_pending_confirm() {
    let mut app = app_in_project_view();
    let confirm_item = CockpitActionItem {
        task_id: TaskId::new("task-1"),
        task_handle: "web/fix-login".to_string(),
        reason: "open".to_string(),
        priority: 50,
        action: "resume".to_string(),
    };
    app.notify_task(
        confirm_item.task_id.clone(),
        "press enter again to confirm".to_string(),
        super::Severity::Confirm,
        super::cockpit_state::Origin::UserAction,
    );
    app.pending_confirmation = Some(confirm_item.clone());

    assert!(app.go_home());

    assert!(app.pending_confirmation.is_none());
    assert!(
        !app.notices.contains_key(&confirm_item.task_id),
        "Confirm notice should be cleared on view change"
    );
    let hint = app
        .system_notice
        .as_ref()
        .expect("hint should be posted on view change");
    assert_eq!(hint.msg, "confirm again — context changed");
    assert_eq!(hint.severity, super::Severity::Hint);
}

#[test]
fn view_change_with_no_pending_confirm_does_not_post_hint() {
    let mut app = app_in_project_view();
    assert!(app.system_notice.is_none());

    assert!(app.go_home());

    assert!(app.system_notice.is_none());
}

#[test]
fn selected_row_renders_chevron_prefix() {
    let app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    let buffer = render_buffer(80, 30, &app);
    let inbox_row = find_buffer_row(&buffer, |row| {
        row.contains('!') && row.contains("fix-login")
    })
    .expect("selected inbox feed row should be in the rendered output");
    assert!(
        inbox_row.trim_start().starts_with('>'),
        "selected row should be prefixed with chevron, got: {inbox_row:?}"
    );
}

#[test]
fn feed_uses_named_section_headers_between_groups() {
    let app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    let buffer = render_buffer(80, 30, &app);

    assert_eq!(
        section_header_row(&buffer, "projects").map(|row| row.trim().to_string()),
        Some("-- projects --".to_string())
    );
    assert_eq!(
        section_header_row(&buffer, "tasks").map(|row| row.trim().to_string()),
        Some("-- tasks --".to_string())
    );
}

#[test]
fn drawer_actions_render_directly_under_task_row() {
    let mut app = App::new(sample_repos(), sample_tasks(), sample_inbox());
    for _ in 0..app.selectables.len() {
        if matches!(
            app.selectables.get(app.selected),
            Some(SelectableKind::Task(_))
        ) {
            break;
        }
        app.select_next();
    }
    let task_idx = app.selected;
    app.activate_selected();

    // Drawer actions follow the task in the selectable list.
    let next = app
        .selectables
        .get(task_idx + 1)
        .expect("drawer action follows the task row");
    assert!(matches!(next, SelectableKind::TaskAction { .. }));
}

#[test]
fn drawer_actions_render_on_rows_immediately_after_task_row() {
    let mut tasks = sample_tasks();
    tasks[0].available_actions = vec![OperatorAction::Resume, OperatorAction::Review];
    let mut app = App::new(sample_repos(), tasks, InboxResponse { items: vec![] });
    for _ in 0..app.selectables.len() {
        if matches!(
            app.selectables.get(app.selected),
            Some(SelectableKind::Task(_))
        ) {
            break;
        }
        app.select_next();
    }

    app.activate_selected();

    let content = render_to_string(100, 30, &app);
    let rows = content
        .as_bytes()
        .chunks(100)
        .map(|chunk| std::str::from_utf8(chunk).unwrap().to_string())
        .collect::<Vec<_>>();
    let task_row = rows
        .iter()
        .position(|row| row.contains("web/fix-login"))
        .expect("task row should render");
    let resume_row = rows
        .iter()
        .position(|row| row.contains("resume"))
        .expect("resume action should render");
    let review_row = rows
        .iter()
        .position(|row| row.contains(" R review"))
        .expect("review action should render");

    assert_eq!(resume_row, task_row + 1, "{rows:#?}");
    assert_eq!(review_row, task_row + 2, "{rows:#?}");
}

#[test]
fn expanded_drawer_does_not_repeat_primary_row_status() {
    let mut tasks = sample_tasks();
    let annotation = Annotation::new(
        AnnotationKind::NeedsMe,
        Evidence::SideFlag(ajax_core::models::SideFlag::NeedsInput),
    );
    let row_label = annotation.row_label();
    tasks[0].status_explanation = Some(row_label.clone());
    tasks[0].annotations = vec![annotation];
    let mut app = App::new(sample_repos(), tasks, InboxResponse { items: vec![] });
    for _ in 0..app.selectables.len() {
        if matches!(
            app.selectables.get(app.selected),
            Some(SelectableKind::Task(_))
        ) {
            break;
        }
        app.select_next();
    }

    app.activate_selected();
    let content = render_to_string(100, 30, &app);

    assert_eq!(content.matches(&row_label).count(), 1, "{content}");
}

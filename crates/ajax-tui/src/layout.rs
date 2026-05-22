use std::ops::Range;

use crate::{
    cockpit_state::{AppView, SelectableKind},
    App,
};

/// Compute the row range each selectable occupies in the feed,
/// in the same order as `app.selectables`.
pub(crate) fn selectable_row_layout(app: &App) -> Vec<Range<usize>> {
    let mut rows = Vec::new();
    let mut row = 1; // blank row at the top of the feed

    if app.selectables.is_empty()
        || matches!(
            app.view,
            AppView::NewTaskInput { .. } | AppView::Help { .. }
        )
    {
        return rows;
    }

    let mut prev_group: Option<&'static str> = None;
    for selectable in &app.selectables {
        let group = selectable_group(selectable);
        if prev_group != Some(group) && !matches!(selectable, SelectableKind::TaskAction { .. }) {
            row += 1;
        }

        rows.push(row..row + 1);
        row += 1 + expanded_annotation_rows(selectable, app);
        prev_group = Some(group);
    }

    rows
}

fn selectable_group(kind: &SelectableKind) -> &'static str {
    match kind {
        SelectableKind::NewTask { .. } => "create",
        SelectableKind::Inbox(_) => "hot",
        SelectableKind::Project(_) => "projects",
        SelectableKind::Task(_) => "tasks",
        SelectableKind::TaskAction { .. } => "task-actions",
    }
}

fn expanded_annotation_rows(selectable: &SelectableKind, app: &App) -> usize {
    let Some(open) = app.expanded_task.as_ref() else {
        return 0;
    };
    let (task_id, row_reason) = match selectable {
        SelectableKind::Task(card) if &card.id == open => {
            (&card.id, Some(card.status_label.clone()))
        }
        SelectableKind::Inbox(item) if &item.task_id == open => {
            (&item.task_id, Some(item.reason.clone()))
        }
        _ => return 0,
    };
    app.cards
        .iter()
        .find(|card| &card.id == task_id)
        .map(|card| {
            card.annotations
                .iter()
                .filter(|annotation| Some(annotation.row_label()) != row_reason)
                .count()
        })
        .unwrap_or(0)
}

/// Screen row at which the feed starts. Mouse handling must use this to map
/// terminal rows back to feed-internal coordinates.
pub(crate) fn feed_top_row(_app: &App) -> usize {
    1 // breadcrumb only; counts moved into the header
}

pub(crate) fn visible_feed_height(app: &App, terminal_height: usize) -> usize {
    feed_screen_rows(app, terminal_height).len()
}

pub(crate) fn feed_screen_rows(app: &App, terminal_height: usize) -> Range<usize> {
    let top = feed_top_row(app);
    let notice_rows = usize::from(app.current_notice().is_some());
    let bottom = terminal_height.saturating_sub(notice_rows + 1);
    top..bottom.max(top)
}

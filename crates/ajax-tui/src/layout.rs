use std::ops::Range;

use crate::{rendering, App};

pub(crate) fn selectable_row_ranges(rows: impl IntoIterator<Item = usize>) -> Vec<Range<usize>> {
    rows.into_iter().map(|row| row..row + 1).collect()
}

/// Compute the row range each selectable occupies in the rendered feed,
/// in the same order as `app.selectables`. Must stay in sync with `build_feed`.
pub(crate) fn selectable_row_layout(app: &App) -> Vec<Range<usize>> {
    selectable_row_ranges(rendering::selectable_feed_rows(app))
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
    let notice_rows = usize::from(crate::show_notice_row(app));
    let bottom = terminal_height.saturating_sub(notice_rows + 1);
    top..bottom.max(top)
}

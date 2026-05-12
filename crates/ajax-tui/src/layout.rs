use std::ops::Range;

pub(crate) fn selectable_row_ranges(rows: impl IntoIterator<Item = usize>) -> Vec<Range<usize>> {
    rows.into_iter().map(|row| row..row + 1).collect()
}

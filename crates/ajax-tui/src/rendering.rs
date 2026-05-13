use ratatui::style::Color;
use ratatui::{
    layout::{Constraint, Layout},
    Frame,
};

use crate::App;

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
        StatusBucket::Active => Color::Indexed(110),
        StatusBucket::NeedsYou => Color::Indexed(179),
        StatusBucket::Stuck => Color::Indexed(174),
        StatusBucket::Done => Color::Indexed(108),
        StatusBucket::Idle => Color::Indexed(244),
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
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(frame.area());

    crate::render_header(frame, app, chunks[0]);
    crate::render_feed(frame, app, chunks[1]);
    crate::render_status_bar(frame, app, chunks[2]);
}

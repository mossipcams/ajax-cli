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
    let show_notice = crate::show_notice_row(app);
    let mut constraints: Vec<Constraint> = vec![Constraint::Length(1)];
    constraints.push(Constraint::Min(0));
    if show_notice {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Length(1));
    let chunks = Layout::vertical(constraints).split(frame.area());

    let mut idx = 0;
    crate::render_header(frame, app, chunks[idx]);
    idx += 1;
    crate::render_feed(frame, app, chunks[idx]);
    idx += 1;
    if show_notice {
        crate::render_notice_row(frame, app, chunks[idx]);
        idx += 1;
    }
    crate::render_status_bar(frame, app, chunks[idx]);
}

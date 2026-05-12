use ratatui::style::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusBucket {
    Active,
    NeedsYou,
    Stuck,
    Done,
    Idle,
    Missing,
}

pub(crate) fn bucket_color(bucket: StatusBucket) -> Color {
    match bucket {
        StatusBucket::Active => Color::Indexed(110),
        StatusBucket::NeedsYou => Color::Indexed(179),
        StatusBucket::Stuck => Color::Indexed(174),
        StatusBucket::Done => Color::Indexed(108),
        StatusBucket::Idle => Color::Indexed(244),
        StatusBucket::Missing => Color::Indexed(241),
    }
}

pub(crate) fn bucket_glyph(bucket: StatusBucket) -> &'static str {
    match bucket {
        StatusBucket::Active => "▸",
        StatusBucket::NeedsYou => "?",
        StatusBucket::Stuck => "!",
        StatusBucket::Done => "✓",
        StatusBucket::Idle => "·",
        StatusBucket::Missing => "×",
    }
}

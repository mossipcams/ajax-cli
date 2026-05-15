use ratatui::style::{Color, Modifier, Style};

pub(crate) const fn accent_primary() -> Color {
    Color::Indexed(110)
}

pub(crate) const fn accent_warning() -> Color {
    Color::Indexed(179)
}

pub(crate) const fn accent_danger() -> Color {
    Color::Indexed(174)
}

pub(crate) const fn accent_success() -> Color {
    Color::Indexed(108)
}

pub(crate) const fn text_data() -> Color {
    Color::Indexed(248)
}

pub(crate) const fn text_chrome() -> Color {
    Color::Indexed(244)
}

pub(crate) fn selected_highlight() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

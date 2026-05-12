use ajax_core::models::RecommendedAction;
use ratatui::style::{Color, Modifier, Style};

#[derive(Clone, Copy)]
pub(crate) struct ActionChrome {
    pub(crate) glyph: &'static str,
    pub(crate) glyph_color: Color,
    pub(crate) label_color: Color,
    pub(crate) bold: bool,
}

impl ActionChrome {
    const fn new(glyph: &'static str, glyph_color: Color, label_color: Color, bold: bool) -> Self {
        Self {
            glyph,
            glyph_color,
            label_color,
            bold,
        }
    }

    pub(crate) fn glyph_style(self) -> Style {
        self.apply_weight(Style::default().fg(self.glyph_color))
    }

    pub(crate) fn label_style(self) -> Style {
        self.apply_weight(Style::default().fg(self.label_color))
    }

    fn apply_weight(self, mut style: Style) -> Style {
        if self.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        style
    }
}

pub(crate) fn action_chrome(recommended_action: &str) -> ActionChrome {
    match RecommendedAction::from_label(recommended_action) {
        Some(action) => recommended_action_chrome(action),
        None if recommended_action == "help" => {
            ActionChrome::new("?", Color::LightYellow, Color::White, true)
        }
        _ => ActionChrome::new(".", subtle_text(), muted_text(), false),
    }
}

pub(crate) fn recommended_action_chrome(action: RecommendedAction) -> ActionChrome {
    match action {
        RecommendedAction::SelectProject => {
            ActionChrome::new("P", primary_accent(), primary_accent(), true)
        }
        RecommendedAction::NewTask => {
            ActionChrome::new("+", primary_accent(), primary_accent(), true)
        }
        RecommendedAction::OpenTask => {
            ActionChrome::new(">", primary_accent(), primary_accent(), true)
        }
        RecommendedAction::OpenTrunk => {
            ActionChrome::new("T", primary_accent(), primary_accent(), true)
        }
        RecommendedAction::MergeTask => {
            ActionChrome::new("M", secondary_accent(), secondary_accent(), true)
        }
        RecommendedAction::CleanTask => {
            ActionChrome::new("X", danger_accent(), danger_accent(), true)
        }
        RecommendedAction::RemoveTask => {
            ActionChrome::new("!", danger_accent(), danger_accent(), true)
        }
        RecommendedAction::Status => {
            ActionChrome::new("S", primary_accent(), primary_accent(), true)
        }
    }
}

const fn primary_accent() -> Color {
    Color::Indexed(110)
}

const fn secondary_accent() -> Color {
    Color::Indexed(179)
}

const fn danger_accent() -> Color {
    Color::Indexed(174)
}

const fn muted_text() -> Color {
    Color::Indexed(244)
}

const fn subtle_text() -> Color {
    Color::Indexed(240)
}

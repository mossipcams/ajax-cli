use ajax_core::models::{AnnotationKind, OperatorAction};
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

pub(crate) fn action_chrome(action_label: &str) -> ActionChrome {
    match OperatorAction::from_label(action_label) {
        Some(action) => operator_action_chrome(action),
        None if action_label == "help" => {
            ActionChrome::new("?", Color::LightYellow, Color::White, true)
        }
        _ => ActionChrome::new(".", subtle_text(), muted_text(), false),
    }
}

pub(crate) fn annotation_chrome(kind: AnnotationKind) -> ActionChrome {
    match kind {
        AnnotationKind::NeedsMe => {
            ActionChrome::new("?", secondary_accent(), secondary_accent(), true)
        }
        AnnotationKind::Broken => ActionChrome::new("!", danger_accent(), danger_accent(), true),
        AnnotationKind::Reviewable => {
            ActionChrome::new("R", secondary_accent(), secondary_accent(), true)
        }
        AnnotationKind::Cleanable => ActionChrome::new("~", muted_text(), muted_text(), true),
    }
}

pub(crate) fn operator_action_chrome(action: OperatorAction) -> ActionChrome {
    match action {
        OperatorAction::Start => ActionChrome::new("+", primary_accent(), primary_accent(), true),
        OperatorAction::Resume => ActionChrome::new(">", primary_accent(), primary_accent(), true),
        OperatorAction::Review => {
            ActionChrome::new("R", secondary_accent(), secondary_accent(), true)
        }
        OperatorAction::Ship => {
            ActionChrome::new("S", secondary_accent(), secondary_accent(), true)
        }
        OperatorAction::Drop => ActionChrome::new("X", danger_accent(), danger_accent(), true),
        OperatorAction::Repair => ActionChrome::new("T", primary_accent(), primary_accent(), true),
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
    Color::Indexed(248)
}

const fn subtle_text() -> Color {
    Color::Indexed(244)
}

#[cfg(test)]
mod tests {
    use super::{action_chrome, annotation_chrome};
    use ajax_core::models::AnnotationKind;

    #[test]
    fn action_chrome_uses_operator_verbs() {
        for (label, glyph) in [
            ("resume", ">"),
            ("review", "R"),
            ("ship", "S"),
            ("drop", "X"),
            ("repair", "T"),
        ] {
            let chrome = action_chrome(label);

            assert_eq!(chrome.glyph, glyph, "{label}");
            assert!(chrome.bold, "{label}");
        }
        assert_eq!(action_chrome("open task").glyph, ".");
    }

    #[test]
    fn annotation_chrome_uses_kind_glyph() {
        for (kind, glyph) in [
            (AnnotationKind::NeedsMe, "?"),
            (AnnotationKind::Broken, "!"),
            (AnnotationKind::Reviewable, "R"),
            (AnnotationKind::Cleanable, "~"),
        ] {
            let chrome = annotation_chrome(kind);

            assert_eq!(chrome.glyph, glyph, "{kind:?}");
            assert_eq!(chrome.glyph.chars().next(), Some(kind.glyph()));
        }
    }
}

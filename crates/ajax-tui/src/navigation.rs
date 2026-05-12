use crossterm::event::{KeyCode, KeyModifiers};

pub(crate) fn is_back_key_event(code: KeyCode, modifiers: KeyModifiers) -> bool {
    matches!(code, KeyCode::Esc | KeyCode::Left | KeyCode::Char('h'))
        || is_navigation_backspace_key(code, modifiers)
}

pub(crate) fn is_help_key_event(code: KeyCode, modifiers: KeyModifiers) -> bool {
    matches!(code, KeyCode::Char('?'))
        || matches!(code, KeyCode::Char('/') if modifiers.contains(KeyModifiers::SHIFT))
}

pub(crate) fn is_navigation_backspace_key(code: KeyCode, modifiers: KeyModifiers) -> bool {
    matches!(
        code,
        KeyCode::Backspace | KeyCode::Char('\u{8}') | KeyCode::Char('\u{7f}')
    ) || matches!(code, KeyCode::Char('h') if modifiers.contains(KeyModifiers::CONTROL))
}

pub(crate) fn is_input_delete_key(code: KeyCode, modifiers: KeyModifiers) -> bool {
    matches!(
        code,
        KeyCode::Backspace | KeyCode::Delete | KeyCode::Char('\u{8}') | KeyCode::Char('\u{7f}')
    ) || matches!(code, KeyCode::Char('h') if modifiers.contains(KeyModifiers::CONTROL))
}

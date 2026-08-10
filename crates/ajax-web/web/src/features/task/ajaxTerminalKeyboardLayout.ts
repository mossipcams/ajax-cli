/** Layouts and PTY mapping for the Ajax mobile virtual keyboard. */

export type AjaxKeyboardLayoutName = "default" | "shift" | "numbers" | "symbols";

export const AJAX_KEYBOARD_LAYOUT: Record<AjaxKeyboardLayoutName, string[]> = {
  default: [
    "q w e r t y u i o p",
    "a s d f g h j k l",
    "{shift} z x c v b n m {bksp}",
    "{numbers} {hide} {space} {enter}",
  ],
  shift: [
    "Q W E R T Y U I O P",
    "A S D F G H J K L",
    "{shift} Z X C V B N M {bksp}",
    "{numbers} {hide} {space} {enter}",
  ],
  numbers: [
    "1 2 3 4 5 6 7 8 9 0",
    "- / : ; ( ) $ & @ \"",
    "{symbols} . , ? ! ' {bksp}",
    "{abc} {hide} {space} {enter}",
  ],
  symbols: [
    "[ ] {{ }} # % ^ * + =",
    "_ \\ | ~ < > € £ ¥ ·",
    "{numbers} . , ? ! ' {bksp}",
    "{abc} {hide} {space} {enter}",
  ],
};

export const AJAX_KEYBOARD_DISPLAY: Record<string, string> = {
  "{bksp}": "⌫",
  "{enter}": "return",
  "{shift}": "⇧",
  "{space}": "space",
  "{numbers}": "123",
  "{symbols}": "#+=",
  "{abc}": "ABC",
  "{hide}": "Done",
};

export const AJAX_KEYBOARD_BUTTON_THEME = [
  { class: "ajax-kb-enter", buttons: "{enter}" },
  { class: "ajax-kb-bksp", buttons: "{bksp}" },
  { class: "ajax-kb-done", buttons: "{hide}" },
  {
    class: "ajax-kb-mod",
    buttons: "{shift} {numbers} {symbols} {abc}",
  },
  { class: "ajax-kb-space", buttons: "{space}" },
];

const LAYOUT_SWITCH: Record<string, AjaxKeyboardLayoutName> = {
  "{shift}": "shift",
  "{numbers}": "numbers",
  "{symbols}": "symbols",
  "{abc}": "default",
};

/** Layout-only buttons that must not emit PTY bytes. */
export function isAjaxKeyboardLayoutButton(button: string): boolean {
  return button in LAYOUT_SWITCH || button === "{hide}";
}

/**
 * Toggle / switch layout. `{shift}` from shift returns to default; from
 * default/numbers/symbols goes to shift.
 */
export function nextAjaxKeyboardLayout(
  current: AjaxKeyboardLayoutName,
  button: string,
): AjaxKeyboardLayoutName | null {
  if (button === "{shift}") {
    return current === "shift" ? "default" : "shift";
  }
  const next = LAYOUT_SWITCH[button];
  return next ?? null;
}

/**
 * Map a simple-keyboard button id to a PTY payload.
 * Returns null for layout/hide controls (caller handles those).
 */
export function mapAjaxKeyboardButton(button: string): string | null {
  if (isAjaxKeyboardLayoutButton(button)) return null;
  if (button === "{enter}") return "\r";
  if (button === "{bksp}") return "\x7f";
  if (button === "{space}") return " ";
  // Escaped brace buttons from the symbols layout.
  if (button === "{{") return "{";
  if (button === "}}") return "}";
  if (button.length === 1) return button;
  return null;
}

# Plan: Ajax virtual keyboard (react-simple-keyboard)

## Scope

Replace the OS soft keyboard on touch/narrow viewports with a compact
`react-simple-keyboard` under the existing terminal hotbar. Same PTY behavior;
smaller than iOS (~≤220px).

## Non-goals

- Hotbar redesign/removal
- Composer / Live terminal model
- PTY/WebSocket contract changes
- Desktop fine-pointer OS keyboard replacement

## Decisions

- Gate: `isMobileTerminalLayout()` / `MOBILE_MEDIA_QUERY`
- Keep Esc/Tab/arrows/Ctrl/Paste/⌫/Mic hotbar
- Software keyboard truth via `setSoftwareKeyboardOpen` in `viewport.ts`
- Helper textarea `inputmode=none` to suppress OS soft keyboard

## Checklist

- [x] Add `react-simple-keyboard` dependency
- [x] `ajaxTerminalKeyboardLayout.ts` + mapper tests
- [x] `AjaxTerminalKeyboard.tsx` with hold-to-repeat backspace
- [x] Wire open/close in `TaskTerminal` / `mountTaskTerminalSession`
- [x] `setSoftwareKeyboardOpen` + viewport tests
- [x] Compact Ajax CSS + chrome contract test
- [x] Docs: `TERMINAL.md`, `web-cockpit.md`

## Validation

- `npm run web:check` — pass
- `npm run web:lint` — pass
- Focused vitest (`ajaxTerminalKeyboardLayout`, `ajaxTerminalKeyboardChrome`, `viewport`, `terminalGeometry`, `TaskTerminal`) — 88 passed
- Physical iPhone soft-keyboard absence — manual follow-up (not run here)

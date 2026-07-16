# Inline hotbar above keyboard

## Scope

Fix mobile task terminal layout:
- Hotkey bar keys distribute proportionally across full width.
- Terminal fills space between status chrome and bottom nav (no dead band on new tasks).
- Keyboard-open band keeps hotkeys flush above keyboard.

## Non-goals

- Desktop terminal height rules.
- PTY protocol / FitAddon min-col math.
- Wterm / Ghostty paths.

## Delegation

`Delegation decision: not delegated — focused CSS flex-chain + hotbar distribution; parent implements with TDD.`

## Checklist

- [x] Test: mobile task route flex-fill contract (no 38vh cap)
- [x] Test: hotbar keys flex-grow + keyboard-open safe-area pad
- [x] CSS: task route / task-detail / terminal-panel flex chain
- [x] TaskTerminal: flex-fill interaction wrap + proportional keys
- [x] TaskTerminal: debounced discrete refit on resize while keyboard-open
- [x] Validation: focused vitest + build

## Validation

```bash
cd crates/ajax-web/web && npx vitest run src/components/TaskTerminal.test.ts src/components/App.test.ts
# 45 passed

npm run web:build
# built in ~895ms
```

# Fix: link Open/Copy when keyboard is closed

## Scope

When a terminal HTTP link is tapped while the iOS keyboard is closed, the
Open/Copy floating menu still breaks (mispositioned, dismissed, or untappable).
#735 restored `z-index: 50`, which helps once the keyboard is already open.

**Non-goals:** paste behavior, web-links URL parsing, terminal focus-on-tap for
non-link taps, architecture changes.

## Root cause (working)

1. Link tap focuses the xterm helper textarea → keyboard animates open.
2. `FloatingContextMenu` uses default `strategy: 'absolute'` with `clientX/Y`
   anchors; iOS visualViewport shift leaves the menu off the visible band.
3. Keyboard-induced scroll can dismiss the menu after the 400ms grace.

## Delegation decision

`Delegation decision: delegated via model-router`

- MiniMax (`opencode-go/minimax-m3`) → FAILED (monthly usage limit)
- Rerouted to Cursor `composer-2.5` → code written; structured report missing
  (parent Review Gate used)

## Checklist

- [x] Task 1 — Tests: `strategy: 'fixed'` contract; link-open blurs when keyboard closed; scroll grace covers keyboard animation
- [x] Task 2 — Impl: `FloatingContextMenu` fixed strategy + longer scroll grace; `TaskTerminal` blur on link menu when keyboard closed
- [x] Task 3 — Verify focused vitest + web:build (dist if required)
- [x] Parent review gate + record results

## Validation

```bash
cd crates/ajax-web/web && npx vitest run src/shared/ui/FloatingContextMenu.test.tsx src/features/task/TaskTerminal.test.tsx
# → 39 passed

npm run web:build:check
# → pass (refreshed dist/app.js + dist/terminal.js)
```

## Deviations

- MiniMax lane unavailable (usage limit); Cursor composer-2.5 used instead.
- Delegate omitted structured `DELEGATE_REPORT`; parent accepted after diff +
  validation review.
- Vite also refreshed `dist/app.js` (FloatingContextMenu lives in the app
  bundle); not only `terminal.js`.

## Results

ACCEPT. Diff matches packet: fixed strategy, 800ms grace, blur on both link-open
paths when `!isKeyboardOpen()`. Source contracts + focused vitest green;
`web:build:check` green.

# Fix: recover native paste from helper textarea

## Scope

Make link (and other) pastes work when Safari/`clipboardData` yields no
usable sync text. Today xterm still handles that paste with empty
`text/plain`, clears the helper textarea, and the browser inserts the URL
into the hidden field — so nothing reaches the PTY.

## Non-goals

- No ghostty/xterm library bump
- No change to toolbar Paste / fallback tray UX beyond keeping them working
- No architecture.md changes
- No CLI clipboard changes

## Root cause

1. `#725` / `#730` cover rich `clipboardData` + `beforeinput` + async
   `readText`.
2. Real iOS/Safari (esp. LAN HTTP) often still delivers a paste where sync
   formats are empty/`text/plain` is `""`.
3. xterm `handlePasteEvent` does **not** `preventDefault`; on empty plain it
   still `paste("")` and sets `textarea.value = ""`.
4. Browser default then inserts the real clip into the helper textarea.
5. Our `input` handler only reseeds on delete, so the URL sits hidden and
   never goes to the PTY.

## Approach

1. On capture `paste` with empty `readPasteText`: `stopImmediatePropagation`
   (block xterm empty clear) but **do not** `preventDefault` (allow browser
   insert). Drop the native-paste async `readText` branch — textarea
   recovery is the HTTP-safe path.
2. On `input` after that gesture (`insertFromPaste*` or a paste-expect
   flag): strip `BACKSPACE_SENTINEL`, `sendPastedText(raw)`, reset sentinel.
3. Keep successful sync `readPasteText` path unchanged.

## Approval

- User: “Pasting links is still broken in terminal web”
- Mode: Behavior Change. Delegation via model-router.

## Delegation decision

`Delegation decision: delegated via model-router`

## Task checklist

- [x] **T1 (test):** e2e — empty sync clipboardData paste + simulated browser
      textarea insert sends the URL in one input frame
- [x] **T2 (test):** update TaskTerminal source contract for empty-paste
      `stopImmediatePropagation` without `preventDefault`, and input recovery
- [x] **T3 (impl):** paste + input recovery wiring (smallest change)
- [x] **T4 (verify):** focused vitest + paste e2e; parent reviews diff

## Validation

```bash
npm run web:test -- --run src/features/task/TaskTerminal.test.tsx src/shared/lib/clipboard.test.ts
# 36 passed
npm run web:check
# passed
npm run web:smoke -- --grep "paste"
# 14 passed (incl. empty sync clipboardData textarea recovery)
npm run web:build
# passed
```

## Deviations

- Delegate report envelope failed schema validation (`INVALID_STRUCTURED_REPORT`);
  delta was in scope and parent verification passed.
- Parent review fix: recovery must assign `textarea.value = BACKSPACE_SENTINEL`
  (seed no-ops when ZWS remains beside pasted text).
- Rebuilt `web/dist` after source fix so embedded assets match.

# Web keyboard position + terminal load speed

## Scope

1. **Keyboard / input band** — on mobile, first tap on a task terminal must
   place the typing surface above the soft keyboard, not at the top of the page.
2. **Terminal load speed** — warm Ghostty WASM + terminal chunk before the task
   page needs them; navigate to the new task immediately after start.

## Non-goals

- Ghostty engine swap, architecture changes, Rust changes.
- NewTaskSheet keyboard-band layout (already handled by FullscreenLayer).
- Removing touch→focus or keyboard-open PTY freeze.

## Delegation decision

`Delegation decision: delegated via model-router` → Cursor / composer-2.5
(frontend Svelte/TS UI behavior).

Packet: `.planning/packets/web-keyboard-position-terminal-load.md`

## Approval

User reported both bugs and asked to delegate via ajax router — authorized.

## Task checklist

### Task 1: Anchor hidden textarea + snap before focus

- [x] Failing tests: textarea bottom-anchored; touchBegan resets scroll; keyboard-open scroll reset
- [x] Implement in `TerminalRawView.svelte` + `viewport.ts`
- [x] Verify TerminalRawView + viewport tests

### Task 2: Terminal warm preload

- [x] Add `terminalPreload.ts` (+ tests)
- [x] Wire `App.svelte` idle warm; TerminalRawView uses shared loader
- [x] Verify preload + App tests

### Task 3: Navigate to task after successful start

- [x] Add `taskSlug.ts` (+ tests)
- [x] NewTaskSheet `onOpenTask`; App navigates via `taskHash`
- [x] Verify NewTaskSheet + App tests

## Validation

```bash
npm run web:test -- --run \
  crates/ajax-web/web/src/viewport.test.ts \
  crates/ajax-web/web/src/taskSlug.test.ts \
  crates/ajax-web/web/src/terminalPreload.test.ts \
  crates/ajax-web/web/src/components/NewTaskSheet.test.ts \
  crates/ajax-web/web/src/components/App.test.ts \
  crates/ajax-web/web/src/components/TerminalRawView.test.ts
npm run web:check
```

## Deviations

- Parent reverted an earlier local WIP so the delegate can prove RED→GREEN.
- Worktree `node_modules` symlinked to main ajax-cli install for vitest.
- `touchBegan` only calls `snapScrollbackToBottom` when keyboard is already
  open (avoids yanking scrollback on every touch); first-focus keyboard
  position relies on bottom-anchored textarea + open-edge scroll reset.
- `npm run web:check` fails here: missing `typescript-5` for
  `svelte-check-legacy-ts.cjs`. Parent verified `tsc -p … --noEmit` clean.

## Validation ledger

- RED: focused suite EXIT 1 (new assertions failed as intended)
- GREEN: focused suite EXIT 0 — 213 passed
- Parent re-run: EXIT 0 — 213 passed
- `tsc -p crates/ajax-web/web/tsconfig.check.json --noEmit` — EXIT 0
- Review gate: **ACCEPT**

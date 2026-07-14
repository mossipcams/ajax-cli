# TDD Implementation Packet — R8 wterm flush resize on keyboard close (iOS)

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## Goal
On Surface V2 only: when the iOS soft keyboard closes, flush **exactly one**
PTY `sendResize` with the current wterm cols/rows so tmux catches up after
the freeze (R7). Convert
`it.todo("flushes exactly one server resize once the keyboard closes")`.

## Hard gates
- Surface V2 / `WtermTerminalView*` only; mobile Safari WebKit
- Never `TerminalRawView`

## Allowed files
- `crates/ajax-web/web/src/components/WtermTerminalView.svelte`
- `crates/ajax-web/web/src/components/WtermTerminalView.test.ts`
- `crates/ajax-web/web/dist/terminal.js` (+ app.css/app.js if web:build updates)

## Forbidden changes
- No safe-area / snap / visualViewport debounce yet (R9–R11)
- No TerminalRawView / viewport.ts edits
- No commit/push/branch

## Context evidence
- Graphify/Serena/ast-grep: `NOT_REQUIRED`
- R7 already gates `reportResize` via `layoutPolicy.setKeyboardOpen(isKeyboardOpen())`
- Ghostty listens for keyboard/class changes and flushes one resize on close
- `isKeyboardOpen()` reflects `html.keyboard-open`
- Need a listener: `MutationObserver` on `document.documentElement` class, or
  `visualViewport` resize that detects open→close transition — prefer the
  same pattern Ghostty uses if small; otherwise MutationObserver on classList
  is enough for the unit test (toggling the class)

## Code anchors
- Existing `layoutPolicy` + `reportResize` in `WtermTerminalView.svelte` onMount
- Track `wasKeyboardOpen`; on transition true→false, call
  `reportResize(term.cols, term.rows)` once (or `connection.sendResize` after
  policy allows)

## Test-first instructions
1. Convert flush todo:
   - Mount; add `keyboard-open`; fire `termOnResize!(50, 18)` (withheld)
   - Clear `sendResize` mock
   - Remove `keyboard-open`
   - Expect exactly one `sendResize` with current term cols/rows (mock defaults 72,24 unless resize updated local term — if local term still 72,24 and withheld resize didn't mutate mock cols, flush should send liveTerm.cols/rows; if you update mock cols on onResize even when withheld, flush 50,18 — pick one consistent behavior and document: **prefer updating local grid always, withhold only PTY send, flush last local size**)
2. Assert not called twice on a single close.
3. RED then implement.

## Edit instructions
1. On keyboard close transition, flush one resize of current `term.cols/rows`.
2. Keep R7 withhold while open.
3. Rebuild dist.

## Verification commands
```bash
cd crates/ajax-web/web && npx vitest run src/components/WtermTerminalView.test.ts
cd crates/ajax-web/web && npm run web:build
```

## Validation (2026-07-14)
- RED: `vitest -t "flushes exactly one server resize once the keyboard closes"` → 0 calls (before `wasKeyboardOpen` sync in `reportResize`)
- GREEN: full `WtermTerminalView.test.ts` → 40 passed, 9 todo
- `npm run web:build` (repo root) → ok; dist updated

## Acceptance criteria
- Flush todo green; other keyboard todos remain
- Exactly one flush per close
- RED→GREEN; V2-only

## Stop conditions
- Implementing safe-area/snap/debounce in same round
- Editing TerminalRawView

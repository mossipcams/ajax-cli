---
PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior
DISPATCH_LEVEL: compact
TASK_ID: double-tap-copy-selection
---

## Task

Add double-tap-then-hold-and-drag text highlighting on the Web Cockpit terminal
so the user can paint a selection and use the existing Copy overlay.

Today, single long-press word-selects on lift, and after a matured hold, drag
becomes directional arrow keys. That must stay. The new path is:

1. Quick first tap (touchstart→touchend, short, little movement).
2. Second touchstart nearby within ~350ms arms selection-drag.
3. Immediately select the word under the second contact (reuse current word-char
   rules).
4. While holding, each touchmove extends selection from the anchor cell to the
   current cell with `term.select(col, row, length)` (length may span rows).
5. Selection-drag must `preventDefault` on cancelable moves so scroll does not
   steal the gesture, and must never arm directional arrows or send PTY input.
6. touchend/touchcancel ends selection-drag but keeps the xterm selection so
   Copy stays visible.

`mountTaskTerminalSession.ts` is already ~878 LOC (hard max 1000). Extract the
existing `isWordChar` / `selectWordAtClient` logic (and new cell/range helpers)
into a sibling module under `features/task/` so the mount file does not grow
past the cap. Wire the double-tap detector and selection-drag branch into the
existing touchstart/move/end handlers.

## Scope

Extract selection helpers; wire double-tap-hold drag into existing touch handlers.
Keep mount file ≤1000 LOC.

## Allowed files

- `crates/ajax-web/web/src/features/task/mountTaskTerminalSession.ts`
- `crates/ajax-web/web/src/features/task/terminalTouchSelection.ts`
- `crates/ajax-web/web/src/features/task/terminalTouchSelection.test.ts`
- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`
- `.planning/agent-plans/double-tap-copy-selection.md`

## Forbidden changes

- Backend, PTY, registry, architecture, or non-terminal UI changes
- Changing single long-press word-select or directional-drag behavior
- Changing pinch-zoom, clipboard, Copy overlay UI, or paste flows
- New dependencies
- Commits, pushes, branch changes
- Broad refactors outside the selection gesture path
- Leaving `mountTaskTerminalSession.ts` over 1000 LOC

## Acceptance

1. Double-tap then hold+drag over terminal output selects a multi-character
   (or multi-word) range via xterm selection and shows the Copy control.
2. That gesture sends no PTY `input` frames.
3. Existing single long-press on known text still selects the word and shows
   Copy (existing e2e must pass).
4. Existing held directional drag still sends only the locked arrow and does
   not create a text selection from that path.
5. Pure helpers for cell mapping / range length / word bounds are unit-tested.
6. `mountTaskTerminalSession.ts` stays ≤1000 LOC after the change.

## Constraints

- Reuse existing Copy overlay via `onSelectionChange` → `syncCopyOverlay`.
- Keep `LONG_PRESS_MS = 500` and directional thresholds for the single-hold path.
- Double-tap window ~300–400ms; slop ~24px (pick concrete constants near those).
- Prefer the smallest diff; no abstraction beyond the one extracted helper
  module needed for LOC.
- Prefer `term.select` over private xterm APIs.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: npm --prefix crates/ajax-web/web exec -- vitest run src/features/task/terminalTouchSelection.test.ts
      expected: pass
    - type: existing_test
      command: npm --prefix crates/ajax-web/web exec -- vitest run src/features/task/TaskTerminal.test.ts
      expected: pass (no regressions)
    - type: test
      command: >-
        npm --prefix crates/ajax-web/web exec -- playwright test
        e2e/terminal-behavior.test.ts
        -g "long press on known output text selects word|double tap"
      expected: pass (include new double-tap case if added; keep long-press green)
  broader_checks: []
  reason: >-
    Pure selection math is unit-tested; existing TaskTerminal tests guard fit /
    overlay wiring; Playwright covers the touch gesture contract already used
    for long-press copy.
```

## Stop if

- Fix requires changing directional-drag semantics or architecture
- Patch would exceed ~400 changed lines
- Cannot keep mount file ≤1000 LOC without a larger peel than one helper module
- Existing long-press or directional e2e cannot be kept green without widening scope

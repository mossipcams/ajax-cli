# TDD Implementation Packet — R15 wterm expand resize while keyboard open

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## Goal
On Surface V2 only: entering expand while the iOS keyboard is open still
allows a PTY/grid resize (Ghostty `layoutPolicy.expandEnter()` intent
exemption). Convert
`it.todo("resizes the grid on expand even while the keyboard is open")`.

## Hard gates
- WtermTerminalView* only; mobile Safari WebKit
- Never TerminalRawView

## Allowed files
- `crates/ajax-web/web/src/components/WtermTerminalView.svelte`
- `crates/ajax-web/web/src/components/WtermTerminalView.test.ts`
- `crates/ajax-web/web/dist/terminal.js` (+ app.css/app.js if web:build updates)

## Forbidden changes
- No R9 debounce / R6 80-col in this round
- No TerminalRawView
- No commit/push/branch

## Context evidence
- Graphify/Serena/ast-grep: `NOT_REQUIRED`
- `createTerminalLayoutPolicy().expandEnter()` sets expandActive so
  `allowPtyResize` is true even when keyboardOpen
- Ghostty calls expandEnter on expand and then refits
- Wterm R7 gates reportResize on allowPtyResize; without expandEnter,
  keyboard-open blocks resize during expand layout change

## Code anchors
- `toggleExpanded` / `setExpanded`
- Existing `layoutPolicy` in onMount
- Need layoutPolicy accessible from toggleExpanded (store in component-level
  let, or call expandEnter before reportResize after expand)

## Test-first instructions
1. Convert R15 todo:
   - Mount; add `keyboard-open`
   - Clear sendResize
   - Click Expand
   - Fire `termOnResize!(60, 22)` (simulating autoResize after expand layout)
   - Expect sendResize called with 60,22 despite keyboard-open
     (because expand intent allows it)
   OR: after expand click, expect a flush resize was sent
2. Pick the Ghostty-aligned behavior: expandEnter unlocks allowPtyResize so
   subsequent onResize is sent; also call expandExit on collapse.
3. RED → GREEN

## Edit instructions
1. Keep a module/component reference to `layoutPolicy` usable from
   `toggleExpanded` (assign in onMount to an outer `let layoutPolicy`).
2. On expand: `layoutPolicy.expandEnter()` then optionally
   `reportResize(term.cols, term.rows)` after rAF.
3. On collapse: `layoutPolicy.expandExit()`.
4. Rebuild dist.

## Verification commands
```bash
cd crates/ajax-web/web && npx vitest run src/components/WtermTerminalView.test.ts
cd crates/ajax-web/web && npm run web:build
```

## Acceptance criteria
- R15 todo green; fullscreen block done
- RED→GREEN; V2-only
- Keyboard-open still freezes resize when NOT expanding

## Stop conditions
- Breaking R7 freeze when not expanded
- TerminalRawView edits

# TDD Implementation Packet — R13 wterm expand focuses terminal (iOS keyboard)

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## Goal
On Surface V2 only: first tap of the expand/fullscreen corner button focuses
the terminal so iOS Safari opens the soft keyboard. Convert
`it.todo("focuses the terminal on the first fullscreen tap so iOS opens the keyboard")`.

## Hard gates
- WtermTerminalView* only; mobile Safari WebKit
- Never TerminalRawView

## Allowed files
- `crates/ajax-web/web/src/components/WtermTerminalView.svelte`
- `crates/ajax-web/web/src/components/WtermTerminalView.test.ts`
- `crates/ajax-web/web/dist/terminal.js` (+ app.css/app.js if web:build updates)

## Forbidden changes
- Do not implement blur-on-exit or expand-while-keyboard-resize in this round
- No TerminalRawView
- No commit/push/branch

## Context evidence
- Graphify/Serena/ast-grep: `NOT_REQUIRED`
- Ghostty expand onclick calls `focusTerm()` when entering expand
- Wterm already has `toggleExpanded` / expand button with `aria-label="Expand terminal"`
- Mock exposes `termFocus`

## Code anchors
- Expand button handler in WtermTerminalView
- `term?.focus()` on enter expand only

## Test-first instructions
1. Convert focus todo:
   - Mount; clear termFocus
   - Click Expand terminal
   - Expect termFocus called
   - aria-pressed true
2. RED → edit → GREEN

## Edit instructions
1. When expanding (false→true), call `term?.focus()` (and optionally
   `requestAnimationFrame` like connection onOpen).
2. Leave collapse without blur for R14.
3. Rebuild dist.

## Verification commands
```bash
cd crates/ajax-web/web && npx vitest run src/components/WtermTerminalView.test.ts
cd crates/ajax-web/web && npm run web:build
```

## Acceptance criteria
- Focus todo green; blur + resize-while-keyboard todos remain
- RED→GREEN; V2-only

## Stop conditions
- Implementing blur + keyboard-open resize in same round
- Editing TerminalRawView

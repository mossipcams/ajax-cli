# TDD Implementation Packet — R12 wterm fullscreen toggle (Surface V2 only)

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## Goal
Add Ghostty-parity expand/fullscreen chrome to `WtermTerminalView` only: a
corner button that toggles an expanded terminal mode (class / aria state).
Convert `it.todo("toggles an expanded terminal mode from the corner fullscreen button")`
into a passing test. Do **not** implement focus-on-enter / blur-on-exit /
keyboard-open resize in this round (those are later todos).

## Hard gate
`ajax.terminal.surfaceV2` only — `WtermTerminalView*` + vendored dist.
Never edit `TerminalRawView.svelte`.

## Allowed files
- `crates/ajax-web/web/src/components/WtermTerminalView.svelte`
- `crates/ajax-web/web/src/components/WtermTerminalView.test.ts`
- `crates/ajax-web/web/dist/terminal.js` (+ app.css/app.js if web:build updates them)
- `crates/ajax-web/web/src/styles.css` **only if** an existing global
  `.terminal-expanded` / body class contract must be applied for expand layout
  (prefer mirroring Ghostty's `document.documentElement` / body class pattern
  already used by TerminalRawView; read before editing)

## Forbidden changes
- No focus/blur keyboard side-effects yet (next rounds)
- No keyboard lockstep, 80-col, pinch changes
- No TerminalRawView edits
- No commit/push/branch

## Context evidence
- Graphify: `NOT_REQUIRED`
- Serena: `NOT_REQUIRED`
- ast-grep: `NOT_REQUIRED`
- Ghostty: `expanded` state, `EXPANDED_CLASS = "terminal-expanded"`, button
  `.terminal-expand-corner` with `aria-pressed`, `class:is-expanded` on panel
  in `TerminalRawView.svelte` (~116–146, ~1112–1146, CSS ~1312)
- Wterm todos under `parity gaps: fullscreen and expand chrome`

## Code anchors
- Mirror the smallest Ghostty UX: corner button, toggle `expanded` boolean,
  set `aria-pressed`, add/remove expanded class on panel and/or document
  (match whatever Ghostty uses so existing CSS works if present)
- Read Ghostty toggle handler before inventing a new layout system

## Test-first instructions
1. Convert the first fullscreen todo into a real test:
   - Mount WtermTerminalView
   - Find expand button (role=button, name matching Ghostty — often "Expand" / "⛶" / aria-label)
   - Click → `aria-pressed="true"` and expanded class present
   - Click again → pressed false / class removed
2. RED: `cd crates/ajax-web/web && npx vitest run src/components/WtermTerminalView.test.ts`

## Edit instructions
1. Add `expanded` state + corner button markup + CSS (copy minimal styles from Ghostty expand corner, adapted to wterm panel).
2. Toggle only — no focus()/blur() yet.
3. Rebuild dist.

## Verification commands
```bash
cd crates/ajax-web/web && npx vitest run src/components/WtermTerminalView.test.ts
cd crates/ajax-web/web && npm run web:build
```

## Acceptance criteria
- Toggle todo green; other fullscreen todos remain todo
- V2-only scope
- RED→GREEN proven

## Stop conditions
- Editing TerminalRawView
- Implementing all four fullscreen todos in one round
- Keyboard lockstep scope creep

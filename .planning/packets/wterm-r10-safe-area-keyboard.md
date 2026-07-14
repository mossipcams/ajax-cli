# TDD Implementation Packet — R10 wterm safe-area pad drop when keyboard open

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## Goal
On Surface V2 only: while `html.keyboard-open`, drop safe-area bottom padding
on wterm bottom controls (`.terminal-keys` / paste fallback area) so the key
bar sits above the keyboard without dead home-indicator space. Convert
`it.todo("drops safe-area bottom pad on bottom controls while keyboard is open")`.

Mirror Ghostty's pattern:
`:global(html.keyboard-open) .terminal-bottom-controls { padding-bottom: 6px }`
but for wterm's `.terminal-keys` (and any bottom chrome that uses
`env(safe-area-inset-bottom)`).

## Hard gates
- Surface V2 / WtermTerminalView* only; mobile Safari WebKit
- Prefer scoped `<style>` in WtermTerminalView — do not change global
  `styles.css` unless unavoidable
- Never TerminalRawView

## Allowed files
- `crates/ajax-web/web/src/components/WtermTerminalView.svelte`
- `crates/ajax-web/web/src/components/WtermTerminalView.test.ts`
- `crates/ajax-web/web/dist/terminal.js` (+ app.css/app.js if web:build updates)

## Forbidden changes
- No snap/debounce/80-col in this round
- No TerminalRawView
- No commit/push/branch

## Context evidence
- Ghostty test asserts source match for
  `:global(html.keyboard-open) .terminal-bottom-controls { padding-bottom: 6px }`
- Wterm bottom chrome is `.terminal-keys` (and `.terminal-paste-fallback`)
- Check whether `.terminal-keys` currently has safe-area padding; if not, add
  default `padding-bottom: max(2px, env(safe-area-inset-bottom))` then override
  under keyboard-open to a small fixed pad (6px)

## Test-first instructions
1. Convert todo to a source/CSS contract test (like Ghostty) OR a DOM style test:
   Prefer asserting the Svelte component source / rendered CSS rule exists:
   `html.keyboard-open` + `.terminal-keys` + `padding-bottom: 6px` (or equivalent).
   Importing `WtermTerminalView.svelte?raw` is fine if that matches repo patterns.
2. RED → CSS edit → GREEN

## Edit instructions
1. Ensure bottom controls have safe-area pad when keyboard closed.
2. Under `:global(html.keyboard-open) .terminal-keys` (and paste fallback if needed),
   set `padding-bottom: 6px` (no safe-area).
3. Rebuild dist.

## Verification commands
```bash
cd crates/ajax-web/web && npx vitest run src/components/WtermTerminalView.test.ts
cd crates/ajax-web/web && npm run web:build
```

## Acceptance criteria
- Safe-area todo green
- RED→GREEN; V2-only scoped styles

## Stop conditions
- Editing TerminalRawView or global styles for Ghostty
- Scope creep into snap/debounce

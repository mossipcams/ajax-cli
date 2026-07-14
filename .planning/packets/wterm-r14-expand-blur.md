# TDD Implementation Packet — R14 wterm blur on exit expand (iOS keyboard)

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## Goal
On Surface V2 only: exiting fullscreen/expand blurs the terminal so iOS closes
the soft keyboard. Convert
`it.todo("blurs the terminal when exiting fullscreen so iOS closes the keyboard")`.

## Hard gates
- WtermTerminalView* only; mobile Safari WebKit
- Never TerminalRawView

## Allowed files
- `crates/ajax-web/web/src/components/WtermTerminalView.svelte`
- `crates/ajax-web/web/src/components/WtermTerminalView.test.ts`
- `crates/ajax-web/web/dist/terminal.js` (+ app.css/app.js if web:build updates)

## Forbidden changes
- No expand-while-keyboard resize (R15)
- No TerminalRawView
- No commit/push/branch

## Context evidence
- Graphify/Serena/ast-grep: `NOT_REQUIRED`
- Ghostty blurs textarea / activeElement on exit expand
- Wterm may need `document.activeElement.blur()` or blur hidden input inside host
- Prefer: `(document.activeElement as HTMLElement | null)?.blur()` and/or
  query textarea/input inside hostEl and blur it — WTerm.focus() focuses an
  internal input; blur that

## Code anchors
- Expand toggle when true→false
- Existing expand button tests / R13 focus test

## Test-first instructions
1. Convert blur todo:
   - Mount; click expand (focuses)
   - Append/focus an input inside host to simulate keyboard focus if needed
   - Click expand again to collapse
   - Expect document.activeElement is not the terminal input / was blurred
2. RED → GREEN

## Edit instructions
1. On collapse, blur active element / wterm input inside host.
2. Keep R13 focus-on-enter.
3. Rebuild dist.

## Verification commands
```bash
cd crates/ajax-web/web && npx vitest run src/components/WtermTerminalView.test.ts
cd crates/ajax-web/web && npm run web:build
```

## Acceptance criteria
- Blur todo green; R15 still todo
- RED→GREEN; V2-only

## Stop conditions
- Scope into R15 or keyboard lockstep
- TerminalRawView edits

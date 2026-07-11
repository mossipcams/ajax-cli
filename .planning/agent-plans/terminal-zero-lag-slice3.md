# Plan: Terminal zero-lag ownership (Slice 3)

## Scope

Finish zero-lag ownership: move overlay DOM paint + Ghostty cursor measure
helpers into `terminalZeroLag.ts` (echo policy already lives there). Thin
`TerminalRawView` to wire events only. Document ownership in TERMINAL.md.

## Non-goals

- Paste/copy (Slice 4)
- Layout / scroll-follow changes
- Changing zero-lag prediction algorithm behavior

## Delegation decision

`Delegation decision: delegated via model-router` → cursor-delegate /
composer-2.5

## Task checklist

- [x] Packet + failing tests for painter + measure helpers
- [x] Implement + wire TerminalRawView
- [x] TERMINAL.md ownership row
- [x] Parent validate

## Validation

```bash
npm run web:test -- --run src/terminalZeroLag.test.ts src/components/TerminalRawView.test.ts
npm run web:check
npm run web:build
```

### Results (delegate + parent)

- PASS zeroLag + TerminalRawView (164)
- PASS web:check
- Accepted: painter/measure extracted; prediction body unchanged; dispose wires painter.

### Results (2026-07-11)

- `npm run web:test -- --run src/terminalZeroLag.test.ts` — 23 passed
- `npm run web:test -- --run src/components/TerminalRawView.test.ts` — 141 passed
- `npm run web:check` — 0 errors
- `npm run web:build` — success
- Structural: no `paintZeroLag` / zero-lag `createElement` in TerminalRawView.svelte

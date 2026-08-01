# Fix: restore floating link menu CSS (under terminal)

## Scope

Restore `.floating-context-menu` styles deleted during MusterBar restore
(`ee8dd45a` / #708). Without `z-index: 50`, the Open/Copy portal menu stacks
under the terminal panel (`z-index` 40–45), so taps hit the terminal and
Open/Copy appear to do nothing.

## Non-goals

- No paste-path changes
- No FloatingContextMenu behavior redesign
- No architecture.md

## Root cause

#708 restore kept link TSX but dropped the CSS block from #686 that set
`z-index: 50` and menu chrome. Menu still portals to `body` but paints under
the terminal.

## Approach

1. Restore the exact prior CSS before `.terminal-new-output`.
2. Pin with a styles.css source contract: `.floating-context-menu` has
   `z-index: 50` (above expanded terminal 45).
3. Rebuild `dist/app.css`.

## Approval

- User: “copying and opening a link through the pop ups don't work now, they
  show up under the terminal”
- Mode: Small Fix / Behavior Change.

## Delegation decision

`Delegation decision: not delegated because mechanical restore of a known
deleted CSS block (exact prior content) plus one source-contract assertion —
smaller than a useful work order.`

## Task checklist

- [x] **T1 (test):** FloatingContextMenu styles contract for
      `.floating-context-menu { z-index: 50 }`
- [x] **T2 (impl):** restore CSS block in `styles.css`
- [x] **T3 (verify):** vitest + web:build; confirm menu CSS in dist

## Validation

```bash
npm run web:test -- --run src/shared/ui/FloatingContextMenu.test.tsx
# 7 passed
npm run web:build
# passed; dist/app.css contains .floating-context-menu { z-index: 50 }
```

## Deviations

(none)

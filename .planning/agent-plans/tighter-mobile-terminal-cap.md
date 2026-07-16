# Tighter mobile terminal cap (0–1 blank rows)

## Scope

Lower closed-keyboard mobile wrap from `min(38vh, 300px)` to
`min(24vh, 180px)` so FitAddon proposes fewer rows and the empty PTY band
lands near 0–1 lines. Keep keyboard-open flex-fill and host `height: 100%`.

## Non-goals

- Content-aware height
- Desktop / keyboard-open / expanded geometry
- tmux status placement

## Delegation decision

Delegation decision: not delegated because the change is a single CSS constant
plus matching test assert — smaller than a work order.

## Checklist

- [x] Change mobile wrap to `min(24vh, 180px)`
- [x] Update TaskTerminal.test.ts contract
- [x] Focused vitest + web:build + husky
- [x] New PR (\#531 already merged)

## Validation

```bash
npx vitest run src/components/TaskTerminal.test.ts src/components/App.test.ts  # 45 passed
npm run web:build  # ok
sh .husky/pre-commit  # ok
```

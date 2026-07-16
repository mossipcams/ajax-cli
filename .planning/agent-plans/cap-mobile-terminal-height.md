# Cap mobile terminal height (fix PTY blank band)

## Scope

Restore compact mobile inline terminal height so FitAddon does not propose a
tall empty PTY band between client output and the tmux status line. Keep
keyboard-open flex-fill and equal-width hotbar keys.

## Non-goals

- tmux status placement / PTY protocol
- Desktop `min(58vh, 560px)` rules
- Re-opening display:flex-host-in-wrap

## Delegation decision

Delegation decision: not delegated because this session resumed mid-implement
with the approved plan and parent already owning the CSS/JS/test edits; residual
work is validation + husky + PR.

## Checklist

- [x] Restore `min(38vh, 300px)` mobile wrap; host `height: 100%`; keyboard-open flex fill
- [x] Remove closed-keyboard `.terminal-panel { flex: 1 1 0% }` from `styles.css`
- [x] Scope `syncHostToWrap` to keyboard-open / expanded
- [x] Update TaskTerminal / App layout contracts
- [x] Focused vitest + `npm run web:build`
- [x] Full husky gate (`.husky/pre-commit`) — exit 0
- [ ] PR

## Validation

```bash
npx vitest run src/components/TaskTerminal.test.ts src/components/App.test.ts  # 45 passed
npm run web:build  # ok
sh .husky/pre-commit  # ok (npm run verify + release build/install)
```

## Deviations

None.

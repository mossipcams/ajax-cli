# Plan: xterm-zerolag-input

## Scope

Restore the product zero-lag typed-echo overlay on the React/xterm
`TaskTerminal` surface. Overlay class/testid is exactly `xterm-zerolag-input`.

Port the prediction algorithm from deleted Ghostty `terminalZeroLag.ts`
(commit `a02fc20`) into `shared/lib/xtermZeroLag.ts`, measure via xterm DOM
(`.xterm-screen` / `.xterm-rows`), and wire `beforeinput` / `onData` / PTY
output clear / reconnect reset in `TaskTerminal.tsx`.

## Non-goals

- Recreate forbidden legacy paths (`src/terminalZeroLag.ts`,
  `e2e/terminal-zero-lag.test.ts`, Ghostty canvas measure).
- Change WS protocol, scroll sync, geometry/refit, paste/copy, or expand.
- Commit / push / branch changes.

## Delegation decision

`Delegation decision: delegated via model-router`

## Task checklist

- [x] Packet READY + `scripts/check-packet` passes
- [x] Delegate implements test-first (`xtermZeroLag.test.ts` RED → GREEN)
- [x] Wire TaskTerminal + CSS for `xterm-zerolag-input`
- [x] Update TERMINAL.md ownership + contract Product evidence anchors
- [x] Parent Review Gate: inspect delta, run verification personally
- [x] Record validation results below

## Approval

Not required (behavior restoration of existing Product contract row).

## Deviations

- Delegate wrote a non-schema report (`status: success` instead of
  `DELEGATE_REPORT`); treated as FAILED report but delta was in scope.
- Parent fixed HIGH: sticky Ctrl must `consumeCtrl` before zero-lag note so
  control codes never paint as printables (`zeroLagNoteRef` + fold in
  `onTermData`).

## Validation

```bash
npm run web:test -- --run src/shared/lib/xtermZeroLag.test.ts src/features/task/TaskTerminal.test.tsx src/legacyTerminalRemoval.test.ts
# → 3 files / 43 tests passed
npm run web:lint
# → exit 0
```

Results: PASS (parent-verified).

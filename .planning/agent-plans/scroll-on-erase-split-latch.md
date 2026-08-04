# Fix: split-safe scrollOnErase latch + post-reveal force-off

## Scope

Keep `scrollOnEraseInDisplay` only for the seed window, then reliably latch it off so live agent ED2 stops dumping viewport frames into scrollback.

## Non-goals

- Stripping live ED2 from the PTY stream
- Changing seed capture / bridge attach
- Client line-dedupe

## Approach

1. Split-safe CSI `J` detection with a short carry across `onOutput` chunks (xterm parses across writes; our regex did not).
2. On `revealSeed`, arm a post-reveal grace timer that forces `scrollOnEraseInDisplay = false` if no erase was detected. Do **not** latch off synchronously in `revealSeed` (late attach ED2 race).

## Delegation decision

`Delegation decision: delegated via model-router` (cursor-delegate / composer-2.5)

## Checklist

- [x] Pure carry helper + tests for split `\x1b[` / `2J`
- [x] Wire helper into `onOutput`; clear carry on unseeded open / reset as needed
- [x] Arm force-off timer from `revealSeed`; cancel on erase latch / dispose / new seed
- [x] Update TaskTerminal source-contract tests
- [x] Parent verify

## Validation

```bash
cd crates/ajax-web/web && npx vitest run src/features/task/TaskTerminal.test.tsx src/shared/lib/scrollbackOverwriteProbe.test.ts
# 42 passed (parent)
npx eslint … detectCsiEraseInDisplay.ts  # pass after parent eslint-disable polish
```

## Deviations

- Delegate report schema extract failed; code + vitest green — parent accepted after review.
- Helper extracted to `detectCsiEraseInDisplay.ts` (packet optional scope).
- Parent polish: eslint `no-control-regex` disables on CSI patterns.

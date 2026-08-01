# Remove TerminalComposer (auto-insert restore)

## Scope

User correction: remove the Insert transcript / TerminalComposer box. Finals
auto-insert into the active shell line via paste/PTY again. Keep Moonshine v2,
readiness, completion, ordered finals, no start-over, Mic second-tap, teardown,
and backpressure.

## Non-goals

- No reintroduction of spoken start-over / DEL undo ledger
- No change to worker/provider protocol
- No auto-Enter

## Delegation decision

Delegation decision: not delegated because this reverses the prior destination
lock and must stay coherent with architecture/docs and StrictMode-safe
auto-insert in one pass on the existing STT seam.

## Task checklist

- [x] Task 1 — failing/updated tests assert no TerminalComposer; onFinal auto-inserts ordered deltas
- [x] Task 2 — remove composer UI; restore status strip; paste contiguous finalTranscript deltas in onFinal
- [x] Task 3 — architecture.md + docs/speech-input.md (+ README if needed) say auto-insert, not composer
- [x] Task 4 — delete TerminalComposer files + unused composer CSS; validate focused tests

## Approval

Authorized by explicit user instruction (no insert transcript box).

## Validation

- `rtk npm run web:test -- --run TaskTerminal.test.tsx speechState.test.ts`: 48 passed
- `rtk npm run web:check`: passed

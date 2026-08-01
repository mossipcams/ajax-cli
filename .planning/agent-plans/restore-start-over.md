# Restore spoken start over / start fresh

## Scope

Restore the spoken dictation reset command removed during STT corrections.
Standalone `start over` / `start fresh` undoes auto-inserted shell text (DEL
ledger) and clears speech finals while the session keeps listening.

## Non-goals

- No TerminalComposer
- No auto-Enter
- No change to pause / Mic second-tap / Moonshine v2

## Delegation decision

Delegation decision: not delegated because this restores a known prior speech
control on the existing TaskTerminal/auto-insert seam with docs alignment.

## Task checklist

- [x] Task 1 — speechState: `isStandaloneStartOver` (+ start fresh); clear finals; advance nextExpectedSequence
- [x] Task 2 — restore speechInsertLedger; TaskTerminal tracks pastes and undoes on start-over
- [x] Task 3 — tests + architecture/docs
- [x] Task 4 — focused validation

## Approval

Authorized by explicit user instruction (restore start-fresh functionality).

## Validation

- `npm run web:test` speechState + speechInsertLedger + TaskTerminal: 52 passed
- `npm run web:check`: passed

# Speech STT corrections

## Scope

Correct lifecycle, readiness, persistent streaming worker, cleanup, backpressure,
transcript destination (composer), ordered finals, pause normalization, Mic a11y,
and documentation for the existing Ajax Web continuous STT path.

## Non-goals

- No second STT path, cloud STT, PTY/tmux/task-lifecycle changes, auto-Enter,
  service worker/PWA, or unrelated refactors.

## Delegation decision

Delegation decision: not delegated because R1–R6 are one coherent protocol/sidecar/frontend
seam; parent implements each round sequentially with focused validation so half-finished
delegate deltas cannot leave finalize/ready/composer contracts contradictory.

## Intentional locks

1. Auto-insert finals into the active shell line (no TerminalComposer / Insert box).
2. Persistent Moonshine Small Streaming worker; model loads once.
3. Explicit successful completion (`completed` / `stt.closed`); no `stt.error` on expected finalize.
4. Real readiness from sidecar after model load.
5. Remove spoken `start over`.
6. Observable bounded backpressure; no silent audio drops.

## Task checklist

- [x] Round 0 — architecture.md + docs/speech-input.md contracts
- [x] Round 1 — protocol/provider completion + readiness
- [x] Round 2 — persistent Moonshine v2 worker; legacy moonshine removed
- [x] Round 3 — speechState: pause, remove start-over, ordered finals
- [x] Round 4 — transport teardown + backpressure
- [x] Round 5 — TerminalComposer + Mic a11y + partials
- [x] Round 6 — lifecycle tests + drift pass + parent validation

## Deviations

- Moonshine stack locked to **v2** (`moonshine-voice` + streaming arches only).
  Legacy `useful-moonshine-onnx` / `moonshine/tiny` removed from setup and rejected
  by the worker.
- Parent implements rounds sequentially (coherent protocol/sidecar/frontend seam).
- Round 5 deleted unused `speechInsertLedger` (start-over undo path).
- User correction after R6: remove TerminalComposer / Insert box; restore
  contiguous auto-insert into the PTY (see `remove-terminal-composer.md`).

## Validation

- `rtk cargo test -p ajax-web adapters::stt_provider::tests --lib -- --test-threads=1`: 14 passed
- `rtk cargo test -p ajax-web slices::stt::tests --lib`: 4 passed
- Round 3–6 web: `npm run web:test` on speechState + speechTransport + TerminalComposer + TaskTerminal: **73 passed**
- `npm run web:check`: passed
- `npm run verify`: passed (pre-PR gate)
- Docs drift: architecture.md / docs/speech-input.md / README agree on composer destination,
  Moonshine v2, no WebGPU requirement, no spoken start-over, legacy rejection.
- PR: https://github.com/mossipcams/ajax-cli/pull/740

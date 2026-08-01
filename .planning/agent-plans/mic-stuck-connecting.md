# Fix Mic stuck Connecting

## Scope

Mic stays on Connecting because the host still runs the legacy
`~/.ajax-dev/bin/ajax-moonshine-sidecar`, which never emits `stt.ready`.
Ajax now waits for Ready (correct). Unstick by installing the Moonshine v2
worker and failing loudly if Ready never arrives.

## Non-goals

- Do not invent Ready on process spawn
- Do not restore TerminalComposer

## Delegation decision

Delegation decision: not delegated because this is a host-install + small
readiness-timeout fix on the STT seam already owned in this worktree.

## Task checklist

- [x] Task 1 — install/update host worker from repo script
- [x] Task 2 — bridge readiness timeout → `stt.error` with setup guidance
- [x] Task 3 — focused provider test for ready timeout helper; docs one-liner
- [x] Task 4 — validate focused tests

## Validation

- Host worker smoke: FRAME_START → `stt.ready` (passed)
- `cargo test -p ajax-web adapters::stt_provider::tests --lib -- --test-threads=1`: 15 passed

## Operator note

Restart `ajax web` after updating the on-disk worker so any already-spawned
legacy process is replaced.

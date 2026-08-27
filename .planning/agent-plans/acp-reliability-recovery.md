# ACP Reliability Recovery

**Status:** Phase 2 complete (Phase 3 not started)
**Scope:** Durable prompt ownership, restart-safe host queue semantics, actor-owned
prompt terminal transitions, ACP child health/replacement semantics, and later
browser-protocol reliability hardening for Ajax Chat.
**Non-goals:** Replacing Core task truth, changing the browser reducer model,
or implementing Phase 3 browser-protocol hardening in Phase 2.

## Phase map

- **Phase 1 — Durable prompt ownership and restart-safe queue semantics**
  (complete).
- **Phase 1.5 — Actor-owned prompt terminal lifecycle** (complete): the
  `TaskSession` command loop finalizes each in-flight `clientMessageId` from the
  `session/prompt` command result only, persists the terminal ledger phase before
  dequeuing the next prompt, and blocks browser retries after interrupted rows.
- **Phase 2 — Process supervision and child health** (complete): reconcile
  unexpected ACP child exit through the `TaskSession` actor, preserve durable
  prompt/queue ownership while no healthy client exists, and make child
  replacement reap the prior process without duplicate terminal/error evidence.
- Phase 3 — Browser protocol hardening (not in scope here).

## Phase 1 checklist

- [x] Add a versioned atomic sidecar prompt ledger separate from bounded
  transcript JSONL (`web-session/<handle>.prompt-ledger.json`).
- [x] Persist queued ownership before `prompt_accepted` and dispatching before
  ACP `session/prompt`.
- [x] Recover queued prompts after directory/session recreation; mark recovered
  dispatching prompts interrupted without automatic retry.
- [x] Reject queue saturation without dropping acknowledged work.
- [x] Use the ledger (not transcript `PromptAccepted`) as dedupe authority.
- [x] Preserve existing transcript/session compatibility and Core ownership
  boundaries.
- [x] Add focused regression tests: write failure, queued recovery, interrupted
  recovery, duplicate IDs, queue saturation.
- [x] Update owning architecture documentation.

## Phase 1.5 checklist

- [x] Classify prompt terminal outcome from `session/prompt` RPC result only
  (success, cancellation-shaped abort, terminal error).
- [x] Finalize the active ledger row exactly once per terminal outcome
  (completed vs interrupted) before dequeuing the next prompt.
- [x] Correlate ACP request IDs with durable `clientMessageId` ownership; ignore
  stale, duplicate, and mismatched terminal results.
- [x] Retain terminal outcomes across ledger write failures and retry before FIFO
  advancement; retain queued ownership when dispatch-transition persistence fails.
- [x] Do not treat streamed agent/thought/tool chunks as prompt completion.
- [x] Block browser retries after interrupted ledger rows.
- [x] Isolate fake ACP fixture sidecar files under `FAKE_ACP_STATE_DIR` / tmpdir.
- [x] Extend regression coverage: thought-only, tool-only, no-agent-text,
  terminal RPC error, cancellation ledger idempotency, queued ordering.
- [x] Update owning architecture documentation.

## Phase 2 checklist

- [x] Reconcile unexpected ACP child exit exactly once through the actor.
- [x] Durably interrupt an active prompt before clearing busy state; do not
  advance its FIFO while no healthy ACP client exists.
- [x] Make replacement transactional: do not retain a newly spawned client as
  healthy when prompt-ledger recovery fails; a later acquire must retry recovery.
- [x] Preserve queued ownership across child exit and dispatch it only after a
  healthy replacement client is installed.
- [x] Distinguish expected cancel/detach/shutdown from unexpected exit and avoid
  duplicate operator errors or terminal transitions.
- [x] Ensure replacement closes/reaps the prior child and leaves no competing
  ACP stdio owner.
- [x] Add focused regression coverage for exit during a prompt, idle exit,
  queue preservation, replacement, and idempotent reconciliation.
- [x] Update owning architecture documentation.

## Confirmed defect

- [#1086](https://github.com/mossipcams/ajax-cli/issues/1086) tracks durable
  interruption retry and replacement-before-recovery rollback.
- Required regression: force ledger persistence failure during child exit,
  attempt replacement before persistence recovers, then prove a later acquire
  retries recovery and resumes FIFO only after the interrupted row is durable.

## Approval

Approval: received in chat 2026-08-26 for Phase 1 implementation, the
delegate-until-finished instruction for Phase 1.5 actor-owned prompt lifecycle,
and the subsequent `Continue` instruction for Phase 2 child health/supervision.
Phase 3 requires separate approval before work begins.

## Deviations

None for Phase 1 or Phase 1.5. Phase 2 split cohesive exit/replacement logic into
`task_session_exit.rs` and `task_session_replacement.rs` to keep every changed Rust
file below the 1,000-line hard maximum. Two earlier Phase 2 delegate rounds were
rejected during parent review (failed persist consumption and partial healthy
replacement); this implementation addresses both defects.

## Validation

- `cargo fmt --check` — passed.
- `cargo nextest run -p ajax-web web_session` — passed: 334 passed, 299 skipped.
- `cargo clippy -p ajax-web --all-targets --all-features -- -D warnings` — passed.
- Changed handwritten Rust files — at or below 1,000 lines.
- `git diff --check` — passed.

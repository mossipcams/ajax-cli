# Restart agent recovery

Approval: user requested immediate implementation (2026-10-30, "Do that").

## Scope

- Operator Restart agent resubmits the prompt it interrupted, on the resumed
  session, once, through the normal FIFO under a new `clientMessageId`. It lands
  behind already queued prompts, so a prompt that wedges deterministically cannot
  hold queued work hostage. The interrupted ledger row stays interrupted.
- Fresh session action in chat task details, reusing the existing `/clear` host
  path (`{ type: "clear" }`): no new host command unless a wedged actor cannot
  serve it.
- Restart agent is shown only when a turn is in flight or Core offers
  `restart-agent` (task `Blocked`). Fresh session stays available in chat mode.

## Non-goals

- Host restart, unexpected child death, adoption, and every other interruption
  keep the existing "never resent" contract. Only the operator Restart agent
  action resends.
- No wedge detection, timers, or watchdogs.
- No new Core lifecycle or registry semantics; `offers_restart_agent` unchanged.

## Tasks

- [x] Slice 1, Rust: restart resubmits the interrupted prompt.
- [x] Slice 2, Web: Restart visibility + Fresh session; contract updated in
      `docs/architecture/web-session-behavior.md`; `web/dist` rebuilt.
- [x] Parent review of both deltas and gate rerun.
- [x] Commit and PR, from `feat/web/restart-agent-recovery`.
- [ ] Device check of Restart and Fresh session on a wedged live turn.

## Deviations

- Slice 1 first dispatch stopped before any edit: `TaskSessionDirectory.storage`
  is private, so the ledger was unreachable inside the assigned scope. Reissued
  with `task_session_directory.rs` in scope for a zero-line `pub(super)` change
  (the file is at 994 of 1000 lines).
- Resubmission is gated on an attached holder. The planned unconditional resend
  makes a released slot busy and breaks the existing
  `restart_leaves_no_holder_so_respawned_session_is_idle_evictable`, which was
  not weakened. With no holder the prompt stays interrupted.
- Fresh session uses a two-tap confirm in the sheet because `pendingConfirm` only
  serves server actions. It keeps the unsent draft and reports an error instead
  of closing when the session is disconnected.

## Validation

All passed on the final tree, 2026-10-30:

- `env -u RUST_LOG cargo nextest run --workspace`: 3462 passed, 1 skipped.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: clean.
- `cargo fmt --all --check`: clean.
- `npm run web:test -- --run`: 1894 passed, 9 skipped.
- `npm run web:check`, `web:lint`, `web:sg`: exit 0.
- `npm run web:build`: output matches the working-tree `dist/app.js`.

Not run: `web:smoke`, because mobile-webkit cannot run inside nono; CI Web E2E
covers it.

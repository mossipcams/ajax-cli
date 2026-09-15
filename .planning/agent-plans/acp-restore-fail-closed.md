# Plan: Fail-closed ACP restore (re-land #1099 T6 semantics) — #1151

## Problem

Ajax Chat silently spawns a fresh ACP session behind an existing transcript
when a stored session id fails to restore, or when a restored session cannot
prove the operator model pin. Users experience this as "context resets
randomly"; the pi harness is hit hardest because `pi-acp` replays the entire
transcript inside `session/load`, so restore latency grows with context size
and can exceed the 45s handshake timeout.

Root cause: #1099's fail-closed restore contract (`RESTORE_UNAVAILABLE_MARKER`,
`restore_unavailable_error`, `restore_handshake_timeout`, restore regression
tests) was merged, reverted wholesale in #1118, and never re-landed by the
selective #1108 re-lands.

## Scope

- `crates/ajax-web/src/adapters/web_session_acp/sdk_connection.rs` —
  `initialize_session`, `send_resume`, `send_load`.
- `crates/ajax-web/src/adapters/web_session_acp/client.rs` —
  `spawn_with_operator_pin`, restore-unavailable marker helpers, `SpawnReport`.
- Focused ACP client tests (restore coverage beside existing
  `client_tests.rs` / `spawn_tests.rs` / fake-acp fixtures).
- `docs/architecture/web-session-behavior.md` — document the restore contract.

## Non-goals

- Browser Retry/Start-new continuity UI from #1099 T8 (follow-up; the host
  error plus `/clear` is the explicit fresh-start path for now).
- Any change to Switch/cross-harness reset semantics, `/clear`, idle eviction,
  or prompt-ledger behavior.
- Changes to pi-acp or any external bridge.

## Tasks

- [x] T1 — Restore-failure regression test: fake ACP with a stored resume id
      where session/resume and session/load both fail → spawn must return a
      typed restore-unavailable error and must NOT send `session/new`.
      (`client_restore_tests::fake_load_fail_errors_without_session_new_issue_1151`)
- [x] T2 — Restore-failure regression test: harness advertises neither resume
      nor loadSession with a stored id → same typed error, no `session/new`.
      (`client_restore_tests::restore_requires_resume_or_load_capability_issue_1151`)
- [x] T3 — Pin-retry regression test: first attempt restores successfully but
      the pin is unprovable → keep the restored session (no second fresh
      spawn), surface the model-apply error; fresh-spawn pin recovery
      (#989 path, no resume id) still retries.
      (`client_restore_tests::restored_session_survives_unproven_pin_issue_1151`,
      reworked `cursor_spawn_recovers_after_resume_composer_fast_issue_979` for
      in-band recovery on the restored session, unchanged
      `spawn_with_operator_pin_recovery_waits_for_prior_child_shutdown_issue_989`)
- [x] T4 — Restore timeout: dedicated, longer restore timeout for
      session/resume and session/load (pi-acp replays the transcript inside
      the request); test that a load slower than the 45s handshake budget
      still restores.
      (`RESTORE_HANDSHAKE_TIMEOUT` = 300s, `AJAX_ACP_RESTORE_TIMEOUT_MS` override,
      `slow_session_load_still_restores_issue_1151`,
      `restore_timeout_is_a_typed_error_issue_1151`)
- [x] T5 — Implementation: fail-closed `initialize_session`, pin-retry guard
      in `spawn_with_operator_pin`, restore-unavailable marker helpers,
      restore timeout constant. Companion: `drop_session` clears the stored
      resume id so re-entry after a terminal Drop starts the documented fresh
      context instead of failing to restore a closed session.
- [x] T6 — Update `docs/architecture/web-session-behavior.md` restore section.
- [x] T7 — Focused verification (see validation log).

## Acceptance

- A stored resume id never leads to a silent `session/new`: restore failure is
  a typed, classifiable error and the stored id remains persisted for retry.
- A successfully restored session is never dropped merely because the operator
  pin could not be proven; the apply error surfaces instead.
- All existing ACP client/session tests pass; new regression tests fail on
  the pre-fix behavior.

## Approval status

Approved for execution (defect fix re-landing the previously approved #1099
behavior; issue #1151). Delegate until finished within this plan's scope.

## Validation log

- 2026-09-15: `cargo test -p ajax-web --lib -- web_session_acp -- --test-threads=1` —
  106 passed, 0 failed.
- 2026-09-15: `cargo test -p ajax-web --lib -- web_session -- --test-threads=1` —
  383 passed, 0 failed.
- 2026-09-15: `cargo test -p ajax-web --lib -- --test-threads=1` (full crate) —
  684 passed, 0 failed.
- 2026-09-15: `cargo test -p ajax-web --tests` — 684 passed, 0 failed.
- 2026-09-15: `cargo clippy -p ajax-web --all-targets` — clean;
  `cargo fmt -p ajax-web -- --check` — clean.
- 2026-09-15: `npm run verify:slice -- arch` (architecture suites across
  crates) — all green (ajax-core 8, ajax-web 22, ajax-tui 2, ajax-supervisor 2).
- 2026-09-15: `node --check crates/ajax-web/tests/fixtures/fake_acp.js` — ok.
- 2026-09-15: Pre-PR re-verification after the test-module split
  (`task_session_restore_tests.rs`, `client_restore_tests.rs`):
  full `cargo test -p ajax-web --lib -- --test-threads=1` 684 passed,
  0 failed; `cargo clippy -p ajax-web --all-targets` clean;
  `cargo fmt --check` clean; `node scripts/check-file-loc.mjs --staged`
  0 errors (4 warnings: sdk_connection.rs 926 lines and client_tests.rs
  836 lines exceed the 800-line soft warning; PR total 983 changed lines —
  all under the 1000/1500 hard limits).
- 2026-09-15: Regression proof — with `sdk_connection.rs` reverted to pre-fix
  behavior, 4 of 6 `client_restore_tests` fail as intended
  (`fake_load_fail_errors_without_session_new_issue_1151`,
  `restore_requires_resume_or_load_capability_issue_1151`,
  `restore_timeout_is_a_typed_error_issue_1151`,
  `shutdown_close_prevents_resume_when_advertised`); the pin-guard test
  targets `client.rs` and fails with pre-fix `client.rs` by inspection
  (old code returned the second, non-resumed attempt).

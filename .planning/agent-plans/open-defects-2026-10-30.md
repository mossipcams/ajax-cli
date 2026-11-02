# Plan: open-defects-2026-10-30

## Scope

Triage and resolve the 12 open `bug` issues on `mossipcams/ajax-cli` as of
2026-10-30 (main `1139d018`). Each is verified against current main, because
the issue text and severity labels predate the fixes that landed on 2026-10-28.

Result: 6 are already fixed and only need closing, 1 needs a device check, and
5 need work (one of those is already in progress).

## Non-goals

- Dependency Dashboard #1159 (not a defect).
- Open PRs #1535 and #1536. Neither touches a file in this plan.
- Phase 0 residuals that the issues did not ask for (see Decisions).
- The root cause of the transient `SQLITE_CANTOPEN` noted in #1504.

## Approval

**Status: approved 2026-10-30 (Matt: "Free up space then continue").** The #1482
question went unanswered, so the assumption under Decisions stands. No batch
changes architecture, lifecycle, or security semantics.

## Findings

Stale state came from three things: PRs #1494 and #1495 were a stack that did
not say `Fixes #N`, PR #1482 was closed unmerged, and #1379 did not reference
#1378.

| Priority | Issue | Label | Real status on main | Action |
| --- | --- | --- | --- | --- |
| 0 | #1488 double push tick | low | Fixed by #1494. One call in `spawn_background_loops` (`runtime/mod.rs:194`) | Close |
| 0 | #1489 Ship with open PR | high | Fixed by #1494. `recommended.rs:142,232`; test `open_pull_request_hides_ship_for_reviewable_and_mergeable_tasks` | Close |
| 0 | #1490 Fix CI on chat tasks | medium | Fixed by #1494. `live.rs:583` routes chat tasks to the queue; tests `chat_remediation_*` in `refresh_lane_tests.rs` | Close |
| 0 | #1491 push suppressed after lock | medium | Fixed by #1494. `/api/presence`, TTL 45 s, `presence.ts`; tests `presence_tests.rs` | Close |
| 0 | #1492 attach revision race | medium | Fixed by #1495. `SESSION_ATTACH_ATTEMPTS = 2`; tests in `refresh_lane_tests.rs` | Close |
| 0 | #1493 start holds control lane | high | Fixed by #1495. Test `task_start_external_effects_run_without_control_lane` | Close |
| 1 | #1534 restart silent, false note | high | **Open.** Uncommitted fix in `../ajax-restart-agent` | Finish that branch |
| 2 | #1435 transcript backlog lost | medium | **Partly fixed.** #1504 added the idle retry; shutdown loss remains, and it added a new gap | Batch 2 |
| 3 | #1278 child leak on reader spawn | none | **Open.** `?` at `jsonl_process.rs:106,115`, moved by the shim rework but unfixed | Batch 3 |
| 4 | #1378 Cloudflare sign-in | medium | Fix landed in #1379; needs the device check in the issue | Matt checks |
| 5 | #1407 flaky tests | low | **Half fixed.** #1502 moved the fault guard; the pi dialog test still reads once | Batch 4 |
| 6 | #1365 Codex restore timeout | medium | **Test-only bug.** Shipped code is correct, so the label overstates it | Batch 4 |

Ordering rationale: #1534 hits every restart and misleads operators on the
recovery path. #1435 loses data but needs a prior persistence failure. #1278
needs thread-spawn failure, which is rare. #1407 and #1365 affect only tests.

## Decisions

- **Re-implement from #1482 on current main, do not reopen it.** It is 33
  commits behind and conflicts in `jsonl_process.rs` and `task_session.rs`. It
  was closed without a comment, so this assumes it was superseded by #1504, not
  rejected. Matt should say if there was another reason.
- **#1534 stays on its existing branch.** No second branch.
- **Separate PRs for each batch**, so one regression does not hold up the rest.
- **Residuals stay out of scope.** The second consecutive attach race keeps its
  misleading error (#1492 accepts this), the direct `ajax ship` CLI still merges
  with an open PR (#1489 only covered offered actions), and the "Response ready"
  push is still deferred (#1491).

## Working rules

- Dispatch implementation through `model-router`. Ban `git stash`, `checkout`,
  and `reset` in every delegate prompt.
- Each regression test must fail on main before the fix.
- `fix(web): ...` titles, `Fixes #N` in the body, `scripts/gh-pr-create`.
- **Disk precondition:** the volume had 3.9 GiB free on 2026-10-30, and an
  `ajax-web` `target/` here runs about 7 GiB. Free at least 15 GiB before any
  build. A full disk breaks the live server's SQLite, which is also the trigger
  for #1435.

## Checklist

### Step 0: close what is already fixed

- [x] Close #1488, #1489, #1490, #1491, #1492, #1493, each with a comment naming
  the merged PR and the regression test. Done 2026-10-30.

### Step 1: device check (Matt)

- [ ] #1378: leave the Home Screen app open past an Access expiry. Close the
  issue if it reaches Cloudflare sign-in, otherwise record what appeared.

### Batch 1: #1534, finish `fix/web/restart-silent-success`

Not started by this plan. As of 2026-10-30 the work is still uncommitted in
`../ajax-restart-agent`, owned by that session, so it was left alone.

The worktree has server output, a `Restarting agent…` message, a success
message, and `settle_resent_prompt`, which marks the prompt interrupted between
detach and acquire so recovery writes no false note.

- [ ] Commit the work from its own session, or have that session hand it off.
- [ ] Review: confirm `client.detach()` ends the shim for Claude SDK and Pi as
  well. `tear_down` does this for ACP, but the test only covers Claude SDK. A
  live shim would change the adoption path, whose `interrupt_adopted_prompt`
  also writes the note.
- [ ] Review: if the re-send fails after settling, the transcript has no note,
  only the error banner. Decide whether to add one.
- [ ] Review: `Restarting agent…` must stay visible for the full 2.5 to 15 s.
- [ ] Update `docs/architecture/durable-execution.md:147`, which still says
  dispatching prompts are "never resent". The branch only updates
  `web-session-behavior.md`.

### Batch 2: #1435, PR #1539

Write scope: `task_session.rs`, `task_session_commit.rs`,
`transcript_backlog_tests.rs` (new), `web-session-behavior.md`. Put new logic in
`task_session_commit.rs`, because `task_session.rs` is already over 600 lines.

- [x] Shutdown: flush the backlog after the actor loop exits, before
  `close_on_exit` handling. If a terminal commit is pending, finalize through
  `try_finalize_active_prompt`. Warn if events are still dropped.
- [x] Backoff: retry at 2 s doubling to 30 s. Main retries on every pump, about
  20 times a second during an outage. This gap came from #1504.
- [x] Run the retry on the poll tick. `pump` only runs when a client exists, so
  today a session with no client never retries.
- [x] Retry mid-turn when the active prompt has no terminal yet, so a stalled
  turn still reaches the log.
- [x] Port the tests from `4c3febf6`, adapted to `persist_log_batch`. Skip the
  `session_evidence.rs` dedupe, since main already checks at the call site.

Fail-before evidence: disabling the shutdown flush, the terminal branch, the
pending-terminal skip, and the tick retry each fails its `issue_1435_*` test.

Known small gaps: a backlog created by the ledger-failure path retries at once,
then waits 4 s rather than 2 s. `TaskSessionCommand::Pump` is a `cfg(test)` seam
that resets the backoff, because `issue_1503` expects a pump to clear the fault
immediately. `TaskSessionState::new` was extracted so tests can build bare state;
a line-multiset diff confirmed it is a pure move.

### Batch 3: #1278, PR #1540

Write scope: `jsonl_process.rs`, `jsonl_process_group_tests.rs`.

- [x] Extract the reader-thread spawns from `start()`. On failure, call
  `kill_tree` and `wait` on the owned child.
- [x] Do not kill in the `attach` path. There `child` is `None` and `pid` is a
  live shim holding a running agent. Add a test for this.
- [x] Port `failed_reader_thread_start_kills_and_reaps_the_child` from #1482.

### Batch 4: #1407 and #1365, PR #1541

Write scope: `pi_dialog_tests.rs`, `live_harness_tests.rs`, `client.rs` and
`adapters/web_session_acp/mod.rs` (visibility only). These change tests because
the bugs are in the tests. No assertion changes.

- [x] #1407: poll the ledger with a 5 s deadline in the pi dialog test before the
  existing assertion, as #1482 did.
- [x] #1365: run the live resume on `RESTORE_HANDSHAKE_TIMEOUT` via
  `with_test_restore_timeout`, and expose that constant to the test.
- [x] #1365: run the opt-in live test unsandboxed. It needs real Codex credentials:
  `AJAX_LIVE_HARNESS=codex cargo test -p ajax-web --lib live_harness_parity -- --ignored --nocapture`

## Validation

Required for each Rust batch:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
env -u RUST_LOG cargo nextest run --workspace
```

Also required for Batch 1, which touches web code:

```bash
npm run web:check && npm run web:lint && npm run web:sg && npm run web:test
pkill -f vite; CI=1 npm run web:smoke
```

`web:smoke` cannot run inside nono. Cite the CI Web E2E job instead.

### Results so far

- Disk: freed from 3.9 GiB to 33 GiB by clearing the npm and uv caches and
  removing compiler output from `.ajax-trash` entries. Source, logs, and other
  files in those entries were kept.
- Batch 2: `cargo nextest run -p ajax-web` 1551 passed, 1 skipped (rerun by the
  parent); `cargo fmt --check` clean; workspace clippy reported clean by the
  delegate. The delegate's report is flagged invalid only because its
  fail-before entries correctly recorded `fail`.
- Batch 2 gate on `f7e6849a`: workspace clippy clean, `nextest --workspace
  --all-features` 3441 passed, 1 skipped. CI green on #1539.
- Batch 3 gate on `f7e6849a`: workspace clippy clean, 3435 passed, 1 skipped.
  CI green on #1540. Removing the kill fails its test.
- Batch 4 gate on `f7e6849a`: workspace clippy clean, 3433 passed, 1 skipped. The
  pi dialog test passed 20 of 20. The live Codex test passed once, including the
  resume; the defect failed about 2 runs in 3, so that is not conclusive alone.
- `git merge-tree` shows #1539 merges cleanly with each of #1540 and #1541.
- The triage itself ran no local build, because of the disk precondition.
- Fixed-on-main status comes from reading code at `1139d018` and from CI: Rust
  Test passed on #1532 (`464aaabd`), and Web Unit and Web E2E passed on #1531
  (`efbb4705`). The only later change, #1533, touches a doc and a script.
- CI shows the suites passed, not each named test individually.

## Deviations and assumptions

- Qwen returned `PROVIDER_ERROR` on Batch 3, so Batches 3 and 4 ran on the Haiku
  fallback.
- The status checks were focused reads in this session, not a delegated
  discovery round. Implementation still goes through the router.
- The request was cut off at "resolve each afte". This plan assumes it asked for
  prioritization and a plan only, so no fixes were made.

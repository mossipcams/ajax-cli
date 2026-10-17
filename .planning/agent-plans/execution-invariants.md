# Execution invariants: harness switch, event durability, recovery, fencing

## Status

- Approval: **APPROVED with decisions D1–D3** (Matt, 2026-10-16). PR 1 slice A (T1.1–T1.4 and tests a–c) complete; T1.5, test d, and documentation remain pending.
- Source: external "Deep Architectural Review" (post v0.60.0), verified against
  `main` @ 3fdd0ddd on 2026-10-16. Verdicts below are from reading source, not
  from repro. Each confirmed defect still needs an issue + a failing regression
  test before its fix (`docs/defect-process.md`).

## Verdict on the review

All four findings are real code paths. The proposed remedy (persisted
`SwitchPending` state machine, recovery coordinator, monotonic fencing
generation, Core status derived from execution evidence) is heavier than the
code needs. Every finding closes with a reorder, an existing column, or an
existing loop. Severity is re-ranked: most paths need a SQLite write to fail,
and the database is a local file. rusqlite's default 5 s busy timeout covers
ordinary lock contention, so failures come from disk-full, permission, or
long-held locks. One path, the switch identity split, is reachable **without**
any SQLite failure, through #1272.

| # | Finding | Verified | Re-ranked | Why |
|---|---------|----------|-----------|-----|
| 1B | Registry says new harness, live slot keeps old client | Yes | **P0** | Route commits `selected_agent` (live.rs:390-397), drops the lane, then resets. Reset failing at `release_live_client` (e.g. #1272 idle-cancel-is-fatal on Claude SDK/Pi) leaves old client alive; next `acquire` sets `state.agent = agent` (spawn.rs:40) and reuses it (spawn.rs:42-46). Also: run row `harness` is never updated (`RunUpdate` has no field), and cold acquire resumes `native_session_id` regardless of harness, so a crash between route persist and `clear_acp_session_id` resumes a Claude id under Codex. |
| 1A | Discarded queue resurrects | Yes | P1 | `let _ = try_persist(...)` at spawn.rs:233 and :297. Next acquire's `recover_prompt_ledger` reloads them. **Extra gap the review missed:** no-live-slot switch (`directory.reset_harness_context` → `clear_stored_session_id`, directory.rs:466) never discards the SQLite queue, so headless startup dispatch runs them under the new harness. The doc (web-session-behavior.md:213) says "persist only", so the live and idle paths disagree. |
| 2B | Drained events dropped on append failure | Yes | P1 | `let _ = self.append_to_log(normalized)` (task_session.rs:281); `try_finalize_active_prompt` does not check `transcript_durability_fault`, so the turn can commit as completed with a hole. Mitigation the review missed: the fault blocks new admit and dispatch (exit.rs:344, :482). |
| 2A | Activity reported before transcript commit | Yes | P2 | `filter_events_for_log` reports per event before `try_append_events` (task_session.rs:201), and also before `commit_ledger_with_events` (task_session_commit.rs:14). Status can lead history. Cosmetic until 2B is fixed. |
| 4 | One-shot recovery | Yes, overstated | P2 | `SWEPT_ONCE` set before work (orphan_sweep.rs:40); `headless_scanned.swap(true)` before scan (directory.rs:69); `let _ = report(...)` after `take_recovered_handles` (directory.rs:119). **But** per-session `recover_prompt_ledger` re-interrupts Dispatching prompts on attach, so prompts are not lost. Real losses: orphan children never killed if the first sweep's listing fails; queued work waits for a manual attach; dashboard stays "Agent working". |
| 3 | No fencing; SIGTERM ≠ exit; best-effort child identity | Yes | P2 | Single machine, single DB file. Takeover of a *live* owner needs a ≥30 s heartbeat stall. `commit_execution` does not check the owner. Sweep clears identity right after `kill -TERM` (orphan_sweep.rs:93). `record_child_identity` only logs failures (task_session_child.rs:43). "Agents already running when the lease was lost are not stopped" is **documented** (durable-execution.md §6), so changing it is a semantics change. |

## Scope

Close each verified path with the smallest change, ordered by severity. Each
PR stays under the commit-size gate (600 lines/file, 1500/PR).

## Non-goals (from the review, agreed)

Global task concurrency, tmux removal, ACP naming, sub-agent visualization,
mid-turn tool replay, and wiring `resolve_recovery` into the host.

## Rejected from the review's remedy

- **Persisted `SwitchPending` state.** Rejected. The registry's `selected_agent` already records the switch intent durably. `agent_runs.harness` already exists to record actual identity. A mismatch between the two is the pending-switch record. The session converges on acquire. No new table and no new state enum.
- **Status derived from execution evidence and removed from Core.** Rejected. That crosses the Core boundary for a P2. A reorder fixes it: report after commit.
- **Recovery coordinator.** Rejected. The 10 s heartbeat tick already calls `report_recovered_turns` and is the retry loop. The fix is to set "done" flags only on success.
- **Monotonic fencing generation.** Rejected. `boot_id` is unique per process and each claim overwrites it, which makes it a fencing token already.

## PR 1: harness switch converges to the registry (1B + 1A)

Branch `fix/web/harness-switch-convergence`. Open one issue covering 1A, 1B
and the idle-path gap.

- [x] **T1.1** Add `harness: Option<String>` to `RunUpdate` (ajax-core `execution_store.rs`) and apply it in `commit_execution`. `try_save_meta` / `clear_acp_session_id` callers pass the real harness, so reset stops writing `"unknown"` (spawn.rs:255, :319). The column already exists, so no schema bump.
- [x] **T1.2** Cold acquire (spawn.rs:63-73) and `replace_resume_id`: treat `native_session_id` as `None` when `run.harness != agent_client_name(agent)`. This fixes the crash between route persist and session-id clear.
- [x] **T1.3** Warm acquire (spawn.rs:40-46): compare the incoming `agent` against `state.agent` **before** assigning. On mismatch, run the reset path (`reset_harness_context`) instead of reusing the client. If the reset fails, return the error and do not report success.
- [x] **T1.4** Make the queue discard durable and fail-closed. In `reset_harness_context` and `clear_session_context`, replace `remove_queued` + `let _ = try_persist` with `commit_ledger_with_events(|l| l.remove_queued(), vec![])`, called **before** `release_live_client`. Propagate the error. Apply `apply_cancel_to_queue` in memory only after the commit succeeds.
- [x] **T1.5** Idle path (`directory.reset_harness_context`, no live entry): load the ledger, run `remove_queued`, and persist with the session-id clear. Update web-session-behavior.md:207-226 so both paths say "discard the queue".
Tests (red first; reuse the existing `force_meta_write_fail`-style hooks):

- [x] **(a)** A forced ledger-persist failure during switch leaves the queue and the old client untouched and returns an error.
- [x] **(b)** A live slot plus a registry agent change, followed by `acquire(new_agent)`, does not reuse the old client.
- [x] **(c)** A run with harness=claude and a stored id, cold-acquired as codex, does not resume.
- [x] **(d)** An idle switch with queued SQLite prompts leaves `queued_count() == 0`.
- [ ] **Doc:** Update web-session-behavior.md:207-226 so both live and idle switch paths say "discard the queue".

## PR 2: no observed event is dropped or reported early (2B + 2A)

Branch `fix/web/event-commit-order`. Open one issue.

- [x] **T2.1** Add `unpersisted: Vec<SessionServerEvent>` to `TaskSessionState`. `append_to_log` prepends the backlog. On failure it stores the batch there and returns `Err`. On success it clears the backlog. Order and sequence are preserved because nothing reaches `log` before commit. Add `// ponytail:` with a cap of 2000, matching run retention.
- [x] **T2.2** `try_finalize_active_prompt`: if `unpersisted` is non-empty, move it in front of `terminal.events` so both commit in one `commit_ledger_with_events` transaction. On failure everything stays and the next pump retries. No turn completes with a hole.
- [x] **T2.3** Split `filter_events_for_log` into dedupe (before commit) and `report_activity_for_event` (after commit). Apply the split in both `append_to_log` and `commit_ledger_with_events`.
- [x] **Tests (red first):** the append fails once, then succeeds. Assert: no status reported for the failed batch, the events appear in order after recovery, and the turn is not marked completed until they do.
- [x] Doc: durable-execution.md §4 "No partial visibility" adds that activity reporting follows the commit.

## PR 3: recovery retries until it succeeds (4)

Branch `fix/web/recovery-retry`. Open one issue.

- [x] **T3.1** `sweep` / `recover_interrupted_prompts` return `Result` with a "complete" flag: listing failure, ledger load failure, or persist failure counts as incomplete. Set `SWEPT_ONCE` only when the run completes, using compare_exchange **after** the work. Hold a mutex so ticks cannot run it concurrently.
- [ ] **T3.2** `startup_dispatch_handles`: set `headless_scanned` only on a complete scan. Keep a `HashSet` of handles already handed out, so a rescan does not dispatch the same chat twice.
- [x] **T3.3** `report_recovered_turns`: push handles whose `report` returned false back into `recovered_handles`. The heartbeat retries them every 10 s.
- [x] **T3.4** Call the sweep and scan retry from `heartbeat_execution_ownership` while incomplete. No new task or coordinator.
- [ ] **Tests:** fail `runs_with_children` / ledger load on the first call and succeed on the second within the same process. Assert the orphan is signalled, the prompt is interrupted, and the status is reported exactly once.
- [ ] Doc: durable-execution.md §5/§6 replace "one-time" with "until complete".

## PR 4: fencing and verified termination (3). Ships now (D3), without T4.4 (D2).

Branch `fix/web/execution-fencing`. Open one issue.

- [ ] **T4.1** Add `owner_fence: Option<String>` to `ExecutionCommit`. When it is set, `commit_execution` checks `SELECT boot_id FROM execution_owner WHERE id = 1` inside the IMMEDIATE transaction and returns `ExecutionCommitError::Fenced` on mismatch. The web host's `web_session_store` writes pass `ownership.boot_id()`. The sweep and recovery pass it too, because they run after the claim.
- [ ] **T4.2** Orphan sweep: after `kill -TERM`, poll `pid_alive` for up to 5 s, then `kill -KILL` the group. Clear the identity only once the process is observed gone; otherwise leave the row so the PR 3 retry picks it up. Block `acquire` for that handle while its row still holds a foreign live child.
- [ ] **T4.3** `record_child_identity` returns `Result`. Spawn and replace paths discard the staged client and fail the attach on error. Without this, the sweep cannot find the child later.
- ~~T4.4 Stop local children on lease loss~~ **Dropped (D2).** durable-execution.md §6 stays: agents running at lease loss are not stopped by their own host. The new owner still ends them: its first-claim orphan sweep targets every run whose `owner_boot_id` is foreign and whose child token matches, which includes a live-but-stale previous owner's children. T4.2 makes that termination verified. The old host's late writes are rejected by T4.1 and land in PR 2's `unpersisted` backlog as a durability fault, not in history.
- [ ] **Tests:** two `ExecutionOwnership` instances on one DB; force takeover; assert the old instance's commit returns `Fenced`; assert the new owner's sweep terminates the old instance's recorded child; assert the sweep does not clear identity for a process that ignores SIGTERM until SIGKILL.
- [ ] Doc: durable-execution.md §6 documents the owner fence, the verified sweep, and that a displaced host's agents are ended by the new owner's sweep (not by the displaced host).

## Decisions (Matt, 2026-10-16)

- **D1 = discard.** An idle-path switch discards SQLite-queued prompts (T1.5), and the doc is updated to match the live path.
- **D2 = no.** Do not stop agents on lease loss; T4.4 is dropped and §6's "not stopped" stays.
- **D3 = do not defer.** PR 4 (T4.1–T4.3) ships after PR 3, in order PR 1 → 2 → 3 → 4. PR 4 depends on PR 2's backlog (fenced writes) and PR 3's retry (uncleared sweep rows).

## Delegation

Per AGENTS.md, use model-router → `scripts/run-delegate`, one PR per packet,
red test first. The packet bans stash, checkout and reset. Review gate: the
exact hook commands (`cargo clippy --all-targets --all-features -D warnings`,
`cargo fmt --check`, `cargo nextest run -p ajax-web -p ajax-core`). No UI is
touched, so web:smoke is not required.

## Deviations

- Slice A scope had to widen to `web_session_store/` and `task_session_directory` / `task_session_replacement` for mechanical caller updates.
- `finalize_client_metadata` also passed `"unknown"` as harness and was fixed to pass the real harness; otherwise `try_save_meta` overwrote it and crash-recovery resume broke.
- The 2000-event backlog ceiling is documented in a ponytail comment but not enforced, because dropping would reintroduce a transcript hole.
- Existing sweep/startup-recovery tests were updated mechanically for the new RecoveryResult/SweepResult return shape (assertion meaning kept, `complete` asserts added); the Codex delegate hit its usage quota mid-run but its edits were reviewed and gated by the orchestrator.
- Slice G was split into Qwen-sized packets (G2 T3.3, G3 T3.4, then G1 T3.2 and G4 headless retry + docs) because only the Qwen route was dispatchable (Codex over quota, Cursor needs login, Haiku route rejected its model id).

## Validation

Prior implementation validation reported for PR 1 slice A:

- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo nextest run -p ajax-core -p ajax-web`: 2247/2247 passed. One load flake in `terminal_persist_failure_retries_before_fifo_advances` was seen once; it passed in isolation and on two reruns.

Slice C validation (T1.5, test d, web-session-behavior.md doc update):

- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features -D warnings`: passed.
- `cargo nextest run -p ajax-core -p ajax-web`: 2248/2248 passed.

PR 2 slice D (T2.1/T2.2: unpersisted backlog retained on append failure and committed with the terminal events) validation:

- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features -D warnings`: passed.
- `cargo nextest run -p ajax-core -p ajax-web`: 2250/2250 passed.

PR 2 slice E (T2.3: activity reported only after the transcript commit, in append_to_log and commit_ledger_with_events; durable-execution.md section 4 updated) passed cargo fmt --check, clippy --all-targets --all-features -D warnings, nextest -p ajax-core -p ajax-web 2252/2252.

PR 3 slice F (T3.1: sweep/recover_interrupted_prompts return a completeness flag; SWEPT_ONCE set via compare_exchange only after a complete run, under a mutex) passed cargo fmt --check, clippy --all-targets --all-features -D warnings, nextest -p ajax-core -p ajax-web 2255/2255.

PR 3 packets G2/G3 (T3.3: handles whose activity report failed are requeued and retried by the heartbeat; T3.4: a healthy heartbeat retries an incomplete orphan sweep via sweep_once) passed cargo fmt --check, clippy --all-targets --all-features -D warnings, nextest -p ajax-core -p ajax-web 2257/2257 (one unrelated flake, cursor_sdk_spawn_tests::module_is_resolved_only_from_the_given_root, failed once under load; it passed in isolation and on two full reruns).

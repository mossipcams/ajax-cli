# Plan: single-owner-session-state

## Scope

Fix two structural flaws in chat session state and replace them with a
single-owner design.

- **Flaw A:** registry snapshot saves have many writers. One task's conflict
  fails the whole save, which caused #1555.
- **Flaw B:** chat changes state before the effect is confirmed. This causes
  #1546, #1547, #1548, #1549, #1556, #1557 and #1558.

Target design:

- A per-task `record_activity(task, ...)` in `ajax-core/src/registry/sqlite`:
  one transaction that also appends to `registry_events`, with backoff on
  retry.
- An `AgentSlot` enum, `Absent | Spawning | Live | Exited | Faulted`, where
  every state carries `resume_id`. It replaces `AcpSlot.client: Option<_>` and
  `acp_alive`.
- Actor rules:
  1. Prepare, act, commit (spawn B before releasing A or saving to the
     registry).
  2. Health signals move the slot.
  3. The sidecar exits when its pump dies.
  4. No `Ok` without confirmation.
  5. SIGTERM flushes for up to 5 s; no eviction while backlog is unsaved.
  6. Rate and usage limits map to `RateLimited`, with no Error push.

Steps (S = an evening, M = 2–3 evenings, L = a weekend):

| # | Step | Size | Closes |
|---|------|------|--------|
| 0a | Swap matrix test: fake harness, 4 harnesses × 3 targets × {ok, spawn fails} = 24 cases; asserts harness, liveness, resume id, queued prompts, user-visible error; 12 failure cases `#[ignore = "#1546"]` | M | – |
| 0b | `scripts/health-check <hours>`: grep `ajax.log` for chat signatures (activity report retry, agent stop timeout, transcript, harness swap), count each, non-zero exit if any found | S | – |
| 0c | File an issue for the rate-limit detection gap | – | – |
| 1 | Reorder swap to prepare, act, commit; un-ignore the failure cases | S | #1546 |
| 2 | Classify per-harness limit errors as `RateLimited` | M | 0c |
| 3 | Refresh the usage gauge on model/harness switch; mark it stale when the usage query fails | S | #1548, #1549 |
| 4 | `record_activity` + route chat activity through it + exponential backoff | M | #1555 |
| 5 | Introduce `AgentSlot`; convert every `client`/`acp_alive` read to a match on it | L | – |
| 6 | Health signals: sidecar exits on pump death, slot moves to Exited/Faulted, UI offers Restart with resume | M | #1556, #1557 |
| 7 | Typed error when the harness doesn't confirm a model change | S | #1547 |
| 8 | SIGTERM handler with actor flush; eviction guard | S | #1558 |

## Non-goals

Deferred (not chat):

- #1553 and #1550 (task start/drop as commands).
- The web registry as a read cache, plus deleting `merge_registries`.
- #1554 (`drop_failed_detail`).
- #1378.

## Approval status

Approved by user 2026-11-05 (planning session). Implementation starting at
step 0a. Step 0a PR opened 2026-11-05.

## Checklist

- [x] 0a — Swap matrix test (fake harness, 24 cases; 12 failure cases
  `#[ignore = "#1546"]`). Size: M. Closes: –. Done in this PR: 12 ok cases
  pass; 12 spawn_fails cases `#[ignore = "#1546"]`, each failing at the
  old-harness liveness assertion. Registry-level rollback is asserted by a
  route-level test in step 1.
- [x] 0b — `scripts/health-check <hours>` chat-signature counter. Size: S.
  Closes: –. Signatures (substring match on ajax.log): `session activity
  report failed` (activity_report_retry, 168h = 2); `session/close timed out`
  (agent_stop_timeout, 168h = 0); `transcript events retained for retry`,
  `failed to prune web session transcript`, `failed to delete legacy web
  session transcript`, `imported incomplete web session transcript`
  (transcript, 168h = 0). Harness swap has no log line
  (`harness swap worker failed` and `apply_cross_harness_reset` errors are
  returned to the client, not logged), so it is left out. The stop-timeout
  signature only matches when logged; the `shim_control` close timeout in
  `client.rs` returns an error without logging.
- [ ] 0c — File an issue for the rate-limit detection gap. Size: –.
  Closes: –
- [x] 1 — Reorder swap to prepare, act, commit; un-ignore failure cases.
  Size: S. Closes: #1546
  Done: session reorder (task_session_spawn.rs), route registry rollback
  (live.rs), 24/24 matrix cases pass with none ignored, route tests
  `issue_1546_*` in runtime/tests/suite_6.rs, `harness_swap` health-check
  signature.
- [ ] 2 — Classify per-harness limit errors as `RateLimited`. Size: M.
  Closes: 0c
- [x] 3 — Refresh usage gauge on model/harness switch; mark stale on query
  failure. Size: S. Closes: #1548, #1549
  Done: `NativeEvent::UsageUnknown` -> `usage_reset` on a failed/empty Claude usage query; `usage_reset` after a successful model change for all harnesses; no frontend change (`usageProjection.ts` already clears on `usage_reset`).
- [x] 4 — `record_activity` + route chat activity through it + exponential
  backoff. Size: M. Closes: #1555
  Done: 4a retry backoff (session_evidence.rs), 4b `record_activity` (registry/sqlite/activity.rs), 4c per-task routing (runtime/task_activity_write.rs) with the #1555 two-task regression.
- [x] 5 — Introduce `AgentSlot`; convert every `client`/`acp_alive` read to a
  match on it. Size: L. Closes: –
  Done: `AgentSlot` (web_session/agent_slot.rs) replaces `AcpSlot.client` and `acp_alive`; no reads of either remain.
- [x] 6 — Health signals: sidecar exits on pump death, slot moves to
  Exited/Faulted, UI offers Restart with resume. Size: M. Closes: #1556, #1557
  Done: the Claude sidecar shuts down when its pump dies (claude_sdk_sidecar.mjs), regression `claude_pump_failure_exits_the_sidecar_1556`.
- [ ] 7 — Typed error when the harness doesn't confirm a model change. Size:
  S. Closes: #1547
- [ ] 8 — SIGTERM handler with actor flush; eviction guard. Size: S. Closes:
  #1558

## Deviations

- **No Plan 1 exists.** The original Plan 2 depended on Plan 1's "journey
  gate", health check and rate-limit finding. None of these exist, so step 0
  builds the minimum needed (0a, 0b, 0c).
- **Plan 3 references folded in.** Plan 3 references were folded into this
  plan. If a Plan 3 turns up, those lines can point back to it.
- **Three-strike rule.** If a step takes 3 tries and still has a major
  defect, stop patching and rewrite only the failing unit: keep the tests, and
  make the rewrite smaller than what it replaces. Saved to memory as
  `three_strikes_then_rewrite.md`.
- **Unlogged chat failures (found in 0b).** Harness swap failures and the
  shim_control close timeout in `client.rs` are returned, not logged, so
  `scripts/health-check` cannot count them. The harness swap part is resolved
  by step 1 (`warn!` on swap failure plus a `harness_swap` signature); step 6
  does the same for the stop timeout.
- **Step 4 design (user decision 2026-11-05).** `record_activity` writes no `registry_events` row (that table is renumbered positionally by every full save), and lifecycle-changing activity reports keep the whole-registry save; only non-lifecycle reports go per task.
- **Step 5 scope.** `AgentSlot` has `Absent | Spawning | Live | Exited`. `Faulted` and a slot-held `resume_id` are not added: nothing produces a faulted state today, and the resume id stays in the store (`agent_runs.native_session_id`) and is re-read on every spawn, so both would be dead code. Step 6 adds `Faulted` only if a health signal produces it.
- **Step 6 scope.** #1557 (a dead client never respawned on an open socket) was fixed on main by #1582 (lazy revival on the next prompt, attach, or model change, and Restart as an actor command), which this stack predates. Step 6 therefore only fixes #1556. The cursor sidecar (per-run pump) and Pi (EOF already reported) need no change. No producer of a faulted-but-alive state exists, so `Faulted` is not added. Until the stack is merged with main, a pump death here shows the existing 'ACP process exited' error instead of a silent stall.

Three-strike definitions:

- A try is an attempt that reaches the gate or the live server.
- A major defect is a gate failure, a failing matrix case, a reopened bug, or
  a health-check signature after deploy.
- On the 4th attempt, rewrite the smallest failing unit instead of patching.
  Count tries in the PR description.

Facts checked against the code (so they need not be re-checked):

- `merge_registries` is at `crates/ajax-cli/src/context.rs:293`.
- `acp_alive` is at `crates/ajax-web/src/slices/web_session/acp_slot.rs:11`. It
  is also used in `task_session.rs:201` and `transcript.rs:248`
  (`slot_must_replace`).
- `retry_pending_activity_report` is at
  `crates/ajax-web/src/slices/web_session/session_evidence.rs:88`.
- The web server has no SIGTERM handler; only
  `ajax-supervisor/src/runtime.rs` and
  `ajax-cli/src/tmux_task_session/attach.rs` have one.
- A `RateLimited` variant already exists at
  `crates/ajax-core/src/models/observations.rs:35`. Reuse it in step 2.
- Neither `scripts/health-check` nor a swap matrix test exists yet.
- Docs to update: `architecture.md` (repo root) and
  `docs/architecture/web-session-behavior.md`.

Risks and mitigations:

- Step 5 creeps into the UI. Mitigation: no UI changes except errors and
  Restart, and three-strike rewrites stay inside the failing unit.
- The launchd server runs a stale binary. Mitigation: compare the server start
  time against the binary's mtime.
- #1553 keeps biting web task start. Accepted; restart stuck Provisioning tasks
  by hand.
- New features sneak in. Mitigation: no feature PRs until the plan is done.

## Validation

Gate for every PR:

```bash
env -u RUST_LOG cargo fmt --check
env -u RUST_LOG cargo clippy --workspace --all-targets --all-features -- -D warnings
env -u RUST_LOG cargo nextest run --workspace --all-features
```

Steps 3, 6 and 7 also run:

```bash
pkill -f vite; CI=1 npm run web:smoke
```

After each step, re-run `ajax web service install`.

Tests to add:

- Two chats, one DB, with a conflicting second writer (step 4).
- Limit classification for each of the 4 harnesses (step 2).
- Slot invariants: every state with a `resume_id` offers Restart; nothing
  reports Working without a live client (steps 5–6).
- Error messages name the failed step and show in chat (steps 6–7).

Done when:

- All 24 matrix cases pass with none ignored.
- `RateLimited` works for all 4 harnesses.
- `client`/`acp_alive` are gone.
- All 8 bugs are closed, each with a regression test.
- A conflict on one task no longer blocks another task's activity.
- Seven days of `scripts/health-check 168` at zero.
- Architecture docs are updated (`architecture.md`,
  `docs/architecture/web-session-behavior.md`).
- No step was closed on a 4th patch.

Results: step 0a gate passed (see PR).

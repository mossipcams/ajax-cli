# Plan: Durable ACP chat-context continuity

## Objective

Make Ajax Chat safe to leave and return to. Browser navigation, reload, tab
closure, WebSocket reconnect, idle reclamation, `ajax-web` restart, and ACP
child replacement must preserve the same model context when the harness can be
restored. If restoration is unavailable or fails, Ajax must preserve the old
session identity, expose a blocked continuity state, and refuse prompts; it
must never silently create a fresh model context behind the existing transcript.

## Scope

- Make ACP context identity and continuity host-owned, typed state in
  `slices::web_session`.
- Make an existing stored ACP session fail closed when `session/resume` and
  `session/load` cannot restore it.
- Keep browser/viewer lifetime independent from ACP context lifetime.
- Permit idle detachment only when the session has a persisted restore identity;
  retain that identity until an explicit operator reset or task Drop.
- Expose continuity state in protocol-v2 snapshots and disable prompting while
  context is unavailable.
- Add explicit Retry restore and Start new context operations. Starting new
  context is the only non-Drop path allowed to replace a lost session identity.
- Keep unacknowledged browser prompts across tab closure using the existing
  browser outbox and host ledger, without making the browser queue authority.
- Surface transcript/meta persistence failure and stop accepting new prompts
  while durable session state is unavailable.
- Prove continuity through backend, WebSocket, and browser tests, plus a
  per-harness restore-capability audit.
- Update `architecture.md`, `docs/architecture/web-session-behavior.md`, and
  `docs/architecture/web-cockpit.md` with the final invariant and failure UX.

## Non-goals

- No message broker, database-backed actor system, service worker, or multi-host
  Ajax Web deployment.
- No claim that an in-flight model turn completes across a child/process crash;
  it may be marked interrupted, while completed-turn context must restore.
- No automatic transcript-to-prompt replay presented as exact model-context
  restoration. A later explicit rehydration feature may summarize history, but
  it is not continuity.
- No context migration across harness Switch or model families. Switch discards
  the previous ACP conversation; Ajax does not restore, retry, or preserve it.
  Start new context remains the unavailable-state recovery path on the same
  harness. Switch is only a new-context boundary (fresh `session/new` + epoch).
- No unlimited transcript retention or guarantee through permanent loss of the
  harness's own durable session store.

## Architectural invariants

1. The task-scoped host session owns context identity; a WebSocket owns only a
   viewer lease.
2. A stored ACP session id means "restore this context," never "try it and
   silently fall back to `session/new`."
3. Context state is one of `live`, `restored`, or `unavailable`, with a stable
   context epoch. Only explicit Start new context, harness Switch, or task Drop
   may advance/end the epoch. Switch advances the epoch because the old
   conversation is discarded, not because it is restored.
4. `unavailable` is an attachable, visible, non-promptable state. Retry keeps
   the same stored session id. A failed retry does not mutate durable identity.
5. The host must durably store a new session id before accepting its first
   prompt. Persistence failure tears down the staged client and fails closed.
6. Idle eviction may detach a restorable ACP child but must not close its ACP
   session. A non-restorable live child is not silently evicted.
7. The browser may persist an unacknowledged outbox, but the host prompt ledger
   remains FIFO and dedupe authority.
8. A transcript may not visually imply continuity that the host cannot prove.

## Current root causes

- `sdk_connection::initialize_session` tries resume/load and then calls
  `session/new` when both fail.
- `SpawnReport.resumed` is converted into a transcript note instead of durable
  session state exposed in `SessionSnapshot`.
- `SessionSnapshot` has no context-continuity field, so the browser cannot gate
  prompts or explain whether the agent remembers prior turns.
- idle LRU assumes detach/load is safe based on intent, not a proven durable
  restore contract.
- `save_meta` and transcript append operations swallow persistence errors.
- the browser outbox uses `sessionStorage`, which does not survive a closed tab.

## Approval and execution

- Approval status: **approved** (2026-08-27). User requested
  `Delegate until finished` for this plan; do not pause between tasks for
  confirmation.
- Implementation routing: Ajax `model-router` per bounded task, one `EXECUTION`
  decision each, dispatch through `scripts/run-delegate`; no native subagents.
- Execution follows the workspace TDD gate (failing test, then implementation)
  without waiting for per-task user confirmation.
- Deviation (2026-08-27): operator said they do not care about context if we
  switch harnesses. Remaining tasks must not add Switch restore, Retry, or
  unavailable UX. T10 may keep Switch on the persist-then-install path only as
  a fresh-identity install for the new harness.

## Task checklist

### T1 — Add the Rust continuity snapshot contract

- [x] Test: require `contextState`, `contextEpoch`, and optional `contextError`
      in Rust snapshot serialization.
- [x] Code: add the host enum/fields and snapshot constructor inputs only.
- [x] Verify: focused `protocol_tests` red, then green.

### T2 — Parse the continuity contract in TypeScript

- [x] Test: accept all valid states and reject missing/invalid continuity fields.
- [x] Code: extend `SessionSnapshot` and the existing frame parser.
- [x] Verify: focused transport/parser tests red, then green.

### T3 — Version stored context metadata

- [x] Test: old JSONL metadata loads with epoch zero; new metadata round-trips
      session id plus epoch.
- [x] Code: add the backward-compatible disk field and in-memory value.
- [x] Verify: focused `web_session_store` metadata tests red, then green.

### T4 — Return metadata persistence failures

- [x] Test: forced `save_meta` and identity-clear failures return errors without
      claiming success.
- [x] Code: make those store operations return `Result` and update direct callers
      without changing behavior yet.
- [x] Verify: store failure-injection tests red, then green.

### T5 — Type ACP create versus restore outcomes

- [x] Test: initial attach reports Created; successful resume and load fallback
      report Restored with the same session id.
- [x] Code: replace the ambiguous `resumed` boolean with a small typed outcome.
- [x] Verify: focused ACP fake-client tests red, then green.

### T6 — Remove silent restore-to-new fallback

- [x] Test: stored id plus failed resume/load, missing capability, or timeout
      never sends `session/new` and returns RestoreUnavailable.
- [x] Code: split existing-session restore from initial-session creation.
- [x] Verify: handshake tests inspect fake-agent method calls red, then green.

### T7 — Install new context only after identity persistence

- [x] Test: force metadata failure after `session/new`; no client installs, no
      prompt dispatches, and the previous identity remains unchanged.
- [x] Code: stage the client, persist identity/epoch, then install it.
- [x] Verify: focused task-session reliability tests red, then green.

### T8 — Represent restore failure as attachable host state

- [x] Test: failed restore retains transcript/id, snapshots `unavailable`, and
      rejects prompts without killing the task actor.
- [x] Code: add continuity state to `TaskSessionState` and snapshot projection.
- [x] Verify: actor/outbound tests red, then green.

### T9 — Add Retry restore through the backend

- [x] Test: retry uses the same stored id; failure preserves it; success changes
      state to restored without advancing the epoch.
- [x] Code: add one task-session command, directory method, and WebSocket command.
- [x] Verify: actor, directory, and bridge tests red, then green.

### T10 — Add explicit Start new context

- [x] Test: explicit reset advances epoch once after successful persistence;
      failure retains the old id/epoch and remains unavailable.
- [x] Code: add the typed command and reuse the transaction for harness Switch.
- [x] Verify: replacement and switch tests red, then green.

### T11 — Make idle eviction continuity-aware

- [x] Test: durable idle sessions detach without `session/close`; a live session
      lacking proven restore readiness is not evicted.
- [x] Code: add restore readiness to the existing eviction snapshot predicate.
- [x] Verify: idle-eviction and session-close tests red, then green.

### T12 — Prove viewer disconnect does not own context lifetime

- [x] Test: release/reacquire after browser disconnect uses the same live child,
      session id, and epoch while queued work continues.
- [x] Code: adjust holder/release behavior only if the failing test identifies a
      coupling; otherwise retain production code.
- [x] Verify: task-session and WebSocket reconnect tests red, then green.

### T13 — Project continuity into browser session state

- [x] Test: reducer/connection state preserves transcript while projecting live,
      restored, and unavailable snapshots; unavailable does not reconnect-loop.
- [x] Code: thread the snapshot fields through existing session view state.
- [x] Verify: focused reducer and `useSessionConnection` tests red, then green.

### T14 — Gate the composer and expose recovery actions

- [x] Test: unavailable disables Send and shows Retry plus confirmed Start new;
      live/restored leaves the normal composer unchanged.
- [x] Code: reuse existing notice, button, and confirmation components.
- [x] Verify: composer and ChatSurface tests red, then green.

### T15 — Preserve the unacknowledged outbox across tab closure

- [x] Test: a fresh transport reloads the same prompt/id, acknowledgement clears
      it, and host ledger dedupe executes it once.
- [x] Code: move only the bounded outbox from `sessionStorage` to `localStorage`.
- [x] Verify: transport/outbox and duplicate-prompt tests red, then green.

### T16 — Return transcript append failures

- [x] Test: forced append failure returns an error and never reports a durable
      append.
- [x] Code: make the store append operation fallible; preserve compaction rules.
- [x] Verify: focused store tests red, then green.

### T17 — Block prompts during transcript durability failure

- [x] Test: an unpersisted user event prevents acknowledgement/ACP dispatch;
      mid-turn append failure becomes visible and blocks the next prompt.
- [x] Code: track the minimal actor durability fault/pending append state and
      include it in the snapshot.
- [x] Verify: actor ordering and snapshot tests red, then green.

### T18 — Prove socket leave/return end to end

- [x] Test: deterministic fake ACP remembers a value across WebSocket close,
      browser reconnect, and page-style cold replay.
- [x] Code: add only deterministic fixture observation needed by the test
      (`--remember-context` / `.fake-acp-context-memory` in `fake_acp.js`).
- [x] Verify: runtime WebSocket integration test red, then green.

### T19 — Prove detach/restart/child replacement end to end

- [x] Test: the same fake context survives idle detach, directory recreation,
      and child replacement; forced restoration failure blocks instead of
      creating fresh context.
- [x] Code: adjust only recovery orchestration exposed by the failing cases.
- [x] Verify: runtime/task-session integration tests red, then green.

### T20 — Gate harness admission on durable restore capability

- [x] Test: each Ajax launch mapping reports whether durable restore is required;
      unsupported mappings do not advertise resumable Ajax Chat.
- [x] Code: route unsupported harnesses to Terminal or a typed unsupported state.
- [x] Verify: launch/admission tests red, then green.

### T21 — Live-smoke Cursor restore

- [x] Test: create, complete a turn, replace the Cursor ACP process, restore the
      same session, and confirm continuity evidence.
- [x] Code: none unless the smoke exposes an Ajax defect.
- [x] Verify: record exact command/result; stop on external Cursor failure.

**2026-08-27 evidence (external failure — Ajax admission unchanged):**

```bash
AJAX_ACP_SMOKE=1 cargo test -p ajax-web live_cursor_prompt_and_session_load -- --nocapture
```

Result: **FAIL** after spawn+prompt succeeded; respawn reported ACP restore
unavailable (`session/resume` and `session/load` both failed). Ajax
`supports_durable_restore` remains `true` for Cursor; live restore is not
reliable on this host. Do not flip admission on external bridge failure.

### T22 — Live-smoke Codex bridge restore

- [x] Test: repeat the same session-identity replacement smoke for Codex ACP.
- [x] Code: none in Ajax unless the adapter mapping is wrong.
- [x] Verify: record command/result; stop for external bridge scope if unsupported.

**2026-08-27 evidence:**

```bash
AJAX_ACP_SMOKE=1 cargo test -p ajax-web live_codex_prompt_and_session_load -- --nocapture
```

Result: **PASS** (~8.5s). Respawn reported `SpawnOutcome::Restored` with the
same session id after one completed prompt turn.

### T23 — Live-smoke Claude bridge restore

- [x] Test: repeat the same session-identity replacement smoke for Claude ACP.
- [x] Code: none in Ajax unless the adapter mapping is wrong.
- [x] Verify: record command/result; stop for external bridge scope if unsupported.

**2026-08-27 evidence (external failure — Ajax admission unchanged):**

```bash
AJAX_ACP_SMOKE=1 cargo test -p ajax-web live_claude_prompt_and_session_load -- --nocapture
```

Result: **FAIL** same restore unavailable as Cursor (`session/resume` and
`session/load` failed). Ajax `supports_durable_restore` remains `true` for
Claude.

### T24 — Live-smoke Pi bridge restore

- [x] Test: repeat the same session-identity replacement smoke for Pi ACP.
- [x] Code: none in Ajax unless the adapter mapping is wrong.
- [x] Verify: record command/result; stop for external bridge scope if unsupported.

**2026-08-27 evidence (external failure — Ajax admission unchanged):**

```bash
AJAX_ACP_SMOKE=1 cargo test -p ajax-web live_pi_prompt_and_session_load -- --nocapture
```

Result: **FAIL** same restore unavailable as Cursor/Claude. Ajax
`supports_durable_restore` remains `true` for Pi.

### T25 — Lock architecture and run the regression gate

- [x] Test: architecture checks reject silent existing-id-to-`session/new`
      fallback and require continuity fields in Rust and TypeScript contracts.
- [x] Code: finalize the three owning architecture documents and remove language
      that treats visible history as model continuity.
- [x] Verify: formatting, lint, focused suites, full Ajax Web slice, architecture
      gate, `git diff --check`, and delta review.

**2026-08-27 T25 validation:**

| Command | Result |
|---------|--------|
| `rtk cargo fmt --check` | **fail** — worktree-wide diff (pre-existing); T25-touched Rust files formatted locally |
| `rtk cargo test -p ajax-web web_session` | **pass** — 395 passed (fixed `cursor_spawn_recovers_after_resume_composer_fast_issue_979`: persist known session id via `FAKE_ACP_STATE_DIR`, assert pin recovery respawns with `session/new`) |
| `rtk npm run web:test -- --run` | **pass** — 1475 passed, 9 skipped |
| `rtk npm run web:lint` | **pass** |
| `rtk npm run verify:slice -- web` | **pass** — 696 passed (was blocked by issue_979 test) |
| `rtk npm run verify:arch` | **pass** — 25 ajax-web architecture tests (3 new continuity guards) |
| `rtk git diff --check` | **pass** |

**2026-08-27 T25 regression gate (test fix):**

| Command | Result |
|---------|--------|
| `rtk cargo test -p ajax-web cursor_spawn_recovers_after_resume_composer_fast_issue_979 -- --test-threads=1` | **pass** |
| `rtk cargo test -p ajax-web web_session -- --test-threads=1` | **pass** — 395 passed |
| `rtk npm run verify:slice -- web` | **pass** — 696 passed |

## Validation commands

Run focused red/green commands per task, then the final gate:

```bash
rtk cargo fmt --check
rtk cargo test -p ajax-web web_session
rtk npm run web:test -- --run
rtk npm run web:lint
rtk npm run verify:slice -- web
rtk npm run verify:arch
rtk git diff --check
rtk git status --short
```

Live harness smoke checks are evidence in addition to, not replacements for,
deterministic tests. Record exact commands and results here during execution.

### Live ACP restore smoke (2026-08-27)

Run only when `AJAX_ACP_SMOKE=1` and the harness binary is on PATH. These are
external-bridge evidence checks; failure does not change Ajax Chat admission.

| Harness | Command | Result |
|---------|---------|--------|
| Cursor | `AJAX_ACP_SMOKE=1 cargo test -p ajax-web live_cursor_prompt_and_session_load -- --nocapture` | FAIL — restore unavailable after respawn |
| Codex | `AJAX_ACP_SMOKE=1 cargo test -p ajax-web live_codex_prompt_and_session_load -- --nocapture` | PASS (~8.5s, `SpawnOutcome::Restored`) |
| Claude | `AJAX_ACP_SMOKE=1 cargo test -p ajax-web live_claude_prompt_and_session_load -- --nocapture` | FAIL — restore unavailable |
| Pi | `AJAX_ACP_SMOKE=1 cargo test -p ajax-web live_pi_prompt_and_session_load -- --nocapture` | FAIL — restore unavailable |

All four harnesses retain `supports_durable_restore: true` in
`crates/ajax-core/src/adapters/agent.rs`. Cursor/Claude/Pi live restore is not
reliable on this host; Codex bridge restore passed once. Ajax fail-closes on
restore failure — do not fake restore or drop Chat admission.

## Current validation

- T21–T24 live ACP restore smoke recorded 2026-08-27 (see task checklist and
  validation table). Cursor/Claude/Pi failed external restore; Codex passed.
  Admission flags unchanged (`supports_durable_restore: true` for all four).
- The prior baseline on this worktree passed 650 Ajax Web Rust tests, 1,450
  browser tests with 9 skipped, browser ESLint, and 34 architecture tests.

## Risks and stop conditions

- Public behavior changes: an existing session that cannot restore will block
  instead of silently starting fresh. This is intentional and requires approval.
- If any supported bridge does not implement durable resume/load, stop before
  claiming it is reliable; either update that bridge in an explicitly approved
  scope or remove its durable-chat admission.
- If exact continuity requires changing an external bridge repository/package,
  stop and request scope for that repository rather than faking restoration in
  Ajax transcript replay.
- Do not change task lifecycle truth, cross-harness context-reset semantics,
  authentication, or public network exposure beyond this approved plan.

## Deviations and changed assumptions

- 2026-08-27: operator does not care about context across harness Switch.
  Switch remains a new-context boundary; remaining tasks must not add Switch
  restore, Retry, or unavailable UX.
- 2026-08-27 review fix: `install_replaced_client` persist failure now calls
  `enter_restore_unavailable` (same epoch, stored id unchanged) instead of
  leaving Live/Restored with no client. Browser `applySnapshot` projects
  `transcriptError` into session view and gates Send independently of
  `contextState === "unavailable"`.
- 2026-08-27 second-review fix: `install_replaced_client` and
  `finish_first_acquire` persist failures return `Ok` after
  `enter_restore_unavailable` so `acquire` stays attachable (matches restore
  spawn-unavailable path). WS bridge receives the unavailable snapshot instead
  of a generic error close.
- 2026-08-27 continuity review 3 (P2): `pump()` ignored `append_to_log` errors
  without scheduling an outbound snapshot. `append_to_log` now sets
  `pending_transcript_error_snapshot` on failure; `collect_outbound` emits
  `transcriptError` on the next flush without a generation bump.
- 2026-08-27 P1 fix: `dispatch_queued_prompt` returned `Ok(())` when
  `transcript_durability_fault` was set, so `try_dispatch_next_if_idle`
  popped acknowledged queued prompts without ACP dispatch. It now returns
  `Err` (matching the persist-failure path); regression test
  `queued_prompt_kept_when_transcript_durability_fault_blocks_dispatch`.
- 2026-08-27 P1 fix: replace without resume id (model change / close session)
  passed `attachable_on_persist_failure=false` to `install_replaced_client`, so
  save_meta failure after handshake returned `Err` and aborted WS acquire.
  Removed the flag; all replace persist failures now return `Ok` after
  `enter_restore_unavailable` (same as resume-id replace and
  `finish_first_acquire`). `start_new_context` / `install_new_context_client`
  remain fail-closed.
- "Leave and come back" still includes browser/tab lifecycle, `ajax-web`
  restart, and ACP child replacement on the same harness.

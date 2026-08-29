# System debt: run-state authorship, model pin, session runtime

## Status

**Wave 0: complete (2026-08-28).** Characterization tests and writer inventory
landed; Wave 1 failing tests are active (not ignored).

**Approval: pending** for Waves 1+ (architecture change). Operator requested
immediate implementation 2026-08-28 (Delegate until finished); Wave 0 landed
under that request.

**Kind:** multi-wave, multi-PR program. Not one PR. Not a rewrite.

**Source:** GitNexus whole-codebase assessment on worktree `ajax/gitnexus`
(`5cba9a5e`), 838 files / 12,746 nodes / 7 import cycles, plus open defects
`#1096` `#925` `#1010` `#1013` `#1038` `#1095` `#1064` `#1040`.

## Problem

Ajax already *documents* one operator projector and one agent reducer. The
implementation accumulated parallel writers, parallel model encoders, and a
session god-object from successive AI tasks. Defects keep coming back as
“Running vs NotStarted”, “picker vs child model”, and “tap missed the button”
because ownership is ambiguous.

Docs that already state the intended end state (code does not fully honor them):

- `docs/architecture/core-subsystems.md` — `agent_status` is the single agent
  reducer; `ui_state::derive_operator_status` is the single operator projector.
- `docs/architecture/task-authority.md` — hooks/wrapper feed facts, not display
  statuses; Git/tmux/process remain substrate authority; SQLite is staleable.
- `architecture.md` — Core owns task truth; browser is not a registry; ACP
  session lives in `web_session` + adapters, not in the browser.
- `docs/architecture/web-cockpit.md` — workspace composition owns chrome;
  Chat/Terminal do not.

This program makes code match those documents, then peels session/chrome debt
that the documents do not yet name.

## Target architecture (one page)

```text
Evidence sources (facts, not display)
  supervisor pane / process exit
  canonical agent-event JSONL (hooks + wrapper)
  ACP host SessionActivity (provisioned chat only)
  GitHub checks / git / tmux probes
        │
        ▼
ajax-core run-state owner
  reduce_agent_status: observations → LiveObservation (the only agent reducer)
  apply_reduced_observation: writes LiveObservation onto Task (not a reducer)
  derive_operator_status: Task + lifecycle + CI → operator status (the only projector)
  AgentAttempt: launch-episode history (open until the launch ends, not until every turn ends)
        │
        ▼
Surfaces (CLI JSON, TUI, Web Cockpit DTOs)
  render TaskStatus + explanation; render attempts as history
  do not invent a fifth status

ACP model pin
  core CursorModelIntent (desired)
  adapters::web_session_acp apply_model (one wire apply)
  slice + browser send the pin; they do not encode option ids

ACP session
  web_session: sequencing + command loop (dispatcher)
  web_session_acp: stdio
  web_session_store: JSONL + prompt ledger
  typed session errors at the slice boundary
  TaskSessionState split by owner after errors exist
```

Invariant preserved: operator status is exactly `Running | Waiting | Idle |
Error` (+ optional explanation). `Unknown` remains the honest “no evidence”
case in `derive_task_status`. Lifecycle stays workflow authority.

## Scope

Waves 0–8 below. Each wave is one or more PRs with its own GitHub issue (new
or existing), regression tests, and architecture-doc updates in the **same**
change when behavior or ownership moves.

## Non-goals

- No new crate. No new status vocabulary. No browser registry.
- No rewrite of `web_session` file tree before Wave 5–6.
- No rewrite of `derive_task_status` precedence until Wave 2–3 characterization
  tests exist.
- No TUI product redesign. Peeling `ajax-tui/src/lib/tests.rs` is optional
  hygiene, not this program.
- No SQLite schema version bump unless Wave 1 proves attempts cannot close
  without a column (expected: no migration; `finished_at` already exists).
- No change to Git/tmux/process authority.
- No public-internet / auth-model change (TLS/cookie audit is a gap, not this
  program).
- Do not “fix” import cycles (`api.ts`/`types.ts`, Chat barrels,
  `cockpit_actions`/`cockpit_backend`) except as listed in Wave 8.
- Do not land features into files already over the 1000-line hard max
  (`TaskTerminal.tsx`, `mountTaskTerminalSession.ts`).

## Related plans (do not restart)

| Plan | Relationship |
| --- | --- |
| `.planning/agent-plans/agent-status-conservative.md` | **Finish in Wave 3.** Keep `reduce_agent_status`. Delete unused `ObservationSource::ProcessLiveness`. |
| `.planning/agent-plans/status-detection-rethink.md` | Done for pane demotion. Do not reintroduce keyword tables. |
| `.planning/agent-plans/session-model-authority.md` | Done for snapshot applied-model. Wave 4 extends it: one apply path for live pin (`#1010` `#1013`). |
| `.planning/agent-plans/web-architecture-alignment.md` | Macro web contracts. This program does not reopen control-lane design. |
| `.planning/agent-plans/task-terminal-file-loc-peel.md` | Terminal LOC peel. Wave 7 chrome may touch expand-corner; coordinate, do not duplicate. |

## Human decisions (resolved 2026-08-28)

1. **Keep and finish `reduce_agent_status`.** Do not delete it. The binary
   “reducer vs `apply_reduced_observation` + `derive_operator_status`” was a
   category error: those three are layers, not alternatives.
   - `reduce_agent_status` owns observation freshness, source rank, and the
     parent/child run graph (`WaitingOnDelegated`,
     `CompletedLocallyChildrenActive`). Runtime refresh already calls it;
     `agent_status_cache` exists to feed it. Deleting it dumps that graph into
     `live.rs` or loses delegated-run correctness.
   - `apply_reduced_observation` is the **writer** of `agent_status` / flags /
     `live_status`.
   - `derive_operator_status` is the **projector** over Task + lifecycle + CI.
   - ACP `SessionActivity` may map four host facts onto `LiveObservation` and
     call `apply_authoritative_observation_at`. That is an evidence mapper, not
     a second reducer. Wave 3 may later insert those as
     `ObservationSource::ProviderLifecycle` rows; not required for Wave 1.
2. **Delete `TaskSessionCommand::ApplyModel`.** Chat live picker already sends
   `set_config_option` (`ChatSurface` → `applyConfigOption`). Keep
   `apply_model_pin` as the one catalog-pin → advertised `ConfigApplyStep`s
   function (spawn + in-band). Move dead-child **respawn** out of
   `task_session_spawn::apply_model` into spawn / `apply_config_option` (today
   only `apply_model` respawns when `host_exited`; `apply_config_option`
   errors). WS `set_model` / `SessionClientMessage::SetModel` die with the
   command; migrate those tests onto `SetConfigOption`.
3. **Attempts are launch-episode history, not current-run and not turn log.**
   Keep `Vec<AgentAttempt>` / `registry_agent_attempts` (sequence already
   exists; Details already lists multiple rows). Append on actual launch
   (`AgentCommandSent`; later resume only if it re-sends keys). Close
   (`finished_at` + status) only when the **launch** is over: `NotStarted`
   (never started / spawn-auth fail), `Dead`, Drop. Do **not** close on
   Waiting, Blocked, or ACP `TurnEnded` → `Done` (“Response ready” is idle
   between turns of the same launch). Do **not** append on `TurnStarted`. Drop
   today sets status Dead but leaves `finished_at` None — Wave 1 must set it.
   Do not add a second current-run field.

## Approval

- [x] Operator answers human decisions 1–3 (chat 2026-08-28)
- [x] Operator requested immediate implementation (2026-08-28: Delegate until finished)
- [ ] Operator approves this program for implementation (architecture change gate; Wave 0 landed under immediate-implement request)

## Implementation waves

### Wave 0 — Characterization (no behavior change)

Lock today’s contradictions so later waves cannot “fix” them by weakening tests.

**Files (read/test only plus new tests):**

- `crates/ajax-core/src/models/observations.rs` (`AgentAttempt::new`)
- `crates/ajax-core/src/commands/new_task.rs` (`StartProvisioningStep::AgentCommandSent`)
- `crates/ajax-core/src/live_application.rs` (`apply_reduced_observation`)
- `crates/ajax-core/src/ui_state.rs` (`derive_task_status`)
- `crates/ajax-core/src/agent_status.rs` (`reduce_agent_status`)
- `crates/ajax-web/src/slices/web_session/session_activity.rs`
- `crates/ajax-web/src/slices/cockpit/mod.rs` (`browser_task_detail_view`)
- `crates/ajax-web/web/src/features/task/TaskMetaDetails.tsx`

**Tasks:**

- [x] Inventory every production writer of `Task.agent_status`, `Task.live_status`,
      and `Task.agent_attempts` (table in this plan appendix, keep updated).
- [x] Add a failing-on-purpose **composition** fixture documenting `#1096`:
      provisioned ACP task, `AgentCommandSent`, spawn/auth never starts a turn
      → `agent_status == NotStarted`, Attempts still `Running` + `finished_at
      None`. Landed as Wave 1 failing tests (not `#[ignore]` / `#[should_panic]`).
- [x] Golden table: same `Task` fixture through `reduce_agent_status` vs
      materialized Task fields (apply layer) vs `derive_operator_status` —
      record disagreements, do not “fix” yet
      (`crates/ajax-core/tests/wave0_run_state_characterization.rs`).
- [x] List `Result<_, String>` and `Result<(), ()>` at session/hook boundaries
      (see appendix).

**Verify:** `cargo nextest run -p ajax-core -p ajax-web` for new tests only.
No production edits.

**Stop:** if inventory finds a writer this plan missed, extend Wave 1 scope
before coding Wave 1.

---

### Wave 1 — Close `AgentAttempt` with run-state (correctness, `#1096` `#925`)

**Critical.** Smallest change that removes a second source of truth.

**Owner:** Core owns open/close. Attempts follow **launch-episode** end, not
every `agent_status` change. Web cockpit only renders.

**Production files (expected):**

- `crates/ajax-core/src/models/observations.rs` — `AgentAttempt::close` /
  `sync_open_attempts` next to `new`
- `crates/ajax-core/src/models/task.rs` — `mark_resource_missing` must close
  open attempts when forcing `agent_status` → `Dead` (inventory found this
  writer outside the original Wave 1 list)
- `crates/ajax-core/src/live_application.rs` — after `agent_status` assignment
  in `apply_reduced_observation`, sync attempts
- `crates/ajax-core/src/commands/new_task.rs` — still *open* an attempt on
  `AgentCommandSent`; do not skip ACP (history of launch is valid)
- `crates/ajax-core/src/commands/teardown/drop_observation.rs` — keep Dead on
  drop; should become a call to the shared closer
- `crates/ajax-core/src/runtime_refresh.rs` — `clear_stale_agent_running`
  must close attempts when flipping Running → Unknown/NotStarted
- Tests: `crates/ajax-core/src/commands/new_task/tests.rs`,
  `crates/ajax-web/src/slices/cockpit/tests.rs`,
  `crates/ajax-web/web/src/features/task/TaskMetaDetails.test.tsx` if copy
  depends on “in progress”

**Do not:** retune `derive_task_status` precedence; do not teach the browser to
hide Attempts; do not close the open row on `TurnEnded` / Waiting / Blocked.

**Tasks:**

- [x] `AgentAttempt::close` / `sync_open_attempts` next to `new`
- [x] `apply_reduced_observation` syncs attempts after `agent_status` assignment
- [x] `mark_resource_missing` closes open attempts on `Dead`
- [x] `mark_drop_agent_stopped` uses shared closer + `finished_at`
- [x] `clear_stale_agent_running`: no close on `Unknown`; close on `NotStarted`/`Dead` via sync when applicable
- [x] Wave 1 `#1096` tests + golden-table fixture updated
- [x] `docs/architecture/core-subsystems.md` Live Status — launch-episode attempts

**Acceptance:**

- ACP spawn/auth fail (no `TurnStarted`): Runtime `NotStarted`, Attempts not
  `Running` / “in progress” (`finished_at` set).
- ACP `PromptAccepted` / `SessionActivity::TurnStarted`: live `AgentRunning`,
  `agent_status Running`, open attempt still Running.
- ACP `TurnEnded` (`Done` / “Response ready”): open attempt stays open.
- Drop marks open attempts Dead **and** sets `finished_at`.
- Interactive tmux `AgentCommandSent` still opens a Running attempt and still
  sets `SideFlag::AgentRunning` (existing `#1069` skip only for provisioned).

**Docs in the same PR:** `docs/architecture/core-subsystems.md` Live Status —
attempts are launch-episode history, not an independent run-state writer.
Link `#1096`.

**Verify:**

- `cargo nextest run -p ajax-core -p ajax-web` (new_task, live_application,
  cockpit, session_activity)
- `npm run verify:arch` if docs-only architecture tests do not cover this;
  otherwise skip
- Open/close GitHub `#1096` `#925` when the composition test would have failed
  on `main`

**Stop:** if closing attempts requires a SQLite migration, stop and update this
plan (column already exists: `finished_at_*`, `status`).

---

### Wave 2 — One live-apply contract (ordinary / authoritative / trusted)

**Owner:** `crates/ajax-core/src/live.rs` + `live_application.rs`.

Today three functions do almost the same write:

- `apply_observation*` — runs `reduce_live_observation` then apply
- `apply_authoritative_observation*` — skip reduce (ACP host) then apply
- `apply_trusted_observation*` — apply + lifecycle on Done/Running

ACP `record_session_activity` uses authoritative (`session_activity.rs:79`).
Trusted may mark `Reviewable` on `Done` — ACP **must not** use trusted
(`TurnEnded` → `Done` would otherwise advance lifecycle). Keep that split.

**Tasks:**

- [x] Document the three apply modes in `core-subsystems.md` with the ACP
      prohibition on trusted.
- [x] Collapse duplicated timestamp wrappers if they are pure aliases; keep
      three *meanings*.
- [x] `apply_reduced_observation` remains the only place that writes
      `agent_status` + side flags + (after Wave 1) attempts.
- [x] Ban new `task.agent_status =` outside `live_application` and sqlite
      load (architecture test or grep test).
- [x] `session_activity` stays an evidence source: it only calls
      `apply_authoritative_observation_at`.

**Acceptance:** no new `agent_status` assignment in web/cli/supervisor
production code. Architecture test lists allowed files.

**Verify:** `cargo nextest run -p ajax-core live ui_state session_activity`
plus the new architecture grep test.

---

### Wave 3 — Finish `reduce_agent_status` (decision 1: keep)

**Status: complete (2026-08-28).**

Unblocked. This is not a second reducer to delete.

`core-subsystems.md` already claims `agent_status` is the single agent reducer
and `derive_operator_status` the single operator projector. Runtime refresh
already reduces then applies (`runtime_refresh.rs:356` → `projection.live` →
`apply_authoritative` / `apply_trusted` / `apply_observation`). Finish that
stack; do not invent a fourth path.

**Tasks:**

- [x] Delete unused `ObservationSource::ProcessLiveness` (comment already says
      constructed nowhere; liveness is `ReduceInput.process_liveness`).
- [x] `clear_stale_agent_running` writes `agent_status` only via
      `live_application` (`retract_stale_agent_running_at`).
- [x] Admit `agent_status`, `ui_state`, and `attention` in `KERNEL_MODULES` so
      kernel tests match the docs (`architecture.rs`; all three pass
      `shared_kernel_does_not_depend_on_commands_or_task_operations`).
- [x] Retire `.planning/agent-plans/agent-status-conservative.md` as complete
      or replace leftover packets with a pointer here.
- [x] Optional later: ACP `SessionActivity` as `ProviderLifecycle`
      observations into the same reducer. Not required to land Wave 3.

**Do not:** fold `reduce_agent_status` into `derive_operator_status`. Do not
route ACP `TurnEnded` through `apply_trusted_observation` (would mark
`Reviewable`).

**Acceptance:** one observation→live function (`reduce_agent_status`); one
writer (`apply_reduced_observation`); one projector (`derive_operator_status`);
docs and architecture tests name those three symbols.

**Verify:** `cargo nextest run -p ajax-core -p ajax-cli` (refresh +
agent_status_cache + ui_state). Attention notify tests must stay green
(`attention/tests.rs`).

---

### Wave 4 — One model-pin apply path (`#1010` `#1013`)

**Owner:** `crates/ajax-web/src/adapters/web_session_acp/apply_model/mod.rs`
as the only wire apply. Core `CursorModelIntent` remains the desired-state
type (`adapters/agent.rs`).

**Today’s parallel implementations (must become callers):**

| Layer | Symbol | File |
| --- | --- | --- |
| Core desired | `parse_cursor_model_intent`, `cursor_catalog_to_acp_in_band_token` | `crates/ajax-core/src/adapters/agent.rs` |
| Wire helpers | `find_option_by_category`, reasoning vs effort | `config_options.rs` |
| Wire apply | `apply_model_pin`, `apply_config_option`, `set_config_option` | `apply_model/mod.rs` |
| RPC | `AcpStdioClient.apply_model_pin` / `apply_config_option` | `client.rs` |
| Slice | `task_session_spawn::apply_model` **and** `apply_config_option` | `task_session_spawn.rs` |
| Browser | `encodeCursorSelection` | `desiredModel.ts` |
| Catalog | `SessionModelsResponse` | `session_models.rs` |

**Tasks:**

- [x] One function: desired pin → advertised `ConfigApplyStep`s (model +
      effort + fast). Cursor `reasoning` vs `effort` lives **only** here
      (`#1010`).
- [x] Delete `TaskSessionCommand::ApplyModel`, `TaskSessionDirectory::apply_model`,
      `task_session_spawn::apply_model`, and WS `set_model` /
      `SessionClientMessage::SetModel`. One remaining command:
      `ApplyConfigOption`.
- [x] `apply_model_pin` stays as spawn/handshake catalog mapping; live apply
      is `apply_config_option` only. Move `host_exited` respawn from the
      deleted `apply_model` into spawn (or into `apply_config_option` if a
      live pin must revive a dead child).
- [x] Browser: drop unused `useChatSession.applyModel`; Chat already uses
      `applyConfigOption`. Do not invent catalog ids as
      `set_config_option` values (`#1013`).
- [x] `session_models.rs` remains catalog **display** cache, not apply.
- [x] Keep snapshot applied-model authority from
      `session-model-authority.md` (`#952` `#954`).
- [x] Migrate `ws_bridge_tests` `SetModel` cases (`#942` `#962` `#979`
      `#989`) onto `SetConfigOption`.

**Do not:** grow `config_options.rs` (834). Peel tests first if touching it
(`#[cfg(test)]` sibling) per file-size policy.

**Docs:** `web-session-behavior.md` + `web-cockpit.md` — one apply path;
Task owns catalog picker; Chat owns sending the pin.

**Verify:**

- `cargo nextest run -p ajax-web` (`client_spawn_model_tests`,
  `apply_model_tests`, `ws_bridge_tests` set_config_option persist)
- `npm run web:test -- --run desiredModel ModelPicker`
- Close `#1010` `#1013` when the shared matrix covers refuse-reasoning and
  catalog-id-not-sent-in-band

**Stop:** if a harness needs a second apply protocol besides
`session/set_config_option`, amend this plan; do not add `apply_model_2`.

---

### Wave 5 — Session errors and activity-report (reliability)

**Owner:** `ajax-web::slices::web_session`. Adapters stay stdio/JSONL.

**Tasks:**

- [x] Replace `Result<T, String>` on directory/spawn/answers with a
      `SessionError` enum: `Spawn`, `Persist`, `Protocol`, `Operator`,
      `RestoreUnavailable` (names can match existing
      `is_restore_unavailable`).
- [x] `try_report_activity` (`task_session.rs:321–337`): stop
      `thread::sleep` on the session loop. Bounded retries with a typed
      persist error; never a silent `bool`. Failed report must be visible
      (pending snapshot / transcriptError already exists — reuse).
- [x] `ajax-cli/src/agent_event.rs`: `run_agent_event` must not be
      `Result<(), ()>` with `let _ =` in `run_agent_event_command`. Missing
      identity may still be a no-op **by type** (`NoIdentity`), not success
      swallowing IO failure.
- [x] Duplicate ACP auth errors (`#1040`): errors need identity (request or
      generation), not string-append on reconnect.

**Acceptance:** spawn vs persist vs protocol distinguishable in tests; activity
report failure cannot look like `TurnStarted` on the registry.

**Verify:** `cargo nextest run -p ajax-web` (directory, spawn, activity,
`session_activity_directory_tests`); CLI agent_event tests.

**Docs:** `web-session-behavior.md` error taxonomy; `cli-supervisor.md` hook
write failures.

---

### Wave 6 — Session state peel (after Wave 5)

**Wave 6a: complete (2026-08-28).** Peel **state**, not another
`task_session_*.rs` that still mutates one struct.

**Target types inside `web_session` (still one command loop):**

- `AcpSlot` — client, model/applied_model, config options, commands,
  capabilities, `acp_alive`
- `PromptQueue` — ledger, `active_prompt`, queued prompts
- `SessionEvidence` — `SessionActivityReporter`, pending activity

`handle_command` (`task_session.rs:530`) stays the dispatcher.
`pump` (`:358`) stays the drain tick; it may take `&mut` to the three
owners.

**Tasks (Wave 6a):**

- [x] Introduce `AcpSlot`, `PromptQueue`, and `SessionEvidence` as field owners
      on `TaskSessionState` (`acp_slot.rs`, `prompt_queue.rs`, `session_evidence.rs`).
- [x] Move `submit_prompt` / `cancel` field access through `PromptQueue`; keep
      dispatch in `task_session_exit.rs`.
- [x] Shrink `task_session.rs` (903 → 543 lines); no new `task_session_*.rs`
      god-object splits.

**Also (Wave 6b — complete):** renamed CLI `crates/ajax-cli/src/task_session/` to
`tmux_task_session/` (no homonym with ajax-web `web_session::task_session`).
Updated `docs/architecture/cli-supervisor.md`.

**Homonym is agent reasoning cost (rubric 5).** Do this even if the peel
slips.

**File-size:** `task_session.rs` 903 must not grow; submit_prompt/cancel
(`:675`, `:874`) move with `PromptQueue`.

**Verify:** existing session test suites stay green; no new public wire
fields.

**Stop:** if peel requires `ajax-web::runtime` imports from the slice,
abort (architecture.md forbids).

---

### Wave 7 — Workspace chrome hit-testing (`#1038` `#1095` `#1064`)

**Status: complete (2026-08-28).**

**Owner:** workspace composition (`features/task-workspace`), not Chat or
ACP.

**Packets (separate PRs if needed):**

1. **`#1038` Drop vs row** — `TaskList.tsx` `TaskRow`: reveal `ActionBar`
   must be a hit target; the full-row `<button>` (`:89–116`) must not cover
   it. `useSwipeReveal` stays.
2. **`#1095` Details vs expand-corner** — `layout.css` restores
   `pointer-events` on `.detail-header-controls` only;
   `.session-head-details` (`TaskWorkspaceHeader.tsx`) and
   `.terminal-expand-corner` (`TaskTerminal.tsx:770`) are not in one
   cluster. Put both in the restored controls cluster **or** give Details
   the same `pointer-events: auto` + z-index as copy-overlay
   (`layout.css:180` comment already admits the overlap). Replace CSS
   *text* snapshots in `TaskTerminalView.test.tsx:571–574` with “Details
   remains tappable when expanded”.
3. **`#1064` swipe-right leaves workspace** — cap right-commit in
   `useSwipePageTransition.ts` / `navigateSwipe.ts` when the parent is
   already dashboard. Chat keeps transcript-selection ignore
   (`ChatSurface.tsx:22–45`) only.

**Do not:** split `App.tsx` in this wave unless chrome extraction is required
for (1)–(3). `AppContent` size is a symptom.

**Verify:** `npm run web:test` for TaskList, TaskWorkspaceHeader,
TaskTerminalView, swipe tests; browser pass on iPhone-sized viewport if
tools available.

**Docs:** `web-cockpit.md` Task Workspace — pointer policy belongs to
composition.

**Tasks:**

- [x] `#1038` task-row reveal hit-testing (list.css + TaskList wrap close)
- [x] `#1095` Details z-index/pointer-events in layout.css; behavioral test
- [x] `#1064` cap right-commit at list route in navigateSwipe + hook
- [x] `web-cockpit.md` pointer policy paragraph
- [x] Wave 7 verification tests

---

### Wave 8 — Structural leftovers (optional, after 1–7)

Low score; do not interleave with correctness waves.

- [x] `ToolCard.tsx` stop importing `../public` (Chat barrel cycle).
- [x] Break Chat session barrel cycle among `connection/public.ts`,
      `useSessionConnection.ts`, `session/public.ts`, and `useChatSession.ts`
      (hook wiring uses leaf modules; barrels remain for external re-exports).
- [x] `types.ts` stop importing `ApiError` from `api.ts` (move type).
- [x] `build_cockpit_snapshot` out of `cockpit_backend.rs` so
      `cockpit_actions.rs` does not import backend.
- [x] `commands.rs` stop importing `output` types; use
      `commands/projection.rs` only. Break
      `commands` → `output` → `remediation` → `commands`.
- [x] Peel `#[cfg(test)]` from `crates/ajax-core/src/adapters.rs` (969 lines,
      production is ~26). Same for other over-600 files that are test-heavy.
- [x] `ProcessProtocol` out of `process_observer.rs` to break supervisor
      cycle with `agent/codex.rs`.
- [x] Include `process_protocol` in supervisor `architecture.rs` substrate
      runtime-independence list.
- [x] Duplicate routes `/api/actions` and `/api/operations` — document alias
      or delete one (`runtime/mod.rs:113–114`).
- [x] Break `events.rs` ↔ `models/mod.rs` import cycle: rename
      `models/events.rs` → `models/step_receipts.rs` (step-receipt types) so
      it does not homonym `crate::events` (monitor/process events); re-export
      unchanged via `models::`.

**Verify per PR:** `mcp_gitnexus_check` cycles drop; `npm run verify:arch`.

---

## Defect → wave map

| Issue | Wave |
| --- | --- |
| `#1096` `#925` Attempts Running vs Runtime NotStarted | 1 |
| `#1069` ACP “Agent working” (already shipped; do not regress) | 1–2 |
| `#1010` `#1013` Cursor config/catalog pin | 4 |
| `#952` `#954` applied-model snapshot | keep; 4 must not regress |
| `#1040` duplicate auth errors | 5 |
| `#1038` Drop unclickable | 7 |
| `#1095` expand-corner vs Details | 7 |
| `#1064` swipe-right leaves workspace | 7 |
| `#1092` `#1083` `#1079` session disappear/deadlock/restore | 5–6 (do not expand Wave 1) |

## Docs updated in-program

| Wave | Doc |
| --- | --- |
| 1 | `docs/architecture/core-subsystems.md` (attempts projection) |
| 2 | same (three apply modes) |
| 3 | `core-subsystems.md`, `architecture.md` kernel admission / `KERNEL_MODULES` |
| 4 | `web-session-behavior.md`, `web-cockpit.md` |
| 5 | `web-session-behavior.md`, `cli-supervisor.md` |
| 6 | `cli-supervisor.md` (rename), `architecture.md` session sentence if types rename |
| 7 | `web-cockpit.md` pointer policy |
| 8 | only if a public command/route disappears |

Root `architecture.md` is not a parking lot; lasting ownership sentences move
there only when the kernel/session contract actually changes (Wave 3, 6).

## Validation (program-level)

Per wave, run the listed crate tests. Before any PR that touches rust/web
boundaries:

```bash
npm run verify:arch
```

Before merge of Wave 1 or 4 (correctness):

```bash
cargo nextest run -p ajax-core -p ajax-web
npm run web:test -- --run TaskMetaDetails desiredModel
```

Full gate only when opening the PR, per `docs/agent/pull-requests.md`.

GitNexus after each wave (optional, same worktree):

- `gitnexus analyze`
- `check` cycles
- `impact` on `derive_operator_status`, `apply_reduced_observation`,
  `AcpStdioClient.spawn`

## Delegation

Each **wave** (or packet inside a wave) is one model-router `EXECUTION` after
approval. Orchestrator does not implement. Scope must name files. Stop
conditions are the Stop lines above.

## Risks

- Wave 1 closer too aggressive: closing on every leave-Running (especially
  ACP `TurnEnded` → `Done`) would turn Attempts into a chat-turn log.
  Mitigation: close only `NotStarted` / `Dead` / Drop; never Waiting,
  Blocked, or Done. Do not close on `Unknown` (preserve last launch row).
- Wave 4 deleting `ApplyModel` drops the only dead-child respawn on pin.
  Mitigation: respawn lives on spawn / `apply_config_option` before the
  command is removed; keep `#989` tests.
- Wave 3 fight between refresh and ACP authoritative apply. Mitigation:
  provisioned tasks keep ACP as authoritative; pane evidence remains refused
  (`session_activity.rs` interactive-task test).
- Wave 4 harness-specific option ids. Mitigation: matrix tests, not comments.
- Wave 6 rename of CLI `task_session` is a large mechanical diff. Do it as
  its own PR.

## Deviations / assumptions

- Decisions 1–3 recorded 2026-08-28; Wave 0 landed under immediate-implement request.
- Assumption: no SQLite migration for Wave 1.
- Assumption: `Unknown` operator status stays (honest no-evidence); not
  collapsed to Idle.
- Assumption: one `AgentAttempt` row per start/relaunch is enough; resume that
  does not re-send keys does not append.
- Uninspected areas from the assessment (policy, ghost_task, STT, push TLS
  cookies, operator slice bodies, supervisor cursor/repo_observer) stay out
  of scope unless a wave’s inventory proves they write `agent_status`.

## Appendix — known writers (Wave 0 complete 2026-08-28)

### `Task.agent_status` (production assignments)

| Location | Symbol / context | Notes |
| --- | --- | --- |
| `crates/ajax-core/src/live_application.rs` | `apply_reduced_observation` | Canonical writer via `apply_observation*` / `apply_authoritative*` / `apply_trusted*` |
| `crates/ajax-core/src/runtime_refresh.rs` | `clear_stale_agent_running` | Delegates to `live::retract_stale_agent_running_at` (Wave 3) |
| `crates/ajax-core/src/registry/sqlite/row_codec.rs` | `task_from_row` | Load from SQLite; normalizes legacy `Unknown` → `NotStarted` |
| `crates/ajax-core/src/models/task.rs` | `mark_resource_missing` | Sets `Dead` when a missing-substrate side flag is added — **Wave 1 extended**: must close open attempts |
| `crates/ajax-core/src/commands/teardown/drop_observation.rs` | `mark_drop_agent_stopped` | Sets `Dead` on drop (Wave 1: shared closer + `finished_at`) |

### `Task.live_status` (production assignments)

| Location | Symbol / context | Notes |
| --- | --- | --- |
| `crates/ajax-core/src/live_application.rs` | `apply_reduced_observation` | Canonical writer (same entry points as `agent_status`) |
| `crates/ajax-core/src/runtime_refresh/github_checks.rs` | `clear_github_ci_evidence`, unobservable CI path | Clears `CiPending` / GitHub-owned live rows |
| `crates/ajax-core/src/registry/sqlite/row_codec.rs` | `task_from_row` | Load + legacy normalization |
| `crates/ajax-core/src/models/task.rs` | `clear_live_status_if` | Conditional clear when kind matches |
| `crates/ajax-core/src/commands/task_window.rs` | post `update_task_window_status` recovery | Clears stale `TmuxMissing` / `TaskWindowMissing` live rows |
| `crates/ajax-core/src/commands/task_state.rs` | `mark_task_check_*`, `mark_task_merge_*` | Check/merge lifecycle live rows (`TestsRunning`, `CiFailed`, merge failures) |
| `crates/ajax-web/src/slices/web_session/session_activity.rs` | `record_session_activity` | Indirect via `live::apply_provider_lifecycle_observation_at` (ProviderLifecycle → reduce → authoritative apply) |

Non-canonical `live_status` writers above are Wave 2 inventory (must not grow; eventual ban outside `live_application` + sqlite load).

### `Task.agent_attempts` (production mutations)

| Location | Symbol / context | Notes |
| --- | --- | --- |
| `crates/ajax-core/src/commands/new_task.rs` | `StartProvisioningStep::AgentCommandSent` | `push(AgentAttempt::new(...))` — opens launch episode |
| `crates/ajax-core/src/commands/teardown/drop_observation.rs` | `mark_drop_agent_stopped` | Running → `Dead` on open rows; **`finished_at` still None today** (Wave 1) |
| `crates/ajax-core/src/registry/sqlite/save.rs` | `save_agent_attempts` | Persist round-trip (not a runtime close policy) |
| `crates/ajax-core/src/registry/sqlite/load.rs` | `load_agent_attempts_by_task` | Hydrate from SQLite |

### Session / hook `Result` boundaries (Wave 5 targets)

**`Result<T, String>` — web session slice**

| File | Examples |
| --- | --- |
| `crates/ajax-web/src/slices/web_session/task_session_directory.rs` | `ensure_entry`, `submit_prompt`, `cancel`, `apply_config_option`, spawn/restore helpers |
| `crates/ajax-web/src/slices/web_session/task_session_spawn.rs` | `spawn_and_attach`, `apply_config_option`, `start_new_context`, `retry_restore` |
| `crates/ajax-web/src/slices/web_session/task_session_answers.rs` | elicitation answer paths |
| `crates/ajax-web/src/slices/web_session/task_session.rs` | command loop replies (`TaskSessionCommand` handlers), `submit_prompt`, `cancel` |
| `crates/ajax-web/src/slices/web_session/task_session_exit.rs` | `persist_prompt_ledger`, `interrupt_active_prompt`, `recover_prompt_ledger` |
| `crates/ajax-web/src/slices/web_session/task_session_replacement.rs` | slot replacement / respawn helpers |
| `crates/ajax-web/src/slices/web_session/session_activity.rs` | `record_session_activity` |
| `crates/ajax-web/src/adapters/web_session_acp/client.rs` | `spawn`, `begin_prompt`, `cancel`, `apply_model_pin`, `apply_config_option` |

**`Result<(), ()>` / silent swallow — CLI hook**

| File | Symbol | Notes |
| --- | --- | --- |
| `crates/ajax-cli/src/agent_event.rs` | `run_agent_event` → `Result<(), ()>` | `run_agent_event_command` uses `let _ = run_agent_event(...)`; missing identity is typed no-op but IO failure can look like success (Wave 5) |

**Activity report (not `Result`, but same class of debt)**

| File | Symbol | Notes |
| --- | --- | --- |
| `crates/ajax-web/src/slices/web_session/task_session.rs` | `try_report_activity` → `bool` | Retries with `thread::sleep`; failed persist is silent (Wave 5) |

### Wave 0 characterization tests landed

| Test | Crate | Role |
| --- | --- | --- |
| `wave1_issue_1096_open_attempt_must_not_run_while_agent_not_started` | ajax-core `new_task/tests.rs` | Failing Wave 1 composition (#1096) |
| `wave1_issue_1096_browser_detail_shows_running_attempt_while_agent_not_started` | ajax-web `cockpit/tests.rs` | Browser projection of #1096 |
| `golden_table_records_cross_layer_disagreements_without_fixing_them` | ajax-core `tests/wave0_run_state_characterization.rs` | Reducer vs apply fields vs projector |
| `issue_1096_fixture_matches_documented_contradiction_fields` | ajax-core `tests/wave0_run_state_characterization.rs` | Fixture contract for #1096 |

If Wave 0 finds more writers before Wave 1 ships, add them here first.

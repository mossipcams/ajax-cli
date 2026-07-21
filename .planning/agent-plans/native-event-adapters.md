# Native event adapters + heuristic patchwork removal

## Scope

Replace low-level detection accommodation (pane chrome recognition, dwell
gates, per-agent freshness special cases) with per-client native hook
adapters feeding the existing reducer at ProviderLifecycle tier. Instrumented
sessions get authoritative lifecycle evidence; uninstrumented sessions project
Unknown + liveness instead of heuristic guesses.

## Non-goals

- No socket transport (file-based events; reducer input is transport-agnostic).
- No second event vocabulary — native events translate to existing
  `ObservationSource` × `ActivityKind`.
- No `LiveStatusKind` axis redesign (presentation enum unchanged).
- No removal of shell/command classification (`CommandRunning`/`TestsRunning`)
  where it is process-based, and no removal of pane *display* capture —
  only pane-text *agent-activity* classification is cut.
- No Workmux-style output-silence heuristics.

## Approval

Implementation authorized by user 2026-07-21 ("plan the adapter implementation
and cut the patch work"). Test deletions below are scoped strictly to deleted
behavior (pane classification, dwell, per-agent freshness) — flagged per the
global no-test-weakening rule; deleting a behavior deletes its tests.

## Delegation

Code-changing Behavior Change → model-router before editing source
(packet per task below). Decision recorded here when made.

## Architecture

```
native client (claude|codex|cursor|pi) in tmux pane
  └─ spawned by ajax-cli __agent-runtime (existing wrapper)
       injects: AJAX_TASK_ID, AJAX_RUN_ID, AJAX_AGENT_EVENTS_DIR
  └─ client hook fires → ajax-cli __agent-event --client X --event Y
       (globally installed, exits 0 instantly when AJAX_TASK_ID unset)
       translates native payload → atomic event file
       {events_dir}/{task}.json  { value, observed_at, run_id, parent_run_id }
  └─ agent_status_cache reads event files → ProviderLifecycle observations
  └─ existing reducer (agent_status.rs) — unchanged precedence:
       ProcessExit > ProviderLifecycle > ProviderHook > liveness
```

### Per-client translation (inside `__agent-event`)

| Client | Event → observation |
| --- | --- |
| Claude | `UserPromptSubmit`, `PreToolUse`, `PostToolUse` → working; `Notification`(permission) → ask; `Notification`(idle prompt) → wait; `Stop` → done **only if** stdin payload `background_tasks` is empty, else working |
| Codex | prompt-submit → working; turn-complete/stop → done (verify exact hook/notify surface against installed Codex version at implementation) |
| Cursor | `beforeSubmitPrompt` → working; `stop` → done; ignore subagent hooks (upstream parent-linkage defect) |
| Pi | `before_agent_start` → working; `agent_end` → done; retry/compaction continuation simply overwrites done with working (newest-wins — brief done flicker accepted, no coalescing patchwork) |

### Freshness policy (replaces the per-agent table)

- Terminal events (done/failed) never expire; superseded only by newer events;
  wrapper `ProcessExit` always outranks.
- Non-terminal events: one generous window (30 min) → then Unknown. Working is
  additionally bounded by wrapper liveness (process gone → wrapper exit wins).
- Waiting states clear by supersession (next hook event) or existing
  `acknowledged_at` ack — unchanged.

## Task checklist

### 1. Identity injection + event sink — DONE 2026-07-21

- [x] Test: wrapper child env contains `AJAX_TASK_ID`/`AJAX_RUN_ID`/`AJAX_AGENT_EVENTS_DIR`
- [x] Test: `__agent-event` with no `AJAX_TASK_ID` exits 0, writes nothing
- [x] Test: Claude Stop with non-empty `background_tasks` → working, empty → done
- [x] Test: Claude Notification permission → ask; idle → wait
- [x] Test: Pi agent_end then before_agent_start → newest working wins
- [x] Test: event file write is atomic (tmp+rename) and self-identified by task
- [x] Implement: env injection in `run_agent_runtime` (agent_runtime.rs:101)
- [x] Implement: `__agent-event` subcommand (translation + atomic write)
- [x] Verify: `cargo test -p ajax-cli agent_event agent_runtime` + clippy + check

### 2. Ingestion at ProviderLifecycle tier — DONE 2026-07-21

- [x] Test: event file → `AgentStatusCacheSource::Lifecycle` entry keyed by task
- [x] Test: lifecycle working outranks hook file for same run
- [x] Test: stale (>30 min) non-terminal lifecycle → Unknown, not Idle
- [x] Test: terminal lifecycle done persists past window; wrapper exit still outranks
- [x] Implement: cache read in agent_status_cache.rs; Lifecycle arm in live.rs
      at ProviderLifecycle/High + hook-supersession filter (fresh lifecycle on
      a run drops ProviderHook observations for that run, avoiding
      equal-timestamp conflict projection during hook/adapter coexistence)
- [x] Verify: live:: 107, runtime_refresh:: 50, agent_status_cache 12 passed;
      clippy + fmt clean

### 3. Hook installers

- [ ] Test: idempotent merge into existing Claude settings hooks (no clobber
      of user entries; second run is a no-op)
- [ ] Test: Cursor hooks.json + Pi extension + Codex config merges idempotent
- [ ] Implement: `ajax agent-hooks install` writing global configs; every
      installed command is env-guarded (`AJAX_TASK_ID` unset → exit 0)
- [ ] Verify: install twice against fixture configs; run a real instrumented
      task per client where available

### 4. Cut the patchwork (only after 1–3 verified against live panes)

- [ ] Delete `live_recognize.rs` (PaneHint/Recognition) and its tests
- [ ] Delete `GenericPane` + `StructuredPane` variants from
      `ObservationSource` and pane candidate assembly in live.rs
- [ ] Delete pane-flip dwell gate in live_application.rs
- [ ] Delete per-agent freshness table (`CODEX_WORKING_FRESH_FOR`, per-kind
      windows in `hook_freshness_window`) → single policy above
- [ ] Retire `pane:{session}:{pane_id}` inference once adapter events carry
      run identity (child runs = per-pane adapter events with own AJAX_RUN_ID)
- [ ] Uninstrumented/hookless agent activity → Unknown + `process_alive`
      (accepted cost; wrapper exit still yields done/failed)
- [ ] Update architecture.md Live Status section (precedence list shrinks to
      ProcessExit > ProviderLifecycle > ProviderHook > liveness)
- [ ] Update reducer/refresh characterization tests for removed sources
- [ ] Verify: full status suites + live verification

## Validation

- `cargo nextest run -p ajax-core` (agent_status, live, runtime_refresh)
- `cargo nextest run -p ajax-cli`
- Live verification against real panes with `--state` DB copy
  (note: tasks register Codex but run Claude — check both mappings)
- `npm run verify` (PR gate)

## Risks

- Long single tool call exceeding hook cadence: covered by 30-min window +
  PreToolUse refresh at tool start; residual risk = tool >30 min → Unknown
  (honest, recoverable on next event).
- Broken/uninstalled hooks on an instrumented client → sticky last event for
  up to 30 min, then Unknown. Wrapper exit always corrects terminal state.
- Codex hook surface uncertain: verify against installed version before
  writing that adapter; if Codex exposes only turn-complete notify, working
  comes from wrapper liveness + open remains inferred from prompt-submit
  absence (degraded but still no pane heuristics).

## Deviations

- Router helper scripts (check-packet/delegate-snapshot/router-log) absent in
  repo; manual equivalents used (git status snapshot, manual readiness check).
- GLM, Codex, and MiniMax lanes all hard rate-limited on 2026-07-21; task 1
  packet dispatched on the last available lane (cursor-delegate, composer-2.5).
- Cursor created 8 out-of-scope scripts/ files during task 1; removed at gate.
- Task 1 TEST_FIRST reported NOT_PROVEN (red = new-module compile failure);
  accepted as red-equivalent for new-module work.

## Validation log

- Task 1: `cargo test -p ajax-cli agent_event` 5 passed; `agent_runtime`
  5 passed; `cargo clippy -p ajax-cli -- -D warnings` OK; `cargo check` OK
  (2026-07-21, verified independently at Review Gate).

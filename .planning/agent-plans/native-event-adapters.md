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

### 3. Hook installers — DONE 2026-07-21

- [x] Test: idempotent merge into existing Claude settings hooks (no clobber
      of user entries; second run is a no-op, byte-identical)
- [x] Test: Cursor hooks.json + Pi extension + Codex hooks.json idempotent
- [x] Implement: `ajax agent-hooks install`; commands env-guarded via
      `__agent-event`'s AJAX_TASK_ID no-op
- [x] Formats verified against live configs/docs: Codex uses Claude-schema
      `~/.codex/hooks.json` with the SAME event names (UserPromptSubmit /
      PostToolUse / Stop) — translation table corrected; Pi maps
      `agent_settled` (not `agent_end`) → done, which is the already-coalesced
      settle event, so no flicker mitigation needed
- [ ] Live smoke per client (deferred to post-merge: needs installed binary)

### 4. Cut the patchwork — DONE 2026-07-21

- [x] Delete `live_recognize.rs` (598 lines) and its tests
- [x] Delete `GenericPane` + `StructuredPane` from `ObservationSource`;
      ranks collapse to ProcessExit > ProviderLifecycle > ProviderHook >
      ProcessLiveness
- [x] Delete pane-flip dwell gate + candidate metadata keys in
      live_application.rs (trusted paths already bypassed it; with pane
      evidence gone no caller remained)
- [x] Delete `CODEX_WORKING_FRESH_FOR` special case → uniform 120s hook
      window; lifecycle: terminal persists, non-terminal 30 min → Unknown
- [x] KEPT `pane:{session}:{pane_id}` hook-FILE child runs (scope correction:
      that is ProviderHook file evidence feeding the delegated-run graph, not
      pane-text classification; retire only when per-pane identity events
      exist)
- [x] Uninstrumented/hookless activity → prior + `process_alive` only;
      wrapper exit still yields done/failed
- [x] architecture.md rewritten to the four-tier precedence
- [x] 22 ajax-cli characterization tests aligned: 3 deleted (pinned the
      deleted pane mechanism), 19 converted to agent-events/hook/wrapper
      evidence with original assertions kept (full disposition list in
      packets/cut-pane-classification-cli-tests.md round report)
- [x] Verify: ajax-core 813 passed; ajax-cli 353 passed (nextest
      --no-fail-fast); clippy/fmt/rustdoc clean; zero references to any cut
      symbol remain
- [ ] Live verification vs real panes (post-merge, needs installed binary)

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
- Task 4 spillovers: attention.rs dwell test deleted (outside packet allowed
  files but ajax-core-internal and dwell-only); `tests/live_cli.rs` modified
  in the REVISE round — explicit deviation from the no-tests-dir rule,
  authorized in the revise packet for pane-behavior characterization only.
- One converted web_backend test initially wrote agent-events under a
  crate-relative `.cache/`; fixed locally (2-line isolation under the test's
  temp root) at the gate.
- Codex/Pi event names in task 1 were speculative; corrected in task 3 to
  verified names (Codex = Claude-schema hooks.json; Pi settle = agent_settled).

## Validation log

- Task 1: `cargo test -p ajax-cli agent_event` 5 passed; `agent_runtime`
  5 passed; `cargo clippy -p ajax-cli -- -D warnings` OK; `cargo check` OK
  (2026-07-21, verified independently at Review Gate).

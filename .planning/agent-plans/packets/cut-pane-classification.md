# Packet: cut pane-text classification, dwell gate, per-agent freshness (task 4)

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

`TEST_FIRST: NOT_APPLICABLE` — this is a removal task: behavior is deleted,
its tests are deleted with it, and surviving behavior is pinned by the
existing suites plus a small number of updated characterization tests.

## Goal

Remove the heuristic accommodation layer now that lifecycle events are the
activity authority: pane-text agent-activity classification, the dwell gate
that debounced pane misreads, and the Codex-specific hook freshness special
case. Uninstrumented sessions project no confident activity (prior state +
`process_alive` only; wrapper exit still yields done/failed).

## Allowed files

- `crates/ajax-core/src/live_recognize.rs` (delete file)
- `crates/ajax-core/src/live.rs`
- `crates/ajax-core/src/live_application.rs`
- `crates/ajax-core/src/agent_status.rs`
- `crates/ajax-core/src/runtime_refresh.rs`
- `architecture.md`

## Forbidden changes

- KEEP pane-scoped hook FILE ingestion (`pane:{session}:{pane_id}` child
  runs in `agent_status_cache.rs` / ProviderHook tier) — hook files are
  evidence, only pane TEXT parsing is cut. Do not touch
  `crates/ajax-cli/**`.
- KEEP `tmux.capture_pane` adapter method itself (other consumers); remove
  only the runtime_refresh status-classification usage.
- KEEP `AgentClient::Other` ignoring hook files.
- KEEP acknowledged_hold, attention, lifecycle transition behavior.
- No changes to the Lifecycle arm added in task 2 (constants, ranks,
  supersession filter).
- No edits under any standalone `tests/` directory; inline `#[cfg(test)]`
  modules only (lifecycle guard splits files at inline cfg(test) — do not
  move tests to sibling files).

## Context evidence

- Pane classification: `live_recognize.rs` (whole file);
  `live.rs:513+` `project_pane_activity` + `mod recognize` declaration and
  its `#[path]`; pane tests in live.rs (`:669`, `:691` area).
- Refresh pane fallthrough: `runtime_refresh.rs:425-~520` (capture_pane
  probe, `project_pane_activity`, pane projection apply) and the
  `has_pending_live_class_candidate` check at `:389`. capture-pane test
  fixtures at `:1004`, `:1080`, `:1101`, `:1327`, `:1489`, `:1522`, `:1589`.
- Dwell gate: `live_application.rs:9-155` (candidate keys, `defers_*`,
  `defer_class_candidate`, `has_pending_*`, `shows_*`, dwell tests at
  `:385+`). Trusted/authoritative paths already bypass it; after pane
  removal no caller reaches the dwell branch, so it is dead.
- Per-agent freshness: `live.rs:15-25` (`CODEX_WORKING_FRESH_FOR`),
  `hook_freshness_window` `live.rs:104-118` Codex arm.
- Pane observation sources: `agent_status.rs:55-80`
  (`ObservationSource::{StructuredPane, GenericPane}` + rank), reducer tests
  using them.
- Docs: `architecture.md` Live Status section (precedence list lines
  ~134-157) currently lists 6 tiers including pane recognition.

## Code anchors / ordered edits

1. `runtime_refresh.rs` — delete the pane fallthrough block (capture_pane
   probe through pane projection application) and the
   `has_pending_live_class_candidate` condition at `:389`. When the status
   decision does not apply, refresh simply moves on (prior state persists;
   missing-substrate handling above is untouched). Update/delete
   capture-pane-driven characterization tests: tests that asserted
   pane-derived activity either (a) convert to Lifecycle/Hook/wrapper
   candidates when they pin still-existing semantics (e.g. busy chrome must
   not assert AgentRunning is now vacuous — delete), or (b) are deleted with
   a one-line justification in the report. Probe-failure recording for
   capture-pane goes away with the block.
2. `live.rs` — delete `project_pane_activity`, the `recognize` module
   declaration, pane tests; delete `CODEX_WORKING_FRESH_FOR` and the Codex
   special case in `hook_freshness_window` (all agents except `Other` use
   `HOOK_DEFAULT_FRESH_FOR` for the accepted kinds).
3. Delete `live_recognize.rs`.
4. `live_application.rs` — delete dwell machinery:
   `WAITING_CANDIDATE_SINCE_KEY`, `RUNNING_CANDIDATE_SINCE_KEY`,
   `WAITING_CONFIRMATION_DWELL`, `defers_unconfirmed_attention`,
   `defers_unconfirmed_running`, `defer_class_candidate`,
   `has_pending_waiting_candidate`, `has_pending_running_candidate`,
   `has_pending_live_class_candidate`, `shows_running_evidence`,
   `shows_waiting_evidence`, `unix_seconds`, and dwell tests.
   `apply_observation_at` becomes reduce + `apply_reduced_observation`.
   Stale metadata keys on existing tasks are simply ignored (no migration).
5. `agent_status.rs` — delete `ObservationSource::StructuredPane` and
   `GenericPane`; ranks collapse to ProcessExit 0, ProviderLifecycle 1,
   ProviderHook 2, ProcessLiveness 3. Update/delete reducer tests that used
   pane sources; keep every other reducer behavior identical.
6. `architecture.md` — rewrite the Live Status precedence list to the four
   remaining tiers, state that pane text is no longer classified for agent
   activity (uninstrumented sessions: prior + process liveness + wrapper
   exit; instrumented: lifecycle events), keep the pane-scoped hook-file
   child-run paragraph, and note terminal lifecycle persistence + the
   30-minute non-terminal lifecycle window.

## Test-first instructions

NOT_APPLICABLE: removal task; deleted behavior takes its tests with it.
Surviving semantics are already pinned by live::, runtime_refresh::,
agent_status:: suites which must stay green after updates.

## Edit instructions

Follow the ordered edits; delete rather than comment out. Every deleted
characterization test must be either genuinely about deleted behavior or
converted, never silently weakened: if a test pins surviving behavior
(e.g. "process liveness alone never asserts AgentRunning"), convert its
evidence source instead of deleting the assertion.

## Verification commands

```bash
cargo test -p ajax-core
cargo test -p ajax-cli
cargo clippy -p ajax-core -p ajax-cli -- -D warnings
cargo fmt -p ajax-core -p ajax-cli -- --check
cargo doc -p ajax-core --no-deps 2>&1 | grep -ci warning
```

(doc check because rustdoc runs `-D warnings` in CI and `live_recognize`
links appeared in doc comments before — ensure none remain.)

## Acceptance criteria

- `live_recognize.rs` gone; no references to `recognize_pane`, `PaneHint`,
  `Recognition`, `project_pane_activity`, `StructuredPane`, `GenericPane`,
  dwell candidate keys, or `CODEX_WORKING_FRESH_FOR` anywhere in `crates/`.
- Full ajax-core + ajax-cli suites green; fmt/clippy clean; no rustdoc
  warnings.
- Pane-scoped hook-file child-run ingestion still tested and green
  (ajax-cli agent_status_cache suite untouched).
- architecture.md matches the new four-tier precedence.

## Stop conditions

- A non-pane caller of the dwell gate or of `project_pane_activity` exists
  that this packet did not list.
- Removing a test would weaken an assertion about surviving behavior with no
  conversion possible.
- Patch exceeds ~1500 changed lines (deletion-heavy task; higher cap than
  the default 400 applies to deletions of the listed surfaces only).

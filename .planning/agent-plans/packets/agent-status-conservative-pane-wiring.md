# TDD Implementation Packet — agent status pane + refresh wiring

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

Complete packet 2 of the conservative agent-status work: (1) apply
`WaitingOnDelegated` projections from `select_status_observation`, (2) route
capture-pane evidence through `agent_status::reduce_agent_status` as
StructuredPane (Medium) vs GenericPane (Low) so historical/heuristic pane text
cannot confidently assert `AgentRunning` alone, (3) plumb `process_alive` on
`StatusDecision` for the pane fallback, (4) update `architecture.md` Live Status
precedence to match the new model.

## Allowed files

- `crates/ajax-core/src/live.rs`
- `crates/ajax-core/src/agent_status.rs` (only if a tiny helper/export is required)
- `crates/ajax-core/src/runtime_refresh.rs`
- `architecture.md` (Live Status / precedence paragraphs only)
- `.planning/agent-plans/agent-status-conservative.md` (checklist only)

## Forbidden changes

- Do not add pane regex vocabulary or expand needle lists in `pane_evidence` /
  `looks_like_*`.
- Do not edit `ajax-web`, `ajax-cli` cache/runtime (except via core ports already
  used), SQLite migrations, or unrelated modules.
- Do not invent new public `LiveStatusKind` variants.
- No formatting sweeps, renames, drive-by cleanup.
- Do not commit, push, merge, rebase, or change branches.
- Stop if the patch would exceed ~400 changed lines.

## Context evidence

- **Review Gate MEDIUM:** `live.rs` ~319 — `WaitingOnDelegated` has
  `selected_source: None` and falls through to `preserved`, so parent-waiting-on-
  child never applies.
- **Review Gate MEDIUM:** `runtime_refresh.rs` ~432 — after non-applied
  decision, `classify_agent_pane` + `apply_observation` still confidently sets
  `AgentRunning` from heuristics; packet 1 only stopped wrapper→activity.
- **Reducer already encodes:** GenericPane+Low+Working/WaitingApproval →
  Unknown (`is_untrustworthy_low_pane` in `agent_status.rs`); StructuredPane can
  assert activity.
- **Existing classify split:** Cursor stream-json = structured; busy chrome /
  generic needles = generic; agent-specific prompts (Claude permission / Codex
  composer) = structured UI recognition (Medium), not generic.

## Code anchors

`crates/ajax-core/src/live.rs`:
- `StatusDecision` / `select_status_observation` match at ~319–352
- `classify_agent_pane` / `classify_cursor_stream_json_line` /
  `has_recent_busy_indicator` / `classify_agent_prompt` / `pane_evidence`

`crates/ajax-core/src/runtime_refresh.rs`:
- pane fallthrough block ~400–453

`architecture.md`:
- Agent runtime / hook precedence ~93–105
- Live Status section ~329–371

Locked design:

1. Add `pub process_alive: bool` to `StatusDecision` (default false). Set it from
   the wrapper heartbeat scan already computed inside `select_status_observation`.

2. When `projection.phase == WaitingOnDelegated`, return
   `applied: true`, `observation: Some(projection.live)`,
   `source: None` (or keep source None), `observed_at: Some(now)`,
   `process_alive` as computed. Update `runtime_refresh` to apply when
   `decision.applied && decision.observation.is_some()` even if `source` is
   None, using `apply_observation_at` (ordinary, not trusted lifecycle).

3. Add `pub fn project_pane_activity(agent, pane, now) -> Option<StatusObservation>`
   in `live.rs` (or `agent_status` if cleaner) that:
   - Returns `None` for empty/Unknown-only panes
   - Cursor stream-json hit → `StructuredPane`, Medium, TTL 60s
   - Agent-specific prompt hit (permission/idle composer) → `StructuredPane`,
     Medium, TTL 60s
   - Busy chrome / generic `pane_evidence` activity → `GenericPane`, Low, TTL 15s
   - Map kinds via existing `live_kind_to_activity`
   - `run_id: "primary"`, `parent_run_id: None`
   - **Do not add regexes**; branch on which existing classifier path fired

4. Replace direct pane `apply_observation` in `runtime_refresh` with:
   - Build pane `StatusObservation` via `project_pane_activity`
   - `reduce_agent_status` with `process_liveness` from `decision.process_alive`
     and observations = `[pane_obs]` (if any)
   - If projection phase is `Unknown`, do **not** overwrite live status (same
     as today's Unknown handling / continue without fabricating)
   - Otherwise `apply_observation_at` with projection.live (ordinary path)
   - Never call `apply_trusted_observation` for pane-only evidence

5. `architecture.md`: replace the old wrapper-heartbeat-as-activity precedence
   with the conservative order (exit → lifecycle → hook → structured pane →
   generic pane → liveness informational) and note process_alive ≠ AgentRunning;
   mention parent/child aggregation at a short paragraph level.

## Test-first instructions

In `live.rs` tests and/or `runtime_refresh` tests:

1. `waiting_on_delegated_projection_is_applied` — construct reducer input with
   only a child Working observation (via `select_status` only if child
   candidates exist; otherwise unit-test a small helper / directly assert the
   match arm by feeding observations through an exported test path). Minimum:
   unit test that when `reduce_agent_status` returns `WaitingOnDelegated`,
   `select_status_observation` (after wiring child hook candidates **or** a
   focused internal test of the decision mapping) sets `applied: true` with
   WaitingForInput summary containing `delegated`.

   Practical approach: extend `StatusCandidate` is NOT required. Add a unit test
   in `live.rs` that calls the decision-mapping logic by using
   `select_status_observation` with empty candidates is insufficient for
   children. **Instead:** add test `waiting_on_delegated_status_decision_applies`
   that uses a new test-only or public helper
   `status_decision_from_projection(projection, process_alive, ack_hold, prior)
   -> StatusDecision` extracted from the match arms — OR add optional
   `extra_observations: &[StatusObservation]` to `StatusDecisionInput` for
   tests and refresh pane path. Prefer adding
   `pub extra_observations: &'a [crate::agent_status::StatusObservation]`
   defaulting to empty in production callers; tests pass a child Working obs.

2. `generic_pane_busy_alone_projects_unknown` — `project_pane_activity` on text
   that today's `classify_agent_pane` would call AgentRunning via busy chrome
   (`"thinking"` / `"codex is working"`) yields GenericPane Low; reducing with
   process_alive=true yields Unknown.

3. `structured_cursor_json_pane_can_project_running` — a Cursor `{"type":"thinking"}`
   line projects StructuredPane Medium Working and reduce → AgentRunning.

4. `runtime_refresh` test (extend existing Scripted patterns): when only wrapper
   `working` + pane with historical busy text, live status must **not** become
   AgentRunning solely from that pane (Unknown or preserved prior).

RED:

```bash
cargo test -p ajax-core --lib live::tests::waiting_on_delegated -- --nocapture
cargo test -p ajax-core --lib live::tests::generic_pane_busy -- --nocapture
cargo test -p ajax-core --lib live::tests::structured_cursor -- --nocapture
```

Expected: fail until implemented.

Also keep:

```bash
cargo test -p ajax-core --lib agent_status:: -- --nocapture
cargo test -p ajax-core --lib live:: -- --nocapture
```

## Edit instructions

1. RED tests first.
2. Extract/extend `StatusDecision` + `select_status_observation` mapping for
   `WaitingOnDelegated`; add `process_alive` + optional `extra_observations`.
3. Implement `project_pane_activity` without new regexes.
4. Wire `runtime_refresh` pane fallthrough through reducer.
5. Update `architecture.md` Live Status precedence.
6. Check off packet-2 plan items.

## Verification commands

```bash
cargo test -p ajax-core --lib agent_status:: -- --nocapture
cargo test -p ajax-core --lib live:: -- --nocapture
cargo test -p ajax-core --lib runtime_refresh:: -- --nocapture
cargo clippy -p ajax-core --lib -- -D warnings
```

## Acceptance criteria

- WaitingOnDelegated applies (`applied: true`, waiting summary).
- Generic busy pane + process_alive alone → not AgentRunning.
- Structured Cursor JSON pane can still yield AgentRunning.
- Wrapper working remains liveness-only.
- architecture.md documents new precedence.
- Existing agent_status + live suites stay green (update only tests that
  asserted confident pane AgentRunning from generic busy alone, if any break).

## Stop conditions

- Need new regex needles → stop.
- Need SQLite / web / CLI cache redesign beyond optional run metadata → stop.
- Patch > ~400 lines → stop and split.
- Broader live pane classifier test failures requiring behavior beyond
  GenericPane Low demotion → stop and report.

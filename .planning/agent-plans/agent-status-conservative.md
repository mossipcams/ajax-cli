# Agent status: conservative observation architecture

## Scope

Fix false-positive `AgentRunning`, stale evidence overrides, and missing
delegation awareness in Ajax agent status. Introduce an internal
observation/run model; keep existing `LiveStatusKind` as a presentation
projection for UI compatibility.

## Non-goals

- No new pane regex vocabulary (reuse existing classifiers at low confidence).
- No SQLite schema migration unless persistence of run graph proves required;
  prefer in-memory/ephemeral run observations keyed by task, derived each refresh.
- No Web Cockpit UI redesign beyond consuming derived `LiveStatusKind`.
- No provider-specific subagent protocol invention beyond what hooks/wrapper
  already expose (or can expose with minimal wrapper metadata).

## Approval

Architecture change authorized by explicit user implementation request
(diagnose → plan → implement). Update `architecture.md` Live Status section
in the same change.

## Delegation decision

`Delegation decision: delegated via model-router` (architecture-wide; packet
then `pi-delegate` / GLM unless router selects otherwise).

Packet 1: ACCEPT (GLM hung after write; parent Review Gate).
Packet 2: in progress — pane/refresh wiring + architecture.md.

## Diagnosis summary (see user-facing report)

1. Wrapper heartbeat maps process-alive → `"working"` → `AgentRunning`, and
   outranks hooks while fresh.
2. Pane heuristics treat historical chrome as live activity with full confidence.
3. Observations lack expires_at/confidence/run_id; tier priority can beat newer
   run-scoped evidence.
4. No parent/child run graph; task completion ignores delegated work.

## Target architecture

### Evidence sources (ordered)

1. Terminal process exit / fatal runtime error
2. Structured provider lifecycle event
3. Provider hook event
4. Structured pane/UI recognition (Cursor stream-json, strong chrome)
5. Generic pane heuristic (low confidence fallback)
6. Process liveness — informational only (`process_alive`), never alone →
   `AgentRunning`

### Internal model

- `StatusObservation`: source, observed_at, expires_at, confidence, run_id,
  optional parent_run_id, activity payload (not process liveness).
- `AgentRunGraph`: runs keyed by run_id; parent/child edges; terminal when
  exit/failed/done and all non-detached descendants terminal.
- `process_alive: bool` separate from activity.
- Ambiguous/contradictory fresh evidence → `LiveStatusKind::Unknown`.
- Derive `LiveStatusKind` (+ agent_status / side flags) at presentation /
  application boundary from aggregated run state.

### Parent aggregation

| Parent local state | Active non-detached children | Projected parent |
| --- | --- | --- |
| working | any/none | actively working |
| waiting input/approval | any/none | waiting for user |
| idle/waiting | ≥1 active | waiting on delegated runs |
| done/exited | ≥1 active | completed locally; children remain |
| done/exited | none | fully completed |
| ambiguous | — | Unknown |

Do not mark task Reviewable/Done until all non-detached descendants are terminal.

### Freshness

- Expire observations past `expires_at`.
- Never let older evidence overwrite newer run-scoped evidence for the same
  run_id.
- Cross-source conflicts at comparable freshness/confidence → Unknown (or keep
  prior only if prior still unexpired and higher tier).

## Task checklist

### 1. Observation/run model + pure reducer

- [x] Test: idle live process → not AgentRunning; Unknown or idle/waiting
- [x] Test: live process waiting for approval (hook/structured)
- [x] Test: stale working heartbeat then waiting hook → waiting
- [x] Test: historical pane “running”/“approval required” → low conf / Unknown
- [x] Test: parent waiting on one active child
- [x] Test: parent complete, child active → not fully done
- [x] Test: mixed children (running/failed/completed)
- [x] Test: child complete then parent resumes
- [x] Test: orphaned/stale delegated run expires
- [x] Test: conflicting observations (time + confidence)
- [x] Test: process exit beats stale active pane
- [x] Test: no trustworthy evidence → Unknown
- [x] Implement model + `select`/`aggregate` in `ajax-core` (prefer new
      `agent_status` module or focused expansion of `live.rs`)
- [x] Verify focused unit tests

### 2. Wire adapter + refresh + application

- [x] Wrapper snapshot: expose `process_alive` / do not map Running→working activity
      (via StatusDecision.process_alive + liveness-only wrapper working)
- [x] `agent_status_cache`: pass through source, timestamps, and pane child
      `run_id` / `parent_run_id` (`pane:{session}:{pane_id}` → primary)
- [x] `runtime_refresh`: ingest observations; apply aggregation; derive LiveStatus
- [x] Pane fallthrough: StructuredPane Medium vs GenericPane Low via reducer
      (capture-pane must not confidently assert AgentRunning alone)
- [x] Apply `WaitingOnDelegated` even when `selected_source` is None
- [x] `live_application`: ordinary apply for pane / delegated; trusted Done only
      from wrapper ProcessExit when aggregation is FullyCompleted
- [x] Preserve dwell gate behavior for ordinary pane class flips where applicable
- [x] Verify refresh integration tests + existing status tests updated

### 3. Docs + validation

- [x] Update `architecture.md` Live Status precedence to match new order
- [x] `cargo nextest run -p ajax-core` (status-related filters as needed)
- [x] `cargo nextest run -p ajax-cli` agent_status / agent_runtime tests
- [x] Broader check if touch surface requires: `cargo check -p ajax-core -p ajax-cli`
- [x] `npm run verify` (PR gate)
- [ ] Commit + open PR

## Delegation aggregation rules (notifications)

Parent phases that wait on children (`waiting on delegated runs`,
`delegated runs still active`) project `Waiting` in the UI but are
**not** operator-actionable: they do not set `NeedsInput`, do not annotate
`NeedsMe`, and do not fire attention webhooks. Real user waits
(`Waiting for approval` / `Waiting for input`) still notify.
- Pi/GLM hung ~17m after writing the reducer (empty buffered `raw.log`);
  parent killed worker and ran Review Gate locally.
- Packet 2: parent implemented locally after GLM hang on packet 1
  (`Delegation decision: not delegated because pi/GLM hung without report on
  packet 1; parent completed packet 2 after ACCEPT`).
- Same-run selection is tier-then-timestamp (evidence order), not
  timestamp-regardless-of-tier; same-source newer-wins covered by tests.
- Child run_id emission from tmux-agent-status pane files is implemented
  (`pane:{session}:{pane_id}`, parent `primary`).
- Characterization fixtures that need confident AgentRunning now use
  structured Cursor `{"type":"thinking"}` pane evidence; generic
  `codex is working` alone stays Unknown.

## Validation log

- `cargo test -p ajax-core --lib agent_status::` → 12 passed
- `cargo test -p ajax-core --lib live::` → 99 passed
- `cargo test -p ajax-core --lib runtime_refresh::` → 49 passed
- `cargo test -p ajax-cli agent_status` → 10 passed
- `cargo test -p ajax-cli agent_runtime` → 4 passed
- `cargo clippy -p ajax-core --lib -- -D warnings` → OK
- `cargo check -p ajax-cli` → OK
- Review Gate packet 1: ACCEPT (2026-07-19)
- Packet 2: implemented by parent (2026-07-19)
- Characterization tests updated for structured busy pane (2026-07-19)
- `npm run verify` → passed (2026-07-19)

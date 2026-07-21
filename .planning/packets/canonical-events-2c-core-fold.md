# Packet: move open-set fold into ajax-core (Phase 2c)

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

Move canonical event kinds, `RunSnapshot` fold, and `project_snapshot` from
`ajax-cli` into `ajax-core` so the reducer boundary owns facts→snapshot.
Add a pure adapter that turns `RunSnapshot` into `StatusObservation`s and
prove via test that `reduce_agent_status` yields the expected live kinds.
`ajax-cli` keeps translate/write/JSONL I/O and calls core for fold/project.
Do not add socket or pane code.

## Allowed files

- `crates/ajax-core/src/canonical_agent_event.rs` (new)
- `crates/ajax-core/src/lib.rs`
- `crates/ajax-core/src/agent_status.rs` (only if a tiny public helper fits;
  prefer putting `observations_from_run_snapshot` in canonical_agent_event.rs)
- `crates/ajax-cli/src/agent_event.rs`
- `crates/ajax-cli/src/agent_status_cache.rs` (import path updates only)

## Forbidden changes

- No socket, no pane recognizer, no installer changes.
- Do not change ObservationSource rank order.
- No new dependencies.
- No commits.
- Stop if move exceeds ~400 changed lines — then leave a thin re-export shim
  and report (prefer completing the move).

## Context evidence

- Fold implementation today: `crates/ajax-cli/src/agent_event.rs` ~414–640
  (`RunPhase`, `RunSnapshot`, `fold_envelopes`, `project_snapshot`,
  `fold_and_project_jsonl`) and fold tests ~713+.
- Cache calls `crate::agent_event::fold_and_project_jsonl` in
  `agent_status_cache.rs:221`.
- Reducer: `ajax_core::agent_status::{reduce_agent_status, StatusObservation,
  ActivityKind, ObservationSource, Confidence}`.
- Lifecycle maps: working→Working, wait→WaitingInput, ask→WaitingApproval,
  done→Done, failed→Failed (`live_kind_to_activity` / classify).

## Code anchors

1. Create `canonical_agent_event.rs` in ajax-core with (moved) public types:
   `CanonicalEventKind`, `ActivityKind`, `AttentionReason`, `TurnOutcome`,
   `CanonicalEventDetail`, `ParsedEnvelope`, `RunPhase`, `RunSnapshot`,
   `fold_envelopes`, `project_snapshot`.
2. Add:
   ```rust
   pub fn observations_from_run_snapshot(
       snapshot: &RunSnapshot,
       now: SystemTime,
       run_id: &str,
   ) -> Vec<StatusObservation>
   ```
   Map: Active→Working ProviderLifecycle High TTL 30min;
   Blocked+Permission→WaitingApproval; Blocked else→WaitingInput;
   Settled→Done; Failed→Failed; Unknown→empty vec.
   `expires_at`: Failed/Done use long TTL (same spirit as LIFECYCLE_TERMINAL);
   others now+30min. `observed_at = now` for this packet.
3. Move fold unit tests into core module tests (keep assertions).
4. `ajax-cli` `agent_event.rs`: delete moved types/fold; `use ajax_core::canonical_agent_event::*`
   (or explicit imports); keep translate/write/jsonl parse that builds
   `ParsedEnvelope` and calls `fold_envelopes`/`project_snapshot`.
5. `agent_status_cache.rs`: call `ajax_core::canonical_agent_event::...` or
   keep calling `agent_event::fold_and_project_jsonl` wrapper in cli.
6. Core test: build envelopes → fold → observations_from_run_snapshot →
   reduce_agent_status → AgentRunning / WaitingForApproval / Done as
   appropriate (at least one Active and one Blocked+Permission case).

## Test-first instructions

Red: `cargo test -p ajax-core canonical_agent_event -- --nocapture`

1. Port `fold_two_tools_one_finish_stays_working` (and the other two fold
   tests) into core — they fail until types/fold move.
2. `run_snapshot_feeds_reduce_agent_running` — Active snapshot → reduce →
   live kind AgentRunning.
3. `run_snapshot_feeds_reduce_waiting_approval` — Blocked+Permission →
   WaitingForApproval.

Then update cli to compile against core; run cli tests.

## Edit instructions

Move code carefully; preserve fold semantics exactly. Prefer `pub` on core
types used by cli. Avoid duplicating enums in both crates.

## Verification commands

```bash
cargo test -p ajax-core canonical_agent_event
cargo test -p ajax-cli agent_event
cargo test -p ajax-cli agent_status_cache
cargo clippy -p ajax-core -p ajax-cli --all-targets -- -D warnings
cargo fmt -p ajax-core -p ajax-cli -- --check
```

## Acceptance criteria

- Fold lives in ajax-core; cli is I/O + translate only.
- reduce_agent_status fed from RunSnapshot in at least one core test.
- Existing cli fold/cache behaviors remain green.

## Stop conditions

- Serde/orphan issues force a dependency change.
- Need to redesign ObservationSource.
- Patch clearly >500 lines with no compile — stop and report progress.

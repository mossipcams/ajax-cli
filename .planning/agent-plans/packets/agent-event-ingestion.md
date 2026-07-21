# Packet: ingest __agent-event files at ProviderLifecycle tier (task 2)

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

Event snapshots written by `__agent-event` (task 1) become
`ProviderLifecycle`-tier observations in the status decision: read
`{cache_dir}/agent-events/*.json` in the agent-status cache, map them through
a new `Lifecycle` evidence source, and classify them in
`select_status_observation` at `ObservationSource::ProviderLifecycle` with
High confidence — outranking hook files, outranked only by wrapper
ProcessExit. Lifecycle evidence ignores `selected_agent` (identity comes from
the event; tasks may register one client and run another).

## Allowed files

- `crates/ajax-cli/src/agent_status_cache.rs`
- `crates/ajax-core/src/runtime_refresh.rs`
- `crates/ajax-core/src/live.rs`

## Forbidden changes

- No changes to `crates/ajax-core/src/agent_status.rs` (reducer stays as-is).
- No changes to `agent_event.rs` write side or wrapper.
- No edits under any `tests/` directory; tests are inline `#[cfg(test)]`.
- No changes to existing Hook/RuntimeWrapper arm behavior or freshness
  constants.
- No new dependencies.

## Context evidence

- Event file schema (writer): `crates/ajax-cli/src/agent_event.rs:13`
  `AgentEventSnapshot { task_id, run_id, parent_run_id: Option, value,
  observed_at_unix_millis: u128 }`; dir = `{cache_dir}/agent-events`, file
  `{task_stem}.json`. Reuse this struct for deserialization.
- Cache types: `crates/ajax-core/src/runtime_refresh.rs:21`
  (`AgentStatusCacheEntry`), `:33` (`AgentStatusCacheSource { Hook,
  RuntimeWrapper }`).
- Cache read: `crates/ajax-cli/src/agent_status_cache.rs:80`
  (`from_roots_at`), `:103` (`from_runtime_cache` — has `cache_dir`), `:159`
  (`read_agent_runtime_entry` pattern: terminal entries always `fresh`).
- Candidate mapping: `crates/ajax-core/src/runtime_refresh.rs:346-352`
  (source match → `live::AgentEvidenceSource`).
- Evidence enum + decision arms: `crates/ajax-core/src/live.rs:142`
  (`AgentEvidenceSource`), `:249` (candidate loop; RuntimeWrapper arm),
  `:297` (Hook arm to mirror for ack suppression), `:471`
  (`classify_agent_status_value`: working|wait|ask|done|failed →
  LiveStatusKind).
- Freshness constants live at the top of live.rs (`HOOK_DEFAULT_FRESH_FOR`
  etc., live.rs:15-25).
- Plan freshness policy (`.planning/agent-plans/native-event-adapters.md`):
  terminal lifecycle events never expire (superseded only by newer events;
  wrapper exit outranks); non-terminal events get one 30-minute window.

## Code anchors

1. `runtime_refresh.rs:33` — add `Lifecycle` variant to
   `AgentStatusCacheSource` (doc comment: agent-event lifecycle snapshot);
   map it at `:348` to `live::AgentEvidenceSource::Lifecycle`.
2. `live.rs:142` — add `Lifecycle` variant (doc: structured provider
   lifecycle event from the __agent-event sink).
3. `live.rs` candidate loop — new `AgentEvidenceSource::Lifecycle` arm:
   - `classify_agent_status_value(&candidate.value)`; unknown value → skip.
   - Do NOT consult `selected_agent` and do NOT call
     `hook_observation_if_eligible`.
   - Ack suppression identical to the Hook arm
     (`is_acknowledgeable_kind` + `acknowledged_at` → `acknowledged_hold`).
   - Eligibility/expiry: terminal kinds (`Done`, `CommandFailed`) are always
     eligible with `expires_at = observed_at + LIFECYCLE_TERMINAL_FRESH_FOR`
     (new const `Duration::from_secs(365 * 24 * 3600)`); non-terminal kinds
     (`AgentRunning`, `WaitingForInput`, `WaitingForApproval`) require
     `within_window(now, observed_at, LIFECYCLE_FRESH_FOR)` (new const
     `Duration::from_secs(30 * 60)`) and use that window for `expires_at`.
   - Push `StatusObservation` with
     `source: ObservationSource::ProviderLifecycle`,
     `confidence: Confidence::High`, run id defaulting to primary like the
     wrapper arm (live.rs:276-280).
4. `agent_status_cache.rs` — `from_roots_at` (or a sibling helper called from
   `from_runtime_cache`) also reads `{cache_dir}/agent-events/*.json`:
   deserialize `AgentEventSnapshot`; `observed_at` from
   `observed_at_unix_millis`; entry
   `{ value, observed_at, fresh: terminal || within fresh_for, source:
   AgentStatusCacheSource::Lifecycle, run_id: None when snapshot.run_id ==
   "primary" else Some(run_id), parent_run_id }` keyed into `by_task` under
   `snapshot.task_id`. Malformed files are skipped silently.

## Test-first instructions

Red command: `cargo test -p ajax-core live::lifecycle` and
`cargo test -p ajax-cli agent_status_cache` (new tests fail first).

1. live.rs `lifecycle_working_outranks_hook_wait`: candidates = Lifecycle
   `working` (now) + Hook `wait` (now), selected_agent Claude → decision
   applied with `AgentRunning`.
2. live.rs `lifecycle_ignores_selected_agent_other`: selected_agent
   `AgentClient::Other`, single Lifecycle `working` (now) → applied
   `AgentRunning` (hooks would be ignored for Other; lifecycle is not).
3. live.rs `stale_nonterminal_lifecycle_does_not_assert_activity`: single
   Lifecycle `working` observed 31 minutes ago → `applied == false` (no
   confident activity; prior preserved).
4. live.rs `terminal_lifecycle_persists_and_wrapper_exit_outranks`:
   Lifecycle `done` observed 2 hours ago alone → applied `Done`; adding
   RuntimeWrapper `failed` (fresh) → `CommandFailed` wins.
5. agent_status_cache.rs `agent_event_files_become_lifecycle_entries`: write
   `{root}/agent-events/web__fix-login.json` with the AgentEventSnapshot
   shape (run_id `primary`) → `status_entries_for_task` returns an entry with
   `source == Lifecycle`, `run_id == None`; a snapshot with run_id
   `pane:s:%1`, parent `primary` → `run_id == Some("pane:s:%1")`,
   `parent_run_id == Some("primary")`.

## Edit instructions

Exactly the anchors above; smallest edit that turns the five tests green.
Public enum variants need doc comments (rustdoc gate runs `-D warnings`).

## Verification commands

```bash
cargo test -p ajax-core live::
cargo test -p ajax-core runtime_refresh::
cargo test -p ajax-cli agent_status_cache
cargo clippy -p ajax-core -p ajax-cli -- -D warnings
cargo fmt -p ajax-core -p ajax-cli -- --check
```

## Acceptance criteria

- All five new tests pass; existing live/runtime_refresh/agent_status_cache
  suites stay green.
- Hook and RuntimeWrapper behavior byte-identical (no changed constants or
  arms).
- fmt/clippy clean.

## Stop conditions

- Any anchor mismatch.
- The Lifecycle arm cannot be added without modifying `agent_status.rs`.
- Existing suite failures unrelated to the new arm.
- Patch would exceed ~400 changed lines.

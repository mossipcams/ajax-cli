# Packet: open-set fold over JSONL on read (Phase 2a)

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

When `{stem}.jsonl` exists, fold canonical envelopes into an orthogonal
open-set `RunSnapshot` and project a legacy status string for
`AgentStatusCacheEntry`. Prefer the fold over the last-write `{stem}.json`
value so concurrent tools and sticky attention work. Keep feeding
`AgentStatusCacheSource::Lifecycle` with the same entry shape. No live.rs /
agent_status.rs rewrite; no socket.

## Allowed files

- `crates/ajax-cli/src/agent_event.rs`
- `crates/ajax-cli/src/agent_status_cache.rs`

## Forbidden changes

- Do not edit ajax-core, agent_hooks, architecture.md, cli.rs, lib.rs.
- Do not change `AgentEventSnapshot` field names used by writers.
- Do not add dependencies.
- No commits.

## Context evidence

- JSONL append + envelope kinds: `agent_event.rs` (Phase 1a).
- Cache read: `agent_status_cache.rs` `read_agent_event_entry` (~212) deserializes
  `{stem}.json` only; `from_roots_at` scans `*.json` in agent-events dir.
- Plan: `.planning/agent-plans/canonical-agent-events.md` open sets + projection.
- Legacy values: working|wait|ask|done|failed (classify_agent_status_value).

## Code anchors

1. In `agent_event.rs`, add compact (private or pub(crate)) types:

```text
RunSnapshot {
  liveness ignored for now,
  phase: Active | Blocked | Settled | Failed | Unknown,
  activity: Option<ActivityKind>,
  blocker: Option<AttentionReason>,
  outcome: Option<TurnOutcome>,
  active_tools: HashMap<String, ()>,  // activity_id or synthetic
  pending_attention: Option<AttentionReason>, // single sticky ok for v1
}
```

2. `fold_envelopes(events: &[ParsedEnvelope]) -> RunSnapshot` rules (minimal v1):
   - TurnStarted / ActivityStarted(Tool) → Active; insert tool id (use
     activity_id or `"anon-{index}"`).
   - ActivityFinished(Tool) → remove matching tool id; if tools empty and no
     attention and not settled → stay Active only if a TurnStarted happened
     without TurnSettled; if no open tools and turn not settled → Active with
     no activity (still working) OR Unknown — **prefer keep Active/working
     until TurnSettled** (matches agent still in turn).
   - AttentionRequested → Blocked + pending_attention; legacy ask/wait.
   - AttentionCleared / TurnStarted → clear pending_attention.
   - TurnSettled { Failed } → Failed / failed.
   - TurnSettled { Completed|Interrupted|Unknown } → if active_tools non-empty
     keep Active; else Settled / done.
   - SessionClosed → close tools, Settled/done unless Failed already.
   - SessionOpened / ChildStarted → Active/working.
   - Ignore Heartbeat lines if present.
   - Malformed JSONL lines skipped.

3. `project_snapshot(snapshot) -> &'static str`:
   - Failed → failed
   - Blocked + Permission → ask
   - Blocked + other → wait
   - Settled → done
   - Active → working
   - Unknown → do not emit entry (None) so prior can hold — or working if
     tools non-empty. Prefer: Unknown → None from fold path.

4. `agent_status_cache::read_agent_event_entry`:
   - Let stem = path with `.json` stripped.
   - If `{stem}.jsonl` exists and fold yields Some(value), build
     AgentStatusCacheEntry from that value; use max envelope
     received_at for observed_at; source Lifecycle; keep run_id mapping.
   - Else existing `.json` AgentEventSnapshot path.

5. Ensure `from_roots_at` still only iterates `*.json` (not `*.jsonl`) so each
   task yields one entry; fold pulls sibling jsonl by stem.

## Test-first instructions

Red: `cargo test -p ajax-cli agent_event agent_status_cache -- --nocapture`

(Use two invocations if cargo filter is single-string.)

1. `fold_two_tools_one_finish_stays_working` — envelopes: TurnStarted,
   ActivityStarted id=a, ActivityStarted id=b, ActivityFinished id=a →
   project working.
2. `fold_attention_then_tool_finish_keeps_ask` — AttentionRequested Permission,
   ActivityFinished → still ask.
3. `fold_turn_settled_with_open_tool_stays_working` — ActivityStarted +
   TurnSettled Completed → working until tool finished; then if only settle
   after finish → done. (Implement: TurnSettled with open tools → Active.)
4. Cache integration: write jsonl+json via helpers; `status_entries_for_task`
   returns fold-projected ask after attention even if json says working.

Then implement.

## Edit instructions

Pure fold + project in agent_event.rs; thin read change in
agent_status_cache.rs. Reuse existing temp dir test helpers.

## Verification commands

```bash
cargo test -p ajax-cli agent_event
cargo test -p ajax-cli agent_status_cache
cargo clippy -p ajax-cli --all-targets -- -D warnings
cargo fmt -p ajax-cli -- --check
```

## Acceptance criteria

- Open-set fold tests green; cache prefers jsonl fold when present.
- Legacy path still works when only `.json` exists.
- Scope limited to two allowed files.

## Stop conditions

- Need ajax-core live.rs changes.
- Patch > ~400 lines.
- Ambiguous TurnSettled+open-tools rule forces redesign — stop and report.

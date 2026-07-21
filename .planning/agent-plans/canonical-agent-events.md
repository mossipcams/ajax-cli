# Canonical agent events (facts → reducer → operator status)

## Scope

Replace client-specific status strings (`working`/`wait`/`ask`/`done`) with a
stable **canonical event vocabulary**. Per-client adapters only identify native
facts. Ajax’s reducer owns `Running` / `Waiting` / `Idle` / `Error` via an
orthogonal `RunSnapshot`, open activity sets, and capability profiles.

Builds on (does not throw away):

- launch wrapper identity injection + `__agent-event` helper
- `ObservationSource` precedence / run graph / confidence
- `ui_state::derive_operator_status` → `Running|Waiting|Idle|Error`
- `agent_status::reduce_agent_status` / parent–child aggregation

Supersedes the last-event-wins path in `native-event-adapters.md` task 1–2
event files (`{task}.json` with a single `value`).

## Non-goals

- No second operator-status axis (keep `TaskStatus` / `LiveStatusKind` as
  projections of `RunSnapshot`).
- No HTTP/direct transports per client; one helper + spool.
- No restoring broad pane-text activity classification. Pane fallback only for
  capabilities marked `unavailable` / `unverified`, low confidence, short TTL.
- No inventing wait/ask for Cursor/Pi when native coverage is absent.
- No implementing socket/spool in the same round as the event schema (completed in
  Phase 3; socket notify is best-effort, JSONL fold on refresh is durable).

## Approval

**Approved for implementation** 2026-07-21 (“pull from main then delegate
until finished”).

Delegation decision: delegated via model-router (sequential packets).

Synced with `origin/main` at `e2ae4f4` before Phase 0.

## Why current path is wrong

Today `__agent-event` translates native events **directly into status values**:

```text
claude Stop → "done" | "working"
cursor stop → "done"
```

That collapses identification into display, uses last-write-wins on one JSON
file, and cannot represent concurrent tools, sticky attention, or “capability
unavailable.” Cursor/Pi registered as `AgentClient::Other` then lose ProviderHook
fallback, so gaps become silent Unknown/sticky Working rather than explicit.

## Target shape

```text
Client hooks / Pi extension / wrapper
  → adapters (identify only)
  → Canonical event envelope (JSONL log + optional socket)
  → Per-run open-set reducer → RunSnapshot
  → Parent–child aggregation
  → Project Running | Waiting | Idle | Error
```

### Canonical kinds (v1)

`SessionOpened` | `SessionClosed` | `TurnStarted` | `TurnSettled{outcome}` |
`ActivityStarted{activity,id?}` | `ActivityFinished{activity,id?,outcome}` |
`AttentionRequested{reason}` | `AttentionCleared` |
`ChildStarted{child_run_id}` | `ChildSettled{child_run_id,outcome}` |
`Heartbeat`

Supporting: `ActivityKind` (Thinking|Tool|Compaction|Delegation),
`AttentionReason`, `TurnOutcome` (Completed|Interrupted|Failed|Unknown).

Envelope fields: `schema_version`, `event_id`, `task_id`, `run_id`,
`parent_run_id`, `client`, `client_version?`, `client_session_id?`,
`client_turn_id?`, `native_event`, `kind`, `detail`, `occurred_at`,
`received_at`, `source` (`native_hook`|`wrapper`|`pane_fallback`).

### Orthogonal snapshot (not a combo enum)

```text
RunSnapshot {
  liveness, phase, activity?, blocker?, outcome?,
  active_children, attention_required
}
```

Projection rules (sketch):

| Snapshot | Operator |
| --- | --- |
| Failed / outcome Failed | Error |
| Active (tools/thinking/children working) | Running |
| Blocked + attention_required | Waiting (actionable) |
| Blocked on child only | Waiting (non-actionable; existing delegated summaries) |
| Settled + trusted wrapper completion | existing Reviewable / Done path |
| Settled without wrapper | Idle / Done presentation until ack (existing ack) |
| Unknown + alive | Unknown → do not invent Running |

### Open sets

- `active_tools: HashMap<ActivityId, …>`
- `active_children: HashMap<RunId, …>`
- `pending_attention: HashMap<AttentionId, AttentionReason>`

One tool finish ≠ idle; parent Stop ≠ complete while non-detached child active;
duplicates idempotent; late finish recorded without wiping the run; session /
process exit closes remaining opens.

### Capability profile (per run / client)

Declare coverage: `native` | `wrapper` | `pane_fallback` | `unavailable` |
`unverified` for turn_started, turn_settled, permission_wait, question_wait,
subagents, session_closed, …

Absence of an event ≠ absence of state. Cursor marks question/permission
`unavailable`; Ajax must not invent high-confidence Waiting from silence.

### Freshness

- Attention: until clear / new activity / session end (not 120s TTL).
- Child / tool: until settle/finish; degrade to Unknown if heartbeat dies.
- Heartbeat → liveness only.
- Pane hints: short TTL + dwell only.
- New TurnStarted / ActivityStarted clears stale Settled / Waiting.

### Client mapping (install targets)

| Fact | Claude | Codex | Cursor | Pi |
| --- | --- | --- | --- | --- |
| SessionOpened | SessionStart | SessionStart | sessionStart | session_start |
| SessionClosed | SessionEnd | SessionEnd (native; wrapper exit backup) | sessionEnd | session_shutdown |
| TurnStarted | UserPromptSubmit | UserPromptSubmit | beforeSubmitPrompt | before_agent_start |
| Tool activity | Pre/PostToolUse | Pre/PostToolUse | pre/postToolUse | tool_execution_* |
| Permission | PermissionRequest | PermissionRequest | unavailable | extension UI only |
| Question | Notification / Elicitation | limited | unavailable | settle only (ambiguous) |
| Child | SubagentStart/Stop | SubagentStart/Stop | verify CLI | delegation / wrapper |
| Compaction | Pre/PostCompact | Pre/PostCompact | preCompact | session_*compact |
| TurnSettled | Stop | Stop | stop | **agent_settled** (not agent_end) |
| Failure | StopFailure / payload | wrapper / app | postToolUseFailure + wrapper | tool/provider error |
| Liveness | wrapper | wrapper | wrapper | wrapper |

## Phases

### Phase 0 — Contract freeze (docs only)

- [x] Land this plan + `architecture.md` status section rewrite (facts → snapshot →
      projection; capability profiles; pane fallback narrowly scoped).
- [x] Freeze envelope schema_version 1 and enum lists (in this plan).
- [x] Record per-client capability matrix as source of truth for installers.
- [x] Approval: user authorized implement (“delegate until finished”).

### Phase 1 — Canonical log + adapters (no UI change yet)

- [x] **1a** Envelope types + native→kind translate + dual-write JSONL + legacy
      `{stem}.json` projection (readers unchanged). Packet:
      `.planning/packets/canonical-events-1a-envelope.md`
      Delegated cursor/composer-2.5 (GLM rate-limited). Parent gate ACCEPT:
      `cargo test -p ajax-cli agent_event` 11 passed; clippy/fmt OK.
- [x] **1b** Session open/close install + translate. Packet
      `canonical-events-1b-session.md`. Parent ACCEPT: agent_event 12 +
      agent_hooks 3; clippy/fmt OK.
- [x] **2a** Open-set fold over JSONL on read. Packet
      `canonical-events-2a-fold.md`. Parent ACCEPT: agent_event 15 +
      agent_status_cache 13 (incl. prefer-ask over stale json); clippy OK.

**Delegation:** model-router + tdd packet per slice; Pi lanes rate-limited →
cursor-delegate composer-2.5 for all write rounds.

### Phase 2 — Open-set RunSnapshot → core projection

- [x] Fold + project in ajax-cli cache path (2a).
- [x] **2b** Capability profiles (`agent_capability.rs`). Parent ACCEPT: 3 tests.
- [x] **2c** Fold/`RunSnapshot` in ajax-core + `observations_from_run_snapshot`
      feeds `reduce_agent_status`. Parent ACCEPT: 5 canonical_agent_event tests;
      cli agent_event/cache green.

### Phase 3 — Transport

- [x] **3a** JSONL always + best-effort Unix socket notify. Parent ACCEPT:
      `socket_send_delivers_line_when_listener_present`.
- [x] **3b** Web serve binds `agent-events/notify.sock` listener. Parent ACCEPT:
      `listener_accepts_writer_line`; wired in `serve_mobile_web_with_paths`.

### Phase 4 — Narrow pane fallback (capability-gated only)

- [x] **4a** `pane_fallback.rs` wait/ask only; gated by capability; no Busy→Running;
      wired in `runtime_refresh` when lifecycle did not apply. Parent ACCEPT:
      6 pane_fallback tests; live filter 136 passed; clippy OK.
- [x] architecture.md already describes gated exception.

## Validation

```bash
cargo nextest run -p ajax-core
cargo nextest run -p ajax-cli
# focused: agent_event, agent_status, live, runtime_refresh, agent_hooks
npm run verify   # before PR
```

Live smoke (post-merge): one turn each Claude / Codex / Cursor / Pi under
stable profile; confirm event log + operator status + attention webhooks.

## Risks

- Scope creep if socket + pane + schema land together → ship Phase 1–2 first.
- Reintroducing pane classification without capability gates recreates FP sticky
  statuses.
- Cursor subagent hooks unverified — keep `unverified`, do not trust for
  completion.
- Dual write (old value file + new JSONL) during migration; delete old path in
  same PR once readers moved.
- `AJAX_PROFILE=dev` in shell vs stable task cache — operator confusion; not
  fixed by this plan but document in doctor/smoke.

## Deviations

- GLM and MiniMax weekly Go usage limits; implementation via cursor-delegate
  composer-2.5. Structured report YAML often wrapped so `run-delegate` schema
  check failed; parent reviewed delta + re-ran verification (ACCEPT).
- User corrected: Phases 2b–4 are **not** deferred — continue to finish.

## Validation log

- 2026-07-21: fast-forward `origin/main` → `e2ae4f4`.
- Phase 0–4a complete (no deferrals after user correction).
- All implementation rounds: cursor-delegate composer-2.5 (Pi rate-limited).
- Parent gates: focused tests + clippy/fmt per packet as recorded in checkboxes.

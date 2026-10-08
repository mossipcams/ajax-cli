# ACP execution state (ACP events → AcpExecutionState → reduce_agent_status)

## Scope

Today the ACP host collapses session events into a four-value
`SessionActivity` (`web_session/session_activity.rs`) and writes it straight to
the task with `live::apply_authoritative_observation_at`, bypassing
`agent_status::reduce_agent_status`. Tool calls, tests, input-vs-approval
waits, `state_update`, and child work are invisible to status.

Insert one thin, pure `AcpExecutionState` in `ajax-core` between ACP events
and the existing reducer:

```
AcpClientEvent / SessionServerEvent   (ajax-web, ACP v1 or v2)
  → AcpExecutionEvent                 (ajax-core, neutral facts)
  → AcpExecutionState::apply          (ajax-core, pure fold)
  → Vec<StatusObservation>            (source ProviderLifecycle, run ids)
  → reduce_agent_status               (unchanged precedence)
  → StatusProjection → task live status
```

State tracks only: root turn (`Idle | Running | Settled | Failed |
Cancelled`), open tool calls by `toolCallId` (kind, status, tests flag),
pending permission/input requests by id, optional children by run id, and
last-activity timestamp.

## Non-goals

- No change to task authority, freshness/confidence windows, `Unknown`
  semantics, or substrate precedence in `reduce_agent_status`.
- ACP v2 is not required; v1-only harnesses fall back to coarse `Working`.
- No inferred capabilities: children are tracked only when the harness exposes
  explicit lineage.
- No orchestration graph; children are a flat open set keyed by run id.
- No new `LiveStatusKind` / operator-status values.
- Browser does not derive status; it renders the core projection.

## Event mapping (acceptance table)

| ACP event | Effect |
| --- | --- |
| `session/prompt` accepted / in flight | root turn `Running` |
| `tool_call` | create/update tool; absent status → `Pending` |
| `tool_call_update` | merge by `toolCallId`; `in_progress` → Running, `completed` → Completed, `failed` → Failed |
| running `execute` tool | `CommandRunning` |
| running `execute` tool recognised as tests | `TestsRunning` |
| other active tool | `Working` |
| `session/request_permission` | `WaitingApproval` |
| elicitation / structured user-input request | `WaitingInput` |
| permission/input resolved, prompt still active | resume derived active state, else `Working` |
| `state_update: running` | `Working`, structured provider evidence only |
| `state_update: requires_action` | waiting only with a correlated open request; otherwise `Unknown` |
| `state_update: idle` | supporting evidence; never alone `Done` |
| successful prompt response / turn end | root `Settled`; `Done` iff no completion-holding child is open |
| prompt/turn terminal error | `Failed` (blocked) |
| cancel / interrupt | settled, non-running, not failure unless provider reports failure |
| child start/update/end | tracked only with reliable lineage; open completion-holding child prevents parent `Done` |
| message / thought chunks | activity timestamp only; never prove `Working` |
| unknown / malformed | no transition |

## Tasks

- [x] 1. Core: `crates/ajax-core/src/acp_execution_state.rs` — `AcpExecutionEvent`,
      `AcpExecutionState::{apply, observations}`; one test per table row plus a
      `reduce_agent_status` round-trip for Done-held-by-child and
      requires_action-without-request → Unknown.
- [x] 2. Web slice: map `AcpClientEvent`/`SessionServerEvent` to
      `AcpExecutionEvent`; replace `SessionActivityReporter` coarse mapping with
      state → observations → `reduce_agent_status` → task. Keep report
      dedupe, retry, and ACP-capable-task gate.
- [x] 3. Consumers: task cards, TUI, and CLI read core task status, which the
      reducer now feeds (not audited surface by surface). Chat head
      (`features/chat/status/headView.ts`) no longer derives state from raw
      `acpState`: pending decision/elicitation, then turn busy, then task
      waiting/error. Matt approved rewriting the two raw-state test cases
      (2026-10-07). Core `running` is deliberately not fed to the head: an
      existing integration test pins "task running, no turn" to idle.
- [x] 4. Docs: `docs/architecture/task-authority.md` (ACP as a structured
      lifecycle producer through the reducer), `web-session-behavior.md`.
- [x] 5. Gate: fmt, `clippy --all-targets --all-features`, nextest, vitest,
      `pkill -f vite; CI=1 npm run web:smoke` if web UI files change.

## Approval

User requested immediate implementation 2026-10-07 ("Refactor Ajax's ACP
status handling…"). Delegated via model-router, one packet per task.

## Deviations

- Task 2 was split into six single-purpose packets after two whole-task
  delegate rounds timed out with empty diffs.
- `reduce_agent_status` Running arm now projects the specific kind
  (`CommandRunning` / `TestsRunning`, otherwise `AgentRunning`). Required for
  the mapping table; no existing producer emits those kinds. Precedence,
  freshness, confidence, Unknown, and child aggregation untouched.
- Request resolved with no recorded prompt promotes the turn to Running
  ("otherwise Working"); one task-1 test assertion
  (`resolution_without_tools_resumes_only_an_active_turn`) was corrected.
- `state_update: running` promotes only an Idle turn; it never reopens a
  Settled/Cancelled turn (prevents a stuck "Agent working").
- Web session stream has no child/subagent events and no separate tool
  command field: children are unfed from ACP, tests detection uses the title.
- `acp_execution_state.rs` split into `acp_execution_state.rs` (types, fold)
  and `acp_execution_state/observe.rs` (projection) to satisfy the 600
  changed-lines-per-file gate; tests stay inline in both.
- Shipped as two stacked PRs (core, then web on top) because the single PR
  was 1,723 changed source lines against the 1,500 gate.
- `SessionActivity` kept as the report contract (three variants added) so no
  file under a `tests/` directory changed; human summaries unchanged.

## Validation

2026-10-07/08, on the final tree (parent-run, not delegate-reported):

- `cargo fmt --check` — pass
- `cargo clippy --all-targets --all-features -- -D warnings` — pass
- `cargo nextest run --workspace` — 2471 passed, 0 skipped
- `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace --all-features` — pass
- `npm run web:check`, `npm run web:lint` — pass
- `vitest run` (full) — 1533 passed, 9 skipped
- `pkill -f vite; CI=1 npm run web:smoke` — 152 passed, 2 skipped
- `npm run verify` — passed in the delegate's run before the file split

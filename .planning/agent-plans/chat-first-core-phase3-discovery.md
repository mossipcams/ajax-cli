# Phase 3 discovery: one chat status source (factual map)

Scope: read-only map for `.planning/agent-plans/chat-first-core.md` Phase 3 ("one chat status source"). Paths are relative to the repo root. Line numbers were read from this worktree (`ajax-architectural-flaws`, branch `ajax/architectural-flaws`, HEAD `7cd78734`). Items marked **SPECULATION** are design inferences, not facts. Where the plan text disagrees with the code, the code is cited and the drift is listed in section 9.

Shorthand:
- `web_session/` = `crates/ajax-web/src/slices/web_session/`
- `runtime/` = `crates/ajax-web/src/runtime/`
- `cockpit/` = `crates/ajax-web/src/slices/cockpit/`
- `core/` = `crates/ajax-core/src/`
- `web/` = `crates/ajax-web/web/src/`

---

## 1. Every path by which chat activity reaches the registry today

### 1.1 Summary

Chat activity is not written to the registry by the session actor directly. It goes through a callback that the runtime wires into `TaskSessionDirectory`, and the callback applies the observation to the in-memory `Task` and then rewrites the whole registry snapshot. There are two layers of queueing in front of the write:

1. `try_report_session_activity` retries the callback up to 3 times (`web_session/session_activity.rs:85-100`).
2. When the control lane is busy, the callback enqueues into a **deferred queue** and a drainer thread applies it later (`runtime/state.rs:423-503`).

### 1.2 Step-by-step path (normal event)

| # | Step | Location |
|---|---|---|
| a | Actor receives a committed transcript event, after the transcript append succeeds | `web_session/task_session.rs:244-278` (`append_to_log` calls `report_committed_events` at :278) |
| b | `report_committed_events` flushes any pending report, then feeds each event to the reporter | `web_session/task_session.rs:220-242` |
| c | `SessionEvidence::report_activity_for_event_with_probe` | `web_session/session_evidence.rs:93-106` |
| d | `SessionActivityReporter::activity_for_event_with_probe` → `execution_event` → `AcpExecutionState::apply`, then `reduce_activity` | `web_session/session_activity.rs:130-148`, `:206-244` (reducer: `ajax_core::agent_status::reduce_agent_status`) |
| e | `SessionEvidence::report` calls the runtime callback (`try_report_session_activity`, up to 3 attempts); on success `activity_reporter.commit(activity)`; on failure sets `activity_report_fault` and `pending_activity_report` | `web_session/session_evidence.rs:108-124`, `web_session/session_activity.rs:85-100` |
| f | The callback is `report_or_defer_task_session_activity`, installed by `wire_session_activity_reporter` | `runtime/state.rs:505-519`, `:423-458` |
| g | If the deferred queue is empty, `report_task_session_activity` → `block_in_place` + `catch_unwind` → `try_lock` on `control_lane` (busy → `CONTROL_LANE_BUSY_ERROR`) | `runtime/state.rs:326-345`, `:347-362`, `:48` |
| h | Under the lane: `record_task_session_activity_under_lane` → `apply_task_session_activity(reload=false)`, and on error retries with `reload=true` (`reload_registry_from_disk`, then `cleanup_unowned_sessions`) | `runtime/state.rs:364-375`, `:377-418` |
| i | `record_session_activity` finds the task by qualified handle (must be `skip_interactive_agent()` and ACP-launchable), then calls `live::apply_authoritative_observation_at(task, activity.observation(), now)` | `web_session/session_activity.rs:55-83`, `:24-52` (observation mapping `:28-52`) |
| j | `commit_durable`: revision check against `base_revision`, then `persist_registry_snapshot`, then swap `guard.context`, bump `guard.revision`, clear `cockpit_cache` | `runtime/state.rs:377-418` (calls :401), `:634-652` |
| k | `persist_registry_snapshot` (CLI bridge) → `persist_changed_state` → `save_context_with_state` → `SqliteRegistryStore::save_if_revision` → `save_registry` | `crates/ajax-cli/src/web_backend.rs:369-373`, `:435-446`; `crates/ajax-cli/src/context.rs:236-283`; `core/registry/sqlite/mod.rs:117-165`; `core/registry/sqlite/save.rs:28-124` |

`save_registry` deletes and reinserts **every** registry table (`save.rs:33-67`), then reinserts every task and every event (`save.rs:69-98`). Per-activity cost is therefore whole-registry, which is the Context item 3 problem in the plan.

### 1.3 The deferred activity queue (to be removed by Phase 3)

- Type: `DeferredActivityReports = Mutex<VecDeque<(String, SessionActivity)>>` — `runtime/state.rs:50-52`.
- Pending predicate: `deferred_activity_pending` — `runtime/state.rs:54-64`.
- Enqueue: `report_or_defer_task_session_activity` pushes to the back when the queue is non-empty or the lane is busy — `runtime/state.rs:423-458` (push at :452-457).
- Drainer: `spawn_deferred_activity_drainer` — a `std::thread` that takes `control_lane.blocking_lock()`, applies the front item with `record_task_session_activity_under_lane`, then pops it — `runtime/state.rs:462-503`.
- Directory plumbing: `TaskSessionDirectory.deferred_activity_pending` field `web_session/task_session_directory.rs:47`, setter `:71-73`, read for spawn `:174-176`, passed to `spawn_task_session` in `ensure_entry` `:179-207` (:195) and `ensure_entry_for_acquire` `:209-244` (:232).
- Consumer: `spawn_task_session` reads `deferred_activity_pending` to decide whether to settle a persisted-running chat on first attach — `web_session/task_session.rs:426-432` via `first_attach_activity` (`web_session/session_activity.rs:103-108`).
- Type alias `DeferredActivityPending` — `web_session/task_session_directory.rs:14`.

### 1.4 Retry of a failed report (pending report + fault)

- Pending: `SessionEvidence.pending_activity_report: Option<SessionActivity>` — `web_session/session_evidence.rs:9`.
- Retry on every actor poll tick (50 ms interval at `task_session.rs:434`, retry call at :454), before each `pump` (`task_session.rs:297-300`), before collecting outbound (`web_session/task_session_outbound.rs:40`), and on actor shutdown (`task_session.rs:465`).
- Fault surfaced to the client as `transcript_error` through the attach/outbound snapshot — `session_evidence.rs:43-47`, `task_session_outbound.rs:24`, `:56-60`, `:118`.

### 1.5 Other chat-path activity sources (all end at the same callback)

| Source | Activity sent | Location |
|---|---|---|
| Startup recovery of an interrupted turn | `TurnFailed` for recovered handles that have no live actor and no run this boot | `web_session/task_session_directory.rs:143-157` (`report_recovered_turns`) |
| First attach of a persisted-running chat | `TurnFailed` if the registry says running and no deferred report is pending | `web_session/task_session.rs:426-432`; `web_session/session_activity.rs:103-108`; predicate `web_session/startup_recovery.rs:151-161` (`persisted_chat_running`) |
| Actor exit mid-turn (not close) | `TurnFailed` if the last reported activity was running | `web_session/task_session_exit.rs:24-31`, called at `task_session.rs:492-494` |
| Prompt acceptance | `TurnStarted` through `PromptAccepted` in the reducer | `web_session/acp_execution_map.rs:11`, `web_session/session_activity.rs:246-254` (`commit`) |
| Turn end (`stop_reason`) | `TurnEnded` / `TurnFailed` / cancelled through `execution_event` | `web_session/acp_execution_map.rs:47-53` |
| Background child held/released | `BackgroundRunning` via `set_background_child` | `web_session/session_activity.rs:186-204`; probe at `task_session.rs:229-231` |

### 1.6 Task fields that the activity path sets

Applied by `apply_authoritative_observation_at` → `apply_reduced_observation` (`core/live_application.rs:26-46`, `:94-192`) and `core/live.rs` re-exports:

| Field | Set to | Location |
|---|---|---|
| `live_status` | `Some(observation)`; `None` only for `Unknown` | `core/live_application.rs:181`, `:99-100`, `:113-...` |
| `live_status_observed_at` | `Some(observed_at)`; cleared for `Unknown` | `core/live_application.rs:182` |
| `agent_status` | Running / Waiting / Blocked / Done / Dead / NotStarted, per kind | `core/live_application.rs:113-170` |
| side flag `AgentRunning` | add for running kinds; remove for waiting/blocked/done | `core/live_application.rs:118-121`, `:133`, `:142`, `:147`, `:156`, `:164`, `:172-173` (remove) |
| side flag `NeedsInput` | add for waiting (non-delegated) and blocked; remove for running and done | `core/live_application.rs:123`, `:127-131`, `:141`, `:173` |
| side flag `TestsFailed` | add only for `CiFailed`; `CiPending` removes | `core/live_application.rs:146`, `:151` |
| side flag `Conflicted` | `MergeConflict` only; cleared by `clear_recovered_live_flags` | `core/live_application.rs:153-156`, `:214-223` |
| `last_activity_at` | `SystemTime::now()` for refreshing kinds (running, waiting, blocked, shell-idle, rate-limited) | `core/live_application.rs:186-188`, `:225-` |
| side flag `Stale` | removed when activity refreshes | `core/live_application.rs:189` |
| agent attempts (open/closed) | `sync_open_attempts` by `agent_status` | `core/live_application.rs:190`; `core/models/observations.rs:137-...` |
| `lifecycle_status` | → `Active` for AgentRunning / CommandRunning / TestsRunning (through `transition_lifecycle`) | `core/live_application.rs:33-45`; `core/lifecycle.rs:34` |

Not set by the activity path: `lifecycle_status` → Reviewable (only the trusted path does this, `core/live_application.rs:57-75`), metadata, attention ack, PR state.

### 1.7 Other chat-path snapshot writes (not activity, but they rewrite the whole snapshot)

- Model change: `persist_task_session_model` → `persist_task_session_model_on_control_lane` — `runtime/state.rs:521-563` (control-lane persist at :539).
- Chat attach from snapshot: `prepare_task_session_attach` / `attach_task_session_from_snapshot` — `runtime/state.rs:565-632`, persist at :621, revision bump at :629.
- Agent notification delivery (chat tasks via `web_session::deliver_agent_notification`) and push marks: `runtime/task_routes/cockpit.rs:~285-330` (delivery), `:220-265` (`deliver_and_record`, persist at :254, reached only when `notified || pushed`).
- Push dedupe state (`last_notified_status`, `last_notified_at`, `notify_quiet_since`, `notify_candidate_since`) is written into `task.metadata` by `take_attention_transition` (`core/attention.rs:8-11`, `:30-84`), so push also depends on the snapshot write.

---

## 2. The 1 s convergence polling

### 2.1 Where it is

- **Client only.** `web/src/features/task-workspace/TaskWorkspace.tsx:23` defines `SESSION_CONVERGE_MS = 1000`.
- `sessionConverging = mode === "chat" && sessionBusy && taskDetail?.status !== "running"` — `TaskWorkspace.tsx:79-80`.
- While `sessionConverging`, `setInterval(() => onMutated?.(), SESSION_CONVERGE_MS)` — `TaskWorkspace.tsx:82-86`.
- `onMutated` for the chat route is `reload` from `useTaskDetailResource` (`app/routes/TaskWorkspaceRoute.tsx`, prop `onMutated={reload}`; `App.tsx:609-640` passes `reload`).
- `reload` refetches `GET /api/tasks/{handle}` through `fetchDetail` (`web/src/shared/lib/api.ts:243-246`; query at `web/src/features/task/useTaskDetailResource.ts:37-48`).
- A busy edge also triggers one immediate `onMutated` — `TaskWorkspace.tsx:200-206` (`onSessionActivity` handler), which is fed by `ChatSurface` (`web/src/features/chat/ChatSurface.tsx:56`, call at :312).

### 2.2 What it converges

- Converges the **registry-derived** `BrowserTaskDetail.status` (from `/api/tasks/{handle}`) toward the **stream-derived** `session.turn.busy` (WebSocket transcript, client reducer `web/src/features/chat/session/turnProjection.ts`).
- Stops when the server status becomes `running` (the effect's guard at :80), or when the stream is not busy.
- The interval exists because the registry write for an activity can be deferred (section 1.3), so the detail endpoint lags the WS stream.

### 2.3 Who reads the result

- `TaskWorkspace` passes `taskAttention = {status, explanation}` from the detail to `ChatSurface` (`TaskWorkspace.tsx:100-102`, `:188`), which passes it to `ChatLiveHead` → `buildHeadView` (`web/src/features/chat/status/headView.ts:75-`) → `headState` (`headView.ts:39-47`).
- `TaskWorkspace` also uses `sessionBusy` for `harnessSwapDisabled` (`TaskWorkspace.tsx:136`).
- The detail's `status` is also used by the redirect effect (`TaskWorkspace.tsx:72-77`) only for the terminal-redirect check.

### 2.4 Other timers on the same path (for context, not the 1 s loop)

| Timer | Period | Location |
|---|---|---|
| `useTaskDetailResource` refetch | 10 s (`REFRESH_INTERVAL_TERMINAL_MS`) | `web/src/features/task/useTaskDetailResource.ts:47`; `web/src/shared/lib/polling.ts:2` |
| App cockpit poll | 3 s active (`REFRESH_INTERVAL_ACTIVE_MS`), policy-dependent | `web/src/app/App.tsx:466-471`; `web/src/shared/lib/polling.ts:1`; `cockpitRefreshIntervalMs` |
| Chat activity-age tick while busy | 30 s | `web/src/features/chat/session/useChatSession.ts:139-141` |
| Presence heartbeat | `PRESENCE_HEARTBEAT_MS` | `web/src/shared/lib/presence.ts:45-70` |
| Server push tick | 30 s (`DEFAULT_PUSH_POLL_SECONDS`) | `crates/ajax-web/src/slices/push.rs:27`; `runtime/mod.rs:203-222` |
| Server ownership heartbeat | `HEARTBEAT_EVERY` | `runtime/mod.rs:189-199` |
| Session actor poll | 50 ms | `web_session/task_session.rs:434` (`tokio::time::interval(50ms)`, poll tick :435-470) |

There is **no 1 s server loop**.

### 2.5 Tests that pin the convergence

- `web/src/features/task-workspace/TaskWorkspace.test.tsx:310` `describe("session busy convergence")`, with four cases:
  - :341 "refetches every second while the stream is busy and the server status is not running"
  - :354 "stops interval refetches once the server status reports running"
  - :367 "keeps refetching a waiting status while the stream is busy, then stops once running"
  - :385 "does not interval-refetch when the stream is not busy"
- The busy-edge helper `chatSurfaceCapture.latest?.onSessionActivity` is at :21 and :225, :338.

---

## 3. How cockpit / dashboard status for a chat task is computed today

### 3.1 Core pure functions (ajax-core)

| Function | Location | Inputs read from `Task` |
|---|---|---|
| `ui_state::derive_operator_status(&Task) -> OperatorStatus` | `core/ui_state.rs:67-69` → `derive_task_status` `:119-239` | `lifecycle_status`; `runtime_projection.observation_error`; missing-substrate flags; `git_status` (checkout mismatch, `canonical_checkout_mismatch_explanation` `:304`); `live_status` (kind, summary) and `live_status_observed_at`; `side_flags` (`TestsFailed`, `Conflicted`, `AgentRunning`, `NeedsInput`, `AgentDead`); `agent_status`; `attention_acknowledged_at`; `last_activity_at` (workflow boundary `:255-258`); metadata `agent_process_alive_at` (`:248-251`); `skip_interactive_agent()` (`:207-209` in the idle/unknown fallback) |
| `ui_state::attention_band(&OperatorStatus, LifecycleStatus) -> AttentionBand` | `core/ui_state.rs:26-47` | derived status + lifecycle |
| canonical label maps | `core/ui_state.rs:263-302` (`live_running_explanation`, `canonical_waiting_explanation` (`Done` → "Response ready", soft) `:282-292`, `canonical_error_explanation` (`Blocked` → "Agent blocked") `:294-302`) | live kind |
| `attention::annotate(&Task) -> Vec<Annotation>` | `core/attention.rs:212-` | tasks' flags, live, lifecycle, git, runtime |
| `recommended::operator_action(&Task)` / `available_operator_actions(&Task)` | `core/recommended.rs:19`, `:188` | task state (drives `primary_action`; note the memory says never order by it) |
| `remediation::remediations_for_task(&Task)` | `core/remediation.rs` (called from `projection.rs:97`) | live kind `CiFailed` / `MergeConflict` (`remediation.rs:58-66`) |
| `commands::projection::task_card(&Task) -> TaskCard` | `core/commands/projection.rs:90-107` | `derive_operator_status`, `operator_action`, `annotations_for_task`, `remediations_for_task`, `attention_band` |
| `commands::projection::task_summary(&Task)` | `core/commands/projection.rs:67-88` | same derived status plus `needs_attention = !annotate(task).is_empty()` |
| `commands::cockpit_view(&CommandContext) -> CockpitView` | `core/commands.rs:243-` → `build_cockpit_projection` → `inbox_from_cards` (`projection.rs:110`) | all tasks; `is_cockpit_menu_task` (`projection.rs:63`) |
| `commands::cockpit_projection` | `core/commands.rs:229-241` | same |
| `agent_status::reduce_agent_status(ReduceInput) -> projection` | `core/agent_status.rs` (used by the reporter, `web_session/session_activity.rs:206-214`) | `primary_run_id`, `observations` from `AcpExecutionState::observations`, `process_liveness` |

### 3.2 ajax-web responses (what the browser receives)

- `slices/cockpit/mod.rs:47-58` `browser_cockpit_view(&context)` → `commands::cockpit_view` + `browser_task_card` (`cockpit/mod.rs:79-94`) → `status`, `status_explanation`, `attention`, `session_capable` (`cockpit/mod.rs:98-105`), `browser_actions` (`slices/actions.rs`).
- `slices/cockpit/mod.rs:137-196` `browser_task_detail_view(&context, handle)`: same card by handle, plus `live_status_kind`, `live_status_summary`, git, tmux, annotations, attempts.
- Routes: `runtime/mod.rs:108` `GET /api/cockpit` → `axum_cockpit` (`runtime/task_routes/cockpit.rs:71-97`) → `cached_cockpit_response` (`runtime/state.rs:822`; TTL 750 ms, `state.rs:42`) or `refresh_cockpit_and_cache_locked` (`cockpit.rs:137-202`, which calls `bridge.refresh_cockpit` (tmux/git/gh probe) and then `cockpit_response`) or `current_projection_response` (`cockpit.rs:99-103`).
- `runtime/mod.rs:112` `GET /api/tasks/{handle}` → `axum_task_get` (`cockpit.rs:354`) → `axum_task_detail` (`cockpit.rs:336-352`) → `browser_task_detail_view` from the **in-memory shared context** (`state.shared().context`), no refresh.
- Chat-only sub-routes (`/session`, `/session/transcript`) are in `axum_task_get` (`cockpit.rs:360-405`).

### 3.3 Push notification path (attention → push tick)

- Trigger: `refresh_cockpit_and_cache_locked` → `deliver_and_record(state, deliver_notifications)` (`cockpit.rs:~178`, `:220-265`).
- Push tick: `spawn_push_tick` every 30 s runs `refresh_cockpit_and_cache(tier=Full, deliver_push = !browser_connected && has_subscriptions)` (`runtime/mod.rs:203-229`; `background_refresh_plan` `:224-229`).
- `browser_connected` uses `BROWSER_CONNECTED_TTL = 45 s` (`runtime/state.rs:43`).
- `slices/push.rs:263-311` `deliver_attention_pushes(&mut context, &hub)`: for every task calls `attention::take_attention_transition(task)` (`push.rs:291`).
- `core/attention.rs:30-84` `take_attention_transition_at`:
  - inputs: `lifecycle_status`, `derive_operator_status(task)`, `is_actionable_attention` (`attention.rs:107-109`), `should_suppress_error_attention` (`:111-`), metadata dedupe keys `LAST_NOTIFIED_STATUS_KEY` / `NOTIFY_QUIET_SINCE_KEY` / `NOTIFY_CANDIDATE_SINCE_KEY` (`attention.rs:8-11`), dwell `NOTIFY_CONFIRMATION_DWELL = 15 s` (`attention.rs:15`, `confirm_notify_candidate` `:162-`).
  - side effect: writes `task.metadata` (candidate, last-notified stamp and time).
- Non-transition statuses clear the candidate and quiet-episode keys (`attention.rs:73-81`).
- Result: the push decision reads `derive_operator_status`, i.e. the same registry-derived status as the cockpit, and it persists its dedupe state into the registry metadata.

---

## 4. What `TaskSession` / `TaskSessionDirectory` already know in memory

### 4.1 Per-session state (`TaskSessionState`, `web_session/task_session.rs:152-168`)

| Knowledge | Field | Location |
|---|---|---|
| Root turn state (Running / Settled / Cancelled / Failed), observations, background child | `evidence.activity_reporter.state` (`AcpExecutionState`) and `turn_running()` / `turn_finished()` / `background_held()` | `web_session/session_activity.rs:120-175` |
| Last reported activity (`SessionActivity`, incl. `last_reported_running`) | `evidence.activity_reporter.last` | `web_session/session_activity.rs:115-120`, `:165-172` |
| Activity report pending / failed | `evidence.pending_activity_report`, `evidence.activity_report_fault` | `web_session/session_evidence.rs:9-12` |
| Transcript durability fault, corruption | `evidence.transcript_durability_fault`, `evidence.transcript_corruption` | `web_session/session_evidence.rs:13-17`, `:43-52` |
| Active prompt (client id, terminal state, cancel requested) | `prompts.active_prompt: Option<ActivePrompt>` | `web_session/prompt_queue.rs:13-21`, `:59-69` |
| Queued prompts | `prompts.queued: VecDeque<QueuedPrompt>` | `web_session/prompt_queue.rs:59-64` |
| Busy | `prompts.busy(client_busy)` where `client_busy = acp.client.prompt_in_flight()` | `web_session/task_session.rs:288-295`, `web_session/prompt_queue.rs:71-` |
| Ledger unusable reason | `prompts.ledger_unusable` | `web_session/prompt_queue.rs:64` |
| Pending exit interruption | `prompts.pending_exit_interruption` | `web_session/prompt_queue.rs:65` |
| Pending permission / elicitation | derived from transcript log (`pending_permission(&log)`, `pending_elicitation(&log)`) | `web_session/task_session_outbound.rs:88-89` (`snapshot`), `web_session/replay.rs` |
| Background level (last turn-end probe) | `background: BackgroundLevel` | `web_session/background_jobs.rs:60-` (`record_turn_end`) |
| Generation, holders (for eviction), last released | `generation`, `holders`, `last_released` | `web_session/task_session.rs:156-160` |
| Worktree path, agent | `worktree_path`, `agent` | `web_session/task_session.rs:161-163` |

### 4.2 Directory state (`TaskSessionDirectory`, `web_session/task_session_directory.rs:42-50`)

- `sessions: Mutex<HashMap<String, SessionEntry>>` where each entry has only `command_tx`, `join_handle`, `last_released` (`:36-40`). **No status is cached in the directory.**
- `report_activity`, `deferred_activity_pending` callbacks (`:46-47`).
- `command_tx(handle)` (sync, mutex-guarded) returns the sender only (`:739-745`). `has_live_entry` (`:747-749`) is sync existence only.

### 4.3 Existing read accessors (all go through the actor, which is async)

- Every actor-state read is an async command: `send_command` (`directory.rs:16-34`), used by `attach_snapshot` (`:585-604`, `TaskSessionCommand::AttachSnapshot` `task_session.rs:~65-70`), `collect_outbound` (`:629-655`), `eviction_snapshot` (`:752-756`), `submit_prompt`, `cancel`, etc.
- `attach_snapshot` already returns `busy`, pending permission/elicitation, `transcript_error`, `prompts_blocked` (`task_session_outbound.rs:8-25`, `:96-120`). That is the closest existing "status" read, but it is a full attach snapshot, not a status, and it needs an async round trip.
- `detached_attach_snapshot` (`directory.rs:606-627`) and `collect_outbound` fallback (`:641-655`) read the **disk** transcript when no actor is live. They have no status fields.
- `unfinished_prompt_count` (`directory.rs:106-121`) reads the prompt ledger from disk.
- `eviction_snapshot` at `directory.rs:727-731` is `#[cfg(test)]`.

**Conclusion:** no synchronous in-memory status accessor exists. A cockpit handler (`std` sync `current_projection_response` at `cockpit.rs:99`, and `browser_task_detail_view` inside `state.shared()`) cannot read live turn state today without either an async command or a new published cell. `runtime/task_routes/cockpit.rs:~303-310` already uses `tokio::runtime::Handle::current().block_on(...)` to call into the directory from sync code, so that pattern exists.

---

## 5. Chat head vs cockpit rows: divergence

| Surface | Endpoint / source | Status fields | Refresh |
|---|---|---|---|
| Cockpit rows (dashboard) | `GET /api/cockpit` → `cockpit_view` | `status`, `status_explanation`, `attention` from `derive_operator_status` (`cockpit/mod.rs:79-94`) | 750 ms server cache TTL; App poll 3 s active / policy; push tick 30 s |
| Chat head (task workspace, mode chat) | `GET /api/tasks/{handle}` → `browser_task_detail_view` (`cockpit/mod.rs:137`) for `taskAttention`; **plus** WS `session.turn.busy` (client reducer) | `taskAttention.status` / `explanation` from the registry card; `busy` from the stream | detail 10 s + the 1 s convergence (`TaskWorkspace.tsx:82-86`) + busy-edge reload (:200-206) |
| Terminal task view | `GET /api/tasks/{handle}` + terminal polling | same registry card | detail 10 s |

Divergences, from the code:

1. The head state is **two-source**: `headState(decision, elicitation, busy, taskAttention)` (`web/src/features/chat/status/headView.ts:39-47`) gives `working` when `busy` is true and the registry status is `idle`/`unknown`, and `working` when the registry says `running`, regardless of the stream. The cockpit row shows only the registry status. Example: the stream is busy and the registry says `idle` (activity still deferred) → head "Working", cockpit row "Idle". This is the case the 1 s loop converges.
2. The chat's `workingText` uses the registry `explanation` (`headView.ts:94-96`), so the explanation text follows the registry, not the stream.
3. Push and cockpit both read `derive_operator_status`, so they agree with each other; the head is the only surface that reads the stream.
4. The head's `harnessSwapDisabled` uses the stream `sessionBusy` (`TaskWorkspace.tsx:136`), not the registry.
5. Confirmed by the test names in `headView.test.ts` ("busy stream + idle/done server -> working", "falls back to the stream busy flag when task detail is unavailable") and `LiveHead.test.tsx:36` ("maps session busy to working").

---

## 6. Tests that pin current behavior that Phase 3 would change

Convention: "tests/ dir" means the file is under a `tests/` directory. CLAUDE.md says those must not be modified. Inline and sibling `*_tests.rs` files are editable, but no assertion may be deleted or weakened (AGENTS.md, CLAUDE.md "Test Rules").

### 6.1 Files under a `tests/` directory (cannot be modified)

| Test | File:line | Pins |
|---|---|---|
| `report_task_session_activity_fails_fast_when_control_lane_busy_issue_1083` | `crates/ajax-web/src/runtime/tests/suite_5.rs:286` | Deferred queue behavior: reports fail fast when lane busy (expects `CONTROL_LANE_BUSY` error at `:331`) |
| `issue_1227_session_activity_fails_when_registry_is_not_saved` | `crates/ajax-web/src/runtime/tests/suite_6.rs:79` | Activity write is a durable snapshot save that can fail |
| `golden_table_records_cross_layer_disagreements_without_fixing_them` and rows `row_acp_turn_ended` | `crates/ajax-core/tests/wave0_run_state_characterization.rs:224-268`, `:164-207` | `Done` between turns: stored `agent_status` Done, operator status **Waiting** (asserted at `:265`), reducer `FullyCompleted` |
| `wave2_agent_status_assignments_are_allowlisted`, `wave2_allowlist_covers_known_inventory_writers` | `crates/ajax-core/tests/wave2_live_apply_contract.rs:97`, `:129` | Only `live_application.rs`, `registry/sqlite/row_codec.rs`, `models/task.rs`, `commands/teardown/drop_observation.rs` may assign `agent_status` (any new projector must not assign it) |
| `response ready` explanation test | `crates/ajax-core/src/runtime_refresh/tests/suite_3.rs:423-439` (in a `tests/` dir) | `Response ready` explanation |
| `background_loops_start_push_tick_once` is **not** in `tests/`; see 6.2 | | |
| SQLite schema-version assertions (7 sites, expect `10`) | `crates/ajax-core/src/registry/sqlite/tests/suite_2.rs:331,418,774`, `suite_3.rs:29,271,301`, `execution_store.rs:574` (`PRAGMA user_version` = 10) | Any new migration (schema 11) breaks these. See section 7 (P3.5). |
| `execution_drop.rs` (CLI drop by snapshot omission) | `crates/ajax-core/src/registry/sqlite/tests/execution_drop.rs` | Snapshot-omission drop (already a noted deviation in the plan) |
| CLI web backend tests mentioning activity strings | `crates/ajax-cli/src/web_backend/tests/suite_1.rs`, `crates/ajax-cli/src/lib/tests/suite_3.rs`, `crates/ajax-cli/src/lib/tests/suite_13.rs:246` (deferred drop restore) | Persist-on-change and status strings (not read in full; **UNVERIFIED** which assertions) |

### 6.2 Inline or sibling test files (editable, assertions must stay)

Reporter and session evidence:
- `crates/ajax-web/src/slices/web_session/session_activity.rs` inline `mod tests` (`:286-`, 24 tests): `a_turn_in_flight_makes_the_task_read_as_running` (:290), `an_ask_makes_the_task_read_as_waiting` (:305), `the_turn_ending_clears_the_running_state` (:320), `an_error_outside_a_turn_is_not_a_stopped_agent` (:342), `an_error_during_a_turn_stops_the_agent` (:352), `an_unchanged_state_reports_once` (:367), `an_interactive_task_refuses_acp_evidence` (:381), `prompt_acceptance_is_the_turn_starting` (:401), `a_turn_ends_done_and_an_errored_turn_ends_blocked` (:431), `detail_inside_a_turn_reports_nothing` (:447), `a_cancelled_turn_ends_not_fails` (:550), `off_turn_events_report_nothing` (:565), `a_repeated_prompt_acceptance_reports_once` (:586), `an_error_after_a_recovered_turn_fails_it` (:596), `a_running_status_starts_the_turn` (:644), `an_idle_status_never_marks_the_turn_done` (:679), `an_off_turn_idle_status_reports_nothing` (:701). These pin the reducer's `TurnStarted/TurnEnded/TurnFailed/AwaitingOperator` outputs and `status_of` derive results (`:55-` helper `status_of`, `derive_operator_status`).
- `crates/ajax-web/src/slices/web_session/session_evidence.rs` inline `mod tests` (`:127-275`): `activity_report_failure_does_not_set_transcript_durability_fault` (:129), `successful_flush_clears_activity_report_fault` (:152), `issue_1136_deferred_turn_start_is_not_replayed_over_a_reported_turn_end` (:172), `prompts_blocked_is_set_only_for_the_durability_fault` (:249), `clearing_the_activity_report_fault_republishes_the_transcript_error_snapshot` (:265).
- `crates/ajax-web/src/slices/web_session/session_activity_directory_tests.rs` (13 tests): `issue_1069_append_path_clears_agent_working_without_websocket` (:64), `issue_1069_failed_report_retries_turn_end_on_next_append` (:105), `issue_1132_deferred_turn_end_retries_on_session_poll_without_later_append` (:164), `issue_1176_turn_interrupted_by_restart_clears_agent_working` (:214), `record_reports` (:262), `issue_1439_actor_ending_mid_turn_clears_agent_working` (:320), `issue_1439_actor_ending_settled_reports_nothing_extra` (:356), `issue_1439_first_acquire_settles_a_running_card_with_nothing_running` (:389), `issue_1439_first_acquire_of_an_idle_card_reports_nothing` (:449), `issue_1439_running_chat_without_dispatching_prompt_settles_on_startup` (:476), `issue_1439_recovered_handle_with_a_live_actor_is_not_settled` (:520), `issue_1475_first_attach_waits_for_a_deferred_report` (:569), `issue_1439_dropping_a_chat_mid_turn_reports_no_settle` (:630). Several of these assert registry `task_status` after a snapshot write (helper `task_status` `:49`).
- Other web_session tests referencing the reporter or activity: `task_session_reliability_tests.rs` (15), `task_session_idle_eviction_tests.rs` (10), `background_jobs_tests.rs` (16), `background_jobs_live_tests.rs` (3), `execution_ownership_tests.rs` (18), `startup_recovery_tests.rs` (`persisted_chat_running` cold-start cases). Individual names not enumerated here (**UNVERIFIED** per-test impact).

Runtime (sibling `_tests.rs`, not under `tests/`):
- `crates/ajax-web/src/runtime/activity_report_tests.rs`: `deferred_activity_reports_apply_in_order_once_control_lane_frees` (:34), `unknown_handle_report_is_rejected_without_queueing` (:105), `free_control_lane_applies_activity_synchronously` (:130), `deferred_activity_pending_matches_only_queued_handles` (:158), `deferred_activity_stays_pending_until_it_is_applied` (:172), `activity_report_recovers_when_disk_diverged_from_memory` (:274). Five of these pin the deferred queue directly.
- `crates/ajax-web/src/runtime/refresh_lane_tests.rs`: `session_activity_report_succeeds_while_probe_runs` (:90), `background_loops_start_push_tick_once` (:74; pins exactly 3 background tasks: push flusher, one push tick, heartbeat).

Core (inline):
- `crates/ajax-core/src/live_application.rs` inline (`:259-722`): `busy_task_applies_waiting_observation_immediately` (:542), `non_busy_task_applies_waiting_immediately` (:560), `trusted_waiting_bypasses_gate` (:576), `retract_stale_agent_running_sets_unknown_without_closing_attempts` (:685).
- `crates/ajax-core/src/attention/tests.rs`: `response_ready_waiting_does_not_notify` (:725), `idle_to_waiting_fires_once` (:368), `notify_debounce_holds_then_fires_once` (:792), `running_and_idle_never_fire` (:613), `delegated_waiting_does_not_notify` (:649); plus `attention/poll_notify_tests.rs`.
- `crates/ajax-core/src/ui_state/tests.rs` (sibling) and `crates/ajax-core/src/commands/projection.rs` inline tests (`:278-`): `task_card_carries_canonical_status_and_explanation` (:393), `task_card_names_probe_failure_instead_of_showing_unknown_or_idle` (:411).

Architecture guards (inline, editable, but they bound where code can go):
- `crates/ajax-web/src/architecture_web_session.rs:158` `runtime_production_does_not_import_session_internals` (forbids `web_session::task_session::`, `acp_drain`, `acp_map`, `web_session_store` in `runtime/` production code).
- `crates/ajax-web/src/architecture.rs:121-135` `SLICES` and `:243` `each_web_slice_is_isolated_from_sibling_slices_and_runtime`: no slice may depend on a sibling slice or on `runtime`. So `slices/cockpit` and `slices/push` cannot call `slices/web_session` to read the live status; the status must be passed in by the runtime, or come from ajax-core types.

Web client (editable test files):
- `web/src/features/task-workspace/TaskWorkspace.test.tsx:310-400` (the convergence four, see 2.5).
- `web/src/features/chat/status/headView.test.ts` (about 25 cases; busy-stream and fallback cases at :51-:72 and :138-:164).
- `web/src/features/chat/status/LiveHead.test.tsx:36` ("maps session busy to working"), `:44`, `:287` ("derives task-level attention without BrowserTaskDetail").
- `web/src/features/chat/status/LiveHead.integration.test.tsx:54` ("shows when a busy turn has stopped producing ACP activity").
- `web/src/features/chat/ChatSurface.details.test.tsx` (uses `buildHeadView`).

---

## 7. Single-row UPSERT capability in the SQLite registry today

- **Whole-snapshot save is the only persistence entry point.** `SqliteRegistryStore::save_if_revision` (`core/registry/sqlite/mod.rs:117-131`) and `save` (`:192`) both call `save_registry`, which deletes and reinserts every registry table (`save.rs:33-67`), then reinserts all tasks and events (`:69-98`).
- **Revision.** `save_if_revision_with_empty_policy` (`mod.rs:133-165`) compares `registry_meta.revision` to `expected_revision` and bumps it inside the transaction (`:155-161`). Any new per-row write that does not bump the revision would bypass the CLI's revision-conflict merge (`ajax-cli/src/context.rs:254-263`). Whether bumping is required is **SPECULATION** until the Phase 3 design decides it.
- **Per-task writer exists but is private to the save path.** `save_task` (`save.rs:129-`) does an `INSERT ... ON CONFLICT(task_id) DO UPDATE` for `registry_tasks` (`:150-165`) and `registry_task_workflow` (`:185-200`), then calls per-child helpers. It is `pub(crate)` and only called from `save_registry` (`save.rs:89`).
- **Child rows are not idempotent per task.** `save_task` inserts side flags (`:221-227`), metadata (`:229-235`), and attempts (`:237-`) with plain `INSERT`, relying on the global `DELETE` in `save_registry`. Calling `save_task` alone would duplicate them. The per-task helpers that do delete first are `save_live_status` (`save.rs:278-` with `DELETE ... WHERE task_id` at :286), `save_runtime_projection` (:325), `save_git_status` (:358), `save_tmux_status` (:398), `save_task_window_status` (:422).
- **Existing one-row-per-task table.** `registry_task_live_status` has `task_id TEXT PRIMARY KEY` (`core/registry/sqlite/migrations.rs:104-110`). It is currently rewritten by `save_live_status` (delete then insert). A per-task UPSERT into this table needs no schema change, but `save_registry` deletes all of its rows on every snapshot, so the two writers would conflict unless the snapshot path stops rewriting it (**SPECULATION**).
- **Step receipts** use a real `ON CONFLICT(task_id, operation, step_key, target) DO UPDATE` (`save.rs:458-`), so the SQLite UPSERT syntax is already used in this crate.
- **Schema version is 10** (`migrations.rs:5`). A new table needs a migration (`migrate_v9_to_v10`-style, `migrations.rs:51-54`), and that would contradict the seven `user_version == 10` assertions in `registry/sqlite/tests/` (section 6.1). Options that avoid a bump: reuse `registry_task_live_status` (no migration); or add an idempotent `CREATE TABLE IF NOT EXISTS` at v10 without bumping (**SPECULATION**, and it weakens the migration model; needs Matt's decision).
- **No per-task update API.** There is no `update_task_*` in `registry/sqlite/` for live status. `Registry` trait updates are in memory (`core/registry.rs` e.g. `update_task_window_status` used at `runtime_refresh.rs:392`).
- **Persisted running flag is read from the registry.** `persisted_chat_running` (`startup_recovery.rs:151-161`) reads `task.live_status` from the loaded snapshot. If the activity path stops writing the snapshot, the last-known status must still reach this check after a restart (cold read and Phase 2 shim adoption both depend on it).

---

## 8. Other facts the plan needs

- The reducer already exists as a pure function: `reduce_agent_status` (`core/agent_status.rs`), used by `SessionActivityReporter::reduce_activity` (`web_session/session_activity.rs:206-244`). The projector can wrap it rather than invent one.
- `apply_authoritative_observation_at` is the only writer that sets `Active` from ACP evidence without the trusted gate (`core/live_application.rs:26-46`). Chat turn-end does **not** set `Reviewable` (`Done` in the authoritative path only sets `agent_status` Done and removes flags). "Ready for review" for a chat task is therefore not produced by the activity path today (`ui_state.rs:206-210` shows `Reviewable` only from lifecycle).
- The runtime's `refresh_cockpit` probe skips chat tasks for live-status writes: `runtime_refresh.rs:159-172`, `:257`, `:397-412` (`skip_interactive_agent` → `continue`), so live status for chat tasks is written only by the reporter (and by the CI monitor, see below).
- The CI monitor writes live status for chat tasks too: `runtime_refresh/ci_monitor.rs:83-` (`refresh_ci_monitor`, no `skip_interactive_agent` gate seen in `refresh_task` lines 83-130) → `apply_observation` (`ci_monitor.rs:397-410`, `CiFailed`/`CiPending`/healthy) and writes PR metadata `AJAX_PULL_REQUESTS_KEY` (`ci_monitor.rs:148`; key in `core/diff_review.rs:6`). These are the "PR/CI state" inputs of the projector.
- Git conflict observations: `core/events.rs:~100-110` (`GitSnapshot` with `conflicted` → `MergeConflict`) → `apply_monitor_event_to_task` (`events.rs:~115-`).
- Wave-0 golden test records that stored `agent_status` Done + live Done projects operator `Waiting` ("Response ready"); any projector must reproduce that mapping or change the test (BLOCKED).

---

## 9. Plan drift (plan text vs code today)

| Plan says | Code says |
|---|---|
| Push "pushes are suppressed while the browser polled in the last 90 s (`runtime/state.rs:33`)" | `BROWSER_CONNECTED_TTL = 45 s` at `runtime/state.rs:43` |
| "The loop is started twice (`runtime/mod.rs:165-166`)" | `spawn_background_loops` is called once (`runtime/mod.rs:165`), push tick spawned once inside it (`:189-190`); pinned by `refresh_lane_tests.rs:74` |
| "every chat activity event clones the whole registry (`runtime/state.rs:288-330`)" | Reporter path now at `runtime/state.rs:326-418`, whole-registry rewrite in `registry/sqlite/save.rs:28` |
| "`ui_state.rs:289`" for "Response ready" | Correct: `canonical_waiting_explanation` at `ui_state.rs:289` |
| "`TaskWorkspace.tsx:84`" for the 1 s poll | Correct: interval at `TaskWorkspace.tsx:84` (constant at :23) |

---

## 10. Proposed slicing of Phase 3

Principles: each packet compiles and tests on its own; behavior never regresses before the packet that intends to change it; a packet that would alter a `tests/` assertion is marked **BLOCKED** and needs Matt's sign-off first. Each packet is at most about 4 files.

**P3.1 Pure core projector, no behavior change** (ajax-core)
- Files: new `core/chat_status.rs` (pure fn `project_chat_status(turn: ChatTurnInput, task: &Task) -> OperatorStatus`), `core/lib.rs` (module declaration), inline tests in the new file.
- Behavior: none. Tests: an equivalence table of chat fixtures against `derive_operator_status` (every `LiveStatusKind` and turn state from section 1.6), in inline tests.
- Pins touched: none.

**P3.2 In-memory turn view published by the actor** (ajax-web, read-only)
- Files: `web_session/session_evidence.rs` (publish on `report`/`commit`), `web_session/task_session.rs` (spawn wiring), `web_session/task_session_directory.rs` (`turn_view(handle)` sync read from a shared map), new `web_session/chat_turn_view.rs` (struct + inline tests).
- Behavior: none (snapshot writes unchanged). Sync read path proves the in-process accessor. Tests: view matches reporter state after each `SessionActivity`.
- Pins touched: none. Architecture: `runtime/` must not import `web_session::task_session::`; use the directory's public method (`architecture_web_session.rs:158`).

**P3.3 Cockpit and detail read the projection** (ajax-web + ajax-core)
- Files: `slices/cockpit/mod.rs` (accept turn views as a parameter; no slice-to-slice import), `runtime/task_routes/cockpit.rs` (gather views from the directory, pass in), `core/commands/projection.rs` (`task_card` takes optional turn input), `runtime/state.rs` (helper to gather views).
- Behavior: cockpit/detail status now reflects the live turn; snapshot writes still happen (so cold reads agree). This is the first visible change, and it is the fix for the divergence in section 5.
- Pins touched: `wave0` golden row expectations must still hold (`row_acp_turn_ended`); confirm with the test before merge. If the mapping changes, **BLOCKED**.

**P3.4 Push reads the projection; decide dedupe state** (ajax-web + ajax-core)
- Files: `slices/push.rs` (`deliver_attention_pushes` takes the projected status), `core/attention.rs` (`take_attention_transition` input), `runtime/task_routes/cockpit.rs` (pass view).
- Decision needed (Matt): the dedupe keys (`LAST_NOTIFIED_STATUS_KEY` and friends, `attention.rs:8-11`) live in `task.metadata` and are only persisted by a snapshot write. Options: keep a last-notified cell in memory (lost on restart → possible re-notify), or persist them with the single-row UPSERT (P3.5). **SPECULATION**: the UPSERT option is the one that keeps Phase 3's "no snapshot on activity" goal.
- Pins touched: `attention/tests.rs` (`response_ready_waiting_does_not_notify` :725 and the debounce cases) must keep passing.

**P3.5 Last-known status UPSERT, no schema bump** (ajax-core registry + ajax-cli bridge)
- Files: `core/registry/sqlite/save.rs` (new `upsert_last_known_status` per-task function with `ON CONFLICT(task_id)`), `core/registry/sqlite/mod.rs` (store method, no revision bump unless decided), `core/registry/sqlite/load.rs` (read back into `live_status` on cold load), and one test file for the round trip.
- Behavior: none yet (nothing calls it). Avoids schema 11 because it reuses `registry_task_live_status` (`migrations.rs:104`). If a new table is preferred, it is **BLOCKED** by the seven `user_version == 10` assertions in `registry/sqlite/tests/` (section 6.1).
- Pins touched: `save_registry` deletes `registry_task_live_status` rows; a test pins snapshot round trips. Must not change.

**P3.6 Stop chat activity snapshot writes** (ajax-web runtime)
- Files: `runtime/state.rs` (`apply_task_session_activity`: lifecycle transitions only → `commit_durable`; other activity → in-memory update + P3.5 UPSERT), `web_session/session_activity.rs` (`record_session_activity` returns whether lifecycle changed), `runtime/task_routes/cockpit.rs` (nothing new), `web_session/session_evidence.rs`.
- **BLOCKED** — these tests contradict the required behavior and must be named to Matt first:
  - `crates/ajax-web/src/runtime/tests/suite_6.rs:79` `issue_1227_session_activity_fails_when_registry_is_not_saved` (asserts activity is a durable save).
  - `crates/ajax-web/src/runtime/tests/suite_5.rs:286` `report_task_session_activity_fails_fast_when_control_lane_busy_issue_1083` (asserts deferral failure path; it is the deferred-queue contract, so it goes with P3.7).
  - Inline `session_activity_directory_tests.rs` `issue_1069_failed_report_retries_turn_end_on_next_append` (:105) and `issue_1132_...` (:164) keep turn-end persistence and must still hold, which they will for lifecycle writes.

**P3.7 Remove the deferred queue and drainer** (ajax-web runtime + web_session)
- Files: `runtime/state.rs` (delete `DeferredActivityReports`, `deferred_activity_pending`, `report_or_defer_*`, `spawn_deferred_activity_drainer`; `:48-64`, `:423-503`), `web_session/task_session_directory.rs` (delete `DeferredActivityPending` `:14`, fields `:47`, setter `:71-73`, plumbing `:174-176`, `:195`, `:232`), `web_session/task_session.rs` (`:354`, `:426-432`), `web_session/session_activity.rs` (`first_attach_activity` `:103-108`).
- **BLOCKED** — deletes assertions in `runtime/activity_report_tests.rs` (six tests, listed in 6.2), `session_activity_directory_tests.rs:569` (`issue_1475_first_attach_waits_for_a_deferred_report`), and `runtime/tests/suite_5.rs:286`. Matt must approve deleting or rewriting these as contracts for the new design. Do not weaken them in place.

**P3.8 Remove 1 s convergence and unify the head** (web client)
- Files: `web/src/features/task-workspace/TaskWorkspace.tsx` (delete `SESSION_CONVERGE_MS`, `sessionConverging` interval `:23`, `:79-86`; keep `sessionBusy` only for `harnessSwapDisabled`), `web/src/features/chat/status/headView.ts` (`headState` takes the projected status from `taskAttention`; no stream-only fallback), `web/src/features/chat/status/ChatLiveHead.tsx` (pass-through only).
- **BLOCKED** — deletes the convergence tests (`TaskWorkspace.test.tsx:341-395`) and changes `headView.test.ts` busy/fallback cases (`:72`, `:150`) and `LiveHead.test.tsx:36`. Matt must approve these as contract changes.

**P3.9 Docs** (same PR as the code that makes them true)
- Files: `architecture.md` (chat status section), `docs/architecture/web-cockpit.md` (status source, push source, polling), `.planning/agent-plans/chat-first-core.md` Phase 3 checkboxes only after verification (the plan itself is not edited by this discovery).

**Ordering:** P3.1 → P3.2 → P3.3 → P3.4 (needs the dedupe decision) → P3.5 → P3.6 (BLOCKED) → P3.7 (BLOCKED) → P3.8 (BLOCKED) → P3.9. P3.1–P3.5 do not change behavior or any pinned assertion, so they can land without sign-off. P3.6–P3.8 each need Matt to approve named test changes.

**Flags for contradictions with `tests/` directories:**
- `runtime/tests/suite_5.rs:286` and `runtime/tests/suite_6.rs:79` (cannot be modified; both contradict P3.6/P3.7).
- `registry/sqlite/tests/` schema-10 assertions (seven sites; contradict any schema bump, P3.5).
- `crates/ajax-core/tests/wave0_run_state_characterization.rs` (`row_acp_turn_ended` expects operator `Waiting` after turn end; contradicts any projector that maps turn end differently).
- `crates/ajax-core/tests/wave2_live_apply_contract.rs` (`agent_status` writer allowlist; the new projector must not assign `agent_status` outside `live_application.rs`).
- `crates/ajax-core/src/runtime_refresh/tests/suite_3.rs:423-439` ("Response ready" explanation) under a `tests/` dir.

---

## 11. Speculation and unverified items (summary)

- SPECULATION: a new published status cell (P3.2) is preferable to a command round trip for cockpit reads. Not proven by any existing test.
- SPECULATION: reusing `registry_task_live_status` for the last-known status (P3.5) avoids the migration, but `save_registry` still deletes that table on any snapshot, so the snapshot path must stop rewriting it. Needs confirmation with the CLI save path (`ajax-cli/src/context.rs:236-283`).
- SPECULATION: whether a per-row write must bump `registry_meta.revision` (section 7).
- UNVERIFIED: the individual assertions in `ajax-cli/src/web_backend/tests/suite_1.rs`, `ajax-cli/src/cockpit_backend/tests.rs`, and the `web_session` background/ownership test files. The files were identified by grep; their assertions were not read.
- UNVERIFIED: whether `refresh_ci_monitor` is gated by `skip_interactive_agent` in a part of the file not read (`ci_monitor.rs:83-130` showed no gate).
- Not run: no `cargo`, `nextest`, or `npm` commands were run for this discovery. It is a read-only map. Line numbers are from reading the files; confirm before editing.

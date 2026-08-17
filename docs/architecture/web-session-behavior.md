# Web Session Behavior Contract

Falsifiable invariants for the optional flag-gated Cursor ACP orchestration chat
mode in Web Cockpit. Later PRs implement against this ledger; nothing here is
implementation how-to.

## Flag-off parity

When the Ajax web session preference is **off**, dashboard navigation, task
detail, embedded raw terminal, Diff Review, and operate flows behave exactly as
they do today. No session routes, WebSocket, or UI chrome may appear or alter
existing paths.

## Launch

- Provisioned starts skip tmux send-keys but still create the task tmux session.
  Every harness with an ACP entry point (Cursor native, Codex/Claude/Pi via their
  bridges) may use that launch mode; a harness without one cannot.
- The browser routes a task to chat only when its projection reports
  `session_capable`; anything else opens the terminal, including a session URL
  typed or bookmarked for an interactive task.
- Session attach is only for tasks whose registry metadata records
  `skip_interactive_agent` (provisioned launch) **and** whose agent has an ACP
  entry point. Interactive tasks (tmux send-keys launch) receive HTTP 409
  `NotOrchestrationChat`.
- The model chosen when the task was created is stored on the task (`session_model`
  metadata) and used for its session. Reconnect must not send a browser
  `localStorage` preference on the WebSocket URL to override that metadata
  ([#910](https://github.com/mossipcams/ajax-cli/issues/910)). With no stored
  model, Cursor runs `CURSOR_DEFAULT_MODEL` and a bridge harness picks for itself.
- Cursor takes its model on the spawn argv; Codex, Claude, and Pi take
  `session/set_config_option` once the session exists (the host applies the model
  id and any reasoning-level option separately). A harness that refuses the
  selection keeps its own default and the session continues.
- Changing the model while connected persists `session_model` on the task through
  a core-owned operation before the host replaces the ACP child; a persistence
  failure returns a typed `error` event and leaves the running child unchanged.
- Moving a task to another harness is refused unless it was launched over ACP,
  and drops the live ACP slot so the next attach spawns the new harness.
- Ajax orchestration sessions are trusted local automation, and the Settings
  toggle discloses that supported agents run with full tool access and without
  approval prompts. After session creation or restore, the host reads only ACP
  `configOptions`, finds the exact `mode` option and an exact advertised value,
  then sends `session/set_config_option`: `agent-full-access` for Codex or
  `bypassPermissions` for Claude. It ignores legacy `modes` and never sends
  `session/set_mode`; it also does not reinterpret model, thought-level, Pi, or
  unknown Cursor options. Missing, unadvertised, or refused configuration keeps
  the standard ACP permission flow as the safe fallback.

## Queue and cancellation across WebSocket reconnect

- At most one `session/prompt` is in flight on the ACP host at a time.
- Additional composer submits while a turn is in flight are queued in FIFO order
  (cap 8; oldest dropped when full).
- Cancel with `keepQueue: false` clears the queue and cancels the in-flight turn.
- Cancel with `keepQueue: true` cancels the in-flight turn but preserves queued
  prompts for the next flush.
- After a WebSocket drop and reconnect, the browser supplies the last applied
  cursor on the WebSocket URL (`?cursor=`). The host sends a protocol v2
  `snapshot` plus only events after that cursor; invalid or compacted-away
  cursors trigger a reset snapshot and bounded full replay.
- The last-applied cursor lives in the page session only (same JS heap as the
  reducer). A cold load or full reload omits `?cursor=` and receives full replay;
  only unacknowledged prompts persist in `sessionStorage`.
- Each browser prompt has a stable `clientMessageId`; the host persists a
  `prompt_accepted` acknowledgement and dispatches each ID at most once. The
  browser retries only prompts still absent from that acknowledgement.
- The browser keeps only an unacknowledged-prompt outbox for resend; it does not
  maintain a second FIFO queue. Submitting while busy sends one host-queued prompt;
  Stop owns cancellation.
- A host background pump continues draining ACP slots after the last socket
  closes, so an in-flight or queued turn does not depend on browser presence.
- Idle LRU eviction must not drop slots with a non-empty host queue **or an in-flight turn**.

## Model switching across ACP process replacement

- `set_model` while idle persists the desired model on the task, then respawns the
  ACP child with the new model pin.
- The UI transcript on disk and in replay is unchanged except for host-emitted
  status/note events.
- A live `ready` event on an established socket must not reset browser reducer
  state; only reconnect-after-drop may clear and replay.
- Each attach delivers one protocol v2 `snapshot` wire frame to the browser
  reducer (as a synthetic `ready` event for turn state), then cursor-bearing
  `event` envelopes for replay/live traffic.

## Restart and transcript recovery

- UI transcript survives `ajax-web` restart via JSONL under `state_dir`.
- On acquire after restart, the host restores the stored ACP session id with
  `session/resume` when advertised, otherwise `session/load`. If resume fails
  and load is advertised, load is attempted before a new session is created.
- Cursor may emit `session/update` replay notifications before the load result;
  the host drains them so JSONL is not duplicated.
- If load is unsupported or fails, the JSONL transcript still reloads and exactly
  one agent-visible note states that model context reset; the composer keeps
  working.
- Transcript events append to JSONL without a per-event full rewrite; bounded
  compaction preserves absolute replay cursors. Streamed agent/thought text is
  normalized to full-content `message` updates with stable host `itemId` values
  before persistence.

## Reconnect model and ACP capabilities

- Reconnect does not read the browser `ajax.web.session.model` preference for the
  WebSocket URL; task `session_model` metadata and the host attach plan decide
  the model. The preference may still be updated from `ready` so it seeds the
  New Task picker for the next task only.
- The ACP client keeps v1 `SessionNotification` values typed through mapping.
  Message, thought, tool, plan, mode, configuration, session-info, and usage
  updates have explicit mappings; unsupported capability announcements are
  ignored by the chat projection.
- Tool calls carry their ACP `content` array to the browser as `text` and `diff`
  entries. Dropping it left the browser able to say only that an unnamed edit
  happened. A `tool_call_update` without `content` revises the other fields and
  keeps the content already received.
- `usage_update` is a first-class `usage` event, not an `artifact`. A zero
  window means the harness does not report context and is dropped, so it never
  renders as 0% used.
- `messageId` is optional in ACP v1. It is carried when present and refines both
  host-side coalescing and browser-side grouping; with it absent, role adjacency
  decides message boundaries as before.
- Ajax advertises neither `fs/*` nor `terminal/*` client capabilities. Agents
  must not depend on those requests until Ajax adds worktree-scoped handlers;
  the ACP protocol's unsupported-method response remains the boundary. This is
  also why `ToolCallContent::Terminal` has no mapping: no agent can create a
  terminal to embed, and execute output arrives as text.

## Permission persistence

- Unrecognized ACP `sessionUpdate` kinds (except dropped capability announcements) are
  stored as `artifact` events in the host transcript.
- ACP `session/request_permission` is correlated by its JSON-RPC request id; an
  approval or rejection selects a matching advertised ACP permission option,
  and cancellation resolves every pending request with the standard cancelled
  outcome before sending `session/cancel`.
- Operator answers to ACP permission requests are recorded as
  `permission_resolved` in the host transcript.
- Reconnect or full page reload replay must not resurrect a permission prompt
  whose `requestId` already has a matching `permission_resolved` entry.

## In-flight activity freshness

- The host-reported unresolved prompt remains the only authority for whether a
  turn is in flight; browser timers do not alter task or session lifecycle.
- While a turn is in flight, the live head measures time since the most recent
  ACP event. After one minute without an event it says `No recent activity` and
  shows the elapsed minutes while preserving Stop. A later event resets the
  indicator, and turn completion removes it.
- This is a freshness warning, not a claim that the agent is stalled: stable
  ACP v1 has no portable stalled-state signal.

## Duplicate process and prompt prevention

- At most one ACP stdio process may hold a given task session slot; acquire must
  not spawn a second child for the same handle while one is live.
- A second composer submit while a turn is in flight is queued (not a second
  in-flight `session/prompt`). Reconnect and double-Enter must not start a
  parallel prompt against the same slot.

## Terminal and Diff Review escape paths

- Opening the raw tmux terminal from a session is an overlay/sheet escape hatch;
  it attaches to the task tmux session, not the ACP chat process.
- Navigating to `#/t/<handle>` is terminal-first host attachment, not ACP chat
  continuation.
- Diff Review opened from a session returns to `#/session/<handle>` (chat-first),
  not the terminal-first task page.
- Session horizontal gestures must not steal Diff Review panes and vice versa.

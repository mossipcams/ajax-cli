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
- The model chosen when the task was created is stored on the task and used for
  its session unless the socket pins a different one. With neither, Cursor runs
  `CURSOR_DEFAULT_MODEL` and a bridge harness picks for itself.
- Cursor takes its model on the spawn argv; Codex takes `session/set_model` and
  Claude and Pi take `session/set_config_option` once the session exists. A
  harness that refuses the selection keeps its own default and the session
  continues.
- Moving a task to another harness is refused unless it was launched over ACP,
  and drops the live ACP slot so the next attach spawns the new harness.
- Ajax orchestration sessions are trusted local automation. After session
  creation or restore, the host selects an exact, advertised non-interactive
  mode for harnesses whose stable mode id is known: `agent-full-access` for
  Codex and `bypassPermissions` for Claude. It never infers a security mode
  from its display name and does not reinterpret Pi thinking modes or unknown
  Cursor modes. An unavailable or refused mode keeps the standard operator
  permission flow as the safe fallback.

## Queue and cancellation across WebSocket reconnect

- At most one `session/prompt` is in flight on the ACP host at a time.
- Additional composer submits while a turn is in flight are queued in FIFO order
  (cap 8; oldest dropped when full).
- Cancel with `keepQueue: false` clears the queue and cancels the in-flight turn.
- Cancel with `keepQueue: true` cancels the in-flight turn but preserves queued
  prompts for the next flush.
- After a WebSocket drop and reconnect, the host replays the durable transcript
  from cursor; queued prompts and in-flight state remain host-owned — reconnect
  must not duplicate or lose queued work that survived on the host.
- Idle LRU eviction must not drop slots with a non-empty host queue **or an in-flight turn**.

## Model switching across ACP process replacement

- `set_model` while idle respawns the ACP child with the new `--model` pin.
- The UI transcript on disk and in replay is unchanged except for host-emitted
  status/note events.
- A live `ready` event on an established socket must not reset browser reducer
  state; only reconnect-after-drop may clear and replay.

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

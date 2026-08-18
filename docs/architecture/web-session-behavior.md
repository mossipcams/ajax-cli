# Web Session Behavior Contract

Falsifiable invariants for the optional flag-gated Cursor ACP orchestration chat
mode in Web Cockpit. Later PRs implement against this ledger; nothing here is
implementation how-to.

## Flag-off parity

The browser preference (`ajax.web.session.orchestrationChat`) defaults **on**
when unset; only an explicit stored value of `false` disables orchestration
chat.

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
- Task `session_model` is **desired** state. Protocol v2 `snapshot.model` is
  **applied** state: the harness-reported model id after `session/new` or
  resume/load and any in-band apply. It must not echo the attach-plan pin
  ([#952](https://github.com/mossipcams/ajax-cli/issues/952)).
- Cursor takes its model on the spawn argv when the harness pins at spawn; every
  harness (including Cursor) also applies an operator pin in-band when
  `configOptions` or `models.availableModels` advertises a model control. A
  refused or unprovable pin keeps the session, emits a typed `error` event, and
  leaves `snapshot.model` on the harness-reported id (or empty), not the
  rejected pin.
- Changing the model while connected persists `session_model` on the task through
  a core-owned operation before the host replaces the ACP child; a persistence
  failure returns a typed `error` event and leaves the running child unchanged.
  Persist `None` for Auto/unspecified; never store the literal string `auto` as
  a harness model id ([#952](https://github.com/mossipcams/ajax-cli/issues/952)).
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
- Each live `TaskSession` Tokio task continues draining its ACP child and host
  queue after the last socket closes, so an in-flight or queued turn does not
  depend on browser presence.
- Idle LRU eviction must not drop slots with a non-empty host queue **or an in-flight turn**.
- The per-task Tokio command loop continues after the last WebSocket subscriber
  detaches while a turn is in flight or the host queue is non-empty.

## Shutdown and slot retention

- Dropping a task or changing harness shuts down the live `TaskSession` before a
  new attach creates one for that handle.
- Idle LRU eviction sends `Shutdown` only to slots with zero subscribers, no
  in-flight turn, and an empty host queue; evictable slots must not hold pending
  work.
- WebSocket detach releases the directory holder count but does not cancel an
  in-flight turn or clear the host queue.
- `ajax-web` restart reloads JSONL transcripts and cursors from disk; live ACP
  children do not survive process exit.

## Model switching across ACP process replacement

- `set_model` while idle persists the desired model on the task, then respawns the
  ACP child with the new model pin. An explicit operator choice always replaces
  the running child even when slot metadata already matches.
- The UI transcript on disk and in replay is unchanged except for host-emitted
  status/note events.
- A live `ready` event on an established socket must not reset browser reducer
  state; only reconnect-after-drop may clear and replay.
- Each attach delivers one protocol v2 `snapshot` wire frame to the browser
  reducer (as a synthetic `ready` event for turn state), then cursor-bearing
  `event` envelopes for replay/live traffic.
- The in-session picker reverts only on model-change failures from the host
  (persistence refused, invalid model, refused/unprovable harness pin, and similar).
  Unrelated `error` events during the next prompt must not restore the previous
  picker value ([#942](https://github.com/mossipcams/ajax-cli/issues/942)). The
  picker binds to `snapshot.model` (applied state), not task metadata
  ([#952](https://github.com/mossipcams/ajax-cli/issues/952)).

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
  before persistence. The browser `MessageBuffer` rAF-coalesces those updates to
  the latest full text per `itemId` during the turn instead of holding them until
  `turn_end`; boundary events still flush pending lanes first so ordering is
  preserved.

## Reconnect model and ACP capabilities

- Reconnect does not read the browser `ajax.web.session.model` preference for the
  WebSocket URL; task `session_model` metadata and the host attach plan decide
  the model. The preference may still be updated from `ready` so it seeds the
  New Task picker for the next task only.
- New Task and task-details model pickers list the full harness catalog from
  `GET /api/session/models`. A failed catalog read shows an operator-visible
  error with retry; it must not fall back to Auto plus the live session model
  ([#948](https://github.com/mossipcams/ajax-cli/issues/948)).
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
  ACP v1 has no portable stalled-state signal. The head must not invent
  thinking content from that timer — it shows the latest ACP `thought` text (one
  quiet line) while working with no tool or plan step, and `Thinking…` only
  before the first thought, tool, or plan arrives. Reasoning in the transcript
  auto-expands while it is the live tail of a busy turn and collapses when a
  later item arrives or the turn settles.

## Duplicate process and prompt prevention

- At most one ACP stdio process may hold a given task session slot; acquire must
  not spawn a second child for the same handle while one is live.
- A second composer submit while a turn is in flight is queued (not a second
  in-flight `session/prompt`). Reconnect and double-Enter must not start a
  parallel prompt against the same slot.

## Terminal and Diff Review view paths

- **Ajax terminal** in Ajax chat task details navigates to `#/t/<handle>` and
  stores a per-task terminal preference in browser localStorage; it is not a
  session overlay or sheet escape hatch.
- While that preference is set, dashboard opens, later visits, and
  `#/session/<handle>` for that handle land on the terminal page until **Ajax
  chat** in terminal task details clears the preference.
- Navigating to `#/t/<handle>` is terminal-first host attachment, not ACP chat
  continuation.
- Diff Review back navigation returns to `#/t/<handle>` when the task prefers
  terminal view, otherwise to `#/session/<handle>` for session-capable chat
  tasks.
- Session horizontal gestures must not steal Diff Review panes and vice versa.

## Task actions from Ajax chat

- Drop and other destructive actions live in the task details sheet, not the
  session head ActionBar (destructive actions are filtered there).
- Arming Drop confirm closes the details sheet so the shell ResultPanel
  (z-index 40) is usable; ResultPanel must not be raised above NewTaskSheet /
  FullscreenLayer globally.
- `#/session/<handle>` counts as staying on the dropped task for shell confirm,
  the drop leave latch, and post-Drop dismiss — same as `#/t/<handle>` and diff
  review for that handle. Leaving the session route cancels an armed confirm
  without POSTing Drop.

## Mobile keyboard band

- Orchestration session chat owns its mobile layout boundary: a bounded flex
  column (`session-chat-surface`) with LiveHead (`flex: none`), transcript
  scroller (`.session-thread`: `flex: 1 1 0%`, `min-height: 0`,
  `overflow-y: auto`), and composer (`flex: none`) as siblings. Every ancestor
  from `.app-viewport` through the surface has `min-height: 0`; route-scroll
  does not compete as a vertical scroll owner on `#/session/<handle>`.
- While session chat is mounted, `html[data-session-viewport="owned"]` tells
  global CSS to **not** apply the `position: fixed` visual-viewport pin on
  `.app-viewport`. Task and terminal surfaces still use `html.keyboard-open` /
  `--app-height` / `.app-viewport` fixed pinning. Session chat uses one
  authoritative visible-height calculation via `useMobileKeyboard` +
  `sessionKeyboardPadding`: reserve bottom padding only on iOS regular Safari
  (visualViewport shrinks, innerHeight may not); zero padding when the layout
  viewport already shrank (iOS PWA / Android) so keyboard band geometry is not
  applied twice.
- No nested `position: fixed` pin on `.session-page.session-chat` inside the
  global band — double-applying `--app-top` strands the composer ([#877](https://github.com/mossipcams/ajax-cli/issues/877)).
- Keyboard or composer resize is a layout change, not user scroll-up. Before
  the transition, capture the transcript geometry (`scrollTop`, `scrollHeight`,
  `clientHeight`, live-edge intent). Poll until flex layout settles (stable
  `scrollHeight` / `clientHeight`, no animation). While settling after keyboard
  close, if the operator was at the live bottom (`pinnedRef` or recent live-edge
  intent), keep `scrollTop = scrollHeight` each frame so growing `clientHeight`
  does not paint a keyboard-sized gap; history mode leaves `scrollTop` untouched
  until settle completes. Then restore once: live bottom → new live edge;
  reading history → same visible content plus any `scrollHeight` delta from
  content inserted above the viewport. Ignore Safari resize-generated scroll
  events as user scrolling during the transition.
- While the operator stays pinned (`pinned` / `pinnedRef`), transcript growth
  from streaming or layout (items effect, thread `MutationObserver` for
  scrollHeight growth, thread `ResizeObserver` for box resizes) keeps
  `scrollTop = scrollHeight`. Keyboard restore to the live bottom re-asserts
  `pinned`. Scroll-up clears `pinned`; unpinned readers are not yanked back
  to the live edge.
  `useMobileKeyboard` clears keyboard geometry immediately on composer blur
  (focusout with no form control focused) and ignores stale visualViewport
  shrinks until a field is focused again, catching up when the viewport grows.
  No stale `keyboard-open`, `--app-height`, bottom padding, or fixed positioning after
  dismissal; safe-area is preserved without a second blank strip below the
  composer.
- Tapping anywhere on the session page outside the composer controls (textarea,
  Mic, Send, and other interactive targets) blurs the composer so iOS can
  dismiss the keyboard without leaving it stranded mid-viewport.
- Clearing `keyboard-open` blurs a focused session composer textarea and resets
  document scroll so iOS does not keep `visualViewport.offsetTop` on a
  still-focused input.

## Session composer speech

- The session Mic control keeps the **Mic** text label. At rest it is plain
  Soft Steel Blue text (`--accent` / `--soft-steel-blue`, `#87afd7`): no pill,
  no fill, no border or background (`border-radius: 0` overrides the shared
  composer button pill). Send remains the accent-filled pill CTA
  (`border-radius: 999px`).
- While listening or pause-pending (`is-armed`) and while connecting
  (`is-connecting`), Mic stays text-only; the label may use `--warn` so the
  state change is obvious. Hover and focus must not restore a filled chip.
  Connecting must not look like a disabled no-op at reduced opacity.

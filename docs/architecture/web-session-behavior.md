# Web Session Behavior Contract

Falsifiable invariants for the optional flag-gated **Ajax Chat** orchestration
session mode in Web Cockpit. Ajax Chat is multi-harness: Cursor speaks ACP
natively; Codex, Claude, and Pi reach ACP through their bridge packages. Later
PRs implement against this ledger; nothing here is implementation how-to.

## Task Workspace and Ajax Chat default

Ajax Chat is one peer surface inside the Task Workspace; it is not the whole
workspace and does not own task metadata, actions, harness switching, mode
preference, or Diff routing.

- Public workspace hashes stay `#/session/<handle>` for Ajax Chat and
  `#/t/<handle>` for Ajax Terminal. Bare `#/session` opens New Task only.
- When orchestration chat is enabled, provisioned tasks whose projection reports
  `session_capable` default to Ajax Chat (`#/session/<handle>`) unless the
  operator has set the per-task Terminal preference.
- Interactive tasks (tmux send-keys launch) and tasks that are not
  `session_capable` fall back to Ajax Terminal; a session URL for such a task
  redirects to `#/t/<handle>` rather than opening a refused ACP socket.
- **Ajax terminal** in task details navigates to `#/t/<handle>` and stores the
  per-task preference; **Ajax chat** in task details clears that preference and
  returns to `#/session/<handle>`.
- Diff Review Back follows the same mode selection: Terminal preference or
  non-chat-capable tasks return to `#/t/<handle>`; otherwise `#/session/<handle>`.
- Terminal mode preserves the raw xterm.js/tmux contract documented in
  [`web-cockpit.md`](web-cockpit.md); Terminal is not the default Task Workspace
  surface for session-capable provisioned tasks.

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
- Task `session_model` is **desired** state (Ajax pipe-form or catalog id for New
  Task / Switch). Protocol v2 `snapshot.model` is **applied** state: the model
  config option's advertised `currentValue` only — not a reconstructed bracket
  string ([#952](https://github.com/mossipcams/ajax-cli/issues/952),
  [#997](https://github.com/mossipcams/ajax-cli/issues/997)).
- Live session configuration follows ACP `configOptions` (Agent of Empires
  contract): after `session/new`, resume/load, every `session/set_config_option`
  response, and every `config_option_update`, the host stores the complete
  advertised list and exposes it on the snapshot as `sessionConfigOptions`
  (id, category, name, type, currentValue, choices). Replace the list; do not
  merge. `config_option_update` refreshes applied state; it is not a transcript
  artifact.
- Live slash commands follow ACP `available_commands_update`: after `session/new`
  and any later replacement, the host stores the complete advertised list and
  exposes it on the snapshot as `availableCommands` (`name`, `description`,
  optional `inputHint`). Replace the list; do not merge. Updates are live session
  capability state, not JSONL transcript rows. The chat composer completes
  advertised `/name` tokens (keyboard Tab/Enter and tappable rows on iOS Safari)
  and sends the operator's text unchanged on `session/prompt`; Ajax does not
  implement local slash handlers.
- Live prompt content capabilities come from ACP `initialize` `agentCapabilities.promptCapabilities`
  (`image`, `embeddedContext`; never `audio`). The host stores the handshake values and
  exposes them on the snapshot as `promptCapabilities`. Replace on handshake; do not
  invent capabilities. The browser shows Attach only when `promptCapabilities.image`
  or `promptCapabilities.embeddedContext` is true. It may attach `image` only when
  `promptCapabilities.image` is true and embedded `resource` bodies only when
  `promptCapabilities.embeddedContext` is true. The file picker does not synthesize
  `resource_link` stubs for local paths; `resource_link` remains valid on the host wire
  for real URIs. Before send, the browser downscales/compresses attached photos so the
  prompt JSON frame fits the 256 KiB WebSocket cap with headroom for typed text.
  WebSocket `{ type: "prompt", text, clientMessageId, contentBlocks? }` remains
  backward compatible; omitted `contentBlocks` is text-only. The host validates block
  types against advertised capabilities, sends a real ACP `ContentBlock` array on
  `session/prompt`, and records only operator text plus attachment names in JSONL
  (no base64 in the operator transcript).
- Non-text **output** from ACP `session/update` (agent/user/thought chunks and tool-call
  content) maps `image`, `resource_link`, and embedded `resource` blocks into wire
  `message.contentBlocks` and extended `tool_call.content` (text chunks and diffs
  unchanged). The host normalizer accumulates non-text blocks per message lane alongside
  streamed text. JSONL keeps compact wire shapes: prefer `uri` + `mimeType` over inline
  base64 when the agent supplies a durable URI; otherwise image data stays on the replay
  event. ACP `ToolCallContent::Terminal` and `terminal/*` are never advertised, mapped, or
  rendered.
- Live session chrome follows ACP `session_info_update`: when the agent advertises
  a `title`, the host stores it as live session state and exposes it on the snapshot
  as `sessionTitle` (omit when none; `title: null` clears). Updates republish to
  connected browsers without reconnect or model change. This is agent-reported session
  chrome only — it does not replace the Ajax task handle or become Core task truth.
  Updates are not JSONL transcript rows.
- Ajax `initialize` advertises `clientCapabilities.session.configOptions.boolean:
  {}` so harnesses may expose boolean Fast; filesystem and terminal capabilities
  remain false. Cursor `_meta.parameterizedModelPicker` is a vendor extra, not the
  model contract.
- In-band apply maps the desired pin onto **currently advertised** options: find
  selectors by `category` (`model`, `thought_level`, `model_config`, `mode`) with
  id fallback; send `session/set_config_option` with the advertised `configId`
  and value id (select) or `{ type: "boolean", value }` (boolean). Never send Ajax
  catalog ids or reconstructed bracket tokens on the wire
  ([#954](https://github.com/mossipcams/ajax-cli/issues/954)). A Cursor pin is an
  exact full match: split-axis send of base plus advertised effort/Fast, or one
  exploded advertised id whose parsed intent matches, including Fast. Otherwise
  refuse before ACP. After apply, verify `configOptions.currentValue` against that
  same intent; persist Ajax collapsed pipe-form (`claude-opus-5-thinking|effort=high|fast=false`)
  even when the harness currentValue is exploded (`claude-opus-5-thinking-high`).
  Pin satisfaction
  is per-option `currentValue` match, not string equality on a synthetic id.
- Cursor spawn `--model` is a launch hint only (`grok-4.6` when Auto/unspecified;
  catalog ids unchanged for explicit pins). Legacy WebSocket `set_model` persists
  desired `session_model` first, then applies in-band; keep process, `sessionId`,
  and JSONL. Respawn (`session/new`, no resume) only when the child is dead or no
  model control is advertised. In-band refusal is a typed error; the child keeps
  running ([#989](https://github.com/mossipcams/ajax-cli/issues/989)).
- Connected session model, effort, and Fast controls list only advertised
  `sessionConfigOptions`. New Task before a session exists uses
  `GET /api/session/models`. Deprecated `models.availableModels` is not authority.
  Verification requires matching Fast bracket flags: a non-Fast catalog pin such as
  `cursor-grok-4.6-high` must not be satisfied by `grok-4.6[effort=high,fast=true]`
  or Composer Fast. When spawn or resume/load still leaves a different model
  running while a pin is unsatisfied and the child is alive with model control
  advertised, the host applies in-band again; it does not respawn solely because
  in-band apply was refused ([#979](https://github.com/mossipcams/ajax-cli/issues/979),
  [#997](https://github.com/mossipcams/ajax-cli/issues/997)). After a successful
  live `set_config_option`, `snapshot.model` is the harness-reported applied id. If
  apply fails because a requested value is not advertised, the host emits a typed
  `error` event and leaves `snapshot.model` on the harness-reported id (or empty),
  not the rejected pin; the child keeps running.
- Changing the model while connected applies the advertised option first, then
  persists pipe-form `session_model` from the confirmed descriptors. A refused
  pick does not persist. A persistence failure keeps the live change and reports
  that restart may restore the prior pin. Legacy WebSocket `set_model` still
  persists desired state then maps through the spawn pin path. Cross-harness
  Switch resets backend context on the same public Ajax session: cancel any
  in-flight turn, discard the host queue, shut down the old ACP child, clear the
  stored resume id, spawn the new harness with `session/new` (no resume/load, no
  transcript replay), append a host note (`Client switched harness. Context
  reset.`), and keep the TaskSession slot, JSONL transcript, and WebSocket
  identity. With no live slot, persist only and clear the stored resume id so the
  next attach uses empty context. Switch sends only `{ agent }`; a model field is
  refused. Persist `None` for Auto/unspecified; never store the literal string
  `auto` as a harness model id ([#952](https://github.com/mossipcams/ajax-cli/issues/952)).
- **Live config-option apply (MVP).** Connected pickers send WebSocket
  `set_config_option` `{ configId, value }` with the exact advertised pair. The
  command loop validates that the id and value are advertised, calls ACP once,
  replaces the complete `configOptions` list from the successful response, publishes
  the replacement snapshot, and persists pipe-form `session_model` only after ACP
  success. Refusal leaves confirmed browser state unchanged and does not persist.
  Post-apply persistence failure keeps the live change and emits a warning. Legacy
  `set_model` remains for compatibility; it persists desired state then maps through
  the spawn pin path rather than forwarding a single advertised descriptor.
- Moving a task to another harness is refused unless it was launched over ACP.
  Cross-harness Switch resets backend context on the live slot when present, or
  clears the stored resume id when idle, so the next attach spawns the new harness
  with empty context.
- Ajax orchestration sessions are trusted local automation, and the Settings
  toggle discloses that supported agents run with full tool access and without
  approval prompts. After session creation or restore, the host reads only ACP
  `configOptions`, finds the advertised `mode` option (`category: mode`, else id
  `mode`), and sends `session/set_config_option` with the first advertised select
  value from the documented full-access list (`agent-full-access`,
  `bypassPermissions`, `agent`, `code`). It ignores legacy `modes` and never
  sends `session/set_mode`; it also does not reinterpret model, thought-level, Pi,
  or unknown Cursor options. Any remaining `session/request_permission` is
  auto-approved on the host (`AllowAlways` when advertised, otherwise
  `AllowOnce`; otherwise cancelled with a warning).

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
  reducer). A cold load or full reload omits `?cursor=` and receives full replay
  with `snapshot.reset: true`; only unacknowledged prompts persist in
  `sessionStorage`.
- Each browser prompt has a stable `clientMessageId`; the host persists a
  `prompt_accepted` acknowledgement and dispatches each ID at most once. The
  browser retries only prompts still absent from that acknowledgement.
- The browser keeps an unacknowledged-prompt outbox for resend and at most **one**
  editable follow-up held in the composer; it does not maintain a second FIFO
  queue. Submitting while a turn is in flight queues that follow-up in the
  browser rather than dispatching a second `session/prompt`:
  - the queued message renders as a muted user message labelled `Queued`, and
    stays editable and removable until it is dispatched;
  - when the active turn resolves normally, the browser dispatches it as the next
    prompt;
  - a second submit while it is queued sends `session/cancel`, shows `Stopping…`,
    waits for the active prompt to resolve as cancelled, appends a `Stopped`
    divider, and only then dispatches the queued prompt. The cancelled prompt and
    the queued prompt are never in flight together.
  Everything that reaches the host is still one prompt at a time; Stop owns
  cancellation.
- Each live `TaskSession` Tokio task continues draining its ACP child and host
  queue after the last socket closes, so an in-flight or queued turn does not
  depend on browser presence.
- The per-task poll loop drains stdio whenever a live ACP client is installed,
  including finished disconnected slots during `IDLE_RELEASE_GRACE` (zero
  holders, no in-flight turn, empty host queue). That keeps host exit and late
  ACP events observable before reconnect and avoids unnecessary respawn on
  backgrounded tabs.
- Idle LRU eviction must not drop slots with a non-empty host queue **or an in-flight turn**.
- The per-task Tokio command loop continues after the last WebSocket subscriber
  detaches while a turn is in flight or the host queue is non-empty, and keeps
  draining idle grace-retained slots until eviction or shutdown.

## Shutdown and slot retention

- Dropping a task or cross-harness Switch shuts down the live ACP child; the
  TaskSession slot and JSONL transcript stay unless the task is dropped.
- A task owns its orchestration session only while it exists in the registry and
  is not in the `Removed` lifecycle ([#977](https://github.com/mossipcams/ajax-cli/issues/977)).
  Sessions with no such owner are stale: Web Cockpit initialization deletes
  their persisted JSONL transcript, and a successful Drop performs the same
  cleanup (live ACP slot plus transcript) before the qualified handle can be
  reused. Ownership is derived from registry task handles only, not browser
  routes, `localStorage`, or ACP slot state.
- Idle LRU eviction sends `Shutdown` only to slots with zero subscribers, no
  in-flight turn, and an empty host queue; evictable slots must not hold pending
  work.
- After WebSocket detach, finished disconnected slots stay out of the idle-LRU
  pool for **15 minutes** (`IDLE_RELEASE_GRACE`). During that grace window the
  live ACP child is kept and the poll loop keeps draining it so a backgrounded
  PWA or Safari tab can reconnect without paying a full spawn handshake. Once
  grace expires, the slot becomes an ordinary idle-LRU candidate (oldest
  released first) and the idle cap can reclaim it. Reattach clears the release
  marker; in-flight turns, queued prompts, and held slots are never evicted
  regardless of grace.
- WebSocket detach releases the directory holder count but does not cancel an
  in-flight turn or clear the host queue.
- `ajax-web` restart reloads JSONL transcripts and cursors from disk; live ACP
  children do not survive process exit.

## Model switching on the live ACP session

- `set_model` persists the desired model on the task, then applies it in-band on
  the live ACP session via `session/set_config_option` when a slot exists and the
  harness advertises a model control. The ACP process, `sessionId`, and JSONL
  transcript stay put; `snapshot.model` updates from the harness-reported applied
  id. In-band apply that is unadvertised or refused is a typed error; the child
  keeps running ([#989](https://github.com/mossipcams/ajax-cli/issues/989),
  [#997](https://github.com/mossipcams/ajax-cli/issues/997)). Respawn (`session/new`,
  no resume) runs only when the child is dead or no model control is advertised.
- The UI transcript on disk and in replay is unchanged except for host-emitted
  status/note events and typed model-change errors.
- A live `ready` event on an established socket must not reset browser reducer
  state when `reset` is false; only reconnect-after-drop with `reset: true` or a
  generation-change snapshot may clear and replay.
- Cold attach (no browser `?cursor=`) must send protocol v2 `snapshot.reset:
  true` so a reducer that already painted cached JSONL stand-in history is cleared
  before the authoritative replay from disk ([#1031](https://github.com/mossipcams/ajax-cli/issues/1031)).
- A later protocol-v2 `snapshot` on an already-ready WebSocket with `reset: true`
  (for example after host generation change) must dispatch `ready` with
  `reset: true` so the browser replaces cached rows instead of appending duplicates
  ([#1031](https://github.com/mossipcams/ajax-cli/issues/1031)).
- Incremental in-page reconnect with a resume cursor stays `reset: false` and
  applies only the tail after the last-applied cursor.
- Each attach delivers one protocol v2 `snapshot` wire frame to the browser
  reducer (as a synthetic `ready` event for turn state), then cursor-bearing
  `event` envelopes for replay/live traffic.
- After the last replay envelope below `snapshot.cursor`, the browser reapplies
  that snapshot's turn state as a non-resetting synthetic `ready`. Historical
  prompt acknowledgements, agent messages, and tool rows therefore cannot
  override an authoritative idle snapshot and disable model/effort controls
  ([#994](https://github.com/mossipcams/ajax-cli/issues/994)).
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
  the host suppresses transcript-shaped replay during `session/resume` and
  `session/load` until the live session is installed on the slot. Capability
  updates (`config_option_update`, `available_commands_update`,
  `session_info_update`) still flow during handshake. Transcript replay must not
  land in JSONL even when notifications arrive after spawn returns
  ([#1031](https://github.com/mossipcams/ajax-cli/issues/1031)).
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
- New Task lists the full harness catalog from `GET /api/session/models`. After
  connect, model / effort / Fast controls bind only to advertised
  `sessionConfigOptions`; there is no separate task-details model picker
  ([#979](https://github.com/mossipcams/ajax-cli/issues/979)). The connected
  model sheet presents those advertised options in New Task's picker vocabulary:
  advertised models as full-width radio rows, then an **Effort** row and a
  **Fast** Off/On row. Only the model list scrolls, so Effort and Fast stay
  reachable under a long catalog. A model the host reports but does not
  advertise renders as a disabled selected row, because the bridge accepts only
  advertised values. New Task shares `ModelPicker` (`features/task`). Bridge harnesses keep the reasoning picker from
  `catalog.reasoning`. Cursor New Task shows one row per model base (Fast catalog
  variants collapsed), an **Effort** row when the catalog encodes multiple levels
  on that base, and a **Fast** Off/On row (default Off). Selection persists as
  pipe-form `session_model` such as `grok-4.6|effort=high|fast=false` or
  `composer-2.5|fast=false`; Auto stays `auto`. ACP bracket-form live snapshots
  such as `gpt-5.6-sol[fast=false]` decode to the matching catalog base and expose
  effort controls; choosing a level emits pipe-form with `effort=`. Legacy exploded
  catalog ids such as `cursor-grok-4.6-high` still decode in the New Task picker
  and apply on the backend. For Cursor, `GET /api/session/models` still reads ids
  and effort/fast axes from `agent models`, but row labels are overlaid from a
  short-lived Cursor ACP handshake (`choice.name` on the advertised `model`
  option) so New Task matches the connected switcher; when that overlay is
  unavailable the route fails open to the CLI labels. **Switch** is harness-only: it opens with the current
  harness disabled, sends `{ agent }` for a different harness, clears the prior
  pin, and resets backend context so the new harness starts with empty context
  while prior chat turns stay visible. A failed catalog read shows an
  operator-visible error with retry; it must not fall back to Auto plus the live
  session model ([#948](https://github.com/mossipcams/ajax-cli/issues/948)).
- The ACP client keeps v1 `SessionNotification` values typed through mapping.
  Message, thought, tool, plan, mode, configuration, session-info, usage,
  turn_usage, and status-like updates have explicit mappings; unsupported
  capability announcements are ignored by the chat projection.
- v1 `sessionUpdate: "status"` and raw `state_update` both map to head-only
  `status` wire events. ACP `status.state` (`running` / `waiting` / `idle` /
  `requires_action`) drives the live-head primary label (Working / Needs you /
  Ready); human `detail` is stored but not shown as a quiet line.
  Typed `CurrentModeUpdate` is ignored like capability announcements.
- Tool calls carry their ACP `content` array to the browser as `text` and `diff`
  entries. Dropping it left the browser able to say only that an unnamed edit
  happened. A `tool_call_update` without `content` revises the other fields and
  keeps the content already received.
- `usage_update` is a first-class `usage` event, not an `artifact`. A zero
  window means the harness does not report context and is dropped, so it never
  renders as 0% used. When the harness reports a non-zero window, the live head
  shows the current fraction (`Context N% full`) in idle and working states; at
  90%+ the indicator uses the warning tone.
- Per-turn token usage from `session/prompt` result.usage maps to a separate
  `turn_usage` wire event. Cursor reports camelCase fields (`inputTokens`,
  `outputTokens`, `cacheReadTokens` / `cachedReadTokens`, `cacheWriteTokens` /
  `cachedWriteTokens`, `totalTokens`); snake_case equivalents normalize to the
  same shape. Missing fields are omitted from the wire event, never sent as zero.
  When `totalTokens` is absent but component fields are present, the host may
  emit a summed total. Duplicate usage for the same request, generation, or turn
  id is dropped. Cursor does not emit standard `usage_update` events, so context
  pressure stays unknown unless another harness reports it — per-turn tokens must
  not populate the context meter. When the host emits `turn_usage`, the live head
  shows a quiet line (`Turn tokens: input N · …`) listing only the counts that
  were present on the wire; missing input, output, cache, or total fields are
  omitted rather than shown as zero. Context pressure and per-turn tokens may
  both appear; they stay separate indicators.
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
- ACP `session/request_permission` is auto-approved on the host for trusted Ajax
  Chat sessions: the adapter selects an advertised allow option (`AllowAlways`
  when present, otherwise `AllowOnce`) or the standard cancelled outcome when no
  allow option exists. Cancellation via `session/cancel` still resolves any
  still-pending permission with the cancelled outcome before sending cancel; in
  the normal path there should be none because the host answers immediately.
- Operator answers to permission requests that remain pending (tests and legacy
  wire) are recorded as `permission_resolved` in the host transcript. Live Chat
  does not emit `permission_request` for auto-answered asks.
- When the operator answers but the ACP request is already gone (for example
  after host auto-approve), the host still records `permission_resolved` so
  reconnect replay cannot resurrect the prompt
  ([#1018](https://github.com/mossipcams/ajax-cli/issues/1018)).
- Live Chat clears the head permission panel immediately on Approve/Reject;
  it does not wait for a matching `permission_resolved` replay event
  ([#1018](https://github.com/mossipcams/ajax-cli/issues/1018)).
- Reconnect or full page reload replay must not resurrect a permission prompt
  whose `requestId` already has a matching `permission_resolved` entry.

## Form elicitation (slice 4)

- Ajax `initialize` advertises `clientCapabilities.elicitation.form: {}` only.
  URL elicitation is not advertised and requests in URL mode receive JSON-RPC
  invalid params (`-32602`) on the host.
- ACP `elicitation/create` in form mode is held on the host until the browser
  operator answers. Accept sends schema-shaped `content`; Decline and Cancel
  send the standard non-accept outcomes and do not include content.
- Form schemas must not collect secrets, passwords, or tokens. The host rejects
  secret-like field names before surfacing a prompt to the browser.
- Live Chat treats elicitation as an agent request in the session head (not task
  registry truth). The head shows a schema-driven form with Accept, Decline, and
  Cancel; elicitation takes precedence over permission when both are pending.
- Operator answers are recorded as `elicitation_resolved` in the host transcript.
  Live Chat clears the head form immediately on answer; it does not wait for a
  matching `elicitation_resolved` replay event.
- Reconnect or full page reload replay includes `pendingElicitation` on the
  session snapshot when an unanswered form elicitation remains open, using the
  same pending-permission snapshot pattern.
- `session/cancel` resolves any still-pending elicitation with the cancelled
  outcome before sending cancel, mirroring permission cleanup.

## Session close (slice 6)

- During ACP `initialize`, the host reads `agentCapabilities.sessionCapabilities.close`
  and stores whether `session/close` is advertised on the live stdio client.
- When tearing down a live ACP child (TaskSession shutdown, harness Switch, slot
  replacement, idle eviction, or client drop), the host sends ACP `session/close`
  for the current session id and waits for the response (bounded timeout) before
  killing stdio when close is advertised. When close is not advertised, teardown
  keeps today's cancel-then-kill behavior.
- ACP `session/close` ends only the agent-side session on the child. It does not
  Drop the Ajax task, delete JSONL, or touch Ajax Terminal/tmux.
- Close failure or timeout still tears down the child; the host appends a typed
  `error` session event rather than hanging the slot.

## In-flight activity freshness

- The host-reported unresolved prompt remains the only authority for whether a
  turn is in flight; browser timers do not alter task or session lifecycle.
- While a turn is in flight, the live head measures time since the most recent
  ACP event. After one minute without an event it says `No recent activity` and
  shows the elapsed minutes while preserving Stop. A later event resets the
  indicator, and turn completion removes it.
- This is a freshness warning, not a claim that the agent is stalled: stable
  ACP v1 has no portable stalled-state signal. The head must not invent
  thinking content from that timer.
- The turn's activity row narrates the operation, in the transcript, where the
  conversation is. The head does not repeat it: printing the running tool and
  active plan step a screen away from the same words gave the operator two live
  regions with a void between them. The head shows `Thinking…` only until the
  turn's first thought, plan or tool arrives — before that the transcript has
  nothing to show — and otherwise carries state, Stop, freshness and context
  usage alone.

## ACP run-state as task evidence

A provisioned chat task has no agent pane, so the supervisor's pane classifier
has nothing true to say about it: without this the dashboard, task page, TUI and
`ajax status` read a pane-derived `Waiting` through an entire ACP turn, while
only the chat live head knew a turn was in flight.

- The ACP host is the observer of its own child, so it reports run state on the
  same contract the supervisor uses: a `LiveObservation` applied to the task.
  Status derivation (`ui_state::derive_task_status`) is unchanged — this supplies
  evidence, it does not add a second status vocabulary, and the browser remains a
  projection.
- Transitions are read off the outbound wire the browser already receives, so the
  task page and the chat head cannot disagree: `prompt_accepted` and a resolved
  ask report `AgentRunning`; `permission_request` / `elicitation_request` report
  `WaitingForApproval`; `turn_end` reports `Done`, or `Blocked` when the turn
  errored. Detail inside a turn (messages, tool calls, usage) reports nothing.
- Only tasks with `skip_interactive_agent()` accept this evidence. An interactive
  tmux task is the supervisor's to observe, and two producers writing one field
  is how a status starts oscillating.
- Reporting is best-effort and off the turn's critical path: a lost race with
  another writer is dropped, never surfaced to the operator or allowed to
  interrupt the turn it described.
- A runtime refresh must not overwrite this evidence with a shell reading; the
  provisioned task keeps its ACP-reported state.

## Transcript composition

The transcript is a conversation, not the ACP event stream. It contains only
user messages, assistant responses, one activity disclosure per run of adjacent
work,
permission asks the operator still owes an answer to, agent form elicitation
asks still open, errors, and hairline
dividers for cancellations, reconnects, harness switches and context resets.

- A turn chapter is the user message and everything that followed it, **in the
  order it arrived**: an agent that speaks, works, then speaks again reads that
  way. Adjacent work items collapse into one disclosure; prose between them ends
  the run and opens the next. Items are keyed by host `itemId` / `toolCallId`, so
  replay after a reconnect updates existing rows instead of appending duplicates.
- Assistant responses are revealed by completed paragraph, never token by token,
  and never split inside a fenced block. The whole response renders once the turn
  ends. The paragraph gate applies only to the row still being written: once a
  tool call, an ask or a later message follows a message, that message is
  finished and renders whole, so a one-paragraph "Let me look at the handler."
  is not withheld for the rest of the turn.
- A plan belongs to the turn that produced it; a later turn opens its own plan
  row rather than rewriting the first one.
- A turn that ends settles the tool calls it left open (`cancelled`, or `failed`
  when the turn errored). ACP need not send a terminal update for a call the
  operator stopped, and an unsettled call otherwise reads as in flight for the
  rest of the session.
- A turn reports its failure once: the generic "stopped without a response" note
  is for a turn that produced neither an answer nor a host error, never a second
  line under one the host already explained.
- Permission and elicitation titles are stripped of markdown delimiters at the
  projection boundary, so every reader — approval control and transcript marker
  alike — shows the command as it will run.
- The activity disclosure carries thoughts, plans, tool calls, command output and
  diffs. Collapsed, it shows the current operation while the turn runs (replacing
  it, never appending) and a counted summary once the turn settles
  (`Read 6 files · edited 2 files · ran 4 commands · 38s`). A completed tool call
  leaves the collapsed row as soon as ACP reports completion — there is no
  collapse timeout.
- It expands itself for failed, blocked and approval-required activity; a manual
  open or close by the operator wins from then on.
- Reasoning is a row on the activity grid inside that disclosure, never an italic
  message in the conversation.
- The transcript opens positioned at its latest content, follows new content only
  while the operator is already at the bottom, offers `Jump to latest` whenever
  the operator is away from the live edge — not only when new content has arrived
  since — and never animates a scroll.
- Conversation text and tool labels are proportional; monospace is reserved for
  code, commands, paths and output.

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
  chat** in terminal task details (header Details sheet pinned primary tools row
  or footer Task details disclosure) clears the preference.
- On `#/t/<handle>`, the header Details sheet pins Ajax chat outside
  `.session-details-body` so it appears in the first viewport; the sheet uses
  contained overflow with the body as the bounded scroller (iOS-safe). While
  `html.terminal-expanded`, the header Details control remains reachable without
  permanently reserving terminal band space.
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

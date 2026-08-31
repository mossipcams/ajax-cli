# Web Cockpit Architecture

Browser adapter ownership, slices, runtime, terminal, speech, and persistence.

Operator setup for speech lives in [`docs/speech-input.md`](../speech-input.md).


`ajax-web` is the browser Cockpit adapter. It is a vertical presentation adapter
over the same Cockpit projection and task-operation contracts used by Native
Cockpit. It may shape responses for browser ergonomics, but it must not own task
lifecycle rules, registry truth, runtime reconciliation, Git/tmux
interpretation, substrate evidence, operation outcomes, or action policy.

Web Cockpit is a first-class browser operator surface that is dashboard-first.
Opening a task enters one **Task Workspace** — shared task header and task
details, **Ajax Chat**, **Ajax Terminal**, and Diff Review navigation — not a
single-surface terminal page by default. Native Cockpit and Web Cockpit consume
shared Cockpit projections and task-operation contracts; neither surface owns
task truth. The dashboard still leads with task state, required decisions, and
next actions; the workspace then composes the peer modes below.

### Task Workspace (product boundary)

The Task Workspace is the browser product boundary for a selected task handle.
Public hashes remain `#/session/<handle>` (Ajax Chat) and `#/t/<handle>` (Ajax
Terminal). Bare `#/session` is the New Task sheet, not a workspace.

| Surface | Role |
| --- | --- |
| **Task Workspace composition** | Mode selection, per-task view preference, capability fallback, Back and Diff routing, one shared task header, one shared task-details sheet, and composition of task actions, metadata, and harness switching |
| **Ajax Chat** | Multi-harness ACP orchestration chat (Cursor native; Codex, Claude, and Pi via their ACP bridges). Default for provisioned, session-capable tasks when orchestration chat is enabled and Terminal is not preferred |
| **Ajax Terminal** | Authenticated raw xterm.js/tmux bridge to the task tmux session. Required for interactive/non-session-capable tasks, when the operator selects Terminal, or when session attach is unavailable |

**Frontend import boundaries (production):** cross-feature coupling goes only
through each feature's `public.ts` (`features/task-workspace/public.ts`,
`features/chat/public.ts`, `features/terminal/public.ts`,
`features/task/public.ts`, `features/settings/public.ts`). Inside Ajax Chat,
`ChatSurface.tsx` is the sole composer of top-level capabilities
(`composer/`, `conversation/`, `scrolling/`, `status/`, `permissions/`,
`elicitation/`,
`model/`, `session/`). Capabilities import `session/public` and shared UI only;
`conversation/` may also import `activity/public`. Raw protocol transport stays
in `features/chat/session/transport/` and does not escape `session/public`.
Task owns the reusable desired-model catalog and `ModelPicker` used by New Task
and harness Switch; Chat owns applying a selected model to the live ACP session.
Settings owns orchestration-chat enablement storage
(`orchestrationChatPreference.ts`); App passes the value to New Task and Task
Workspace. ESLint (`npm run web:lint`) enforces these paths on production files;
tests may import fixtures across boundaries.

Chat and Terminal are peer views of the same task. Neither owns task metadata,
task actions, harness switching, mode preference, or Diff routing. Those stay
with workspace composition (`TaskDetailsSheet`, head-action composition, and
terminal header Details wiring in `features/task-workspace`; terminal footer
`Task details` disclosure remains in `TaskMetaDetails`). Chat-owned CSS lives
under `styles/chat/`; task-details sheet CSS lives under
`styles/task-workspace/`. Both ship through the single `styles.css` manifest.

Interactive tasks whose tmux pane is still running the harness agent, and tasks
whose projection is not `session_capable` with no ACP entry point, open
Terminal (`#/t/<handle>`) instead of Chat. ACP-capable tasks whose agent has
exited may still reach Ajax Chat from task details or `#/session/<handle>`; the
host promotes them on attach. A per-task browser preference
(`ajax.web.taskView.terminal` in localStorage) selects Terminal for
session-capable tasks until cleared from task details. Diff Review
Back returns to the mode the workspace selected.

An optional flag-gated **Cursor ACP orchestration chat** session mode is
specified in [`web-session-behavior.md`](web-session-behavior.md). The
preference defaults **on** (`ajax.web.session.orchestrationChat` in
localStorage; only an explicit `false` disables it). When off, dashboard and
embedded terminal behavior is unchanged.
The browser session client lives under `features/chat/session/transport/`; the
`useChatSession` hook and pure session reducers fold validated protocol v2 frames
into typed `ChatSessionView` state; they retain only unacknowledged prompt IDs,
the last applied transcript cursor, and transient UI state. They do not own the
transcript, prompt queue, or ACP process.

The Settings **Orchestration chat session** toggle (`ajax.web.session.orchestrationChat`,
default **on**, storage in `features/settings/orchestrationChatPreference.ts`)
gates `#/session`, discloses full tool access without approval
prompts for supported agents, and makes task creation provisioned: with it on,
the New task sheet calls `startTask` with `orchestration_chat: true` for the
chosen harness. When the flag is on,
`#/session/<handle>` renders the Task Workspace in chat mode: the shared task
header, then ChatSurface (composition only: live head, transcript, composer,
model chrome, and permission/elicitation slot) for
session-capable tasks that prefer chat; **Ajax terminal** in task details switches
to `#/t/<handle>` and remembers that choice in browser localStorage
(`ajax.web.taskView.terminal`). **Ajax chat** in the footer Task details
disclosure (`TaskMetaDetails`, summary "Task details") and the header Details
sheet (pinned primary tools row outside the scrolling body, same reachability
pattern as Ajax terminal on the session Details sheet) clears the preference and
returns to `#/session/<handle>`. Session routes omit dashboard `.cockpit-chrome`;
the session route scroller owns `env(safe-area-inset-top)` so the shared task
header clears the iPhone notch. The Details sheet is iOS-safe: `.session-details-sheet`
contains overflow and `.session-details-body` scrolls. While the terminal is
fullscreen (`html.terminal-expanded`), the header Details control stays
reachable without permanently reserving terminal band space. Diff Review remains swipe-left.
The browser still does not own transcript/queue.

When that mode is enabled, agent conversation runs over ACP stdio via the
`ajax-web` ACP adapter, not PTY paste. Model catalog parsing lives in the
`session_models` slice. Authenticated `GET /api/tasks/{handle}/session` WebSocket
upgrade is transport-only: runtime routes resolve the core-backed attach plan,
acquire a `TaskSessionDirectory` handle, and delegate to
`slices::web_session::ws_bridge`, which forwards typed browser commands and
protocol v2 envelopes only. Optional `?cursor=` on reconnect requests incremental
replay; cold load omits it for full replay.

The adapter uses the official Rust `agent-client-protocol` runtime for JSON-RPC
framing, request correlation, and typed ACP messages. It initializes with stable
protocol v1 and rejects a peer that selects another version. Session restore
prefers `session/resume` when advertised, falls back to `session/load`, then
creates a new session. Trusted local orchestration auto-approves every ACP
`session/request_permission` on the host by selecting an advertised allow option
(`AllowAlways` when present, otherwise `AllowOnce`; otherwise the standard
cancelled outcome). Auto-answered requests are not surfaced to the browser.
Ajax advertises form elicitation only (`clientCapabilities.elicitation.form: {}`);
URL elicitation is refused on the host. Pending form elicitation is replayed on
reconnect via `pendingElicitation` on the session snapshot and answered through
the browser head form (Accept / Decline / Cancel). After
session create or restore the adapter also sends `session/set_config_option` when
the harness advertises a documented full-access `mode` select value
(`agent-full-access`, `bypassPermissions`, `agent`, or `code`; first match wins)
so agents may stop asking entirely. Legacy `modes` and `session/set_mode` are
intentionally unsupported.

ACP is per harness, not Cursor-only. `acp_launch_for_agent` in core maps each
harness to its ACP entry point and to how it accepts a model:

| Harness | ACP entry point | Model selection |
| --- | --- | --- |
| Cursor | `agent acp` (native) | `--model` launch hint; live switch via advertised `configOptions` |
| Codex | `codex-acp` | `session/set_config_option` |
| Claude | `claude-agent-acp` | `session/set_config_option` |
| Pi | `pi-acp` | `session/set_config_option` |

Every bridge answers `session/set_config_option { sessionId, configId, value }`,
which carries both the model and the reasoning level those harnesses expose as a
**separate** option (`effort`, `reasoning_effort`, `thought_level` — matched by
its `thought_level` category). Cursor has no second axis: its model ids already
name the level. A selection is therefore stored as `model|configId=value`, e.g.
`opus|effort=low`, parsed by `parse_model_selection` in core and applied one
config option at a time.

Cursor is the only harness that speaks ACP itself today; the others are reached
through their Agent Client Protocol adapters, which are separate installs:
`@agentclientprotocol/codex-acp`, `@agentclientprotocol/claude-agent-acp`, and
`pi-acp`. `ajax doctor` reports each one as `acp:<harness>` and names the package
when it is missing, and the host falls back to `npx -y <package>` so a host
without the global install still gets a session.

Harness binaries are resolved through `adapters::program`: the server's own
`PATH`, then the operator's login shell. `ajax-cli web` runs under tmux or a
service manager, so a version manager moving `codex` or an adapter between node
versions would otherwise make a harness silently invisible — which the catalog
then reported as "no models". Each mapping also names the harness CLI that *could*
serve ACP natively (`codex`, `claude`, `pi`), and the host prefers that CLI as
soon as its `--help` advertises an `acp` subcommand — asked rather than
attempted, because an unknown argument is a prompt to some CLIs. A harness with
no mapping keeps the tmux send-keys launch, and when no candidate program can be
spawned the host reports an install hint rather than a spawn error.

A session with no operator-chosen model runs the harness default: Cursor ACP spawn
uses [`CURSOR_DEFAULT_SPAWN_MODEL`] (`agent --model grok-4.6 acp`); the Ajax catalog
default [`CURSOR_DEFAULT_MODEL`] remains the attach-plan and UI default for Cursor.
Bridge harnesses are left to pick for themselves.

A provisioned start (`orchestration_chat: true`, no send-keys) is therefore
offered for every mapped harness, and session attach admits any task whose agent
has an ACP launch **and** whose registry metadata carries the provisioned bit.

Cards and task detail carry that same answer as `session_capable`, so the browser
opens chat for provisioned tasks the host will attach. An interactive task with
a live harness in tmux opens its terminal instead; a session URL for that case
falls back to the terminal route. `session_capable` stays the provisioned bit and
is not cleared when an ACP child exits; Ajax chat also stays visible in task
details for ACP-capable agents before promotion.

The **New task** sheet is two steps: repository/title/harness, then a model page
listing the full `GET /api/session/models?agent=` catalog for that harness.
Bridge harnesses show a reasoning level beside the model list when the handshake
advertises one. Cursor collapses effort and Fast out of its catalog ids:
`GET /api/session/models?agent=cursor` serves unique model bases
(`composer-2.5`, `grok-4.6`, …) with optional `efforts[]` and `hasFast` derived
from exploded `agent models` siblings; thinking ids such as `claude-opus-5-thinking`
stay separate from their non-thinking base. The picker shows one row per base, an
**Effort** row when multiple levels exist (live `thought_level` unioned with
catalog `efforts[]` when connected so sparse live choices cannot hide advertised
Grok levels, otherwise catalog `efforts[]` when the selected base has more than
one), and a **Fast** Off/On row when live
boolean Fast is advertised or the catalog row has `hasFast` (default Off; Auto is
never Fast). New Task and Switch persist pipe-form `session_model` such as
`grok-4.6|effort=high|fast=false`. In-band apply maps that full selection onto
advertised `configOptions`: send base plus effort/Fast when that split-axis
contract exists; otherwise send one exploded catalog id that matches the whole
Cursor intent (`claude-opus-5-thinking-high` for
`claude-opus-5-thinking|effort=high|fast=false`). Reject before ACP when neither
is an exact full match. After a successful apply, `snapshot.model` stays the
harness `currentValue`; task `session_model` storage keeps Ajax collapsed
pipe-form. The catalog endpoint serves the complete list —
Cursor from collapsed `agent models`, the bridges from their own `session/new`
handshake.

That handshake costs a short-lived bridge process, so the catalog is cached
against the **harness CLI version** rather than a clock: each request reads
`<harness> --version` (cheap), reuses the stored catalog when it matches, and
re-reads the catalog only after the harness has been updated. A version that
cannot be read is never treated as a cache hit.

The chosen selection is stored on the task (`session_model` metadata) and applied
when its session starts; `POST /api/tasks` validates its shape and rejects a
model for an agent with no ACP launch. Bare `#/session` opens the same New Task
sheet as the dashboard (orchestration chat pre-selected when the flag is on);
the duplicate Cursor-only Session Starter is removed
([#911](https://github.com/mossipcams/ajax-cli/issues/911)).

`POST /api/tasks/{handle}` with `{ "agent" }` is **Harness Switch**. A `model`
field is refused (`unsupported_capability`). Same-harness Switch is refused;
connected model, effort, and Fast changes use the composer controls. Cross-harness
Switch clears the prior harness model pin, resets backend context on the live slot
(cancel in-flight work, shut down the old ACP child, spawn the new harness with
empty context), and keeps the TaskSession slot and JSONL transcript; with no live
slot, persist `agent` with `session_model: None` and clear the stored resume id so
the next attach uses `session/new`. Switch is refused for a task that was launched
interactively, because that task's agent is live in its tmux pane and the registry
must not name a harness that is not the running process.

When spawn argv or resume/load leave a model that does not match the operator pin
(for example Cursor CLI default Composer Fast while Grok High was chosen), the
session host respawns only when the ACP child is dead or the harness advertises no
model control: drop the child, `session/new` (no resume), then apply the pin
in-band again ([#979](https://github.com/mossipcams/ajax-cli/issues/979)).
When in-band apply fails because a requested value is not advertised, the host
emits a typed error, keeps the child running, and leaves `session_model` as the
operator pin ([#997](https://github.com/mossipcams/ajax-cli/issues/997)).
Ajax advertises `clientCapabilities.session.configOptions.boolean: {}` and Cursor
`_meta.parameterizedModelPicker: true` on ACP `initialize` (filesystem and terminal
capabilities remain false). Protocol v2 snapshots carry `sessionConfigOptions` as
the live connected-control contract and optional `availableCommands` as the live
slash-command contract (pass-through on `session/prompt`; not transcript) and optional
`promptCapabilities` as the live rich-prompt attach contract (from ACP initialize; not
transcript) and optional `sessionTitle` as live agent-reported session state (from ACP
`session_info_update`; on the v2 snapshot wire but not rendered in workspace header chrome;
not transcript or Core task truth). New Task still lists models from
`GET /api/session/models`. `snapshot.model` is the model option's `currentValue`
only. In-band refusal leaves `session_model` as the prior restart pin and
`snapshot.model` on harness-reported evidence.

**Slash commands (ACP).** When `snapshot.availableCommands` is present, the chat
composer offers prefix completion for advertised `/name` commands (Tab, Enter, arrow
keys, and tap-to-insert on iOS Safari). Submitting `/name` plus optional args sends
that exact string on `session/prompt`; Ajax does not parse or rewrite slash input.

**Rich prompt content (ACP).** When `snapshot.promptCapabilities` advertises `image`
and/or `embeddedContext`, the chat composer exposes a tappable Attach control
(iOS Safari–safe; not hover-only) and accepts image paste when `image` is
advertised. The file picker attaches `image` blocks when `image` is advertised
and embedded `resource` bodies when `embeddedContext` is advertised; it does
not synthesize `resource_link` stubs for local files. `resource_link` remains
valid on the host wire for real URIs supplied by agents or other surfaces.
Submitting still requires typed text and sends
`{ type: "prompt", text, clientMessageId, contentBlocks? }`; the browser
downscales/compresses attached photos so the JSON frame fits the 8 MiB WebSocket
cap (with headroom for typed text and up to eight image blocks) before send. If
compression cannot fit the frame, the composer keeps the attachment and surfaces a
specific error (not the generic “shorten the message” prompt). The host
revalidates frame size, base64, image MIME/format, block count, and advertised
capabilities before mapping to ACP. Queued attachment bytes stay in memory only
and are not written to `localStorage`. The host forwards a full ACP
`ContentBlock` array and keeps JSONL to text plus attachment names only.

**Non-text output (ACP).** Agent/user/thought message updates and tool-call content may
carry `image`, `resource_link`, or embedded `resource` blocks. The host maps them into
`message.contentBlocks` and extended `tool_call.content` wire payloads (text and diffs
unchanged). JSONL omits redundant base64 when a durable `uri` is present; otherwise image
data stays on the replayed event. Ajax does not advertise or render ACP `terminal/*`
embeds.

**Session title (ACP).** When `snapshot.sessionTitle` is present, the host carries it on
the live session snapshot for protocol consumers. The task workspace header shows only the
Core task title (`detail.title` / handle); it does not render `sessionTitle`. Agents often
mirror the first prompt there, so displaying it duplicated the task identity and blew up
header chrome ([#1055](https://github.com/mossipcams/ajax-cli/issues/1055)). It does not
rename the task in Core or replace the Ajax handle.

**Session close (ACP).** When the agent advertises `sessionCapabilities.close`, the
host sends `session/close` only on terminal ends: task Drop and cross-harness Switch.
Idle eviction, `ajax-web` restart, and same-session respawn detach stdio without
close so `session/resume` / `session/load` can restore the stored id
([#1061](https://github.com/mossipcams/ajax-cli/issues/1061)). Close ends the
ACP session on the child only; Ajax task truth, JSONL transcripts, and tmux
terminals are unchanged. Close failure or timeout still tears down the child and
surfaces a session error event.

**Connected model controls (MVP).** When `snapshot.sessionConfigOptions` advertises
model, effort/thought-level, and/or Fast options, the chat composer hotbar exposes
pessimistic pickers bound to those descriptors only. Each pick sends WebSocket
`set_config_option` with the exact advertised `configId` and string or boolean
value; the UI keeps the last confirmed value until a replacement snapshot arrives.
After ACP accepts the change, the host replaces the complete advertised option
list, derives pipe-form restart storage from the confirmed descriptors, and persists
`session_model` through the existing core operation. Refusal does not persist or
change confirmed browser state; persistence failure keeps the live change and
reports a warning. New Task and idle catalog selection still use only
`GET /api/session/models?agent=` — there is no second option-catalog endpoint.

**Harness Switch (MVP).** Cross-harness Switch in task details sends only the target
harness (no model picker). It clears the prior harness model pin, resets ACP
context, and keeps the transcript. Same-harness model changes use the connected
composer controls (`set_config_option`), not Switch.

Orchestration chat transcripts persist as JSONL under ajax-web `state_dir`
(`web-session/<encoded-handle>.jsonl`), not in the registry or tmux. Prompt
ownership and idempotency persist separately in an atomic sidecar ledger
(`web-session/<encoded-handle>.prompt-ledger.json`). The
`web_session` slice owns per-task session runtimes (`TaskSessionDirectory` +
one `TaskSession` Tokio command loop per handle): FIFO prompt queueing,
request-ID-correlated one-in-flight turns, durable terminal-before-next
advancement, cancellation, model switching, permission answers,
elicitation answers,
  idempotency, subscriber fan-out, idle LRU retention, ACP child-exit reconciliation,
  transactional replacement, and transcript cursors.
JSONL persistence lives in `adapters::web_session_store` (transcript append +
prompt ledger sidecar). ACP stdio and typed
request/notification I/O remain in `web_session_acp`; the slice maps
`AcpClientEvent` values into persisted session events and normalizes streamed
agent/thought text to full-content updates with stable host `itemId` values
before persistence.

### Task-session module ownership

Production sources under `slices::web_session` split orchestration-chat by
mechanism. Architecture tests in `crates/ajax-web/src/architecture_web_session.rs`
enforce the forbidden-import table below (production `.rs` only; test modules are
excluded). See also `.planning/agent-plans/architecture-granular-rules.md`.

| Module | Owns |
| --- | --- |
| `protocol` | v2 snapshot/event envelopes |
| `acp_map` | ACP update → `SessionServerEvent` |
| `normalize` | host stream normalization / item ids |
| `acp_usage` | usage dedupe |
| `acp_slot` | live ACP child and advertised session capabilities |
| `prompt_queue` | active/queued prompt and durable ledger ownership |
| `session_evidence` / `session_error` | activity/fault evidence and typed failures |
| `replay` | cursor replay planning |
| `transcript` | in-memory cursor and permission filter |
| `ws_bridge` | socket forward to directory |
| `session_cleanup` | registry-owned JSONL retention |
| `model_change` | harness-switch reset via directory |
| `task_session*` / `acp_drain` | command loop, spawn, ACP poll (may call adapters) |
| `adapters::web_session_acp` | ACP stdio only |
| `adapters::web_session_store` | JSONL transcript + prompt ledger sidecar |
| `runtime` production | cookie/origin, attach plan, delegate to `ws_bridge` |

Each row's production code must not import sibling internals listed in the
architecture test forbidden table (for example mapping modules must not reach
into the store, ACP client, command loop, or runtime route layer).

### Task-session wire protocol (v2)

Each attach sends one protocol v2 `snapshot` frame
(`protocolVersion`, `cursor`, `model`, `turnState`, `reset`, optional
`pendingPermission`, optional `sessionConfigOptions`, optional `availableCommands`, optional
`promptCapabilities`, optional `sessionTitle`, optional `transcriptError`) followed by cursor-bearing
`event` envelopes whose
`payload` is the existing typed session event union. The `model` field is the
harness-reported applied id after handshake apply, not the task desired pin
([#952](https://github.com/mossipcams/ajax-cli/issues/952)). Every persisted row has a
monotonically increasing absolute cursor. Reconnect supplies the browser's last
applied cursor on `?cursor=` and receives only newer envelopes; invalid or
compacted-away cursors force `reset: true` and bounded full replay from the
store's retained floor. Cross-language JSON fixtures live under
`crates/ajax-web/testdata/web_session/` and `crates/ajax-web/web/testdata/web-session/`.

Prompt frames carry a browser-generated `clientMessageId`. The host records a
`prompt_accepted` event and ignores duplicate IDs, while the browser keeps
unacknowledged prompts in a session-scoped outbox and retries them after a
socket drop. Unsent composer textarea text and the one editable queued
follow-up are stored separately in `localStorage` per task handle for restore
after navigation or tab close; drafts are text-only presentation state, queue
entries require text and may include JSON-serializable content blocks, and both
are cleared on send, queue removal/dispatch, or committed Drop. Each live
`TaskSession` continues draining its ACP child and host
queue after the last browser subscriber detaches until the turn finishes, the
queue empties, or idle retention evicts the slot.

Reconnects do not send the browser `ajax.web.session.model` preference on the
session WebSocket URL; task metadata remains authoritative
([#910](https://github.com/mossipcams/ajax-cli/issues/910)). Connected model
controls send only `set_config_option` with the exact advertised pair; the host
applies ACP `session/set_config_option` and persists the confirmed pipe-form pin
through a core-owned operation. Cross-harness Switch resets backend context on the live slot
instead of dropping it.
Incoming ACP v1 notifications remain typed through the per-task command loop.
Stable message, thought, tool, plan, mode, configuration, session-info, and usage
updates are mapped explicitly; unsupported capability announcements are
dropped, and unknown legacy updates remain generic artifacts. Ajax advertises
no ACP filesystem or terminal client capabilities until worktree-scoped
handlers exist.

From a selected task, swipe-left navigation opens Diff Review
(`#/t/<handle>/diff`), a read-only PR/file/hunk viewer with core-projected
orientation, judgment flags, and reading-order guide chips, fed by
`GET /api/tasks/.../pull-requests` and `GET /api/tasks/.../diff`. Swipe
navigation finger-follows; on commit the shell keeps the outgoing workspace
surface mounted (React `Activity`) while the destination slides in beside it as
one continuous transform over roughly `SWIPE_PAGE_COMMIT_MS` (≈220ms),
shortened on fast flicks when average gesture velocity is high. Button Back
uses the same cross-slide commit path; list-to-task opens still use the
one-shot CSS enter class when no swipe gesture is in flight; other chrome
navigations may stay instant.
Diff Review must not steal terminal horizontal pans. The browser submits only an
Ajax task handle; `ajax-web` resolves that handle to the registered
`tmux_session` and attaches to the fixed ` task window` target. The browser
must not accept raw tmux target names or make pane captures, snapshot viewers,
key-send endpoints, or answer routes the default task interaction path.

The browser shell is not an offline-first Ajax client and must not introduce a
second browser-side task model. Git, tmux, SQLite, supervised processes, and
the Ajax backend remain authoritative for task state and operations. The
primary iPhone target is normal iOS Safari; full Cockpit works without Home
Screen install. Web Cockpit does not ship classic PWA packaging — no
`manifest.webmanifest`, app icons, or service worker. Optional Add to Home
Screen (Safari-native; safe `apple-mobile-web-app-*` metadata only) enables
Declarative Web Push phone pings on capable browsers.

Web Cockpit is host-native only. `ajax-cli web` is the live-control backend and
runs with the same host authority as SQLite, configured repos, worktrees, tmux
sessions, agent CLIs, and host process state. Docker is no longer part of the
Ajax Web Cockpit architecture, and no Docker-based web runtime is supported.

Ajax does not implement its own daemon manager. Persistent Web Cockpit
deployments may run the host-native `ajax-cli web` process under an external
host supervisor such as launchd, `systemd --user`, tmux, or another service
manager. The supervised process remains host-native and retains live-control
authority over the selected Ajax runtime profile.

WireGuard or an equivalent private network is the Web Cockpit access boundary.
Mutable routes accept callers that can reach the private listener. Public
internet exposure is unsupported. Operators are responsible for binding the
server to a trusted interface or restricting access at the network layer.

The host-native Web Cockpit server is served by `ajax-cli web` through an
Axum-based HTTP transport. Axum owns routing, request extraction, response
construction, static browser shell serving, TLS wiring, and future stream/WebSocket
endpoints. It does not own task lifecycle, action policy, registry truth, or
substrate interpretation. Route handlers are thin adapters that delegate to the
existing Ajax backend/core operation boundaries.

## Client state ownership

TanStack Query owns transient in-memory HTTP read state for ordinary Cockpit
reads: task detail, diff review (pull requests and selected/local diff), per-harness
session model catalogs, version metadata, and Test-in-Dev deployment status.
Selected mutation lifecycles (`startTask`, harness swap, and `/api/operations`)
also run through Query mutations with retries disabled.

Query does **not** own Cockpit polling or projection ordering (`useCockpitResource`),
hash navigation (`useHashRoute`), ACP/WebSocket session reducers (`useTaskSession`),
or terminal/PTY state. Mutation responses that include a Cockpit projection still
call `applyCockpit` directly; the custom poll gate remains authoritative.

## Speech Input Architecture

Speech input is a host-side dictation capability for Web Cockpit. The iPhone
supplies microphone audio; the MacBook hosting Ajax owns speech recognition.
The browser never downloads a speech model or runs heavy inference locally.
Finalized transcript text is auto-inserted into the active shell line through
the existing paste/PTY input path; Ajax does not auto-press Enter or execute
commands. Partial recognition text is session metadata only and is never
written to the PTY.

```text
iOS Safari / optional standalone shell
  -> user-gesture microphone capture
  -> authenticated task-scoped STT WebSocket
  -> ajax-web STT session service
  -> persistent host-side Moonshine v2 streaming worker
  -> partial/final/VAD/ready/completed events
  -> TaskTerminal auto-insert (paste/PTY input path)
  -> user presses Enter or edits as usual
```

The current Web Cockpit terminal remains a raw xterm.js/tmux terminal. Speech
auto-insert writes finalized text through the same paste/PTY input path the
terminal already uses for manual paste, so dictated text appears on the active
shell line without a separate review composer. This is not a second terminal,
registry, task model, or command workflow. Normal keyboard input, terminal
focus, tmux attachment, and PTY behavior remain unchanged.

### Ownership

- `TaskTerminal.tsx` owns the visible Mic control, transcript auto-insert,
  accessibility, focus, and the single frontend speech state machine. Finalized
  speech pastes into the active shell line in contiguous sequence order;
  partials remain non-visible session metadata.
- A small frontend STT controller (`speechTransport`) owns microphone capture,
  PCM conversion, WebSocket lifecycle, bounded audio backpressure, and
  session-scoped finalization timing. It does not own task truth or PTY input.
  Cancel, finalize-complete, provider error, and visibility interruption share
  one teardown path that stops tracks, closes or suspends the audio context,
  stops processing, clears timers, invalidates the session ID, and ignores
  delayed events.
- `ajax-web` owns authenticated STT routing, session IDs, bounded audio queues,
  provider lifecycle, protocol validation, health reporting, and cleanup.
- The provider adapter owns model-specific startup, a persistent worker
  process, audio ingestion, incremental inference, provider VAD, and provider
  event translation. The rest of Ajax sees only the provider interface and
  versioned STT events.
- `ajax-core` owns durable configuration values only (`[stt]` language, timing,
  provider command). Speech sessions and transcripts are ephemeral
  browser/backend session state, not task records or registry truth.

### Provider abstraction and supervision

The initial provider is a supervised local **Moonshine v2** worker
(`moonshine-voice`, default **Small Streaming**) appropriate for an M1
MacBook Air. Inference runs on the Ajax host, not on the phone. Legacy
`useful-moonshine-onnx` / `moonshine/tiny` batch sidecars are not supported.
Model-specific code and package assumptions stay behind a narrow provider
interface:

- ensure the persistent worker is running and the model is loaded;
- start a session (isolated by session ID);
- push bounded PCM audio;
- receive a session-ready signal only after the worker can accept audio;
- receive partial transcripts;
- receive ordered final transcripts;
- receive speech-started and speech-ended events;
- finalize a session and receive successful completion;
- cancel a session without terminating the worker;
- report provider health and availability;
- shut down the worker cleanly when Ajax exits.

The main Rust process does not import Python internals. Ajax starts one
persistent worker (at provider startup or lazy on first use), loads the
streaming model once, health-checks it, restarts it within configured limits
after crashes, and shuts it down with Ajax. Recognition sessions reuse the
loaded model and remain isolated by session ID; cancelling one session does
not terminate the worker. The worker is isolated from PTY and task-session
failures, binds only to a local/private interface when a socket is required,
and is never exposed as a public STT endpoint. Provider startup or crash
becomes a typed provider error and leaves the rest of Ajax operational. A
future local or remote engine can implement the same interface without
changing the browser state machine, transcript reducer, or WebSocket contract.

Audio ingestion continues while inference runs. The worker must not repeatedly
reprocess the entire accumulated phrase on every frame; it uses Moonshine’s
incremental streaming path (or bounded rolling windows with a documented
reason). Phrase-level VAD behavior is preserved. Audio and transcript buffers
remain bounded.

### Authenticated audio transport

Speech uses a separate authenticated WebSocket rather than the PTY terminal
socket, because terminal binary frames must continue to mean PTY input. The
initial route is task-scoped as `/api/tasks/{handle}/stt`; it uses the existing
HttpOnly same-origin browser-session cookie and the existing same-origin
WebSocket `Origin` check. It never accepts an unauthenticated or public STT
connection, raw tmux target, or browser-supplied provider address.

Protocol messages are versioned and carry the active `sessionId`. JSON control
messages use the existing Ajax WebSocket framing style:

- `stt.start`: session ID, PCM16 encoding, 16 kHz sample rate, mono channel,
  and protocol version (recognition language comes from host `[stt]` config,
  not from the browser start message);
- `stt.stop`: request provider finalization;
- `stt.cancel`: abandon provider work for the session and release session
  resources without killing the persistent worker;
- `stt.ready` (includes `pauseGracePeriodMs` and `finalizationTimeoutMs`) —
  forwarded only after the worker reports the session can accept audio;
- `stt.partial`, `stt.final`, `stt.speech_started`, `stt.speech_ended`,
  `stt.error`, and `stt.closed` server events.

Successful session completion is distinct from failure: after finalize, the
provider drains finals, receives an explicit worker completion signal, and
emits `stt.closed`. Expected completion must not emit `stt.error`. Unexpected
worker or session termination still produces a useful `stt.error`. Delayed
process-exit signals must not overwrite a successfully completed or idle
frontend state.

Audio is bounded binary transport. Each binary frame contains one PCM16 audio
chunk and a monotonically increasing frame sequence in the transport envelope;
JSON/base64 wrapping is not used for every audio chunk. The configured frame
duration and maximum buffered audio duration are shared configuration, not
scattered constants. Client and server apply **observable** backpressure with
bounded queues and a maximum queued audio duration. Sustained backpressure
surfaces a user-visible warning or recoverable error before meaningful speech
is lost; silent frame dropping is not allowed. A session must not finalize as
successful after substantial unreported audio loss. Reconnects reuse the
active browser controller/session identity only when the session is still
valid and never create a duplicate provider session. A failed reconnect enters
an explicit recoverable error state.

### Speech state machine

The frontend has one explicit state, not independent listening/connecting/timer
booleans:

```text
idle -> connecting -> listening -> pause_pending -> listening
                              \\-> finalizing -> idle
idle/connecting/listening/pause_pending/finalizing -> error
connecting/listening/pause_pending/error -> idle  (cancel/recovery)
```

Every session receives a unique ID. Every provider event, browser event, and
timer callback is checked against the active session ID. Invalid transitions
are ignored or rejected without changing the current state. Only one pause
timer can exist per session; cancelled timer callbacks cannot finalize a later
session or a resumed session.

The centralized initial timing and language configuration (host `[stt]` and
`stt.ready`) is:

- `phraseEndSilenceMs = 700` — provider phrase finalization only;
- `pauseGracePeriodMs = 9000` — spoken stop grace period (surfaced on
  `stt.ready`);
- `language = "en-US"` — host config passed into the provider session, not the
  browser `stt.start` body;
- `maxBufferedAudioMs` — bounded transport/provider buffering;
- `finalizationTimeoutMs` — provider finalize deadline and browser stop
  fallback (surfaced on `stt.ready`).

There is no short ordinary-inactivity timeout and no browser STT reconnect
budget yet: a failed socket or capture path enters an explicit recoverable
error and the operator taps Mic again. Any future defensive maximum session
duration or reconnect limit must be generous, configurable, documented, and
visible before it can terminate a session.

### VAD and transcript lifecycle

Voice activity detection is separate from phrase finalization. Provider-side
VAD is authoritative for `speech_started`, `speech_ended`, interruption, and
inactivity events used by the backend/provider contract. The browser does not
run a second energy detector or independent stop policy; `speech_started` is
what cancels `pause_pending`.

Ordinary silence may finalize a phrase and start a new provider segment, but it
does not stop capture, close the session, or finalize the whole dictation.
Final segments carry stable sequence numbers. Duplicates are ignored. Future
segments are buffered until missing earlier sequences arrive; a permanently
missing segment surfaces a recoverable warning and must not cause later text to
be auto-inserted out of order. Contiguous finals paste into the active shell
line as they become applicable.

While listening, `partialTranscript` is kept as session metadata for tests and
control handling; it is not a visible composer and is never written to the PTY.
Partials must not duplicate into finalized text.

The standalone normalized finalized utterance `pause` is the only spoken
control command. Only an utterance whose normalized content is exactly `pause`
(including `Pause.`, `Pause,`, `Pause!`, `Pause?`, and Unicode terminal
punctuation equivalents) triggers it. Partial transcripts never trigger it.
Sentence content such as `Add a pause between retries` remains transcript text.
When triggered, the command is removed from the transcript, `pause_pending`
begins, capture and the provider session remain active, and a monotonic
nine-second countdown is shown. Any provider `speech_started` event cancels the
timer immediately and returns to `listening`, even before a partial or final
transcript arrives. If the full grace period expires, the session enters
`finalizing`, stops accepting new audio, flushes buffered audio, asks the
provider to finalize, waits for pending finals and successful completion,
releases microphone and audio resources, closes the STT session, cancels
timers, and returns to idle without `stt.error`. No automatic Enter, shell
execution, or prompt submission occurs. Standalone spoken `start over` or
`start fresh` (including punctuated forms) clears finalized speech state and
undoes auto-inserted shell text for the active session while capture continues;
sentence uses of those phrases remain dictated text.

Manual cancel and provider failure share the same teardown path. They stop
capture and transport, cancel the session and all timers, release browser audio
resources, invalidate the session ID, ignore delayed events, preserve
already-inserted terminal text, clear unstable partial metadata, and return to a
stable idle or explicit error state. Permission denial, unsupported browser
APIs, unavailable hardware, audio interruption, background/suspension,
WebSocket failure, provider failure, unsupported format, overflow, duplicate or
out-of-order events, stale session events, and finalization timeout all preserve
already-inserted text and expose a useful recovery message.

### Terminal, authentication, and iOS boundaries

Finalized recognition output is auto-inserted into the active shell line via
the existing paste/PTY input path. Ajax does not auto-press Enter or execute
commands. Existing physical/software Ctrl+C continues through the normal
xterm/PTY path. Removing the visible `^C` shortcut only removes that toolbar
button and exclusive UI code; it does not remove the shared control-key
infrastructure or backend SIGINT behavior.

The Mic control is in the existing shortcut bar immediately after Paste, keeps
the visible label `Mic` in every state, and uses the existing key height,
spacing, typography, border, focus, touch, responsive, and disabled-state
styles. Accessible names are state-dependent: idle Start voice input;
connecting Connecting voice input; listening Voice input listening;
pause-pending includes the remaining countdown; finalizing Finalizing voice
input; error describes that voice input failed; disabled exposes why the
provider is unavailable. Error must not visually appear as active listening.
Duplicate activation is prevented while connecting or finalizing. A second Mic
tap while listening or pause-pending finalizes the session and releases the
microphone, keeping already-inserted terminal text; **Cancel voice** remains
the abandon path.

Microphone capture starts only from the Mic user gesture after `stt.ready`
confirms the host model can accept audio. The UI must not claim Listening
before that handshake. The implementation handles permission denial, absent
hardware, audio-route interruption, visibility/background changes, screen lock,
JavaScript suspension, socket interruption, and duplicate-stream prevention. If
iOS cannot guarantee capture continuity, the session becomes an explicit
interrupted/recoverable error and does not claim to still be listening.
Completion, cancellation, and provider errors always stop tracks and release
audio resources.

Web Cockpit remains a live same-origin Safari-first shell. No speech model is
downloaded to the phone, WebGPU is never required, and no service
worker/offline mutation path is introduced. Optional standalone/Home Screen
behavior is treated as an iOS lifecycle variant, not an authentication,
storage, or PWA dependency.

Browser files live under `crates/ajax-web/web`. The install slice owns serving
the HTML shell, the boot client JavaScript (`app.js`), the deferred terminal
chunk (`terminal.js`), and one deterministic stylesheet artifact
(`dist/app.css`) from that directory. Source CSS is authored as
`web/src/styles.css` — the ordered manifest and Tailwind `@theme inline`
bridge — which `@import`s owned modules under `web/src/styles/`; Vite bundles
that graph into the sole shipped CSS file. The manifest is not a second asset.
`ajax-web::runtime` owns HTTP transport wiring, local TLS setup, and shell asset
delivery.
`ajax-web::adapters::browser_session` owns browser-session token persistence,
cookie formatting, `Set-Cookie` application, and request-cookie matching.
`ajax-cli` remains a thin native bridge: it resolves stable/dev context paths,
reloads and saves the authoritative SQLite state, and delegates native command
execution for browser-submitted actions.

Classic PWA packaging surfaces are unsupported: no `manifest.webmanifest`,
service worker, install icons, or offline cache. Safe standalone metadata
(`apple-mobile-web-app-*`, theme-color) remains so operators can optionally Add
to Home Screen for Declarative Web Push. The browser shell must remain a live
same-origin client for the host-native backend.

Web Cockpit syncs server-authoritative Cockpit projections, not browser-owned
task records. `GET /api/cockpit` returns the latest backend projection, but it
may reuse a short-lived in-memory projection cache and single-flight concurrent
refreshes before re-rendering. Mutable operations return typed operation
outcomes, invalidate the cached projection, and either include or cause a
refresh of the latest Cockpit projection. The browser may keep transient UI
state such as "sending" or "failed," but it must not persist pending task
operations or replay mutations after reload.

Web API access follows an explicit adapter-level API access policy. Non-API
shell and asset routes are public, `/api/health` is public for reachability
checks, and `POST /api/session` is public only as a browser-session bootstrap on
the private listener. When Web Cockpit is deliberately placed behind Cloudflare
Access, runtime configuration may require protected routes to validate
`Cf-Access-Jwt-Assertion` against the configured issuer, audience, and JWKS
before accepting the browser-session cookie. Cloudflare Access narrows the
supported external exposure model; it does not make direct origin bypass safe,
so operators must still protect the origin with Cloudflare Tunnel, firewalling,
or equivalent origin access controls. Live-control API routes such as
`/api/cockpit`, `/api/version`, `/api/server/restart`, `/api/operations`,
`/api/tasks`, and the task terminal WebSocket route require the server-issued,
HttpOnly, Secure, same-origin browser-session cookie. The HTML shell sets the
cookie on normal loads, and `POST /api/session` exists only to renew or
bootstrap that same cookie when a live browser shell receives a `401` from a
protected API route. Session renewal does not authenticate public clients,
create browser-owned task state, persist pending work, cache operational data,
or replay mutations. It is a transport recovery mechanism for the host-native
private listener.

The app must function correctly without a service worker. If a service worker
is kept, it is non-critical and limited to cleanup or safe static assets. It
must never intercept or cache live Ajax endpoints, including `/api/cockpit`,
`/api/session`, `/api/actions`, health checks, polling endpoints, streaming
endpoints, WebSocket/SSE endpoints, or any future `/api/*` endpoint.

Browser storage is intentionally limited. The browser shell must not use
IndexedDB, background sync, local task queues, offline mutation replay, or
cached operational/API data.

PostHog SDK analytics persistence (localStorage/cookie for distinct id and
related session properties) and telemetry identity keys (`install_id`,
`sequence` in `localStorage`; `session_id` in `sessionStorage`) are allowed as
non-operational observability state alongside other UI prefs. They must not
store prompts, terminal content, tokens, or task truth.

No browser WASM runtime asset is currently shipped; the shell must not add Yew,
Trunk, or a large frontend architecture unless the project explicitly adopts
those elsewhere.

Stable and dev runtime profiles remain separated by the host-native
`ajax-cli web` process and explicit runtime context. Stable uses the stable
state database and default web port, while dev uses the development state
database and dev web port. The browser shell must not merge profile state in
browser storage.

### Test in Stable process model

Settings **Test in Stable** (stable profile only) rebuilds and redeploys the
stable Web Cockpit from `origin/main` without taking `https://127.0.0.1:8787`
down during the build.

Flow:

1. Operator POSTs `/api/server/test-in-stable`. The live stable server spawns
   `scripts/test-in-stable.sh` through a short delayed thread and **does not
   exit**. The JSON response is `{ok:true,restarting:true}` so Settings waits
   for cutover (version change or down-edge then two healthy checks). From dev,
   the same POST returns `{ok:true,restarting:false}` because dev only triggers
   stable rebuild remotely.
2. `test-in-stable.sh` re-execs into a new session (`AJAX_TIS_DETACHED`), drops
   inherited stdio, and starts `dev-web-restart.sh --profile stable` inside
   tmux session `ajax-test-in-stable` with its own log under
   `<host-clone>/.ajax-dev-web/test-in-stable.log`.
3. `dev-web-restart.sh` fetches `origin/main` from the **host clone**
   (`REPO_ROOT`, the checkout that launched stable web). Git reset/build uses a
   **dedicated detached main worktree** (default
   `~/.ajax-dev/worktrees/<repo-basename>-main`, override
   `AJAX_STABLE_MAIN_WORKTREE`). The operator's current branch/checkout is
   never reset. pid files and logs stay at `REPO_ROOT/.ajax-dev-web`.
4. Build (`npm ci`, `web:build`, `cargo install --force` into `~/.cargo/bin`)
   finishes **before** the script stops tmux session `ajax-web-stable`. A stale
   pid file after stop warns and continues instead of aborting.
5. Cutover stops the old tmux session and starts the new binary. If
   `start_web` fails, the script restores the previous `~/.cargo/bin/ajax-cli`
   snapshot taken before `--force` install and retries so `:8787` is not left
   empty.

Test in Dev (`--worktree`) keeps the same slot-binary install under
`.ajax-dev-web/bin` and must never target profile stable.

Operator flow:

1. Task details POST `/api/dev-deploy` with `{ task_handle }` only. The host
   resolves the ajax-cli worktree from registry state, rejects non-ajax tasks and
   unmanaged paths, and spawns `scripts/dev-web-restart.sh --worktree <path>
   --profile dev --port 8788` (never stable).
2. JSON `{ ok: true, deploy }` returns `202 Accepted` while the slot moves through
   `building` → `restarting` → `dev_ready` / `failed`.
3. The browser panel polls `GET /api/dev-deploy` only while `deploy.active` is
   true. After a successful start POST, the client cancels any in-flight status
   read before seeding query cache so a stale ready snapshot cannot hide the run
   (GitHub issue #1035).

Restart-script resolution order: the selected worktree's
`scripts/dev-web-restart.sh`, then `AJAX_WEB_RESTART_SCRIPT`.

### PostHog Cloud telemetry

Web Cockpit may send approved outbound product telemetry to **PostHog Cloud**
via `posthog-js` wrapped by `@/shared/lib/telemetry`. Callers must use the
wrapper — not import `posthog-js` directly. This is an approved **browser egress**
exception: the Ajax Web Cockpit listener remains private (WireGuard / equivalent;
no public-server exposure model). Operators need outbound HTTPS from the browser
to PostHog; blocked egress fails soft and does not change live-control authority.

**CSP allowlist (US hosts only):** Web Cockpit responses set
`Content-Security-Policy` in `crates/ajax-web/src/adapters/http.rs`. PostHog
egress requires exact host entries — no reverse proxy, no `https:` wildcard:

- `connect-src`: `https://us.i.posthog.com` (ingest / `$web_vitals` and other API traffic)
- `script-src`: `https://us-assets.i.posthog.com` (PostHog remote config / assets)

#### Initialization (env-gated)

| Variable | Required | Default | Behavior |
| --- | --- | --- | --- |
| `VITE_POSTHOG_KEY` | no | Ajax project write key in source | Overrides the default browser write key. Set to `off`, `0`, or `disabled` to disable `initTelemetry()` / `track`. |
| `VITE_POSTHOG_HOST` | no | `https://us.i.posthog.com` | PostHog ingest host |

Boot calls `initTelemetry()` once with SDK `defaults: '2026-05-30'`. On success:

- **Identify:** `ajax:${window.location.hostname}` with person properties
  `host`, `origin`, and optional `app_version`.
- **Super-properties:** `posthog.register` with `standalone`, `install_id`, `host`,
  and optional `app_version` so automatic events (`$web_vitals`, `$pageview`) inherit
  Ajax dimensions.
- **Session replay:** off (`disable_session_recording: true`).
- **Exception autocapture:** off.
- **Autocapture:** on with CSS ignorelist for terminal surfaces, sensitive
  attributes, and `.ph-no-autocapture` targets.
- **Web Vitals:** LCP, CLS, FCP, and INP via `capture_performance`
  (`web_vitals_allowed_metrics`).

#### Standalone vs browser tab

`standalone` on every explicit event reflects installed PWA display mode
(`display-mode: standalone` or `navigator.standalone`). It is **observational
only** — Web Cockpit does not require Home Screen install and functions in a
normal Safari tab. `ajax_pwa_launch` and `ajax_pwa_resume` record launch/resume
timing when applicable; they do not gate features.

#### Delivery

Explicit events call `posthog.capture` directly through the typed wrapper after
sanitizing props and merging shared context. There is no IndexedDB or local
event queue — delivery uses the PostHog JS SDK’s normal in-memory/network path
(and its own localStorage/cookie identity persistence). Soft-fail on errors;
never block Cockpit.

#### Common properties (every explicit event)

Merged onto every `track` / `captureEvent` call (context wins over caller props):

| Property | Type | Source |
| --- | --- | --- |
| `event_id` | string | UUID per capture |
| `session_id` | string | `sessionStorage` tab session |
| `install_id` | string | `localStorage` stable install id |
| `sequence` | number | Monotonic counter per install |
| `app_version` | string? | `meta[name="ajax-app-version"]` when set |
| `route` | string | Current `location.hash` |
| `route_kind` | string | `parseRoute(hash).kind` from `@/shared/lib/routes` |
| `host` | string | `location.hostname` |
| `online` | boolean | `navigator.onLine` at capture |
| `visibility` | string | `document.visibilityState` at capture |
| `connection_type` | string? | `navigator.connection.effectiveType` when present |
| `pixel_ratio` | number | `window.devicePixelRatio` rounded to 2 decimals |
| `ios_version` | string? | Parsed from user agent on iOS |
| `viewport_w`, `viewport_h` | number | `window.innerWidth` / `innerHeight` |
| `standalone` | boolean | PWA standalone display mode |

#### Custom event schemas

All events below are emitted via `track` and include the common properties above.
Additional caller properties pass through `sanitizeTelemetryProps` unless noted.

**`ajax_tap_to_feedback`** — tap → first visible feedback

| Property | Type | Required | Notes |
| --- | --- | --- | --- |
| `interaction_id` | string | yes | Id from `beginInteraction` |
| `control` | string | yes | Control identifier from `beginInteraction` |
| `feedback_kind` | string | yes | Feedback classification |
| `duration_ms` | number | yes | Rounded ms from interaction start |

**`ajax_tap_to_operation_complete`** — tap → completed operation

| Property | Type | Required | Notes |
| --- | --- | --- | --- |
| `interaction_id` | string | yes | Id from `beginInteraction` |
| `control` | string | yes | Control identifier |
| `op` | string | yes | Operation name (defaults to `control`) |
| `ok` | boolean | yes | Success flag |
| `outcome` | `"success"` \| `"failed"` \| `"cancelled"` | yes | Derived from `ok` and `error_kind` |
| `error_kind` | string | no | Present when `ok` is false (`confirm_timeout`, `undo`, `unmount`, `operation_failed`, `network`, …) |
| `duration_ms` | number | yes | Rounded ms from interaction start |
| `feedback_ms` | number | no | Rounded ms from interaction start to first feedback when feedback was sent |
| `feedback_kind` | string | no | `feedback_kind` from `endTapToFeedback` when available |

**`ajax_swipe`** — swipe gesture commit

| Property | Type | Required | Notes |
| --- | --- | --- | --- |
| `direction` | `"left"` \| `"right"` | yes | Swipe direction |
| `duration_ms` | number | yes | Gesture duration |
| `distance_px` | number | yes | Travel distance |
| `page_width_px` | number | yes | Page width used for commit threshold |
| `progress` | number | yes | `min(1, distance_px / page_width_px)` rounded to 3 decimals |
| `outcome` | `"completed"` \| `"cancelled"` | yes | Derived from `completed` / `cancelled` |
| `velocity_px_per_ms` | number | yes | Average velocity |
| `completed` | boolean | yes | Reached completion threshold |
| `cancelled` | boolean | yes | Gesture cancelled |
| `settle_ms` | number | yes | Post-release settle time |
| `from_route` | string | no | Origin hash route |
| `to_route` | string | no | Destination hash route |

**`ajax_route_visible`** — navigation → visible content

| Property | Type | Required | Notes |
| --- | --- | --- | --- |
| `duration_ms` | number | yes | From `markNavigationStart` or caller override |
| `route_kind` | string | yes | Destination route kind (`parseRoute(to_route).kind`) |
| `nav_trigger` | string | no | Stored trigger from `markNavigationStart` (`hash`, `swipe`, `open_task`, …) |
| `from_route` | string | no | Origin route |
| `to_route` | string | no | Destination route |

**`ajax_pwa_launch`** — cold launch (once per boot)

| Property | Type | Required | Notes |
| --- | --- | --- | --- |
| `duration_ms` | number | yes | Navigation start → first shell visibility |
| `nav_type` | string | no | `performance.getEntriesByType("navigation")[0].type` when present |
| `dom_interactive_ms` | number | no | Rounded `domInteractive` from navigation timing when present |

**`ajax_pwa_resume`** — resume from background

| Property | Type | Required | Notes |
| --- | --- | --- | --- |
| `hidden_ms` | number | yes | Time document was hidden before this resume |
| `resume_to_visible_ms` | number | yes | Ms from `visibilitychange` → visible until double `requestAnimationFrame` (first paint opportunity) |
| `resume_to_cockpit_ms` | number | yes | Ms from visible until debounced cockpit recovery (`loadCockpit`) finishes |
| `resume_debounce_ms` | number | yes | Debounce constant (`RESUME_DEBOUNCE_MS`, 750) before recovery poll |
| `online` | boolean | yes | `navigator.onLine` at emit |
| `cockpit_ok` | boolean | yes | True when cockpit is `ready` or `stale` with data after `loadCockpit`; false on hard error |

**`ajax_telemetry_diagnostic`** — Settings diagnostics snapshot

| Property | Type | Required | Notes |
| --- | --- | --- | --- |
| `initialized` | boolean | yes | Whether `initTelemetry` succeeded |
| `standalone` | boolean | yes | Current display mode |
| `app_version` | string | no | When available from meta tag |
| `online` | boolean | yes | From shared context |
| `visibility` | string | yes | From shared context |
| `route` | string | yes | From shared context |
| `route_kind` | string | yes | From shared context |
| `sequence` | number | yes | From shared context |
| `host` | string | yes | From shared context |

#### Privacy guardrails

Telemetry must never capture prompts, PTY input, terminal buffer content, or
other operator secrets. Sensitive property keys (terminal, token, password,
command, buffer, diff, etc.) and suspicious string values are stripped before
capture. Terminal surfaces are excluded from autocapture via CSS ignorelist.

Declarative Web Push is the supported attention channel when the operator has
enabled notifications from an installed Home Screen web app on a Declarative
Web Push–capable browser (for example iOS Safari 18.4+). Settings uses
`window.pushManager` (no service worker) with browser-session-protected
`/api/push/vapid`, `/api/push/subscribe`, and `/api/push/test`. The server
persists VAPID keys and subscriptions under `state_dir` and delivers
`web_push: 8030` payloads via encrypted curl. Cockpit remains usable without
Home Screen install; without install/subscribe there is no phone ping. This
does not permit service worker registration, offline mutation, or browser-owned
task truth.

The existing server background tick performs a Full refresh every 30 seconds
regardless of browser foreground presence or push subscriptions. Those signals
gate web push only; they never gate task-associated PR/CI discovery or agent
notification delivery. Task details project persisted delivery evidence as
`queued`, `accepted`, or a retained `error`; the browser does not poll GitHub,
select PRs, deduplicate failures, or route prompts.

Attention delivery fires once per actionable episode and only for statuses
the operator can act on. Actionable Waiting is allowlisted to `Waiting for
input` / `Waiting for approval` (structured hooks/lifecycle events, including
Cursor `beforeShellExecution` / `beforeMCPExecution` plus pane fallback; Cursor
`Notification` matchers are best-effort only); all other Waiting explanations
stay inbox-visible but silent. `Error`-class evidence
(CI failed, merge conflict, command failed, blocked, runtime probe failure)
each fire a single push after the same shared 15-second confirmation dwell
(`NOTIFY_CONFIRMATION_DWELL`) for every actionable status, except: GitHub CI
failed while attempt status is still `Pending` (no terminal failure yet) or a
post-failure rerun is in flight until a Full refresh records settled terminal
failure (or cleared); merge conflict not yet confirmed by git status; or merge
conflict during a post-failure CI rerun. First-attempt terminal CI failure with
sibling checks still pending still starts an episode and may deliver the agent
CI prompt and operator Error ping — the reducer does not wait until every check
is green. Task-associated CI probes run on Full refresh when `checks_due` allows
(minimum 10-second gap while pending or failed); the web background tick runs
Full refresh and attention delivery every 30 seconds on the same tick.
Transient `Rate limited` Waiting,
lifecycle-only "Ready for review", turn-settled "Response ready" (`Done` from
Cursor `stop` / Claude·Codex·Pi settle), and auth/context waits do **not**
phone-ping — Pi has no native wait/ask, so settle must not look like
actionable attention. Episode dedup is status-class only; the notification body
includes the agent client and explanation
(`repo/handle: Waiting (codex) — …`). Delivery stays on the web background tick
when subscriptions exist — hooks only write event files and must stay
instant. Returning to `Running`/`Idle` arms the next episode only after a
quiet window (`EPISODE_CLEAR_DWELL`, 30s) of sustained clear evidence, so a
turn boundary inside one episode delivers one ping. Opening a task records
an attention acknowledgment that silences the current episode (the next
actionable evidence re-arms), preventing re-fires while the operator is
already looking. There is no fixed re-arm cooldown — only the quiet-clear
gate plus the acknowledge-suppress path.

Browser validation should check local-only shell assets, stable/dev port
separation, clear browser error states for failed live requests or unsupported
actions, connection recovery, diagnostics, and `/api/*` service-worker bypass
when any service worker is present.

`ajax-web` is organized around vertical browser/operator capabilities inside
the crate:

- `ajax-web::slices::*` owns browser/operator capabilities.
- `ajax-web::adapters::*` owns mechanisms such as HTTP routing, TLS, static
  asset embedding, filesystem persistence, network clients, and browser
  serialization formats.
- `ajax-web::runtime` composes slices and adapters into the Web Cockpit server.
  When at least one push subscription is stored and the operator is not in the
  foreground Cockpit, it also runs a background tick that reuses the
  `/api/cockpit` refresh path (same single-flight lock, cache TTL, and
  revision-checked commit) so attention push fires without a browser poll; the
  interval is 30 seconds. The tick skips while foreground presence is warm
  (90s TTL). Cockpit data polls always run at the same speed; only a poll that
  carries `X-Ajax-Foreground: 1` (SPA sets this when
  `document.visibilityState === "visible"`) refreshes that TTL. Background /
  Simulator / hidden-tab polls refresh projection only and do not suppress
  push. Terminal WebSocket attach, operate/action, and terminal keystrokes
  still count as active use. Browser `/api/cockpit` refreshes pass
  `deliver_notifications=false` (UI update only). When the tick runs it
  refreshes at `RefreshTier::Full` and delivers attention push.
- `ajax-web::slices::actions` owns the shared browser action capability
  vocabulary used by both `cockpit` and `operate` without cross-slice imports.

Slices may call adapter facades, but slices are named after capabilities rather
than mechanisms. New browser features should start as a vertical slice when they
represent an operator or browser capability; add an adapter only when the
feature needs a concrete external mechanism.

### `ajax-web::slices::cockpit`

Owns the browser Cockpit read experience. It builds browser DTOs from the core
Cockpit projection and preserves the same task/action meaning as Native
Cockpit. Cards and details share one status contract (`status`,
`status_explanation`) and one ordered `actions` collection containing only
browser-executable action metadata.

Cards additionally carry `attention`, the operator attention band
(`needs-you`, `review`, `active`, `idle`) derived by
`ajax_core::ui_state::attention_band` from the operator status and lifecycle.
It answers which of the operator's questions a task belongs to — what needs
input, what is ready to review, what is merely active — and is the sole
grouping key for the browser dashboard. Its precedence mirrors
`derive_task_status`: an actionable attention gate is classified before the
lifecycle review boundary, and the band reads lifecycle directly so a task
whose review boundary has been acknowledged stays in `review` rather than
falling through to `idle`. Because grouping is headline status, the browser
must render this field and must never re-derive it from `status`,
`status_explanation`, or any lifecycle value. Unsupported actions, legacy UI states, and
action support-state records are absent. Raw live, lifecycle, pane, and runtime
values may remain detail diagnostics, but browser JavaScript must not derive or
override headline status from them. The browser may style the first returned
action as prominent; it does not receive or invent a separate `primary_action`
contract. Confirmation-required actions that carry a typed `BranchAdoptionPlan`
expose the exact expected/observed branch pair from core; the browser retains
that payload between activations and resubmits it unchanged. Core alone
revalidates the pair and mutates task truth; stale or altered evidence is
rejected.

### `ajax-web::slices::operate`

Owns browser-submitted operator actions. It accepts browser action requests,
checks browser capability limits, delegates valid work to the existing core task
operations, and returns the refreshed Cockpit projection. Unsupported
capabilities return typed adapter capability outcomes rather than duplicated
lifecycle policy. Browser `resume` uses the authenticated task terminal bridge
when the operator needs full interactive attach.

Opening a task in the browser is the resume gesture: entering a task workspace
route dispatches the `resume` operation (acknowledging attention through core,
exactly like Enter in the native Cockpit) before mounting Ajax Chat or Ajax
Terminal. The browser renders no separate resume control; the implicit
open=resume acknowledgment is best-effort and never derives task truth in
JavaScript. Confirmed operator
actions must echo the exact `branch_adoption` plan core attached to the action;
the slice forwards that payload to core without recomputing branch policy or
comparing branches in the browser.

#### Operation failure envelopes

Mutation endpoints such as `POST /api/operations` and `POST /api/tasks` return a
typed failure JSON envelope when the HTTP status is non-success:

```json
{ "ok": false, "error": "human-readable message", "code": "optional_recovery_hint" }
```

The browser parses `error` for operator-facing copy and optional `code` into
`ApiError.code`. `code` is a recovery hint from the backend adapter — not browser
policy and not a second task model. The shared `operatorErrorPresentation`
helper maps codes to toast copy suffixes and telemetry `error_kind` values;
missing `code` preserves legacy kind-based behavior (`conflict`, `stale-session`,
`network`, and so on).

Starter codes: `conflict`, `unsupported_action`, `unknown_action`, `needs_terminal`,
`task_not_found`, `tmux_missing`, `substrate_missing`, `stale_session`,
`rate_limit`, `command_failed`, `internal`, `confirmation_required`.

### `ajax-web::slices::install`

Owns the browser shell. It serves the HTML shell, the boot client JavaScript
(`app.js`), the deferred terminal chunk (`terminal.js`), and one deterministic
stylesheet artifact (`dist/app.css`). Source styling lives in
`web/src/styles.css` (ordered manifest + Tailwind bridge) and owned modules
under `web/src/styles/`; the install slice embeds and serves only the built
`app.css`. It must not serve a web manifest,
service worker, install icon, or offline cache surface.

### `ajax-web::slices::terminal`

Owns task-handle-to-terminal attach planning for the browser raw terminal bridge.
The slice resolves a qualified Ajax task handle to the registered
`tmux_session` and fixed ` task window` window target. It does not accept raw
tmux session names from the browser and does not own task lifecycle or registry
truth. The browser task terminal is raw xterm.js/tmux-first on mobile and
desktop; do not reintroduce Live/snapshot/composer as the default terminal mode
without explicit approval. Legacy snapshot, keys, and answer routes are not
supported browser task-control APIs.

`TaskDetail.tsx` mounts one `TaskTerminal.tsx` surface per task route.
The component uses xterm.js for rendering and `terminalConnection.ts` for the
WebSocket lifecycle contract; general viewport helpers remain in `viewport.ts`.
`crates/ajax-web/web/TERMINAL.md` records frontend ownership. The Rust
PTY/WebSocket backend (`/api/tasks/{handle}/terminal` route,
`ajax-web::slices::terminal`, `ajax-web::adapters::terminal_pty`) is unchanged.

Frontend ownership:

- `TaskTerminal.tsx`: lifecycle, DOM, accessibility, composition.
- `terminalConnection.ts`: WebSocket lifecycle/transport.
- `viewport.ts`: document viewport and keyboard truth.
- `terminalGeometry.ts`: pure grid/scale/row/font persistence math.
- `terminalRefit.ts`: frame coalescing, two-frame settling, 100 ms
  PTY debounce, dimension dedupe, and disposal.
- PTY adapter ownership is unchanged.

Both modules exist and are wired into `TaskTerminal.tsx`, and the
mobile-WebKit terminal behavior suite, including the repeated same-dimension
viewport-burst case, passes as of 2026-07-16.

### `ajax-web::adapters::terminal_pty`

Owns the PTY/tmux attach mechanism behind the protected task terminal
WebSocket route. It builds attach commands only from registered task evidence
and forwards terminal I/O over bounded WebSocket frames. Each WebSocket owns a
short-lived PTY child (connection-scoped); ajax-web does not keep an in-process
reconnecting-PTY hub or ring buffer across sockets (PWA/companion must stay
reconnect-safe without process-local bridge state).

The browser’s durable viewport is an **isolated grouped tmux session** named
from the task’s registered session plus a stable per-connection `client=` id
(minted once per terminal connection controller, hashed to a 12-hex suffix).
Duplicated browser tabs must not share that id. Setup uses detached `new-session -d` and treats an already-present ephemeral session (`duplicate session`) as success so reconnect reuses the viewport without attaching during setup; disconnect kills the
PTY child but leaves the ephemeral session for reattach. Destroy/reaper paths
kill orphaned ephemeral sessions; on reconnect the per-connect reaper skips the
linger target so setup can reuse it. Auto-reconnect dials `seed=0` and reuses the
client id; full history seed remains for first connect / manual reconnect.
Browser task terminal WebSocket upgrades require a same-origin `Origin` that
matches the request `Host` in addition to the normal protected-route session
and Cloudflare Access checks.

### `ajax-web::runtime`

Owns Web Cockpit runtime wiring and is not itself a slice. It sets up the Axum
HTTP listener, routing, connection handling, local HTTPS identity, graceful
shutdown, and process-level startup by composing `ajax-web::slices::*` with
`ajax-web::adapters::*`. If `ajax-cli` starts Web Cockpit, the CLI launcher
passes resolved runtime context to `ajax-web` explicitly.

Post-startup Web Cockpit routes snapshot registry state under a short mutex
hold, run external tmux/git probes outside the lock, then merge deltas back
under the lock. `/api/cockpit` refresh follows this pattern so lightweight
routes such as `/api/health` and task detail reads stay responsive during
slow substrate work. Task-terminal WebSocket upgrades read the registered
task evidence needed to build an attach plan, then the PTY/tmux bridge runs
outside the shared-state lock.

Runtime coordination contract (implemented):

- One `tokio::sync::Mutex<()>` process-local async control lane serializes
  refresh, notify, action, and start context replacement.
- Shared `std::sync::Mutex` guards are held only to clone or replace
  in-memory state, never across commands, persistence, probes, or `.await`.
- The CLI bridge and SQLite optimistic revision/merge remain the cross-process
  concurrency authority; the web runtime owns no second merge policy.
- `OperationCoordinator` intentionally admits only one mutation at a time for
  the single-operator / whole-context-snapshot design. Per-task mutation
  concurrency is deferred until task-granular commit semantics exist and
  measurement justifies it.
- Lightweight health/static/detail reads and PTY work remain outside the
  control lane after short snapshot reads.
- Cockpit reads may serve the current server-owned projection from shared
  state when refresh, notify, action, or start work already holds the control
  lane; they do not cache that fallback response. Later polls use the normal
  refresh path when the lane is available or the cache TTL expires.

The control lane (`control_lane`) is acquired by the cockpit refresh path and,
after operation admission, by the action and task-start handlers, pinned by two
runtime concurrency tests.

### Post-startup runtime refresh

`ajax-core::runtime_refresh` owns refresh tiers. Steady-state Cockpit polling
uses `RefreshTier::Live`, which skips default orphan git discovery when runtime
projections are fresh. `RefreshTier::Full` remains available for explicit
recovery and maintenance. Agent status is hydrated once per refresh from the
`AgentStatusSource` (canonical JSONL fold plus wrapper snapshot). Registered
tmux sessions are matched by exact expected
session names, not `ajax-{repo}-{handle}` parsing, so hyphenated repo names do
not trigger false orphan discovery.

External command specs for refresh, status, and pane probes carry bounded
timeouts in `ajax-core::adapters`. `CountingCommandRunner` provides reusable
command-budget fixtures for regression tests.

### Native and Web persistence

`ajax-cli::context` uses an Ajax-owned SQLite revision for optimistic
concurrency. Snapshot saves compare and advance that revision in the same
transaction; stale writers reload and merge independently added durable facts,
while incompatible same-task changes surface an explicit conflict instead of
last-writer-wins overwrite. SQLite mtime remains only a reload optimization.
CLI entry points load through `TrackedContext` so native saves participate in
the same merge contract as Web Cockpit.

The CLI bridge and SQLite optimistic revision/merge are the cross-process
concurrency authority. The web runtime owns no second merge policy; it
delegates commit/reload through the same revision-checked path.

Native Cockpit's interactive loop shares the same reload-on-mtime and
save-on-operator-action contract Web Cockpit uses. Each cockpit refresh checks
the state file mtime and reloads SQLite into the in-memory registry when it
has advanced (typically because the Web Cockpit companion or another writer
has persisted a change), and each pending cockpit action that mutates state is
persisted through `save_context_with_state` before the next iteration. The
exit-time `save_tracked_context` in `run_with_args_to_writer` remains as a
defensive backstop for state that escaped the loop's per-iteration save path.

Start execution exposes persistence checkpoints after provisional intent and
each successful provisioning receipt. The CLI Web adapter persists those
checkpoints before later external effects, so interrupted starts remain
observable and resumable.

Ship, tidy, and drop task operations refresh or re-observe substrates in
`ajax-core::task_operations` before planning destructive work. Web and CLI
surfaces delegate to these core operations rather than duplicating preflight
logic.

The browser shell consumes the same Cockpit view model as the native TUI.
Browser-specific DTOs may be narrower or differently named, but they are
projections of core output contracts, not separate task models. Any
browser-only restriction belongs at the adapter capability boundary; core
remains responsible for deciding which task actions are valid for a task.

Web Cockpit may use HTTP, TLS, filesystem storage for certificates, and static
asset embedding inside `ajax-web`. Those mechanisms must not move into
`ajax-core` or `ajax-tui`.

Web operations are coordinated by request ID and task key. External operation
work runs outside the global shared-state lock, then commits against the
prepared revision; stale commits return conflicts instead of replacing newer
state. When an operate or start reports a durable persist but loses the
process-local revision CAS (for example to a terminal acknowledgment or a
concurrent read-side metadata save), the runtime reloads authoritative SQLite
state into shared memory when present; otherwise it installs the durable operate
clone. The response always includes a fresh cockpit view instead of a false
generic conflict. `/api/cockpit` adds a short refresh TTL and single-flight gate so
near-simultaneous polls reuse the same refreshed projection, and task mutations
invalidate that window. Terminal bridge cleanup and substrate probes are
bounded so browser disconnects, pane probes, or slow external commands do not
starve lightweight routes. Supervisor cancellation terminates and awaits the
child process before reporting completion, with a bounded wait.

The process-local `OperationCoordinator` is an intentional single-operator
ceiling: only one mutation may be in flight at a time, and per-task mutation
concurrency is outside this alignment and deferred until task-granular
commit semantics exist and measurement justifies it.

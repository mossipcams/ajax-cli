---
context: default
slug: ajax-chat-architecture
status: in-progress
approval: user-directed 2026-08-17 — Composer 2.5; Phase 0+1 done; Phase 2 started 2026-08-17
last_updated: 2026-08-17
---

# Ajax chat architecture

## Goal

Give each Web Cockpit orchestration session one runtime owner without replacing
ACP, tmux, JSONL persistence, Ajax task authority, or the current chat product.

The target removes the current split ownership across `WebSessionHub`, the
`web_session` slice, the WebSocket bridge, browser outbox, reducer, and
`SessionChat`. It does not redesign the UI or introduce a new service.

Planning is approved by the user's 2026-08-17 request. Phase 0 and Phase 1
were implemented on 2026-08-17. Phase 2 was approved the same day after Phase 1
was accepted.

## Evidence and prior work

This plan builds on completed behavior work rather than reopening it:

- `.planning/agent-plans/acp-chat-reliability.md`
- `.planning/agent-plans/acp-typed-chat-ui.md`
- `.planning/agent-plans/session-chat-flow.md`
- `.planning/agent-plans/web-chat-harness-smoke-fixes.md`

Those plans established behavior that must survive:

- one ACP child and one in-flight prompt per task;
- host-owned FIFO prompt queue and processing after browser disconnect;
- `clientMessageId` acceptance and replay-safe idempotency;
- append-oriented JSONL persistence with bounded compaction;
- typed ACP updates, permissions, tool content, plans, usage, and model config;
- reconnect recovery, host-reported busy state, and the tmux terminal escape
  hatch.

Current structural pressure:

- `crates/ajax-web/src/adapters/web_session_acp/hub.rs` is 846 lines and owns
  slot lifecycle, queue execution, idempotency, transcript memory, JSONL calls,
  ACP draining, event shaping, permissions, cancellation, model replacement,
  LRU retention, and background pumping.
- `crates/ajax-web/src/adapters/web_session_acp/hub_tests.rs` is 847 lines and
  couples behavior tests to hub internals.
- `web_session` owns wire types and some policy while `bridge.rs` and `hub.rs`
  own other session decisions through an adapter-to-slice exception.
- The browser has four related mechanisms: host queue, unacknowledged prompt
  outbox, `followUpQueuedRef`, and reducer `busy`.
- Stream semantics are shaped in `coalesce_session_events`, `MessageBuffer`,
  and `sessionReducer::appendStreamed`.
- Rust wire events are manually mirrored in TypeScript and then transformed
  again into `ConversationItem`.
- Every reconnect sends the global browser model preference, which can override
  the task's stored model.
- `SessionStarter` duplicates task creation, remains Cursor-only, and does not
  submit its selected model.
- `webSessionTransport` currently dispatches one `ready` event twice.

`WebSessionHub::acquire` already releases the session-map lock before spawning
an ACP child. The plan must preserve that property. The remaining contention is
the process-wide session map around pumping, slot mutation, and transcript
append calls.

## Scope

- Make one per-task runtime own orchestration-session state and sequencing.
- Keep the session directory lock limited to lookup, insertion, and removal.
- Put session policy and state transitions in the `web_session` slice.
- Keep ACP process and protocol mechanics in `web_session_acp`.
- Separate JSONL persistence from the ACP adapter.
- Make core task metadata authoritative for the desired agent, provisioned bit,
  and desired session model.
- Simplify the browser to transport recovery plus presentation state.
- After ownership is stable, add cursor-based incremental replay and stable
  conversation item identities.
- Update the owning architecture documents as each target becomes implemented.

## Non-goals

- No new crate, frontend framework, state library, schema generator, or
  dependency.
- No replacement of the official ACP v1 runtime or ACP-over-stdio transport.
- No move of transcripts into SQLite, the task registry, tmux, or browser
  storage.
- No task lifecycle, registry-authority, authentication, network-exposure, or
  terminal-model change.
- No visual redesign of `SessionChat`, `LiveHead`, `Transcript`, tool cards, or
  Diff Review.
- No support for ACP filesystem or terminal client capabilities.
- No per-task core mutation concurrency or change to the Web Cockpit control
  lane.
- No compatibility layer for undocumented external consumers of the internal
  session WebSocket. If such consumers exist, stop and revise the plan.

## Target ownership

```text
Browser presentation
  -> authenticated session WebSocket adapter
  -> task-session directory
  -> one task-session runtime per Ajax task
       |- session state and command loop
       |- ACP driver
       |- JSONL transcript store
       `- connected WebSocket subscribers

ajax-core
  -> selected agent, worktree, provisioned bit, desired session model
```

### `ajax-core`

Core remains authoritative for facts attached to the Ajax task:

- selected agent;
- worktree path;
- provisioned orchestration-session bit;
- desired session model.

Changing the model must update task metadata through a core-owned operation
before the ACP child is replaced. A browser `localStorage` preference may seed a
new-task picker, but it must not override an existing task on reconnect.

### `ajax-web::slices::web_session`

The slice owns the browser orchestration-session capability:

- typed client commands and server events;
- admission against core-projected task evidence;
- per-task session command loop;
- one-in-flight and FIFO queue policy;
- prompt acceptance and idempotency decisions;
- applied model and ACP child generation;
- in-memory transcript cursor;
- permission and cancellation state transitions;
- mapping normalized ACP events into persisted session events;
- subscriber replay and fan-out.

Use concrete types. Do not add a trait for the single ACP implementation or a
generic session framework.

Suggested cohesive modules, subject to source-level fit:

```text
slices/web_session/
  mod.rs          public capability entry points
  protocol.rs     browser command/event envelopes
  task_session.rs per-task command loop and owned state
  transcript.rs   in-memory cursor, replay, and permission filtering
  acp_map.rs      normalized ACP event -> session event
```

Names may change to match existing Ajax conventions. Splits must follow
responsibility, not line count.

### `ajax-web::adapters::web_session_acp`

The ACP adapter owns only harness mechanics:

- program resolution and process spawn;
- ACP v1 initialization, resume, load, and new-session negotiation;
- typed ACP request/notification correlation;
- model and trusted permission config application;
- prompt, cancel, and permission-response I/O;
- child shutdown and process-exit reporting;
- model-catalog probing.

It returns typed `AcpClientEvent` values. It must not import browser wire types,
own the host prompt queue, mutate transcripts, or decide replay behavior.

### JSONL adapter

Move `store.rs` out of the ACP adapter because transcript persistence is not an
ACP mechanism. The concrete adapter owns:

- encoded handle paths;
- append and metadata records;
- bounded compaction;
- restart loading;
- blocking filesystem execution outside async critical sections.

Keep the current JSONL format during the ownership migration. Any format change
belongs to the later protocol task and must include restart compatibility tests.

### Runtime and WebSocket adapter

HTTP and WebSocket code remains thin:

- validate session cookie and same-origin WebSocket origin;
- resolve the core-backed attach plan;
- acquire a task-session handle;
- forward typed browser commands;
- send replay/live envelopes;
- detach without stopping in-flight work.

The adapter must not interpret queue, model, transcript, permission, or turn
state.

### Browser

Browser ownership is limited to:

- socket connection, visibility-aware reconnect, and disposal;
- a session-scoped set of unacknowledged prompt IDs for safe resend;
- the last applied transcript cursor;
- pure reduction of typed server events into presentation state;
- draft, scroll, sheets, speech input, and other transient UI state.

The browser must not own a FIFO prompt queue, infer durable turn state, or
override the task model on reconnect.

## Target command loop

Use one Tokio task per live or retained task session. A bounded
`tokio::sync::mpsc` command channel serializes:

- attach and detach;
- submit prompt;
- cancel;
- permission response;
- model change;
- ACP events;
- idle expiry and shutdown.

The task owns its mutable session state, so normal command processing needs no
per-session mutex. The process-wide directory stores handles only and holds its
lock for lookup, insert, or remove. Use `tokio::select!` and structured
cancellation. Do not add a second runtime.

The runtime survives its last browser subscriber while a prompt is in flight,
the host queue is non-empty, or the configured idle-retention window has not
elapsed. Dropping a task or changing harness shuts down the old runtime before
the next attach creates one.

## Target wire contract

Keep the current protocol unchanged during the ownership migration. After the
new owner is green, introduce a versioned envelope:

```text
snapshot { protocolVersion, cursor, model, turnState, pendingPermission? }
event    { protocolVersion, cursor, payload }
```

Requirements:

- Every persisted event has one monotonically increasing absolute cursor.
- Conversation items have stable host-generated IDs.
- The host normalizes ACP delta or cumulative text into one full-content item
  update. The browser replaces by item ID instead of guessing append semantics.
- Tool calls continue to update by `callId`.
- Reconnect supplies the last applied cursor and receives only newer events.
- If the cursor predates compaction or fails validation, the host sends a reset
  snapshot and bounded full replay.
- `ready` or its replacement is emitted once per logical attach/generation.
- TypeScript validates external JSON at the WebSocket boundary before the
  reducer sees it.

Do not add Rust-to-TypeScript code generation in this plan. Keep one manually
mirrored discriminated union and add cross-language JSON fixtures. Reconsider
generation only if the stabilized contract continues to change often.

## Implementation checklist

### Phase 0: baseline and defect tracking

- [x] Search GitHub issues before changing confirmed defects such as duplicate
  `ready` dispatch or ignored Session Starter model selection.
- [x] Link existing issues or open focused defect issues — Phase 2 opened
  [#910](https://github.com/mossipcams/ajax-cli/issues/910) and
  [#911](https://github.com/mossipcams/ajax-cli/issues/911) (2026-08-17).
- [x] Run and record the current focused Rust, browser, and session smoke suites.
- [x] Add no tests that merely duplicate the completed reliability plans.
- [x] Identify missing behavior coverage for ownership migration (see ledger).

Acceptance:

- Existing behavior is recorded before production edits.
- Every confirmed product defect to be fixed has an issue and focused failing
  regression test.
- Architecture changes remain behavior-neutral until their owning phase says
  otherwise.

### Phase 1: move session ownership without changing protocol

- [x] Add the task-session command loop and directory in
  `ajax-web::slices::web_session`.
- [x] Move queue, in-flight, idempotency, permission, generation, transcript
  cursor, subscriber, and idle-retention state out of `WebSessionHub`.
- [x] Keep ACP spawn/resume/load/new and process I/O in the ACP adapter.
- [x] Move JSONL storage into its own concrete adapter (`web_session_store`).
- [x] Replace the 50 ms process-wide pump thread with per-session Tokio task
  progress.
- [x] Keep blocking ACP spawn and filesystem work outside the directory lock
  and async critical sections.
- [x] Make the WebSocket bridge forward commands and envelopes only (`ws_bridge`).
- [x] Remove the adapter-to-`web_session` architecture exception after the ACP
  adapter no longer imports slice/browser types.
- [x] Split `hub_tests.rs` into slice behavior tests, ACP adapter tests, store
  tests, and runtime bridge tests. Preserve assertions; do not rewrite tests
  only to match new internals.
- [x] Delete the old `WebSessionHub` once all callers use the new directory and
  no compatibility facade remains necessary.

Acceptance:

- Session protocol JSON and observable chat behavior are unchanged.
- Existing ACP reliability and typed-chat suites pass.
- No handwritten Rust file touched by the phase exceeds 1,000 lines.
- The directory lock is never held across ACP work, persistence, or `.await`.
- One task's slow ACP child or transcript store does not block another task's
  attach, prompt, replay, cancel, or permission response.

### Phase 2: unify task and model configuration

- [ ] Add or reuse one core-owned operation for changing desired session model.
- [ ] Make model change persist before replacing the ACP child; report a typed
  failure if persistence or replacement fails.
- [ ] Stop putting the global browser model preference in every reconnect URL
  ([#910](https://github.com/mossipcams/ajax-cli/issues/910)).
- [ ] Use the global preference only as the default for new-task model choice.
- [ ] Remove the duplicate `SessionStarter` creation implementation. Route
  bare `#/session` through the existing New Task flow with orchestration chat
  selected ([#911](https://github.com/mossipcams/ajax-cli/issues/911)).
- [ ] Do not carry Session Starter's optional constraints/outcome fields into
  the unified sheet unless product evidence requires them. The first composer
  prompt covers that use case.
- [ ] Replace Cursor-specific open-failure copy with capability/task-evidence
  based copy.
- [ ] Align `web-session-behavior.md` with the implemented
  `session/set_config_option` model contract.

Acceptance:

- Cursor, Codex, Claude, and Pi use the same task-creation path.
- The chosen model is submitted on task creation.
- Reconnect cannot silently change an existing task's desired model.
- Harness swap and model change leave registry metadata, live ACP child, and
  browser projection consistent.

### Phase 3: simplify browser state

- [ ] Keep `webSessionTransport` responsible only for wire validation, socket
  lifecycle, and unacknowledged prompt resend.
- [ ] Remove duplicate `ready` delivery.
- [ ] Remove `followUpQueuedRef`, `lastQueuedTextRef`, and the second browser
  queue behavior. Submitting while busy sends one host-queued prompt; Stop owns
  cancellation.
- [ ] Extract one concrete `useTaskSession` hook that owns connection callbacks,
  reducer wiring, and activity state. Keep layout, sheets, scroll, speech, and
  gestures in `SessionChat`.
- [ ] Keep `sessionReducer` pure and exhaustive over the wire union.
- [ ] Make every `parseServerEvent` variant validate its required fields before
  entering the reducer.
- [ ] Clear obsolete outbox and starter-seed records when the host reports task
  removal, harness replacement, or invalid session identity.

Acceptance:

- The browser has one unacknowledged resend set, not a second durable/FIFO
  queue.
- `SessionChat` no longer coordinates transport policy.
- Reload and reconnect preserve accepted prompts without duplicates.
- Permission, tool, plan, usage, Markdown, scroll pinning, speech, terminal
  sheet, and Diff Review behavior remain unchanged.

### Phase 4: add cursor replay and one stream normalization point

- [ ] Increment the internal session protocol version.
- [ ] Add snapshot and cursor-bearing event envelopes.
- [ ] Assign stable host item IDs and normalize ACP streamed text to full item
  updates in the session slice.
- [ ] Persist cursor/item identity needed for restart replay.
- [ ] Have the browser retain the last applied cursor and request incremental
  replay.
- [ ] Fall back to reset plus bounded full replay after compaction or invalid
  cursor.
- [ ] Reduce `MessageBuffer` to render batching only, or delete it if React
  batching is sufficient.
- [ ] Delete semantic text merging from the browser reducer after the host owns
  delta-versus-snapshot normalization.
- [ ] Add cross-language JSON fixtures for every command, snapshot, and event
  variant.

Acceptance:

- Reconnect after one new event transfers only that event plus attach state.
- Replay after restart produces the same ordered conversation as the live path.
- Delta-streaming and cumulative-streaming harnesses render identical text
  without duplicate or missing content.
- Compaction preserves absolute cursor recovery or forces an explicit reset.
- Browser code has no ACP-specific delta/cumulative heuristic.

### Phase 5: documentation and final verification

- [ ] Update `architecture.md` only for durable ownership and dependency rules.
- [ ] Update `docs/architecture/web-cockpit.md` with the implemented task-session
  runtime, model authority, transcript store, and replay protocol.
- [ ] Update `docs/architecture/web-session-behavior.md` with falsifiable queue,
  model, cursor, restart, permission, and shutdown invariants.
- [ ] Update architecture tests to enforce slice, adapter, store, and runtime
  dependency direction.
- [ ] Rebuild tracked browser assets only when source changes are final.
- [ ] Run focused and broad validation, record every result below, and disclose
  failures or skipped checks.

## Expected change slices

Keep review units behaviorally coherent:

1. Session runtime ownership, protocol v1 unchanged.
2. Task/model authority and unified creation path.
3. Browser state simplification.
4. Protocol cursor/item identity and incremental replay.
5. Durable documentation, generated assets, and full verification.

Do not combine all phases into one large rewrite. Each slice must leave the
repository green and preserve flag-off parity.

## Verification commands

Focused Rust:

```bash
cargo test -p ajax-web --lib web_session
cargo test -p ajax-web --lib web_session_acp
cargo nextest run -p ajax-web
npm run verify:arch
```

Focused browser:

```bash
npm run web:test -- --run \
  src/shared/lib/webSessionTransport.test.ts \
  src/features/session/messageBuffer.test.ts \
  src/features/session/sessionThread.test.ts \
  src/features/session/useSessionTransport.test.ts \
  src/features/session/SessionChat.test.tsx
npm run web:check
npm run web:lint
npm run web:sg
```

Session browser regression:

```bash
npx playwright test \
  --config crates/ajax-web/web/playwright.config.mts \
  --project=mobile-webkit \
  crates/ajax-web/web/e2e/session-chat-regression.test.ts
```

Broad gate:

```bash
cargo fmt --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features --test-threads=1
cargo test --doc
npm run web:check
npm run web:lint
npm run web:sg
npm run web:test -- --run
npm run web:smoke
npm run web:build:check
npm run ci:verify
git diff --check
```

If Nextest is unavailable, use the equivalent `cargo test` command and record
the substitution.

## Stop conditions

Stop and revise this plan before:

- changing task lifecycle, registry authority, authentication, public exposure,
  or tmux ownership;
- moving transcripts into core/SQLite or adding browser transcript persistence;
- removing or weakening `clientMessageId` idempotency, permission replay
  filtering, queue survival, or restart recovery;
- adding a generic session framework, a new crate, or a dependency;
- changing the WebSocket protocol before Phase 1 is green;
- removing `SessionStarter` if product evidence shows its optional structured
  brief is a required workflow;
- shipping protocol v2 without confirming that no external client requires v1;
- changing more than one expected change slice in the same review unit;
- weakening existing assertions to make the migration pass.

Any architecture deviation must be written here and approved before
implementation continues.

- **Phase 1 follow-up (2026-08-17):** `TaskSessionDirectory` idle eviction initially
  used a sticky `evictable` flag from `Release`; fixed to query live slot state at
  eviction time (`holders==0` and not busy) so finished disconnected sessions re-enter
  the LRU pool.
- **Phase 1 follow-up (2026-08-17):** `acquire` claims the slot (`last_released = None`)
  in the same directory lock that clones the sender (`ensure_entry_for_acquire`);
  eviction re-lock skips reattached entries and re-checks live evictability before
  `Shutdown`.

## Validation ledger

| Phase | Command | Result |
| --- | --- | --- |
| Planning | Source and architecture inspection | complete |
| 0 | `cargo test -p ajax-web --lib web_session` | 113 passed |
| 0 | `cargo test -p ajax-web --lib web_session_acp` | 36 passed |
| 0 | Focused browser session tests (5 files) | 63 passed |
| 0 | GitHub issue search (ready / SessionStarter) | no matches |
| 1 | `cargo fmt --check` | pass |
| 1 | `cargo test -p ajax-web --lib web_session` | 116 passed (parent re-verify) |
| 1 | `cargo test -p ajax-web --lib web_session_acp` | 36 passed |
| 1 | `cargo test -p ajax-web --lib architecture` | 6 passed |
| 1 | `cargo nextest run -p ajax-web` | 386 passed |
| 1 | `npm run verify:arch` | pass |
| 2–5 | (not started) | pending |

- Checklist: Phase 0 + Phase 1 complete (18/18 items). Phases 2–5 unchanged.

## Approval and status

- Plan creation: approved by user on 2026-08-17.
- Implementation: Phase 0 + Phase 1 complete 2026-08-17. Phase 2 approved 2026-08-17.
- Checklist: Phase 0 + Phase 1 complete (18/18 items). Phase 2 in progress.
- Current repository edits from this plan: session runtime ownership migration (committed); Phase 2 task/model authority.

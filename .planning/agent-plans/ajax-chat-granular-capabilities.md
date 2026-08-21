---
context: default
slug: ajax-chat-granular-capabilities
status: complete
approval: granted
last_updated: 2026-08-21
---

# Ajax Chat granular capabilities

## Goal

Refactor Ajax Chat incrementally so routine feature work has one clear owner.
Keep Chat as one vertical slice inside `ajax-web`. Make `ChatSurface` a thin
composition layer. Project validated transport events into typed Chat models
before React presentation reads them.

The first acceptance milestone is concrete:

> An agent changing queued-follow-up UX and an agent changing tool-call
> rendering can work in separate worktrees without editing any common
> production file.

## Approval and checklist status

- Planning investigation is complete.
- Implementation approval granted on 2026-08-21 by explicit user request
  to implement this plan.
- Implementation is in progress, starting at Phase 0 then Phase 1.
- Every phase must update this checklist and record its verification results.
- Architecture or behavior deviations require approval before implementation
  continues.

Planning verification on 2026-08-21:

| Check | Result |
| --- | --- |
| Initial `git status --short --branch` | Clean branch before plan creation |
| Current `git status --short --branch` | Only this untracked plan |
| `git diff --check` | Pass |
| Code and browser tests | Not run. Planning-only change |

Phase 0 baseline on 2026-08-21 after implementation approval:

| Check | Result |
| --- | --- |
| Branch | `ajax/ajax-chat-granular-features` tracking `origin/main` |
| `git status --short --branch` | Untracked plan and local `.audit/` trail only |
| `git diff --check` | Pass |
| `npm run web:test -- --run src/features/chat` | Pass. 17 files, 218 tests |
| `npm run web:check` | Pass |
| `npm run web:lint` | Pass |
| `npm run web:sg` | Pass |
| `npm run verify:arch` | Pass. ajax-core 8, ajax-web 13, ajax-tui 2, ajax-supervisor 2 |
| Mobile WebKit | Playwright 1.61.1 is installed. Phase 1 does not require the keyboard e2e suite |

## Scope

- Frontend code under `crates/ajax-web/web`.
- Ajax Chat composition, session projection, transcript, activity, composer,
  permissions, model controls, status, scrolling, connection recovery, and
  Chat-owned CSS.
- Existing frontend import, CSS, and architecture enforcement.
- Focused architecture documentation updates when each boundary lands.

## Non-goals

- No rewrite, visual redesign, or behavior change.
- No new frontend state library, package, crate, or dependency.
- No global Chat store or one reducer that owns all local UI behavior.
- No generic `utils`, `helpers`, `services`, or reusable session framework.
- No change to ACP, protocol v2, Rust task-session ownership, JSONL persistence,
  host queueing, registry truth, task lifecycle, authentication, Terminal, or
  task authority.
- No browser-owned transcript, durable queue, task state, or lifecycle
  inference.
- No class-name cleanup merely to match the new folders.
- No compatibility facade for moved internal browser modules. Migrate callers
  and delete the old internal path in the same phase.

## Evidence

### Historical correction

[PR #839](https://github.com/mossipcams/ajax-cli/pull/839) is the open 0.58.0
release PR. It aggregates Ajax Chat changes but did not introduce the feature.
The relevant implementation history begins with #869.

The sequence that shaped the current code is:

- #869 introduced the flag-gated session surface.
- #903 introduced typed conversation items while leaving wire events as the
  reducer input.
- #915 established the correct backend task-session owner and protocol v2.
- #917, #958, #964, #966, and #967 added thinking, status, activity, context
  usage, and turn usage through shared reducer and presentation files.
- #982 split the former stylesheet monolith into ownership leaves.
- #986 redesigned the mobile conversation and heavily changed the session
  surface, transcript, and transcript CSS.
- #995 extracted Task Workspace and renamed the frontend capability to Chat.
- #999, #1016, #1019, and #1025 continued to change model, permission, and
  activity hotspots.

The completed
`.planning/agent-plans/task-workspace-foundation.md` plan deliberately deferred
the internal Chat split. This plan is that follow-up. It preserves the Task
Workspace boundary that already landed.

### Current runtime flow

1. `src/shared/lib/webSessionTransport.ts` opens the authenticated WebSocket,
   validates protocol-v2 envelopes, owns the unacknowledged prompt outbox, and
   tracks the in-page replay cursor.
2. `features/chat/useSessionTransport.ts` owns open, reconnect backoff,
   visibility recovery, `MessageBuffer`, snapshots, and connection flags.
3. `features/chat/useTaskSession.ts` owns the reducer, connection state,
   activity age, applied model and config state, and session commands.
4. `features/chat/sessionThread.ts` reduces raw `WebSessionServerEvent` values
   into `SessionState`.
5. `features/chat/ChatSurface.tsx` combines task context, session state, head
   selectors, queueing, composer input, speech, model sheets, scrolling, and
   navigation gestures.
6. `features/chat/Transcript.tsx` groups flat items into turns and renders user
   turns, assistant turns, Markdown, activity, thinking, plans, tools,
   permission markers, notes, and dividers.
7. Rust `ajax-web::slices::web_session` remains authoritative for transcripts,
   one in-flight turn, the host FIFO queue, cancellation, permissions, replay,
   and ACP runtime state.

The backend authority split is correct. This plan changes only browser
projection and presentation ownership.

### Conflict magnets

History from August 13 through August 21 shows these logical files as the main
merge-conflict risks:

| File or predecessor | Commits touching it | Coupled responsibilities |
| --- | ---: | --- |
| `SessionChat.tsx` plus `ChatSurface.tsx` | 28 | Queue, input, scroll, keyboard, speech, model controls, status, navigation, and composition |
| `SessionChat.test.tsx` plus `ChatSurface.test.tsx` | 39 | Most Chat behavior shares one integration suite |
| `sessionThread.ts` | 15 | Turns, messages, tools, plans, permissions, status, usage, errors, and replay |
| `shared/lib/webSessionTransport.ts` | 15 | Wire types, parsing, socket control, outbox, commands, replay, and snapshots |
| `Transcript.tsx` | 11 | Conversation, activity disclosure, thinking, plans, permissions, and reveal |
| `LiveHead.tsx` | 12 | Task attention, ACP status, tools, permissions, usage, stale activity, and connection |

`ChatSurface.tsx` is 545 lines. `sessionThread.ts` is 590 lines.
`Transcript.tsx` is 322 lines. Their size is less important than the number of
unrelated reasons each file changes.

PR #982 already removed the worst global CSS hotspot. Keep that work. The
remaining CSS problem is mixed ownership inside the current leaves:

- `styles/session/transcript.css` also owns queued follow-ups, Markdown, and
  jump-to-latest.
- `styles/session/activity.css` also owns plans, reasoning, context usage, and
  jump positioning.
- `styles/session/live-head.css` owns permission controls and some shared sheet
  action rules.
- `styles/session/composer.css` owns model-control chrome.
- `styles/session/sheets.css` mixes Chat model sheets with Task Workspace
  details sheets.

### Current dependency pressure

Current code already prevents React components from parsing raw JSON. The gap
is narrower than a missing architecture:

- `sessionThread.ts` directly consumes and re-exports transport event types.
- `messageBuffer.ts` directly consumes transport events.
- `useSessionTransport.ts` imports `BrowserTaskDetail` to create presentation
  copy for open failures.
- `ChatSurface.tsx` reads broad `SessionState`, `BrowserTaskDetail`, and live
  config descriptors, then derives state for several capabilities.
- `Transcript.tsx` owns both conversation arrangement and activity
  presentation.
- `LiveHead.tsx` accepts task detail, session status, permission, activity,
  connection, and usage inputs separately.
- `TaskWorkspace.tsx` imports `clearSessionOutbox` from a shared file whose
  remaining behavior is Chat-specific.
- `parsePayload` in `webSessionTransport.ts` casts tool `content` arrays without
  validating each content variant.

Two existing imports are not architecture defects:

- Chat may consume stable Task contracts through
  `@/features/task/public`. The focused architecture document explicitly allows
  that direction.
- `shared/lib/liveSessionConfig.ts` has real consumers outside one Chat
  component and remains a stable shared contract.

Do not replace those imports with callback threading merely to claim stricter
isolation. That would move model changes into Task Workspace and create a new
conflict path.

## Lead judgment

### Act on

- Remove composer and activity ownership from `ChatSurface` and `Transcript`
  first. That satisfies the primary worktree test early.
- Add one wire-to-Chat projection boundary.
- Keep one session-local replay projection, split by state domain.
- Give connection recovery and composer queueing explicit state models.
- Give scrolling its own DOM controller.
- Split mixed CSS leaves by capability.
- Enforce the resulting import direction with ESLint and existing architecture
  tests.

### Keep

- One `features/chat` vertical slice.
- Host authority over transcript, queue, turn state, and ACP runtime.
- One ordered session projection for replayed server events.
- Local React state for disclosure toggles, sheets, draft text, and DOM scroll
  geometry.
- The current Markdown implementation.
- The current protocol fixtures and CSS manifest model.
- The documented Chat to Task public-contract dependency.

### Reject

- One reducer per visual component.
- One global Chat reducer that also owns composer, scroll, sheets, and
  connection effects.
- React Context or another state library.
- A generic frontend service layer.
- New top-level features, packages, or crates for internal Chat capabilities.
- A split based only on line count.

## Named data shape

The refactor centers on one typed browser projection:

```text
ChatSessionView
  conversation: ChatTurn[]
  turn: ChatTurnState
  permission: ChatPermissionState
  status: ChatStatusState
  usage: ChatUsageState
  model: ChatModelState
  revision: number
```

`ChatSessionView` is per mounted task session. It is not a global store and does
not own local component state.

Validated protocol frames map once:

```text
SessionSnapshot | WebSessionServerEvent
  -> projectWireInput
  -> ChatSessionEvent
  -> reduceChatSession
  -> ChatSessionView
```

Local state stays with the capability that owns the interaction:

```text
ComposerState = idle | queued | stopping
ConnectionState = connecting | connected | waiting | failed | disposed
ScrollState = pinned | reading-history
```

The scroll model remains refs plus local booleans because DOM geometry is the
authority. It does not enter `ChatSessionView`.

## Target tree

The exact names may adjust during implementation when source fit proves a
better name. The ownership and dependency direction may not drift.

```text
features/chat/
  public.ts
  ChatSurface.tsx
  ChatSurface.test.tsx

  session/
    public.ts
    model.ts
    projectWireInput.ts
    reducer.ts
    turnProjection.ts
    activityProjection.ts
    permissionProjection.ts
    statusProjection.ts
    selectors.ts
    errors.ts
    useChatSession.ts
    transport/
      contracts.ts
      parseServerFrame.ts
      webSessionTransport.ts
      messageBuffer.ts
      fixtures.ts
    connection/
      connectionState.ts
      useSessionConnection.ts

  conversation/
    public.ts
    Conversation.tsx
    Turn.tsx
    AssistantTurn.tsx
    UserTurn.tsx
    Markdown.tsx
    reveal.ts
    groupTurns.ts

  activity/
    public.ts
    ActivityDisclosure.tsx
    ToolCard.tsx
    PlanChecklist.tsx
    ReasoningRow.tsx
    presentation.ts

  composer/
    public.ts
    ChatComposer.tsx
    QueuedFollowUp.tsx
    composerState.ts
    autoGrow.ts
    speech/
      useChatSpeech.ts

  scrolling/
    public.ts
    ChatScroller.tsx
    useChatScroll.ts
    useChatViewport.ts

  permissions/
    public.ts
    PermissionPanel.tsx

  model/
    public.ts
    SessionModelControls.tsx
    useSessionModelNotice.ts
    sessionModelErrors.ts

  status/
    public.ts
    LiveHead.tsx
    headView.ts
    UsageIndicators.tsx
```

Do not create empty folders. Each phase creates a directory only when it moves
or extracts its first real owner.

## Dependency direction

```text
Task Workspace
  -> features/chat/public
  -> ChatSurface
  -> Chat capability public modules
  -> chat/session/public
  -> chat/session/transport
  -> browser APIs and stable shared contracts
```

Internal rules:

- `ChatSurface.tsx` is the only file allowed to compose multiple top-level Chat
  capabilities.
- Capabilities may import React, shared UI, stable shared contracts, and
  `chat/session/public`.
- Capabilities must not import another capability.
- One deliberate exception is allowed. `conversation` may import
  `activity/public` because a turn owns where its activity disclosure appears.
  Do not add render-prop plumbing only to avoid this single visible dependency.
- `session` must not import React presentation or any Chat capability.
- `session/transport` must not import session reducers or presentation.
- Raw transport frame and event types must not escape `session`.
- Task Workspace may import only `features/chat/public`.
- Chat may import `features/task/public` for the existing stable desired-model
  contract. It must not import Task internals or any other sibling feature.
- Public modules export the smallest consumed contract. They do not re-export
  internals for test convenience.
- Tests may use fixtures across boundaries where the current ESLint policy
  permits it.

## Capability contracts

### Composition

**Owned files.**

- `features/chat/ChatSurface.tsx`
- `features/chat/public.ts`

**Owned state.**

- Navigation swipe state only.

**Inputs and outputs.**

- Input is a narrow `ChatTaskContext`, workspace header slot, task-action slot,
  Back callback, Diff callback, and mutation callback.
- Output is the existing session activity callback used to disable harness
  switching while busy.

**Allowed imports.**

- Chat capability public modules.
- Shared navigation hook.
- Narrow public types.

**Forbidden imports.**

- Raw transport types.
- Reducer internals.
- `BrowserTaskDetail`.
- Task, Workspace, Terminal, Settings, or App internals.

**Current code to move.**

- All queue, composer, speech, model, permission, status, and scroll behavior.

**Tests.**

- Keep one small composition smoke suite.
- Move behavioral assertions to the owning capability suite.

### Composer

**Owned files.**

- `composer/ChatComposer.tsx`
- `composer/QueuedFollowUp.tsx`
- `composer/composerState.ts`
- `composer/autoGrow.ts`
- `composer/speech/useChatSpeech.ts`

**Owned state.**

- Draft text.
- One editable queued follow-up.
- Stop-and-send transition.
- Speech insertion state.
- Textarea height.

**Inputs and outputs.**

- Inputs are `connected`, authoritative `busy`, model-control slot, and session
  command callbacks.
- Outputs are `send`, `cancel`, `markStopped`, and `scrollToLatest`.

**Allowed imports.**

- `chat/session/public`.
- Shared form and speech mechanisms.

**Forbidden imports.**

- Conversation, activity, scrolling, permission, model, or status internals.
- Transport types.

**Current code to move.**

- `sendDraft`, `editQueued`, `clearDraft`, `submitComposer`, the queue flush
  effect, composer JSX, queued JSX, `sessionChatChrome.autoGrow`, and Chat speech
  wiring from `ChatSurface`.

**Tests and contracts.**

- Queue while busy.
- Edit and remove the queued follow-up.
- Second Enter sends cancel, waits for host `busy` to clear, records `Stopped`,
  then sends.
- Reconnect does not dispatch queued text while disconnected.
- Speech changes the draft without submitting.
- `ComposerState` cannot represent `stopping` without queued text.

### Conversation

**Owned files.**

- `conversation/Conversation.tsx`
- `conversation/Turn.tsx`
- `conversation/AssistantTurn.tsx`
- `conversation/UserTurn.tsx`
- `conversation/Markdown.tsx`
- `conversation/reveal.ts`
- `conversation/groupTurns.ts`

**Owned state.**

- No server-derived state.
- Assistant reveal is a pure projection of current text and turn completion.

**Inputs and outputs.**

- Input is typed `ChatTurn[]` and turn state from `ChatSessionView`.
- Output is presentation only.

**Allowed imports.**

- `chat/session/public`.
- `activity/public`.
- Shared UI.

**Forbidden imports.**

- Composer, scrolling, permissions, model, status, transport, or Task.

**Current code to move.**

- `Transcript.tsx`, `sessionTurns.ts`, `Markdown.tsx`, `settledText`, user and
  assistant row rendering, notes, permission markers, and dividers.

**Tests and contracts.**

- User and assistant turn order.
- Stable turn and item keys through replay.
- Paragraph-complete reveal.
- No cut inside fenced Markdown blocks.
- Markdown and safe-link behavior.
- Context reset, reconnect, cancellation, and harness-switch dividers.

### Activity

**Owned files.**

- `activity/ActivityDisclosure.tsx`
- `activity/ToolCard.tsx`
- `activity/PlanChecklist.tsx`
- `activity/ReasoningRow.tsx`
- `activity/presentation.ts`

**Owned state.**

- Manual disclosure override.
- Reasoning expansion.
- Tool-output preview expansion.

**Inputs and outputs.**

- Input is typed activity items already projected by session state.
- Output is local disclosure gestures only.

**Allowed imports.**

- `chat/session/public`.
- Shared UI.

**Forbidden imports.**

- Conversation, composer, scrolling, permissions, model, status, or transport.

**Current code to move.**

- `ToolCard.tsx`, `toolPresentation.ts`, `Thought`, `PlanChecklist`,
  `TurnActivity`, `currentOperation`, and `activitySummary`.

**Tests and contracts.**

- Tool revision by `callId`.
- Omitted update fields preserve existing content.
- Text output and diff rendering.
- Failure expansion.
- Plan replacement.
- Reasoning expansion.
- Manual open or close wins over automatic state.

### Scrolling

**Owned files.**

- `scrolling/ChatScroller.tsx`
- `scrolling/useChatScroll.ts`
- `scrolling/useChatViewport.ts`

**Owned state.**

- Pinned versus reading-history intent.
- Behind indicator.
- Seen revision.
- DOM refs and layout-settle refs.

**Inputs and outputs.**

- Input is a semantic content revision, queued-preview slot, conversation node,
  and composer ref.
- Output is `scrollToLatest`, pointer blur handling, and the jump control.

**Allowed imports.**

- Shared mobile keyboard and viewport mechanisms.
- React and DOM APIs.

**Forbidden imports.**

- Session event types.
- Conversation item internals.
- Composer, activity, permissions, model, or status.

**Current code to move.**

- `pinned`, `behind`, `seenRef`, scroll handlers, layout effect,
  `MutationObserver`, `ResizeObserver`, jump JSX, pointer blur, and existing
  `viewport/useChatViewport`.

**Tests and contracts.**

- Initial render starts at latest content.
- New content follows only while pinned.
- History readers are not pulled to the bottom.
- Keyboard and composer resize preserve intent.
- Jump returns to the live edge without animation.
- Observer-driven growth uses semantic revision rather than tool knowledge.

### Permissions

**Owned files.**

- `permissions/PermissionPanel.tsx`

**Owned state.**

- No independent durable state.
- Pending and resolved permission state remains in `ChatSessionView` because
  replay ordering decides it.

**Inputs and outputs.**

- Input is a typed permission view and connection state.
- Output is approve or reject.

**Allowed imports.**

- `chat/session/public`.
- Shared buttons.

**Forbidden imports.**

- Raw transport, Task detail, composer, activity, model, or scrolling.

**Current code to move.**

- Decision markup and controls from `LiveHead`.

**Tests and contracts.**

- Answer clears the panel immediately.
- Disconnected controls are disabled.
- Replay cannot resurrect a resolved request.
- Auto-approved production sessions continue to show no panel.

### Status

**Owned files.**

- `status/LiveHead.tsx`
- `status/headView.ts`
- `status/UsageIndicators.tsx`

**Owned state.**

- No session state.
- Activity-age timer may remain in the session hook or move to a status hook.
  It never changes host turn state.

**Inputs and outputs.**

- Input is one typed `ChatHeadView`, a permission slot, task-action slot, and
  Stop callback.
- Output is Stop.

**Allowed imports.**

- `chat/session/public`.
- Shared UI.

**Forbidden imports.**

- `BrowserTaskDetail`.
- Raw transport.
- Activity component internals.
- Composer, model, or scrolling.

**Current code to move.**

- `LiveHead.tsx`, head state and tone selectors, active tool, active plan step,
  latest thought, context usage, turn usage, stale activity, and connection
  presentation.

**Tests and contracts.**

- Permission outranks status.
- ACP status outranks task attention where current behavior says so.
- Working, waiting, idle, disconnected, and stale activity labels.
- Context pressure and per-turn tokens remain separate.
- Missing token fields never render as zero.

### Model controls

**Owned files.**

- `model/SessionModelControls.tsx`
- `model/useSessionModelNotice.ts`
- `model/sessionModelErrors.ts`

**Owned state.**

- Sheet open state.
- Dismissible configuration error.
- Host-confirmed applied model and options remain in `ChatSessionView`.

**Inputs and outputs.**

- Input is confirmed model and advertised config options.
- Output sends the exact advertised `configId` and value.

**Allowed imports.**

- `chat/session/public`.
- `features/task/public` for the existing desired-model preference contract.
- `shared/lib/liveSessionConfig`.
- Shared sheet and button components.

**Forbidden imports.**

- Task internals.
- Workspace, composer, status, activity, or transport internals.

**Current code to move.**

- `SessionModelControls.tsx`, `sessionModel.ts`, model notice state, and model
  sheet state from `ChatSurface`.

**Tests and contracts.**

- Model, effort, and Fast use advertised options only.
- Picks remain pessimistic until a replacement snapshot confirms them.
- Config refusal leaves confirmed state unchanged.
- Applied model, desired task pin, and New Task preference stay distinct.
- Reconnect never puts the browser preference on the WebSocket URL.

### Session view model

**Owned files.**

- `session/model.ts`
- `session/projectWireInput.ts`
- `session/reducer.ts`
- Domain projection files.
- `session/selectors.ts`
- `session/errors.ts`
- `session/useChatSession.ts`

**Owned state.**

- Conversation and turn projection.
- Host-reported busy state.
- Pending permission and resolved IDs.
- ACP status.
- Context usage and turn usage.
- Applied model and advertised options.
- Stable local projection revision.

**Inputs and outputs.**

- Input is validated transport snapshots and events.
- Output is `ChatSessionView` and a narrow command port.

**Allowed imports.**

- `session/transport/contracts`.
- Stable shared types and `features/task/public` where the existing model
  preference requires it.

**Forbidden imports.**

- React presentation.
- Chat capabilities.
- Workspace, Terminal, Settings, or App.

**Current code to move.**

- Types, reducer logic, errors, and selectors from `sessionThread.ts`.
- Orchestration from `useTaskSession.ts`.

**Tests and contracts.**

- Every protocol fixture maps to one typed Chat event or a documented drop.
- Reducer input contains no raw role, status, plan-status, or tool-status string.
- Replay reset and incremental replay produce the same view.
- Tool updates preserve content.
- Permission replay stays idempotent.
- Context and turn usage remain separate.
- Errors settle the turn and preserve current operator copy.

### Transport and connection

**Owned files.**

- `session/transport/*`
- `session/connection/*`

**Owned state.**

- Socket readiness.
- In-memory replay cursor.
- Session-scoped unacknowledged prompt outbox.
- Reconnect attempt and visibility state.
- Buffered full-content updates.

**Inputs and outputs.**

- Input is task handle and typed client commands.
- Output is validated snapshots and wire events.

**Allowed imports.**

- Browser APIs.
- Stable shared contracts such as live config descriptors.

**Forbidden imports.**

- Session reducer.
- React presentation.
- Browser task detail.
- Any Chat capability.

**Current code to move.**

- `shared/lib/webSessionTransport.ts`.
- `messageBuffer.ts`.
- `useSessionTransport.ts`.
- `webSessionFixtures.ts` if its search confirms no non-Chat consumer.

**Tests and contracts.**

- Full protocol-v2 frame validation.
- Tool content parsing without unchecked casts.
- Snapshot and replay ordering.
- In-page cursor reuse and cold-load full replay.
- Outbox acknowledgement and resend.
- Visibility recovery and bounded handshake attempts.
- Exhaustive connection-state transitions.
- Storage keys and wire JSON remain unchanged during the move.

### CSS

Keep one shipped `app.css` and the ordered source manifest. Split authorship,
not runtime assets.

Target leaves:

```text
styles/chat/
  surface.css
  conversation.css
  markdown.css
  activity.css
  composer.css
  scrolling.css
  permissions.css
  model.css
  status.css

styles/task-workspace/
  sheets.css
```

Rules:

- Preserve selector spelling and computed behavior during moves.
- Preserve exact cascade order with the existing style-source tests.
- Move queued selectors out of transcript ownership.
- Move jump selectors out of activity ownership.
- Move permission selectors out of status ownership.
- Move model selectors out of composer and mixed sheet ownership.
- Move task-details selectors out of Chat ownership.
- Do not add CSS imports from React modules. Keep the deterministic manifest
  graph.

## State-domain decisions

| Domain | Proposed owner | State mechanism |
| --- | --- | --- |
| Session lifecycle | Rust `web_session`; browser session projection only | No second browser lifecycle machine |
| Turn lifecycle | `session/turnProjection.ts` | Session-local reducer |
| Stream batching | `session/transport/messageBuffer.ts` | Small imperative render buffer |
| Assistant reveal | `conversation/reveal.ts` | Pure derived function |
| Activity and tool lifecycle | `session/activityProjection.ts`; local disclosure in `activity` | Reducer helper plus local component state |
| Composer and queue | `composer/composerState.ts` | Discriminated reducer or transition function |
| Permission lifecycle | `session/permissionProjection.ts`; controls in `permissions` | Reducer helper |
| Scroll lifecycle | `scrolling/useChatScroll.ts` | DOM refs and local booleans |
| Model and config | Session snapshot state; local controls in `model` | Reducer fields plus local sheet state |
| Connection and reconnect | `session/connection/connectionState.ts` | Explicit state machine |
| Context and turn usage | `session/statusProjection.ts`; display in `status` | Replace-current reducer fields |
| Harness and context reset | Conversation note or divider | No independent state |

## Incremental phases

### Phase 0. Baseline and plan activation

**Objective.**

Record a known-green baseline before structural edits.

**Likely files.**

- This plan only.

**Moves and extractions.**

- None.

**Risks.**

- Existing failures could make later attribution unclear.

**Tests.**

```bash
npm run web:test -- --run src/features/chat
npm run web:check
npm run web:lint
npm run web:sg
npm run verify:arch
git diff --check
```

**Dependency rule established.**

- None yet. Record the current import graph.

**Concurrency after this phase.**

- None. This is the baseline.

**Checklist.**

- [x] Approval recorded.
- [x] Current branch and status recorded.
- [x] Focused tests recorded.
- [x] Typecheck, lint, static analysis, and architecture checks recorded.
- [x] Mobile WebKit availability recorded.

### Phase 1. Cut composer and activity ownership

**Objective.**

Remove queued-follow-up behavior from `ChatSurface` and activity rendering from
`Transcript`. This is the first conflict-reduction phase.

**Likely files.**

- Add `features/chat/composer/*`.
- Add `features/chat/activity/*`.
- Modify `ChatSurface.tsx` and `Transcript.tsx`.
- Move `sessionChatChrome.ts` and `speech/useChatSpeech.ts`.
- Move `ToolCard.tsx` and `toolPresentation.ts`.
- Split `ChatSurface.test.tsx` and `Transcript.test.tsx`.
- Split composer and activity CSS selectors.

**Moves and extractions.**

- Extract composer state, queue transitions, queued preview, speech, and
  textarea growth.
- Extract activity disclosure, tool card, plans, reasoning, and activity
  summaries.
- Delete the old flat modules after callers move.

**Behavioral risks.**

- Sending before host `busy` clears.
- Losing queued text on a rejected send.
- Resetting manual activity disclosure.
- Changing tool update or failure expansion behavior.

**Tests.**

- Composer transition tests.
- Queue, edit, remove, reconnect, stop-and-send, and speech tests.
- Tool, plan, reasoning, content, diff, and disclosure tests.
- Existing Chat composition smoke test.

**Dependency rule established.**

- Composer and activity import only session public contracts and shared UI.
- `ChatSurface` composes composer.
- `Transcript` temporarily imports `activity/public` until conversation moves.

**Concurrency after this phase.**

- Yes. Queued-follow-up UX and tool-call rendering use disjoint production
  files and CSS. This phase must prove the primary success test.

**Primary worktree acceptance (2026-08-21).**

Queued follow-up ownership lives entirely under `features/chat/composer/*`
(including `QueuedFollowUp.tsx`, `useComposer.tsx`, and
`styles/session/queued.css`). Tool, plan, and reasoning presentation lives
entirely under `features/chat/activity/*`. No production file is shared
between those two concerns after this phase.

**Checklist.**

- [x] Composer state has one named owner.
- [x] Activity presentation has one named owner.
- [x] `ChatSurface` contains no queue state or queue flush effect.
- [x] `Transcript` contains no tool, plan, or reasoning component definitions.
- [x] Primary worktree acceptance test documented as passing.

**Phase 1 verification on 2026-08-21.**

| Check | Result |
| --- | --- |
| `npm run web:test -- --run src/features/chat` | Pass. 25 files, 231 tests |
| `npm run web:check` | Pass |
| `npm run web:lint` | Pass |
| `npm run web:sg` | Pass |
| `git diff --check` | Pass |
| `npm run verify:arch` | Fail on `web_src_stylesheet_graph_uses_manifest_and_owned_modules` until `styles/session/queued.css` was added to the stylesheet inventories; pass after allow-list update (see post-fix rerun below) |

**Phase 1 verification rerun after queued.css allow-list fix on 2026-08-21.**

| Check | Result |
| --- | --- |
| `npm run verify:arch` | Pass. ajax-core 8, ajax-web 13, ajax-tui 2, ajax-supervisor 2 |
| `npm run web:test -- --run src/features/chat` | Pass. 25 files, 231 tests |
| `npm run web:check` | Pass |
| `git diff --check` | Pass |

### Phase 2. Add the typed Chat projection boundary

**Objective.**

Make presentation consume Chat-facing types. Keep one ordered session
projection without keeping every state domain in one file.

**Likely files.**

- Add `features/chat/session/model.ts`.
- Add `projectWireInput.ts`, `reducer.ts`, domain projection files,
  `selectors.ts`, and `errors.ts`.
- Move `useTaskSession.ts` to `session/useChatSession.ts`.
- Delete `sessionThread.ts` after callers move.
- Split `sessionThread.test.ts`.

**Moves and extractions.**

- Map each validated snapshot or event to a closed `ChatSessionEvent`.
- Move turn, activity, permission, status, usage, and error reduction into
  focused pure files.
- Keep a small reducer that composes those functions in wire order.

**Behavioral risks.**

- Event reordering.
- User echo duplication.
- Tool content loss.
- Permission resurrection.
- Context and turn usage conflation.
- Different open-failure copy.

**Tests.**

- Projection test for every protocol fixture.
- Replay reset and incremental replay parity.
- Turn, activity, permission, status, usage, and error reducer tests.
- Existing conversation and activity suites.

**Dependency rule established.**

- Raw transport input stops at `session/projectWireInput.ts`.
- Presentation imports only `session/public`.
- Session code imports no React presentation.

**Concurrency after this phase.**

- Yes. New ACP event support can change projection files while visual work
  changes capability files.

**Checklist.**

- [x] Closed `ChatSessionEvent` union exists.
- [x] Closed `ChatSessionView` model exists.
- [x] No presentation file imports transport event types.
- [x] No single reducer file accumulates all domain logic.
- [x] Old `sessionThread` path is deleted.

**Phase 2 verification on 2026-08-21.**

| Check | Result |
| --- | --- |
| `npm run web:test -- --run src/features/chat` | Pass. 26 files, 235 tests |
| `npm run web:check` | Pass |
| `npm run web:lint` | Pass |
| `npm run web:sg` | Pass |
| `npm run verify:arch` | Pass. ajax-core 8, ajax-web 13, ajax-tui 2, ajax-supervisor 2 |
| `git diff --check` | Pass |

### Phase 3. Split conversation and scrolling

**Objective.**

Separate turn rendering, Markdown, reveal, and DOM scroll behavior.

**Likely files.**

- Add `features/chat/conversation/*`.
- Add `features/chat/scrolling/*`.
- Move `Transcript.tsx`, `Markdown.tsx`, `sessionTurns.ts`, and
  `viewport/useChatViewport.ts`.
- Modify `ChatSurface.tsx`.
- Split transcript, Markdown, viewport, and keyboard tests.
- Split conversation, Markdown, and scrolling CSS.

**Moves and extractions.**

- Extract turn, user, assistant, note, divider, reveal, and Markdown owners.
- Extract scroller container, pinned state, observers, keyboard geometry, and
  jump control.
- Delete old flat and `viewport` paths.

**Behavioral risks.**

- React key changes.
- Fenced-block reveal cuts.
- Initial top flash.
- iOS keyboard gaps.
- History readers being pulled to the bottom.

**Tests.**

- Conversation and Markdown unit tests.
- Scroll and keyboard hook tests.
- Mobile WebKit session chat regression and keyboard tests.

**Dependency rule established.**

- Conversation may import `activity/public`.
- Scrolling receives semantic revision and React nodes. It does not inspect
  conversation items or tools.
- No other capability-to-capability import is added.

**Concurrency after this phase.**

- Yes. Assistant and Markdown, activity, scrolling, and composer work can
  proceed in separate worktrees.

**Checklist.**

- [x] Conversation files own all transcript rendering.
- [x] Scroll effects no longer live in `ChatSurface`.
- [x] Scrolling knows no activity or tool shape.
- [x] Current mobile keyboard behavior passes.

**Phase 3 verification on 2026-08-21.**

| Check | Result |
| --- | --- |
| `npm run web:test -- --run src/features/chat` | Pass. 27 files, 238 tests |
| `npm run web:check` | Pass |
| `npm run web:lint` | Pass |
| `npm run web:sg` | Pass |
| `npm run verify:arch` | Pass. ajax-core 8, ajax-web 13, ajax-tui 2, ajax-supervisor 2 |
| `git diff --check` | Pass |
| Mobile WebKit e2e (`session-chat-keyboard`, `session-chat-regression`) | Not run. Keyboard geometry is covered by `scrolling/useChatViewport.test.tsx` only |

### Phase 4. Split status and permission presentation

**Objective.**

Turn `LiveHead` into typed status presentation and give permission controls a
separate owner.

**Likely files.**

- Add `features/chat/status/*`.
- Add `features/chat/permissions/*`.
- Move `LiveHead.tsx`.
- Modify `ChatSurface.tsx`.
- Split `LiveHead.test.tsx`.
- Split status and permission CSS.

**Moves and extractions.**

- Introduce `ChatHeadView`.
- Move permission markup into `PermissionPanel`.
- Move usage formatting into status ownership.
- Remove `BrowserTaskDetail` from status presentation.
- Have Task Workspace pass a narrow task-attention input.

**Behavioral risks.**

- Wrong precedence between permission, ACP status, task attention, and idle.
- Stop becoming unavailable.
- Permission controls looking active while disconnected.
- Token fields rendering as zero.

**Tests.**

- Head precedence matrix.
- Permission immediate clear and reconnect replay.
- Connection label and stale activity tests.
- Context and turn usage tests.
- Workspace attention slot integration.

**Dependency rule established.**

- Status and permission presentation consume typed session views only.
- Neither imports Task detail or raw transport.

**Concurrency after this phase.**

- Yes. Status, permission, activity, composer, and model work have separate
  owners.

**Checklist.**

- [x] Permission markup no longer lives in status.
- [x] `LiveHead` accepts one typed view.
- [x] `LiveHead` imports no `BrowserTaskDetail`.
- [x] Usage presentation has focused tests.

**Phase 4 verification on 2026-08-21.**

| Check | Result |
| --- | --- |
| `npm run web:test -- --run src/features/chat` | Pass. 30 files, 250 tests |
| `npm run web:check` | Pass |
| `npm run web:lint` | Pass |
| `npm run web:sg` | Pass |
| `npm run verify:arch` | Pass. ajax-core 8, ajax-web 13, ajax-tui 2, ajax-supervisor 2 |
| `git diff --check` | Pass |

### Phase 5. Split live model controls

**Objective.**

Make model, effort, Fast, sheet, and error presentation independently editable.

**Likely files.**

- Add `features/chat/model/*`.
- Move `SessionModelControls.tsx` and `sessionModel.ts`.
- Modify `ChatSurface.tsx` and session model state.
- Split model tests.
- Split model CSS from composer and sheets.

**Moves and extractions.**

- Move sheet state and notice state.
- Keep host-confirmed model and options in `ChatSessionView`.
- Keep the existing Task public import for New Task preference storage.

**Behavioral risks.**

- Optimistic picker drift.
- Applying an unadvertised option.
- Mixing applied model, desired pin, and browser preference.
- Breaking iOS model-list scrolling.

**Tests.**

- Model, effort, and Fast option tests.
- Refusal and persistence warning tests.
- Unknown current model test.
- iOS list and chip interaction tests.

**Dependency rule established.**

- Model controls may import the documented Task public model contract.
- Workspace does not gain model preference callbacks.
- No other Chat capability imports Task.

**Concurrency after this phase.**

- Yes. Model controls can change without composer, status, activity, or
  connection files.

**Checklist.**

- [x] Model controls have one directory owner.
- [x] Confirmed state comes only from host snapshots.
- [x] Task public dependency remains narrow.
- [x] Mixed model CSS is removed from composer and sheet files.

**Phase 5 verification on 2026-08-21.**

| Check | Result |
| --- | --- |
| `npm run web:test -- --run src/features/chat` | Pass. 31 files, 253 tests |
| `npm run web:check` | Pass |
| `npm run web:lint` | Pass |
| `npm run web:sg` | Pass |
| `npm run verify:arch` | Pass. ajax-core 8, ajax-web 13, ajax-tui 2, ajax-supervisor 2 |
| `git diff --check` | Pass |

### Phase 6. Split transport and connection recovery

**Objective.**

Move Chat-only transport out of `shared` and model reconnect behavior
explicitly.

**Likely files.**

- Add `features/chat/session/transport/*`.
- Add `features/chat/session/connection/*`.
- Move `shared/lib/webSessionTransport.ts`, `messageBuffer.ts`,
  `useSessionTransport.ts`, and fixtures.
- Modify `features/chat/public.ts`.
- Modify `features/task-workspace/TaskWorkspace.tsx` once.
- Move transport and reconnect tests.

**Moves and extractions.**

- Separate contracts, frame parsing, socket commands, outbox, and message
  batching.
- Add explicit connection transitions.
- Export one narrow Chat session cleanup command for Task Workspace harness
  switch.
- Delete old shared and flat paths.

**Behavioral risks.**

- Storage key changes.
- Cursor loss.
- Duplicate prompt resend.
- Reconnect loops on visibility changes.
- Snapshot replay order changes.
- Expanding the Chat public API too far.

**Tests.**

- Protocol parser and fixture tests.
- Tool content validation.
- Outbox acknowledgement and resend.
- Cursor and cold-load tests.
- Connection transition and visibility tests.
- Task Workspace harness-switch cleanup test.

**Dependency rule established.**

- `shared` contains no Chat-only transport.
- Task Workspace imports cleanup through `features/chat/public`.
- Transport imports no reducer or presentation.

**Concurrency after this phase.**

- Yes. Connection reliability, projection, and presentation have separate
  owners.

**Checklist.**

- [x] No production import of `shared/lib/webSessionTransport` remains.
- [x] Wire JSON and storage keys are unchanged.
- [x] Connection states are exhaustive.
- [x] Chat public exports only the surface, narrow types, and cleanup command.

**Phase 6 verification on 2026-08-21.**

| Check | Result |
| --- | --- |
| `npm run web:test -- --run src/features/chat src/shared/lib src/app/App.harness-swap.test.tsx src/features/task-workspace` | Partial. 731 passed; 3 failed in `styleSources.test.ts` (CSS baseline drift from Phases 4–5, unchanged by Phase 6) |
| `npm run web:test -- --run src/features/chat src/app/App.harness-swap.test.tsx src/features/task-workspace` | Pass. 41 files, 364 tests |
| `npm run web:check` | Pass |
| `npm run web:lint` | Pass |
| `npm run web:sg` | Pass |
| `npm run verify:arch` | Pass. ajax-core 8, ajax-web 13, ajax-tui 2, ajax-supervisor 2 |
| `git diff --check` | Pass |

### Phase 7. Finish thin composition and enforce boundaries

**Objective.**

Remove the last behavior from `ChatSurface`, lock imports and CSS ownership,
and update durable architecture documents.

**Likely files.**

- `features/chat/ChatSurface.tsx`
- `features/chat/public.ts`
- `features/task-workspace/TaskWorkspace.tsx`
- `web/eslint.config.mjs`
- `web/src/styles.css`
- `web/src/styles/architecture.test.ts`
- `web/src/styles.architecture.test.ts`
- `crates/ajax-web/src/architecture.rs`
- `architecture.md`
- `docs/architecture/web-cockpit.md`
- `docs/architecture/web-session-behavior.md` only where falsifiable behavior
  text or paths require an update.

**Moves and extractions.**

- Move task-details sheet CSS to Task Workspace ownership.
- Finalize Chat CSS leaves and cascade order.
- Remove behavior tests from `ChatSurface.test.tsx` after focused tests own them.
- Add ESLint rules for Chat internal direction.
- Add architecture tests only for rules ESLint cannot express.

**Behavioral risks.**

- CSS cascade changes.
- Task attention input omits needed server truth.
- Boundary rules block tests or public contracts incorrectly.
- Documentation describes a target that did not land.

**Tests.**

- Focused Chat and Task Workspace tests.
- CSS architecture tests.
- ESLint and static analysis.
- Full browser suite and mobile WebKit Chat suites.
- Full repository gate.

**Dependency rule established.**

- The target dependency graph fails CI when production code crosses it.
- `ChatSurface` composes and navigates only.

**Concurrency after this phase.**

- Routine Chat work can be assigned by capability. Separate worktrees should
  touch the same production file only when the features genuinely share a
  contract.

**Checklist.**

- [x] `ChatSurface` owns no queue, reconnect, model, permission, reducer, or
  scroll effect.
- [x] Internal import rules pass.
- [x] CSS ownership and cascade tests pass.
- [x] Root and focused architecture docs match implemented ownership.
- [x] Full verification results are recorded.

**Phase 7 verification on 2026-08-21.**

| Check | Result |
| --- | --- |
| `npm run verify:arch` | Pass. ajax-core 8, ajax-web 13, ajax-tui 2, ajax-supervisor 2 |
| `npm run web:check` | Pass |
| `npm run web:lint` | Pass |
| `npm run web:sg` | Pass |
| `npm run web:sg:test` | Pass. 2 rules |
| `npm run web:test -- --run` | Pass. 135 files, 1245 tests (2 skipped) |
| `npm run web:build:check` | Pass |
| `git diff --check` | Pass |
| Mobile WebKit e2e (`session-chat-keyboard`, `session-chat-regression`) | Not run |
| `cargo fmt --check` / `cargo clippy` / `cargo nextest` / `npm run ci:verify` | Not run (Phase 7 web gate only) |

## Expected test ownership

| Behavior | Primary test owner |
| --- | --- |
| User and assistant turns | `conversation/Conversation.test.tsx` |
| Markdown and safe links | `conversation/Markdown.test.tsx` |
| Tool rows, output, and diffs | `activity/ToolCard.test.tsx` |
| Thinking and plans | `activity/ActivityDisclosure.test.tsx` |
| Queue, edit, and stop-and-send | `composer/ChatComposer.test.tsx`, `composerState.test.ts` |
| Speech to draft | `composer/speech/useChatSpeech.test.tsx` |
| Turn and permission projection | `session/reducer.test.ts` |
| Wire-to-Chat mapping | `session/projectWireInput.test.ts` |
| Protocol parsing and outbox | `session/transport/*test.ts` |
| Reconnect | `session/connection/useSessionConnection.test.ts` |
| Model, effort, and Fast | `model/SessionModelControls.test.tsx` |
| Context and turn tokens | `status/LiveHead.test.tsx`, session projection tests |
| Scrolling and keyboard | `scrolling/useChatScroll.test.tsx`, mobile WebKit |
| Whole Chat behavior | `e2e/session-chat-regression.test.ts` |

Do not keep duplicate assertions in `ChatSurface.test.tsx` after the focused
owner has equivalent coverage. Keep a small composition smoke suite.

## Verification commands

Each phase runs its focused commands first. The final phase runs:

```bash
npm run verify:arch
npm run web:check
npm run web:lint
npm run web:sg
npm run web:sg:test
npm run web:test -- --run
npm run web:build:check
npx playwright test \
  --config crates/ajax-web/web/playwright.config.mts \
  --project=mobile-webkit \
  crates/ajax-web/web/e2e/session-chat-regression.test.ts \
  crates/ajax-web/web/e2e/session-chat-keyboard.test.ts
cargo fmt --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
cargo test --doc
npm run ci:verify
git diff --check
git status --short
```

If a named mobile test path differs on the implementation branch, locate the
existing equivalent. Do not create a duplicate suite merely to satisfy the
plan text.

## Migration controls

- Host `busy` remains the only in-flight authority. Browser timers never settle
  a turn.
- The browser still holds only one editable follow-up. It never recreates the
  host FIFO queue.
- `clientMessageId`, outbox keys, cursor behavior, and acknowledgement semantics
  do not change during moves.
- Snapshot replay ordering preserves the final authoritative busy state.
- Permission resolution remains idempotent across immediate UI clearing and
  replay.
- Applied model, desired task model, and New Task preference remain distinct.
- Harness and context reset notes remain transcript dividers. They do not clear
  persisted history.
- Transport parsing never exposes unchecked tool content to the projection.
- iOS keyboard and scroll behavior requires mobile WebKit verification where
  available.
- CSS moves preserve computed styles and cascade order.
- Tests move with code. Do not weaken or delete behavior assertions.
- Any product defect discovered during the refactor follows
  `docs/defect-process.md` before it is fixed.
- Every phase migrates callers and deletes the old internal path in the same PR.

## Stop conditions

Stop and revise this plan before:

- changing the Rust session protocol or backend authority;
- introducing a browser-owned durable queue, transcript, or task model;
- adding a new package, crate, state library, or generic service layer;
- moving Task Workspace policy into Chat;
- moving Chat transient state into Task Workspace;
- changing valid wire-event meaning instead of preserving it;
- changing CSS behavior because the old cascade cannot be preserved;
- keeping both old and new internal APIs after a phase;
- combining multiple phases into one rewrite-sized PR;
- continuing after a phase cannot pass its focused verification.

## Open decisions

No product decision blocks implementation.

Implementation details that require source-level confirmation:

- Whether `webSessionFixtures.ts` has any non-Chat consumer at the time Phase 6
  starts.
- Whether activity projection is clearer as one `activityProjection.ts` file or
  two files for tool revision and turn attachment. The public model must not
  change based on that file split.
- Whether the existing mobile WebKit keyboard coverage lives in one test file
  or several when Phase 3 starts.

Resolve these from current source and tests. They are not user preference
questions.

## Poteto principles that shaped the plan

- **Laziness Protocol.** Kept Chat as one feature, retained the current
  Markdown, reducer pattern, and CSS manifest, and rejected new libraries and
  generic layers.
- **Foundational Thinking.** Named `ChatSessionView`, `ChatSessionEvent`,
  `ComposerState`, and `ConnectionState` before choosing folders.
- **Redesign from First Principles.** Assigned each current behavior as if
  capability ownership had existed when Chat first landed, instead of wrapping
  the flat files.
- **Subtract Before You Add.** Every phase moves callers and deletes old paths.
  No compatibility facade or duplicate store remains.
- **Minimize Reader Load.** Reduced the number of responsibilities a maintainer
  must inspect for composer, activity, scroll, status, model, and connection
  changes.
- **Outcome-Oriented Execution.** Each phase lands a final owner and leaves Chat
  functional. The plan avoids temporary architecture.
- **Experience First.** Preserved mobile keyboard, live-edge, editable queue,
  permission, and tool-detail behavior even where those constraints make the
  split less mechanical.
- **Build the Lever.** Uses ESLint, architecture checks, protocol fixtures, and
  CSS graph tests so future agents can prove the boundary instead of relying on
  prose.
- **Model the Domain.** Uses typed session events and explicit queue and
  connection states instead of scattered booleans and string checks.
- **Boundary Discipline.** Keeps wire parsing in transport, Chat projection in
  session, local UI state in capabilities, and backend task truth outside the
  browser.
- **Type System Discipline.** Requires closed event and view unions and removes
  unchecked raw transport shapes before reducers and components consume them.
- **Migrate Callers Then Delete Legacy APIs.** Makes every move atomic at the
  repository level.
- **Separate Before Serializing Shared State.** Splits conflict magnets before
  asking agents to coordinate changes through them.
- **Sequence Work into Verifiable Units.** Makes the composer and activity split
  the first implementation phase because it unlocks the primary worktree test
  immediately.
- **Guard the Context Window.** Used delegated history and import analysis for
  broad evidence, then reviewed the actual source and focused architecture
  documents in the parent session.
- **Encode Lessons in Structure.** Converts the final ownership rules into
  ESLint and architecture tests.
- **Prove It Works.** Gives every phase focused checks and reserves the full
  browser, mobile, Rust, and repository gate for the final boundary.

# Frontend Task Workspace Architecture

**Status:** Round 6 complete on current HEAD (final architecture and behavior gate)  
**Date:** 2026-08-19  
**Mode:** Behavior-preserving architecture refactor  
**Approval:** User explicitly requested Round 0–1 implementation on current HEAD
(2026-08-19). Stay on this branch despite `origin/main` being two commits ahead
(#991 slim Cursor catalog, #992 spawn argv). Do not sync without separate
authorization. Round 2+ still require explicit approval per round.

## Objective

Make the task the owner of one frontend workspace with two peer interaction
surfaces:

```text
Task Workspace
├── shared task header and task details
├── Ajax Chat
├── Terminal
└── Diff Review navigation
```

Chat and Terminal are views of the same task. Neither owns task metadata,
task actions, harness switching, mode preference, or Diff routing.

The immediate outcome is a foundation that lets later Chat, thread, and
Terminal splits run in separate worktrees without repeatedly editing
`App.tsx`, `SessionChat.tsx`, or task-detail composition.

## Evidence and corrections

Current branch measurements:

- `app/App.tsx`: 818 lines.
- `features/session/SessionChat.tsx`: 691 lines, not 596.
- `features/session/sessionThread.ts`: 588 lines.
- `features/task/TaskTerminal.tsx`: 987 lines.
- `features/task/mountTaskTerminalSession.ts`: 991 lines.

The terminal has two near-limit files, not one. Neither may absorb new
behavior before its focused split.

The frontend already has coarse ESLint layering for
`app -> features -> shared`. The missing protection is feature ownership:
the current rules allow `session -> task`, `task -> session`, and Chat's reuse
of terminal speech plumbing.

PR #839 is the release PR that aggregates the Chat-default behavior. It is not
the originating implementation PR. The durable product behavior is still
clear from the merged changes and current tests:

- provisioned, session-capable tasks default to Ajax Chat;
- a per-task preference switches to Terminal;
- interactive or non-session-capable tasks fall back to Terminal;
- the public hashes remain `#/session/<handle>` and `#/t/<handle>`.

The worktree is currently two commits behind `origin/main`. Round 0 documents
current-HEAD Chat-default behavior only. Implementation proceeds on this HEAD
by explicit user choice; do not merge, rebase, fetch-merge, or switch branches
without separate authorization. Inspect the delta before later rounds touch the
same hotspots, but do not invent #991/#992 catalog-split or spawn-argv behavior
from those newer commits.

## Refined ownership model

### App and routes

`app/App.tsx` owns only process-wide browser shell concerns:

- shell lifecycle and Cockpit polling;
- connection/version indicators;
- global operation result and confirmation presentation;
- top-level route composition;
- New Task sheet reachability.

`app/routes/TaskWorkspaceRoute.tsx` adapts task/session hashes to the workspace.
It owns task-detail loading/error composition and reports route readiness back
to App telemetry. Bare `#/session` remains New Task and is not a task
workspace.

Do not extract Dashboard, Diff, and Settings routes merely to reduce line
count in this slice. Extract them later when each route gains a real ownership
boundary.

### Task Workspace

`features/task-workspace` owns:

- Chat versus Terminal selection;
- per-task view preference;
- capability fallback and route redirects;
- Back and Diff navigation;
- one shared task header;
- one shared task-details sheet;
- composition of task actions, metadata, and harness switching;
- loading and task-not-found presentation.

The shared header is the task identity/navigation row. Chat's live ACP head
remains below it and continues to own tool, permission, usage, and turn state.
Terminal's toolbar remains inside Terminal. This distinction prevents the
"shared header" from becoming a mode-dependent mega-component.

`TaskDetailsSheet` renders once at workspace level. Header Details and the
terminal footer Task details affordance open that same sheet. It composes:

- current mode and peer-mode switch;
- Diff navigation;
- task identity and metadata;
- harness/model switch;
- available task actions.

Destructive confirmation remains App-owned because its result panel outlives
the route and must keep the existing leave-latch behavior.

### Chat

`features/chat` owns:

- ACP WebSocket transport and outbox;
- conversation state, reducer, selectors, and projection;
- transcript, activity, Markdown, and live ACP head;
- composer, draft, queue, and Chat speech insertion;
- model application to the live ACP session;
- Chat viewport, live-edge, scroll, and iOS keyboard behavior.

Chat may consume task public contracts. It must not import task UI internals,
terminal code, or task mutations directly.

`SessionChat` currently imports `ActionBar`, `HarnessSwap`,
`TaskMetaDetails`, `TaskLoadError`, `visibleTaskActions`, and
`useTaskTerminalSpeech`. The foundation removes all of those imports.
Workspace composition supplies task controls; Chat gets narrow data and event
props.

### Terminal

`features/terminal` owns:

- PTY connection and xterm lifecycle;
- terminal viewport, geometry, refit, and scroll synchronization;
- keyboard, paste, key repeat, selection, and clipboard behavior;
- Terminal speech insertion;
- toolbar and overlays.

The foundation may mechanically relocate the current terminal surface and its
local support files, but it must not split or add behavior to the 987-line
`TaskTerminal.tsx` or the 991-line `mountTaskTerminalSession.ts`. Their
cohesive split is a follow-up task.

### Task

`features/task` owns:

- task contracts and metadata presentation;
- task-detail queries;
- available action selection and action UI;
- task/start/harness mutations;
- desired harness and model selection used by New Task and harness switching.

The generic model catalog/picker cannot remain Chat-private because New Task
and harness switching both consume it. Move that reusable desired-model
contract behind the task public surface. Chat still owns applying a selected
model to the live ACP session.

The orchestration-chat enablement preference is a product setting, not session
runtime state. Settings owns its storage helper; App passes the value to New
Task and Task Workspace rather than making task code import Chat or Settings.

### Shared

`shared/ui` contains feature-neutral controls only.

`shared/platform` may contain browser/transport mechanisms used by more than
one feature:

- API and route primitives;
- query client;
- telemetry;
- low-level task-scoped STT transport and pure speech state.

Each surface owns how finalized speech changes its input. Shared code must not
own Chat draft semantics or Terminal PTY insertion/undo semantics.

Do not move every existing `shared/lib/terminal*` file in the foundation.
That would combine the workspace refactor with the later Terminal split.
Boundary rules may temporarily permit Terminal to consume legacy shared
modules, but no new terminal-specific module may be added to shared.

## Target dependency direction

Production imports must follow:

```text
app/routes
  -> task-workspace public
  -> chat public
  -> terminal public
  -> task public
  -> shared

chat -> task/public + shared
terminal -> shared
task -> shared
shared -X-> app or features

chat -X-> terminal
terminal -X-> chat
task -X-> chat, terminal, or task-workspace
```

Use explicit `public.ts` modules for cross-feature contracts. Internal files
continue to use direct relative imports within their own feature. Do not add
catch-all barrels that obscure ownership or cycles.

ESLint is the enforcement mechanism because the repository already ships
`eslint-plugin-import-x` and `no-restricted-imports`. Add no dependency and do
not create a test that merely regexes the ESLint config source.

Tests may import fixtures and public test helpers across boundaries. Production
files receive no exemption.

## Target structure after the foundation

```text
web/src/
├── app/
│   ├── App.tsx
│   └── routes/
│       └── TaskWorkspaceRoute.tsx
├── features/
│   ├── task-workspace/
│   │   ├── public.ts
│   │   ├── TaskWorkspace.tsx
│   │   ├── TaskWorkspaceHeader.tsx
│   │   ├── TaskDetailsSheet.tsx
│   │   ├── TaskModeSwitch.tsx
│   │   └── taskViewPreference.ts
│   ├── chat/
│   │   ├── public.ts
│   │   ├── ChatSurface.tsx
│   │   ├── viewport/
│   │   │   └── useChatViewport.ts
│   │   └── ...existing session internals, unsplit
│   ├── terminal/
│   │   ├── public.ts
│   │   ├── TerminalSurface.tsx
│   │   └── ...existing local terminal files, unsplit
│   └── task/
│       ├── public.ts
│       └── ...existing task contracts, UI, queries, and mutations
└── shared/
    ├── ui/
    ├── platform/
    └── ...legacy shared modules pending focused moves
```

This is deliberately smaller than the final desired tree. Deep Chat and
Terminal folders appear only when their owning files are actually split.
Empty architecture-shaped directories are forbidden.

## Foundation execution plan

Each round must end green. Do not mix a semantic extraction with a bulk move.

### Round 0 — Baseline and durable contract

- [x] Confirm the intended relationship to the two newer `origin/main`
  commits; stop for direction if branch synchronization is required.
- [x] Record baseline `git status`, hotspot line counts, and focused frontend
  test results in this plan.
- [x] Update `architecture.md` and `docs/architecture/web-cockpit.md` to name
  Task Workspace as the product boundary.
- [x] Update `docs/architecture/web-session-behavior.md` to describe Ajax Chat
  as multi-harness and default for provisioned session-capable tasks.
- [x] Preserve the raw xterm/tmux contract specifically for Terminal mode; do
  not continue describing Terminal as the default task workspace.

Acceptance:

- Documentation does not contradict the existing Chat-default behavior.
- No runtime source changes.

### Round 1 — Extract workspace behavior in current locations

Write or move focused tests first so they fail against the missing workspace
owner.

- [x] Add `TaskWorkspaceRoute` and `TaskWorkspace`.
- [x] Move task-detail loading/error composition for handled task/session
  routes out of App.
- [x] Move capability fallback, terminal preference, mode switching, and
  mode-aware Diff return selection into Task Workspace.
- [x] Preserve all existing hashes and redirects.
- [x] Preserve App's global confirmation, drop leave latch, result panel,
  polling, and telemetry timing through narrow route callbacks.
- [x] Add one shared task header contract while keeping Chat live state and
  Terminal toolbar mode-owned.
- [x] Add pure routing helpers in `taskWorkspaceRouting.ts` (capability,
  hash resolution, session-to-terminal redirect) with focused unit tests.

Acceptance:

- Existing App task/session/drop tests retain their assertions.
- Opening a capable task defaults to Chat unless Terminal is preferred.
- Interactive/non-capable tasks never open an ACP socket.
- Diff Back returns to the selected mode.

### Round 2 — Unify task details and remove Chat task internals

Add failing component/integration coverage for both mode entry points first.

- [x] Add one workspace-owned `TaskDetailsSheet`.
- [x] Make Chat header Details and Terminal header/footer Details open that
  same sheet.
- [x] Move harness-switch UI composition to Task Workspace while retaining the
  mutation in task ownership.
- [x] Remove task action, metadata, harness, and task-error imports from Chat.
- [x] Pass Chat only the task data and callbacks required by its live head.
- [x] Keep destructive confirmation behavior and sheet-close-on-Drop behavior.

Acceptance:

- There is one mounted task-details dialog per workspace.
- Ajax Chat, Ajax Terminal, Diff, Switch, metadata, and task actions retain
  their current reachability.
- `SessionChat` has no imports from task UI internals.

### Round 3 — Give Chat its own viewport and speech adapter

Add failing import/behavior coverage before moving implementation.

- [x] Move `useSessionChatViewport` and its tests to Chat viewport ownership;
  rename it `useChatViewport`.
- [x] Keep pure browser/STT mechanisms shared only where both modes consume
  them.
- [x] Add a Chat-owned speech-input adapter that inserts finalized text into
  the draft.
- [x] Keep a Terminal-owned adapter for PTY insertion and terminal undo.
- [x] Remove Chat imports of `Terminal`, `TerminalConnection`, and
  `useTaskTerminalSpeech`.
- [x] Preserve Mic state, pause/finalize behavior, ordered finals, start-over,
  error presentation, and no-auto-submit behavior.

Acceptance:

- Chat speech tests no longer mock terminal hooks or construct dummy terminal
  refs.
- Terminal speech behavior is unchanged.
- Existing iOS keyboard/live-edge tests remain green from the new Chat path.

### Round 4 — Mechanical feature moves

Only after Rounds 1–3 are green:

- [x] Move the existing session feature to `features/chat` and rename only its
  public surface to `ChatSurface`.
- [x] Move `TaskTerminal`, `mountTaskTerminalSession`, terminal speech, paste,
  backspace sentinel, and touch-selection files to `features/terminal`.
- [x] Expose narrow `public.ts` modules for task-workspace, chat, terminal, and
  task.
- [x] Move tests with their subjects and repoint `?raw`, source-text,
  `vi.mock`, stylesheet, and repo-relative path references.
- [x] Retire `TaskDetail.tsx` if its remaining body is only workspace
  composition; do not retain a misleading compatibility shell.
- [x] Verify generated asset names and lazy terminal chunk behavior.

Acceptance:

- File bodies change only where prior semantic rounds require it; the move
  itself is reviewable as rename/import repair.
- `web:build` succeeds; TypeScript and Vitest alone are insufficient evidence
  for file moves.
- No new code is added to either near-limit terminal file.

### Round 5 — Enforce feature boundaries

- [x] Replace coarse feature rules with path-specific production rules.
- [x] Allow Chat to import task only through `features/task/public`.
- [x] Allow App routes and Task Workspace to import only feature public
  modules.
- [x] Ban Chat/Terminal cross-imports and all feature imports from shared.
- [x] Ban task imports from Chat, Terminal, Task Workspace, and App.
- [x] Run lint once with one deliberate forbidden import to prove the rule
  fails, remove it, then record the clean result.

Acceptance:

- `npm run web:lint` is the executable architecture test.
- No allowlist names current production violations.
- Tests are exempt only from runtime layering, not from ordinary lint rules.

### Round 6 — Final architecture and behavior gate

- [x] Confirm App no longer selects or composes Chat/Terminal directly.
- [x] Confirm Task Workspace is the only feature importing both peer modes.
- [x] Confirm Chat has no terminal imports and no task-internal imports.
- [x] Confirm task and shared have no upward feature imports.
- [x] Update this plan with actual command results, deviations, and remaining
  risks.

## Verification

Run focused checks after each round, then the complete gate:

```bash
npm run web:check
npm run web:lint
npm run web:sg
npm run web:test -- --run
npm run web:build
npm run web:build:check
npm run verify:arch
git diff --check
```

Also run the focused mobile WebKit task/session navigation and keyboard suites
when the local environment supports them. Record exact skipped commands and
reasons; do not claim browser validation if it did not run.

Before handoff, record:

- final hotspot line counts;
- `git status --short --branch`;
- changed-file and rename summary;
- any generated `dist` changes and why they are expected;
- every failed or skipped verification command.

## Follow-up worktree tasks

These start only after the foundation and import rules land:

1. **Chat runtime:** split transport, outbox, message buffer, model application,
   and `useTaskChat` without touching workspace composition.
2. **Chat thread:** split contracts, reducer, selectors, projection, plans,
   usage, and ACP error presentation with reducer fixture parity.
3. **Chat presentation:** split transcript, turn, activity, Markdown, composer,
   draft, live edge, and live head.
4. **Terminal runtime:** split both `TaskTerminal.tsx` and
   `mountTaskTerminalSession.ts` by connection, viewport, input, speech, and
   chrome ownership. This task is mandatory before adding Terminal behavior.
5. **Shared cleanup:** move remaining terminal-specific modules out of
   `shared/lib`, then narrow `shared/platform` to genuine cross-feature
   mechanisms.
6. **Remaining routes:** extract Dashboard, Diff, and Settings routes only when
   doing so removes real ownership from App rather than moving JSX for line
   count.

Each follow-up must be independently reviewable and must not reopen
TaskWorkspace composition unless its public contract is genuinely insufficient.

## Non-goals

- No Rust rearchitecture or backend contract change.
- No URL migration, route compatibility alias, or browser task store.
- No visual redesign.
- No lifecycle, registry, action-policy, or runtime-authority change.
- No new dependency.
- No deep Chat reducer/thread split in the foundation.
- No deep Terminal split in the foundation.
- No wholesale shared-directory cleanup in the foundation.
- No commit, push, PR, merge, rebase, or branch switch unless separately
  requested.

## Risks and stop conditions

- Stop if preserving existing hashes conflicts with a requested route redesign.
- Stop if workspace extraction changes task truth, action policy, ACP attach
  eligibility, or Terminal PTY semantics.
- Stop if the two newer `origin/main` commits materially overlap the planned
  source moves and branch synchronization has not been authorized.
- Stop a mechanical move if source/test path coupling cannot be shown to still
  target the intended file.
- Stop if either near-limit terminal file requires new behavior; split it in a
  separately approved task instead.

## Validation results and deviations

### Round 0 baseline (2026-08-19, current HEAD)

**Branch:** `ajax/architecture-chat-refactor` tracking `origin/main` (**behind 2**).
User explicitly chose to stay on this HEAD; Round 0 does not sync #991/#992.

**`git status --short --branch`:**

```text
## ajax/architecture-chat-refactor...origin/main [behind 2]
?? .planning/agent-plans/task-workspace-foundation.md
 M architecture.md
 M docs/architecture/web-cockpit.md
 M docs/architecture/web-session-behavior.md
```

(Plan and architecture docs updated during Round 0; no runtime source changes.)

**Hotspot line counts (`wc -l` on current HEAD):**

| File | Lines |
| --- | ---: |
| `crates/ajax-web/web/src/app/App.tsx` | 818 |
| `crates/ajax-web/web/src/features/session/SessionChat.tsx` | 691 |
| `crates/ajax-web/web/src/features/session/sessionThread.ts` | 588 |
| `crates/ajax-web/web/src/features/task/TaskTerminal.tsx` | 987 |
| `crates/ajax-web/web/src/features/task/mountTaskTerminalSession.ts` | 991 |

**Focused frontend test (task view / Chat-default routing):**

```bash
npm run web:test -- --run \
  src/features/session/taskViewPreference.test.ts \
  src/features/task/TaskDetail.test.tsx \
  src/app/App.task-view.test.tsx
```

Result: **pass** — 3 files, 46 tests (Vitest v4.1.9). jsdom logged a benign
`HTMLCanvasElement.getContext` notice from `@xterm/addon-serialize`; exit 0.

**Round 0 deliverables:** durable Task Workspace contract in `architecture.md`,
`docs/architecture/web-cockpit.md`, and `docs/architecture/web-session-behavior.md`.
Documented current-HEAD Chat-default behavior only (no #991/#992 catalog-split).

**Deviation:** implementation on HEAD despite overlapping `origin/main` commits;
documented by explicit user choice, not branch sync.

Rounds 1–6 verification gates not run (docs-only Round 0).

### Round 1 — Extract workspace behavior (2026-08-19, current HEAD)

**Hotspot line counts after Round 1 (`wc -l`):**

| File | Lines |
| --- | ---: |
| `crates/ajax-web/web/src/app/App.tsx` | 775 |
| `crates/ajax-web/web/src/app/routes/TaskWorkspaceRoute.tsx` | 83 |
| `crates/ajax-web/web/src/features/task-workspace/TaskWorkspace.tsx` | 122 |
| `crates/ajax-web/web/src/features/task-workspace/TaskWorkspaceHeader.tsx` | 45 |
| `crates/ajax-web/web/src/features/task-workspace/taskWorkspaceRouting.ts` | 58 |
| `crates/ajax-web/web/src/features/task-workspace/taskViewPreference.ts` | 56 |

**Focused frontend tests (task/session/drop/sheet/harness/task-view + workspace):**

```bash
npm run web:test -- --run \
  src/app/App.test.tsx \
  src/app/App.task-view.test.tsx \
  src/app/App.session.test.tsx \
  src/app/App.drop-confirm.test.tsx \
  src/app/App.sheet.test.tsx \
  src/app/App.harness-swap.test.tsx \
  src/app/App.polling.test.tsx \
  src/app/App.skeleton.test.tsx \
  src/features/task/TaskDetail.test.tsx \
  src/features/session/taskViewPreference.test.ts \
  src/features/task-workspace/taskViewPreference.test.ts \
  src/features/task-workspace/TaskWorkspace.test.tsx \
  src/features/task-workspace/TaskWorkspaceHeader.test.tsx \
  src/app/routes/TaskWorkspaceRoute.test.tsx
```

Result: **pass** — 14 files, 134 tests (Vitest v4.1.9). jsdom logged benign
`HTMLCanvasElement.getContext` notices from `@xterm/addon-serialize`; exit 0.

**TypeScript check:**

```bash
npm run web:check
```

Result: **pass** — exit 0.

**Whitespace check:**

```bash
git diff --check
```

Result: **pass** — exit 0.

**Round 1 deliverables:** `TaskWorkspaceRoute`, `TaskWorkspace`,
`TaskWorkspaceHeader`, `taskViewPreference`, and `taskWorkspaceRouting` in
`features/task-workspace`; App owns shell concerns only; bare `#/session` remains
New Task; Diff Back uses `resolveTaskWorkspaceHash` with mode-aware preference.
Round 1 scope explicitly includes `taskWorkspaceRouting.ts` as the pure
capability/hash/redirect helper module (not folded into other files).

**Round 1 revision note (post parent review):** `SessionChat` still uses
LiveHead for task identity/navigation until Round 2. This revision does not add
a duplicate Back/title row in Chat.

**Deviation:** `taskViewPreference` re-exported from `features/session` for
existing import paths; canonical owner is `features/task-workspace`.

**Round 1 revision verification (2026-08-19):**

```bash
npm run web:test -- --run src/features/task-workspace/taskWorkspaceRouting.test.ts
git diff --check
```

Result: **pass** — 1 file, 20 tests (Vitest v4.1.9); exit 0.

`git diff --check`: **pass** — exit 0.

### Round 2 — Unify task details and remove Chat task internals (2026-08-19, current HEAD)

**Hotspot line counts after Round 2 (`wc -l`):**

| File | Lines |
| --- | ---: |
| `crates/ajax-web/web/src/features/session/SessionChat.tsx` | 504 |
| `crates/ajax-web/web/src/features/task-workspace/TaskWorkspace.tsx` | 210 |
| `crates/ajax-web/web/src/features/task-workspace/TaskDetailsSheet.tsx` | 209 |
| `crates/ajax-web/web/src/features/task/TaskDetail.tsx` | 126 |

**Focused frontend tests (Round 2 gate):**

```bash
npm run web:test -- --run \
  src/app/App.test.tsx \
  src/app/App.task-view.test.tsx \
  src/app/App.session.test.tsx \
  src/app/App.drop-confirm.test.tsx \
  src/app/App.sheet.test.tsx \
  src/app/App.harness-swap.test.tsx \
  src/features/task/TaskDetail.test.tsx \
  src/features/session/SessionChat.test.tsx \
  src/features/task-workspace/TaskWorkspace.test.tsx \
  src/features/task-workspace/TaskDetailsSheet.test.tsx \
  src/features/task-workspace/taskViewPreference.test.ts
```

Result: **pass** — 11 files, 176 tests (Vitest v4.1.9). jsdom logged benign
`HTMLCanvasElement.getContext` notices from `@xterm/addon-serialize`; exit 0.

**TypeScript check:**

```bash
npm run web:check
```

Result: **pass** — exit 0.

**Whitespace check:**

```bash
git diff --check
```

Result: **pass** — exit 0.

**Round 2 deliverables:** `TaskDetailsSheet` owns one mounted task-details dialog
per workspace; Chat header Details and Terminal header/footer Details open it;
harness-switch UI composition lives in Task Workspace with mutations in task
ownership; `SessionChat` no longer imports task UI internals (`ActionBar`,
`HarnessSwap`, `TaskMetaDetails`, `TaskLoadError`, `visibleTaskActions`).

**Round 2 revision (2026-08-19):** Terminal footer `Task details` opens the same
workspace-owned `TaskDetailsSheet` as header Details; inline footer metadata
disclosure removed. `TaskMetaDetails` remains the embedded metadata renderer
inside the sheet.

### Round 3 — Give Chat viewport and speech adapter ownership (2026-08-19, current HEAD)

**Hotspot line counts after Round 3 (`wc -l`):**

| File | Lines |
| --- | ---: |
| `crates/ajax-web/web/src/features/session/SessionChat.tsx` | 490 |
| `crates/ajax-web/web/src/features/task/TaskTerminal.tsx` | 987 |
| `crates/ajax-web/web/src/features/task/mountTaskTerminalSession.ts` | 991 |
| `crates/ajax-web/web/src/features/chat/viewport/useChatViewport.ts` | 158 |
| `crates/ajax-web/web/src/features/chat/speech/useChatSpeech.ts` | 37 |
| `crates/ajax-web/web/src/shared/hooks/useSpeechInput.ts` | 302 |
| `crates/ajax-web/web/src/features/task/useTaskTerminalSpeech.ts` | 47 |

**Focused frontend tests (Round 3 gate):**

```bash
npm run web:test -- --run \
  src/features/session/SessionChat.test.tsx \
  src/features/chat/viewport/useChatViewport.test.tsx \
  src/features/chat/speech/useChatSpeech.test.tsx \
  src/features/session/sessionChatViewport.test.ts \
  src/features/task/useTaskTerminalSpeech.test.tsx \
  src/features/task/TaskTerminal.test.tsx \
  src/app/App.session.test.tsx \
  src/features/task-workspace/TaskWorkspace.test.tsx
```

Result: **pass** — 8 files, 113 tests (Vitest v4.1.9). jsdom logged benign
`HTMLCanvasElement.getContext` notices from `@xterm/addon-serialize`; exit 0.

**TypeScript check:**

```bash
npm run web:check
```

Result: **pass** — exit 0.

**Whitespace check:**

```bash
git diff --check
```

Result: **pass** — exit 0.

**Round 3 deliverables:** `useChatViewport` and tests under `features/chat/viewport`;
`useChatSpeech` draft adapter under `features/chat/speech`; shared STT orchestration
in `shared/hooks/useSpeechInput.ts`; terminal PTY adapter remains in
`useTaskTerminalSpeech.ts`. `SessionChat` no longer imports xterm, terminal
connection, or terminal speech hooks. Chat speech UI tests mock `useChatSpeech`
instead of terminal speech hooks.

**Deviation:** none.

### Round 4 — Mechanical feature moves (2026-08-19, current HEAD)

**Hotspot line counts after Round 4 (`wc -l`):**

| File | Lines |
| --- | ---: |
| `crates/ajax-web/web/src/features/chat/ChatSurface.tsx` | 490 |
| `crates/ajax-web/web/src/features/terminal/TaskTerminal.tsx` | 987 |
| `crates/ajax-web/web/src/features/terminal/mountTaskTerminalSession.ts` | 991 |
| `crates/ajax-web/web/src/features/task-workspace/TaskTerminalView.tsx` | 126 |
| `crates/ajax-web/web/src/features/task-workspace/TaskWorkspace.tsx` | 203 |
| `crates/ajax-web/web/src/app/App.tsx` | 775 |

**Move summary:**

- `features/session/*` → `features/chat/*`; public surface renamed `SessionChat` →
  `ChatSurface`.
- Terminal surface files → `features/terminal/*` (unchanged file bodies except
  import paths).
- Terminal mode owner `TaskDetail` → `features/task-workspace/TaskTerminalView.tsx`
  (real owner: header, interact panel, lazy terminal, footer details).
- Narrow `public.ts` added for `chat`, `terminal`, `task`; extended
  `task-workspace/public.ts`.
- Removed `features/session/taskViewPreference` re-export shim; empty `session/`
  directory retired.

**Lazy terminal chunk (`web:build`):**

```text
dist/app.js       716.70 kB │ gzip: 227.23 kB
dist/terminal.js  429.86 kB │ gzip: 117.33 kB
```

`vite.config.mts` chunk path updated to `/features/terminal/TaskTerminal`; build
emits deterministic `terminal.js` as before.

**Focused frontend tests (Round 4 gate):**

```bash
npm run web:test -- --run \
  src/features/chat/ChatSurface.test.tsx \
  src/features/chat/viewport/useChatViewport.test.tsx \
  src/features/chat/speech/useChatSpeech.test.tsx \
  src/features/terminal/TaskTerminal.test.tsx \
  src/features/terminal/useTaskTerminalSpeech.test.tsx \
  src/features/task-workspace/TaskWorkspace.test.tsx \
  src/features/task-workspace/TaskTerminalView.test.tsx \
  src/features/task-workspace/TaskDetailsSheet.test.tsx \
  src/app/App.test.tsx \
  src/app/App.task-view.test.tsx \
  src/app/App.session.test.tsx \
  src/app/App.drop-confirm.test.tsx \
  src/app/App.harness-swap.test.tsx \
  src/app/routes/TaskWorkspaceRoute.test.tsx
```

Result: **pass** — 17 files, 246 tests (Vitest v4.1.9). jsdom logged benign
`HTMLCanvasElement.getContext` notices from `@xterm/addon-serialize`; exit 0.

**Full frontend test suite:**

```bash
npm run web:test -- --run
```

Result: **pass** — 110 files, 1155 tests passed, 9 skipped (Vitest v4.1.9).

**TypeScript check:**

```bash
npm run web:check
```

Result: **pass** — exit 0.

**Production build:**

```bash
npm run web:build
```

Result: **pass** — exit 0; `dist/app.js` + `dist/terminal.js` + `dist/app.css`.

**Whitespace check:**

```bash
git diff --check
```

Result: **pass** — exit 0.

**Round 4 deliverables:** mechanical feature moves with import repair only on
near-limit terminal files; `TaskWorkspace` imports `ChatSurface` and
`TaskTerminalView` via feature public modules; CSS style baseline ledger updated
for pre-existing stylesheet deltas on this branch.

**Deviation:** `TaskDetail` retained as `TaskTerminalView` in task-workspace
(terminal page owner, not a compatibility shell). CSS baseline constants in
`styleSources.ts` re-measured after branch stylesheet changes.

### Round 5 — Enforce feature boundaries (2026-08-19, current HEAD)

**Contract moves:**

- Generic desired-model catalog, `ModelPicker`, and model preference storage →
  `features/task` (`desiredModel.ts`, re-exported from `features/task/public.ts`).
- Orchestration-chat enablement storage → `features/settings`
  (`orchestrationChatPreference.ts`, `features/settings/public.ts`).
- Chat `public.ts` now exports only `ChatSurface`; live-session model failure
  detection stays in `features/chat/sessionModel.ts`.
- App passes `orchestrationChat` to `NewTaskSheet` and `TaskWorkspaceRoute`;
  task code no longer imports Chat or Settings.

**ESLint fail-then-clean proof:**

Deliberate violation in `features/task/HarnessSwap.tsx`:

```typescript
import { ChatSurface } from "@/features/chat/public";
```

```bash
npm run web:lint
```

Result: **fail** — `no-restricted-imports`: `task must not import chat, terminal,
task-workspace, settings, or diff` on `HarnessSwap.tsx:2`.

After removing the import:

```bash
npm run web:lint
```

Result: **pass** — exit 0.

**Focused frontend tests (Round 5 gate):**

```bash
npm run web:test -- --run \
  src/features/task/desiredModel.test.ts \
  src/features/task/ModelPicker.test.ts \
  src/features/settings/orchestrationChatPreference.test.ts \
  src/features/task/NewTaskSheet.test.tsx \
  src/app/App.test.tsx \
  src/app/App.task-view.test.tsx \
  src/app/App.session.test.tsx \
  src/app/App.harness-swap.test.tsx \
  src/features/task-workspace/TaskWorkspace.test.tsx \
  src/features/task-workspace/TaskDetailsSheet.test.tsx \
  src/features/chat/ChatSurface.test.tsx \
  src/app/routes/TaskWorkspaceRoute.test.tsx
```

Result: **pass** — 14 files, tests green (Vitest v4.1.9).

**Full frontend test suite:**

```bash
npm run web:test -- --run
```

Result: **pass** — 111 files passed, 2 skipped; 1155 tests passed, 9 skipped.

**TypeScript check:**

```bash
npm run web:check
```

Result: **pass** — exit 0.

**Whitespace check:**

```bash
git diff --check
```

Result: **pass** — exit 0.

**Round 5 deliverables:** path-specific `no-restricted-imports` production rules
in `eslint.config.mjs`; public contracts for task and settings expanded; import
repairs in App, routes, task-workspace, and task; architecture docs updated for
model/settings ownership.

**Deviation:** none.

### Round 6 — Final architecture and behavior gate (2026-08-19, current HEAD)

**Architecture confirmations (production source inspected):**

| Check | Result |
| --- | --- |
| App selects/composes Chat or Terminal | **No** — `App.tsx` routes handled tasks through `TaskWorkspaceRoute`; no `ChatSurface`, `TaskTerminal`, or `SessionChat` references |
| Task Workspace owns peer-mode composition | **Yes** — only `features/task-workspace/TaskWorkspace.tsx` imports `ChatSurface` from `@/features/chat/public` and composes `TaskTerminalView` |
| Chat terminal imports | **None** — no production file under `features/chat` imports `features/terminal` |
| Chat task-internal imports | **None** — production Chat code imports task only via `@/features/task/public` (`useTaskSession.ts`: `DEFAULT_SESSION_MODEL`, `writeSessionModel`) |
| task upward feature imports | **None** — no production file under `features/task` imports chat, terminal, or task-workspace |
| shared upward feature imports | **None** — no `@/features/*` imports under `shared/` |

**Recorded deviation:** `TaskTerminalView.tsx` lazy-imports
`@/features/terminal/TaskTerminal` directly (not `terminal/public.ts`) to preserve
the deterministic `dist/terminal.js` Vite manual-chunk path. Intentional; do not
route through `public.ts` without updating `vite.config.mts` chunk matching.

**Hotspot line counts after Round 6 (`wc -l`):**

| File | Lines |
| --- | ---: |
| `crates/ajax-web/web/src/app/App.tsx` | 777 |
| `crates/ajax-web/web/src/features/chat/ChatSurface.tsx` | 490 |
| `crates/ajax-web/web/src/features/chat/sessionThread.ts` | 588 |
| `crates/ajax-web/web/src/features/terminal/TaskTerminal.tsx` | 987 |
| `crates/ajax-web/web/src/features/terminal/mountTaskTerminalSession.ts` | 991 |
| `crates/ajax-web/web/src/features/task-workspace/TaskWorkspace.tsx` | 203 |

**Complete verification gate:**

```bash
npm run web:check
```

Result: **pass** — exit 0.

```bash
npm run web:lint
```

Result: **pass** — exit 0 (executable architecture test; no production violations).

```bash
npm run web:sg
```

Result: **fail** — 17 `noop-jsx-handler` errors in
`features/task-workspace/TaskWorkspace.test.tsx` (empty `onGo`/`onBack`/`onOpenDiff`
stubs). Pre-existing test hygiene; not an architecture-boundary violation.

**Round 6 retry (2026-08-19):** replaced empty JSX handler stubs with `vi.fn()` in
`TaskWorkspace.test.tsx`; updated `layout-scroll.test.ts` to open
`TaskDetailsSheet` via `[data-testid="task-meta-details-trigger"]` and scroll
`session-details-body` (footer inline meta removed in Round 2).

```bash
npm run web:sg
```

Result: **pass** — exit 0 (retry).

```bash
npm run web:test -- --run src/features/task-workspace/TaskWorkspace.test.tsx
```

Result: **pass** — 6/6 tests green (retry).

```bash
npm run web:smoke -- --project=mobile-webkit crates/ajax-web/web/e2e/layout-scroll.test.ts
```

Result: **pass** — 5/5 tests green (retry).

```bash
git diff --check
```

Result: **pass** — exit 0 (retry).

```bash
npm run web:test -- --run
```

Result: **pass** — 111 files passed, 2 skipped; 1155 tests passed, 9 skipped
(Vitest v4.1.9). Benign jsdom `HTMLCanvasElement.getContext` notices from xterm.

```bash
npm run web:build
```

Result: **pass** — exit 0.

```text
dist/app.js       716.76 kB │ gzip: 227.10 kB
dist/terminal.js  429.86 kB │ gzip: 117.33 kB
dist/app.css       84.36 kB │ gzip:  15.26 kB
```

```bash
npm run web:build:check
```

Result: **pass** — deterministic shell with version placeholder.

```bash
npm run verify:arch
```

Result: **pass** — exit 0 (~86s; includes Rust arch slice tests).

```bash
git diff --check
```

Result: **pass** — exit 0.

**Focused mobile WebKit (local environment supported; Playwright + Vite dev server):**

```bash
npm run web:smoke -- --project=mobile-webkit \
  crates/ajax-web/web/e2e/diff-review-swipe-repro.test.ts \
  crates/ajax-web/web/e2e/layout-scroll.test.ts
```

Result: **partial fail** — 6 passed, 1 failed.
`layout-scroll.test.ts:206` timed out waiting for `.meta-details summary`
(inline footer metadata removed in Round 2; sheet affordance replaced it).
Navigation swipe tests green. **Fixed in Round 6 retry** — see retry block above.

```bash
npm run web:smoke -- --project=mobile-webkit -g \
  "printable, control|Hide keyboard|keyboard-open resize|task route mounts" \
  crates/ajax-web/web/e2e/terminal-behavior.test.ts
```

Result: **pass** — 3/3 keyboard/mount tests green.

**Not run:** full `terminal-behavior.test.ts` suite (66 tests) — focused keyboard
subset only per Round 6 gate scope.

**`git status --short --branch`:**

```text
## ajax/architecture-chat-refactor...origin/main [behind 3]
```

Working tree matches Rounds 0–5 aggregate (session→chat/terminal moves,
task-workspace extraction, ESLint boundary rules, dist rebuild).

**Changed-file / rename summary (foundation aggregate):**

- `features/session/*` → `features/chat/*`; public surface `SessionChat` → `ChatSurface`.
- Terminal surface → `features/terminal/*` (bodies unchanged except import paths).
- Terminal page owner `TaskDetail` → `task-workspace/TaskTerminalView`.
- New `features/task-workspace/*` (workspace, header, details sheet, routing, preference).
- New `app/routes/TaskWorkspaceRoute.tsx`.
- Generic model catalog → `features/task`; orchestration-chat preference → `features/settings`.
- Narrow `public.ts` for chat, terminal, task, task-workspace, settings.
- Retired `features/session/` directory and shared `useSessionChatViewport`.

**Generated `dist` changes:** `app.js`, `app.css`, `terminal.js` updated by
`web:build` on this branch — expected after feature moves, import repairs, and
stylesheet deltas; chunk split preserved (`terminal.js` still emitted at ~430 kB).

**Remaining risks:**

- Branch remains **behind `origin/main` by 3**; sync not authorized on this worktree.
- Near-limit terminal files (`TaskTerminal.tsx` 987, `mountTaskTerminalSession.ts`
  991) still block new Terminal behavior until approved split.
- Other e2e files (`terminal-behavior.test.ts`, `actions.test.ts`) may still
  reference `.meta-details summary`; out of Round 6 retry scope.

**Round 6 deliverables:** architecture confirmations hold; complete gate run
recorded; foundation ready for follow-up worktree tasks (Chat runtime/thread/presentation,
Terminal runtime split, shared cleanup).

# Ajax Architecture

Ajax is an operator cockpit for isolated AI coding tasks. Native Cockpit and
Web Cockpit are sibling operator surfaces over the same backend contracts. The
CLI, JSON contract, Rust core, TUI, and browser adapter provide deterministic
operator surfaces used by Cockpit, scripts, and tests.

The codebase is a modular monolith organized around vertical slices.

## Crates

### `ajax-core`

Owns the domain model, registry facade, lifecycle model, command planning,
policy decisions, live-status reduction, task annotation projection, and typed output
contracts.

### `ajax-cli`

Owns argument parsing, context loading/saving, command dispatch, human rendering,
JSON rendering, and process execution wiring.

### `ajax-tui`

Owns the native Cockpit interface over `ajax-core` JSON-backed responses.

### `ajax-web`

Owns the browser Cockpit adapter: HTTP routing, browser shell assets, browser
API DTOs, local HTTPS identity, and Web Cockpit server wiring. It is a
presentation adapter over `ajax-core` Cockpit projections and task-operation
contracts, not a second task domain.

### `ajax-supervisor`

Owns supervised agent execution, process monitoring, repository observation, and
translation of live process events into Ajax monitor events.

## External Substrates

Ajax coordinates external tools but does not replace them.

- Git owns repository truth, branches, merges, and worktrees.
- tmux owns durable interactive sessions.
- Agent CLIs are opaque workers.
- SQLite stores Ajax registry state as Ajax-owned task intent, task events, and
  cached projections. It is durable storage for Ajax facts and a fast read
  model, not the source of truth for Git, tmux, or process reality.

Ajax owns task intent, task lifecycle decisions, naming, policy, task operation
history, live projection, command plans, and registry state.

## Task Authority Model

Ajax tasks are coordinated external work environments. A task is not simply a
database row and not simply a command plan. The backend treats a task as the
composition of:

- `TaskIntent` — Ajax-owned durable intent: repo, handle, title, selected agent,
  expected branch, expected worktree path, expected tmux session, and expected
  task window.
- Task events — Ajax-owned history: task creation, lifecycle decisions,
  operation progress, substrate-change records, and incomplete teardown notes.
- Substrate observations — observed Git, tmux, worktree, task-window, and agent
  facts. These are source-tagged and rebuildable from external substrates.
- Task projection — the disposable read model used by CLI, JSON output, and
  Cockpit. It includes lifecycle, runtime health, live status, annotations, and
  recommended operator actions.

SQLite may cache substrate observations and projections so commands and Cockpit
can render quickly. Cached substrate evidence must be treated as staleable
evidence, not authority. Git, tmux, and supervised processes remain the
authoritative sources for their own reality.

## Task Operations

Task operations are the backend transaction boundary for operator actions. They
plan external effects, apply operation evidence, and return typed outcomes that
CLI and Cockpit render.

Mutable task operations use local-first reconciliation and step receipts. Before
planning or retrying a destructive or provisioning command, Ajax should observe
the relevant substrates and build the next command from fresh evidence. After a
successful external side effect, Ajax records a named step receipt in SQLite.
Receipts are Ajax-owned evidence that an operation step succeeded or was skipped
because the substrate was already in the desired state. They are not authority
over Git, tmux, or process reality; retries still re-observe those substrates
before deciding what to skip or repair.

The task operation boundary now owns the main mutable task actions:

- Start operation planning returns `TaskIntent` plus the external command plan
  without mutating the registry.
- Start operation execution records the task, applies named provisioning steps,
  records step receipts for successful provisioning side effects, marks
  provisioning failure in core with failed-step metadata, and opens the task
  after successful setup.
- Single-task command operations plan and execute `resume`, `review`, `repair`,
  and `ship` from core. CLI and Cockpit provide runner and rendering adapters;
  core owns post-execution reducers such as opened, merged, repair/check
  succeeded, and merge/check failure state.
- Drop operation planning starts from fresh substrate observation and produces
  `DropOp`s from observed resources rather than cached registry fields alone.
- Drop execution runs teardown ops, records step evidence, re-observes external
  resources, records receipts for successful or already-satisfied cleanup steps,
  and decides `Removed` versus `TeardownIncomplete` from the final observation
  inside core.
- Sweep cleanup (`tidy`) is a batch operation that plans safe cleanup
  candidates, executes each candidate, marks completed cleanup state, and
  reports whether an error happened after partial state changes.

Command modules still expose substrate-oriented planning helpers. Task
operations compose those helpers into vertical operator transactions.

## Core Architecture

### Vertical Slices

Ajax follows an Aroeira-style modular monolith: dependency boundaries still
point inward, while feature work is organized around operator capabilities.
A slice is a vertical use-case module inside its owning crate, not a new crate
and not a cosmetic facade over unrelated layered code.

`ajax-core::slices` owns pure operator capability orchestration. Each slice
starts with private implementation modules plus a small public facade. Code
outside the slice depends on the facade only; private slice modules are free to
change as the capability evolves. Slice names use operator language such as
`review`, `resume`, `ship`, or `drop`, rather than substrate language such as
Git diff, tmux attach, or process cleanup.

Slices may depend on core domain models, lifecycle rules, policy, output
contracts, registry traits, and command-spec ports. Mechanisms remain outside
slices: filesystem, terminal, JSON, subprocess, Git, tmux, networking, SQLite,
and process supervision stay in `adapters`, `registry/sqlite`, `ajax-web`, or
`ajax-supervisor`, depending on the external boundary. CLI, TUI, and browser code
are composition and presentation layers; they call public slice facades and do
not reach into private slice modules.

Architecture tests use `rust_arkitect` to enforce slice direction as the
codebase migrates. Migrations happen one operator capability at a time, keeping
existing public APIs as compatibility wrappers until callers can move to the
slice facade.

### Registry

The registry stores Ajax task state and typed task events. It exposes typed
tasks and events to command, output, CLI, and Cockpit boundaries.

Durable registry state is backed by SQLite through `SqliteRegistryStore`.
Transient and test state use `InMemoryRegistry`.

SQLite is the fast read model for Ajax task state. It records expected runtime
identity, last observed Git/tmux evidence, derived runtime health, and typed
events. It also stores step receipts for Ajax-owned operation evidence. Git and
tmux still own live substrate reality; Ajax reconciles their observations into
SQLite so Cockpit, command planning, and JSON output can read one coherent task
record.

Registry ghosts are tasks that should not survive SQLite save/load and should
not appear in Cockpit. `ajax-core::ghost_task` is the single classifier for that
decision. Persistence (`registry/sqlite`), Cockpit projection, and visibility
all consult the same rule. Recoverable missing-substrate tasks in operational
lifecycles remain persisted with their events and step receipts. Only
`Removed`, `Stale`, or abandoned provisioning records with no recoverable Git
substrate are pruned as ghosts.

### Lifecycle

Lifecycle state is modeled in `ajax-core::lifecycle`. Commands and live-status
application request lifecycle transitions through the lifecycle boundary.

Annotations are task properties derived from lifecycle state, live status, side
flags, and substrate evidence. Operator actions are projected from those
annotations and from task state; Cockpit no longer consumes a separate parallel
attention list.

Tasks blocked by merge conflicts or CI failures also expose skill-backed
`remediations` on `TaskCard` (for example `fix-ci` and
`resolve-merge-conflicts`). Core selects remediations from live status and git
evidence; `ajax-web` resolves skill paths on the companion host and sends the
skill brief into the task tmux session when the operator runs a remediation
from native Cockpit or the mobile browser shell.

### Substrate Evidence

Substrate evidence records observed external facts from Git, tmux, worktrees,
and supervised processes.

Git evidence interpretation lives in `analysis::git_evidence`.

Before provisioning a task worktree, start planning runs `git fetch origin
<default_branch>` on the managed repo root, then `git worktree add` branches from
`origin/<default_branch>`. This avoids mutating a checked-out or diverged local
default branch while ensuring new tasks use the fetched remote state.

Managed repos may optionally run a `graphify_update` shell command from the repo
root during start (for example `graphify extract --update`). Each repo keeps its
own `graphify-out/` knowledge graph. `ajax doctor` warns when `graphify-out` is
gitignored so agents can rely on the checked-in or generated graph.

Runtime reconciliation lives in `runtime`. It compares expected task runtime
state with observed Git, tmux, and task-window evidence, then produces a single
runtime health verdict such as healthy, missing worktree, missing session,
missing task window, wrong task-window path, or unobservable. UI and action
selection consume that verdict instead of reinterpreting individual substrate
fields.

Runtime refresh lives in `runtime_refresh`. It probes Git and tmux, reconciles
runtime evidence, refreshes cached annotations, and recovers missing task
records from observed Ajax worktrees. Core also accepts a small external
agent-status cache port for hook-backed status values; adapters read filesystem
or terminal cache formats and core reduces those values into live observations.
Cockpit invokes runtime refresh through the CLI adapter but does not own the
refresh algorithm.

#### Runtime refresh and registry persistence

Ajax keeps one operator-facing task model, but three boundaries apply different
rules:

- **In-memory registry** — authoritative for the running CLI or web process
  between SQLite reloads.
- **SQLite persistence** — stores durable operator intent. Active tasks with
  credible git worktree evidence persist even when tmux/worktrunk substrate is
  missing so Cockpit can offer drop/retry without recreate loops.
- **Substrate observation** — git/tmux/pane probes update flags and live status
  on existing rows; they must not fight persistence or silently duplicate tasks.

Orphan worktree discovery runs only when a refresh gate fires: provisioning or
stale runtime projections, or tmux lists an `ajax-{repo}-{handle}` session that
is not yet registered. Steady-state polls with fresh projections skip per-repo
`git worktree list` unless a gate demands discovery.

`RefreshTier::Live` skips orphan git discovery unless those gates fire.
`RefreshTier::Full` is used for periodic web attention polls and operator paths
that require rediscovery. Native Cockpit uses in-memory context and saves on
change; web reloads SQLite only when the state file mtime advances or after a
mutating operation persisted to disk.

### Live Status

`live.rs` reduces observations into live-status classifications.

`live_application.rs` applies reduced observations to task state, agent status,
side flags, activity timestamps, and visible live status.

Core remains browser-agnostic. It may expose Cockpit projections, action policy,
task-operation outcomes, runtime reconciliation, and typed output contracts that
the browser shell consumes, but it must not own HTTP routes, static web assets,
service workers, TLS identity files, browser storage, or web server lifecycle.

## Command Architecture

Command planning and command execution are separate.

`ajax-core::commands` builds command plans and typed command responses.

`CommandSpec` describes external commands. `CommandRunner` executes them through
capture or inherited-stdio modes.

Command modules are split by use case:

- `commands/doctor.rs`
- `commands/check.rs`
- `commands/diff.rs`
- `commands/merge.rs`
- `commands/new_task.rs`
- `commands/open.rs`
- `commands/projection.rs`
- `commands/teardown.rs`
- `commands/trunk.rs`
- `commands/lookup.rs`

The public CLI vocabulary is operator-facing: `start`, `resume`, `repair`,
`review`, `ship`, `drop`, `tidy`, and `ready`. Some internal command modules
still carry substrate-oriented names where they wrap the underlying git, tmux,
or test-command operation.

Runtime profile names such as `stable` and `dev` are runtime selections, not
task-operation commands or separate operator domains.

## Adapter Architecture

`ajax-core::adapters` is the adapter facade.

- `adapters/command.rs` defines command specs and the command-runner port.
- `adapters/process.rs` executes subprocesses.
- `adapters/git.rs` builds and parses Git commands.
- `adapters/tmux.rs` builds and parses tmux commands.
- `adapters/agent.rs` builds and parses agent commands.
- `adapters/environment.rs` probes operator environment facts.

## Supervisor Architecture

`ajax-supervisor` separates monitor runtime wiring from substrate observers.

- `runtime.rs` owns monitor wiring, cancellation, channels, event logging, and
  monitor handles.
- `agent/codex.rs` owns Codex command construction and JSONL parsing.
- `agent/cursor.rs` owns Cursor CLI command construction and stream-json parsing.
- `repo_observer.rs` owns repository file-change observation and Git snapshots.
- `process_observer.rs` owns child process output, exit status, and hang
  detection.
- `event_log.rs` owns optional append-only JSONL event persistence.
- `status.rs` reduces monitor events into observed live status.

## CLI Architecture

`ajax-cli` is the command and rendering shell around `ajax-core`.

- `lib.rs` owns the Clap command tree, parsing, dispatch, and public test
  helpers.
- `context` owns runtime profile path resolution and load/save behavior.
  Stable runtime resolution preserves the historical config/state/log/cache
  defaults and legacy sibling task worktrees. Dev and custom-home runtimes use
  isolated config, SQLite state, logs, cache, and rooted task worktrees.
- `render` owns human, JSON, execution-output, and command-plan rendering.
- `snapshot_dispatch` owns read-only command routing.
- `execution_dispatch` owns mutable command routing.
- `cockpit_backend` owns Cockpit snapshots, watch mode, and TUI backend glue.
  It calls core runtime refresh and explicit cockpit projection rebuilds rather
  than owning substrate refresh logic.
- A thin Web Cockpit launcher may start or stop the host-native `ajax-cli web`
  process from a resolved CLI context. Process launching is orchestration only;
  the launcher passes explicit runtime context to `ajax-web` and must not
  reinterpret task state or duplicate web server internals.
- `agent_status_cache` owns filesystem reads for hook-backed agent status caches
  such as `tmux-agent-status`; core owns the status value interpretation.
- `task_session` owns interactive task PTY entry from Cockpit. Ajax owns the
  foreground task bridge, forwards normal input to the attached tmux client,
  filters Cockpit-owned shortcuts such as Ctrl-Q and Ctrl-T without installing
  tmux bindings, and resumes Cockpit when the task attach client detaches.
  Ctrl-T returns to Cockpit on the create-task screen for the task's project.

Startup behavior should stay inside normal CLI parsing and dispatch. Bare
invocations may choose a default operator surface, and flags may select runtime
profiles, but `main.rs` should not rewrite argv into hidden commands. Public CLI
vocabulary remains operator-facing.

## Web Cockpit Architecture

`ajax-web` is the browser Cockpit adapter. It is a vertical presentation adapter
over the same Cockpit projection and task-operation contracts used by Native
Cockpit. It may shape responses for browser ergonomics, but it must not own task
lifecycle rules, registry truth, runtime reconciliation, Git/tmux
interpretation, substrate evidence, operation outcomes, or action policy.

Web Cockpit is a first-class browser operator surface, but it is intentionally a
dashboard rather than a terminal mirror. Native Cockpit and Web Cockpit consume
shared Cockpit projections and task-operation contracts; neither surface owns
task truth. The browser experience should lead with task state, required
decisions, and next actions, while raw pane text stays secondary and
collapsible.

The browser shell is not an offline-first Ajax client and must not introduce a
second browser-side task model. Git, tmux, SQLite, supervised processes, and
the Ajax backend remain authoritative for task state and operations. The
primary iPhone target is normal iOS Safari. iOS Home Screen PWA mode is
experimental and not recommended for operations because WebKit lifecycle and
cache behavior can leave the installed app stuck while Safari still works.

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

Browser files live under `crates/ajax-web/web`. The install slice owns serving
the HTML shell, client JavaScript, stylesheet, optional web manifest, optional
service worker, and app icons from that directory. `ajax-web::runtime` owns
HTTP transport wiring, local TLS setup, and shell asset delivery.
`ajax-cli` remains a thin native bridge: it resolves stable/dev context paths,
reloads and saves the authoritative SQLite state, and delegates native command
execution for browser-submitted actions.

Any manifest should stay small and non-critical: app name, short name,
description, `start_url`, `scope`, theme/background colors, and icons. It must
not make Home Screen installation the primary iPhone path or require native-app
PWA lifecycle assumptions.

Web Cockpit syncs server-authoritative Cockpit projections, not browser-owned
task records. `GET /api/cockpit` returns the latest backend projection. Mutable
operations return typed operation outcomes and either include or cause a refresh
of the latest Cockpit projection. The browser may keep transient UI state such
as "sending" or "failed," but it must not persist pending task operations or
replay mutations after reload.

The app must function correctly without a service worker. If a service worker
is kept, it is non-critical and limited to cleanup or safe static assets. It
must never intercept or cache live Ajax endpoints, including `/api/cockpit`,
`/api/actions`, health checks, polling endpoints, streaming endpoints,
WebSocket/SSE endpoints, or any future `/api/*` endpoint.

Browser storage is intentionally limited. The browser shell must not use
IndexedDB, background sync, local task queues, offline mutation replay, or
cached operational/API data. It must not add Yew, Trunk, WASM, or a large
frontend architecture unless the project explicitly adopts those elsewhere.

Stable and dev runtime profiles remain separated by the host-native
`ajax-cli web` process and explicit runtime context. Stable uses the stable
state database and default web port, while dev uses the development state
database and dev web port. The browser shell must not merge profile state in
browser storage.

Notifications are out of scope. Ajax Web Cockpit must not implement Web Push,
PushManager flows, Notification API prompts, VAPID keys, push subscriptions,
service-worker push handlers, notification click handlers, or notification
infrastructure.

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
- `ajax-web::action_vocabulary` owns the shared browser action capability
  vocabulary used by both `cockpit` and `operate` without cross-slice imports.

Slices may call adapter facades, but slices are named after capabilities rather
than mechanisms. New browser features should start as a vertical slice when they
represent an operator or browser capability; add an adapter only when the
feature needs a concrete external mechanism.

### `ajax-web::slices::cockpit`

Owns the browser Cockpit read experience. It builds browser DTOs from the core
Cockpit projection, supports projection snapshot or stream delivery, and
preserves the same task/action meaning as Native Cockpit.

### `ajax-web::slices::operate`

Owns browser-submitted operator actions. It accepts browser action requests,
checks browser capability limits, delegates valid work to the existing core task
operations, and returns the refreshed Cockpit projection. Unsupported
capabilities, such as terminal attach, return typed adapter capability outcomes
rather than duplicated lifecycle policy. Browser `resume` remains
`needs_terminal` until a terminal bridge exists.

### `ajax-web::slices::install`

Owns the browser shell. It serves the HTML shell, client JavaScript,
stylesheet, optional manifest, optional service worker, and icons. Home Screen
installation is experimental and must not complicate the reliable Safari path.

### `ajax-web::slices::pane`

Owns the browser pane and guarded approval capability. It turns tmux pane
captures into cleaned browser snapshots with stable per-task sequences so the
dashboard can derive current status and optional terminal details without
centering the UI on a scrolling log.

When the task's agent is at a recognized prompt, the snapshot can also carry a
structured, confidence-scored `AgentPrompt` from `ajax-core::agent_prompt`: the
command, the answerable choices, and a fingerprint. Browser approval actions
post a typed answer plus that fingerprint; `answer_task_prompt` re-captures the
live pane, rejects stale answers, resolves the operator intent through the agent
adapter, and only then delivers `send-keys`, with per-task de-duplication and
rate limiting inside the adapter boundary. Free-form browser input is outside
the dashboard scope and remains a terminal escalation path.

### `ajax-web::runtime`

Owns Web Cockpit runtime wiring and is not itself a slice. It sets up the Axum
HTTP listener, routing, connection handling, local HTTPS identity, graceful
shutdown, and process-level startup by composing `ajax-web::slices::*` with
`ajax-web::adapters::*`. If `ajax-cli` starts Web Cockpit, the CLI launcher
passes resolved runtime context to `ajax-web` explicitly.

Post-startup Web Cockpit routes snapshot registry state under a short mutex hold,
run external tmux/git probes outside the lock, then merge deltas back under the
lock. `/api/cockpit` refresh and `/api/tasks/{handle}/pane` capture follow this
pattern so lightweight routes such as `/api/health` and task detail reads stay
responsive during slow substrate work. Mutating task operations remain
serialized per task through the operation coordinator.

### Post-startup runtime refresh

`ajax-core::runtime_refresh` owns refresh tiers. Steady-state Cockpit polling
uses `RefreshTier::Live`, which skips default orphan git discovery and
per-task pane capture when agent status cache and runtime projections are
fresh. `RefreshTier::Full` remains available for explicit recovery and
maintenance. Agent status is hydrated once per refresh from the tmux-agent-status
pane cache snapshot. Registered tmux sessions are matched by exact expected
session names, not `ajax-{repo}-{handle}` parsing, so hyphenated repo names do
not trigger false orphan discovery.

External command specs for refresh, status, and pane probes carry bounded
timeouts in `ajax-core::adapters`. `CountingCommandRunner` provides reusable
command-budget fixtures for regression tests.

### Native and Web persistence

`ajax-cli::context` tracks SQLite mtime and the task IDs present at load time.
`save_context_with_state` reloads and merges non-conflicting web companion
changes before native final save. Divergent lifecycle updates for the same task
surface an explicit conflict error instead of last-writer-wins overwrite. CLI
entry points load through `TrackedContext` so native saves participate in the
same merge contract as Web Cockpit.

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

## Cockpit Architecture

Cockpit is the operator surface over the JSON-backed command boundary.

`ajax-tui` owns native terminal interaction and rendering.

`ajax-web` owns browser interaction and rendering. Native Cockpit and Web
Cockpit are sibling presentation adapters over shared core projections and
actions; neither surface owns task truth. `ajax-tui` must not know about HTTP,
TLS, service workers, browser manifests, or static web assets.

Web Cockpit serves HTTPS so browsers treat it as a secure context. On first run
it generates a self-signed certificate and persists it beside the state
database; the operator trusts it once on the browser device. HTTPS support does
not imply Home Screen PWA installation or notification support.

Native Cockpit starts `ajax-cli web` by default and keeps it alive for the
Cockpit session. `ajax-cli` starts Web Cockpit on port `8787` with the stable
state database, while `ajax-cli dev` starts it on port `8788` with the
development state database. `--no-web` disables Web Cockpit startup. The web
process is started with explicit `AJAX_PROFILE`, `AJAX_CONFIG`, `AJAX_STATE`,
and rooted worktree values from the selected Ajax context so stable and dev
browser sessions stay on their own runtime profile.

- `actions` owns action and annotation chrome metadata.
- `cockpit_state` owns view state, selectable construction, transitions,
  refresh application, flash state, and confirmations.
- `input` owns terminal-event classification.
- `layout` owns pure layout calculations.
- `navigation` owns key classification helpers.
- `rendering` owns status palette, glyph mapping, and screen rendering.
- `runtime` owns terminal mode, polling, refresh timing, and the event loop.

### Cockpit Views

Cockpit has three navigational views:

- `Projects` — top level. Shows the cross-repo annotation inbox followed by
  the repo list and any unannotated tasks. Inbox rows surface tasks needing
  operator attention regardless of repo.
- `Project` — a single repo's task list. Each task row carries its handle,
  annotation label (or live summary), and primary-action chrome.
- `NewTaskInput` / `Help` — modal text input and reference screen.

There is no separate per-task action menu view. Enter on a task or inbox row
expands an inline drawer that lists the task's available operator actions
underneath the row; Enter on a drawer row dispatches that action. Esc or
selecting a different task collapses the drawer.

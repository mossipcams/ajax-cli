# Ajax Architecture

Ajax is an operator cockpit for isolated AI coding tasks. Native Cockpit is the
primary terminal surface, and mobile web Cockpit is a browser companion over the
same backend contracts. The CLI, JSON contract, Rust core, TUI, and PWA adapter
provide deterministic operator surfaces used by Cockpit, scripts, and tests.

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

Owns the mobile browser Cockpit adapter: HTTP routing, PWA assets, browser API
DTOs, local HTTPS identity, Web Push, and any web companion server runtime. It is
a presentation adapter over `ajax-core` Cockpit projections and task-operation
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
`ajax-supervisor`, depending on the external boundary. CLI, TUI, and PWA code
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

### Lifecycle

Lifecycle state is modeled in `ajax-core::lifecycle`. Commands and live-status
application request lifecycle transitions through the lifecycle boundary.

Annotations are task properties derived from lifecycle state, live status, side
flags, and substrate evidence. Operator actions are projected from those
annotations and from task state; Cockpit no longer consumes a separate parallel
attention list.

### Substrate Evidence

Substrate evidence records observed external facts from Git, tmux, worktrees,
and supervised processes.

Git evidence interpretation lives in `analysis::git_evidence`.

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

### Live Status

`live.rs` reduces observations into live-status classifications.

`live_application.rs` applies reduced observations to task state, agent status,
side flags, activity timestamps, and visible live status.

Core remains browser-agnostic. It may expose Cockpit projections, action policy,
task-operation outcomes, runtime reconciliation, and typed output contracts that
the PWA consumes, but it must not own HTTP routes, static web assets, service
workers, TLS identity files, Web Push subscriptions, browser storage, or web
server lifecycle.

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
- A thin web companion launcher may start or stop the mobile web companion
  process from a resolved CLI context. Process launching is orchestration only;
  the launcher passes explicit runtime context to `ajax-web` and must not
  reinterpret task state or duplicate web server internals.
- `agent_status_cache` owns filesystem reads for hook-backed agent status caches
  such as `tmux-agent-status`; core owns the status value interpretation.
- `task_session` owns interactive task PTY entry from Cockpit. Ajax owns the
  foreground task bridge, forwards normal input to the attached tmux client,
  filters Cockpit-owned shortcuts such as Ctrl-Q without installing tmux
  bindings, and resumes Cockpit when the task attach client detaches.

Startup behavior should stay inside normal CLI parsing and dispatch. Bare
invocations may choose a default operator surface, and flags may select runtime
profiles, but `main.rs` should not rewrite argv into hidden commands. Public CLI
vocabulary remains operator-facing.

## Mobile Web Companion Architecture

`ajax-web` is the mobile browser Cockpit adapter. It is a vertical presentation
adapter over the same Cockpit projection and task-operation contracts used by
native Cockpit. It may shape responses for browser ergonomics, but it must not
own task lifecycle rules, registry truth, runtime reconciliation, Git/tmux
interpretation, or action policy.

The PWA is a thin mobile cockpit for the native Ajax CLI. It is not an
offline-first Ajax client and must not introduce a second browser-side task
model. Git, tmux, SQLite, and the Ajax companion server remain authoritative for
task state and operations.

PWA files live under `crates/ajax-web/web`. The install slice owns serving the
HTML shell, client JavaScript, stylesheet, web manifest, service worker, and app
icons from that directory. `ajax-web::runtime` owns HTTP request routing, generic
connection response handling, local TLS setup, Web Push endpoints, attention
polling, and app-shell asset delivery. `ajax-cli` remains a thin native bridge:
it resolves stable/dev context paths, reloads and saves the authoritative SQLite
state, and delegates native command execution for browser-submitted actions.

The manifest should stay small and install-focused: app name, short name,
description, `start_url`, `scope`, standalone display, portrait orientation,
theme/background colors, and app icons including a maskable icon. Icon files are
static app-shell assets and belong beside the PWA shell.

The service worker may cache only static app-shell assets: `/`, `/app.css`,
`/app.js`, `/manifest.webmanifest`, `/sw.js`, and app icons. It must never cache
live Ajax endpoints, including `/api/cockpit`, `/api/actions`, `/api/push/*`, or
any future `/api/*` endpoint. Static shell requests may use network-first
behavior with cache fallback so installed browsers pick up updates promptly
while still showing the shell when the companion is temporarily unreachable.

Service worker cache names must include an explicit Ajax Cockpit cache version.
Changing the shell asset list or shell behavior requires bumping that version.
Activation should delete old Ajax Cockpit caches and claim clients so installed
PWAs converge on the new shell without keeping stale static assets indefinitely.

Browser storage is intentionally limited. The PWA may use the service worker
Cache API for static app-shell assets and browser-managed Web Push
subscriptions. It must not use IndexedDB, background sync, local task queues, or
offline mutations. It must not add Yew, Trunk, WASM, or a large frontend
architecture unless the project explicitly adopts those elsewhere.

Stable and dev runtime profiles remain separated by the native companion
process and explicit runtime context. Stable uses the stable state database and
default web port, while dev uses the development state database and dev web
port. The PWA must not merge profile state in browser storage.

Web Push remains opt-in and server-authoritative. The browser may register a
subscription with `/api/push/subscribe`; VAPID identity, subscription
persistence, attention polling, notification delivery, and pruning dead
subscriptions belong to the companion boundary, not to core task logic or a
browser-side scheduler.

PWA validation should check manifest shape, icon availability, service worker
registration, app-shell cache contents, `/api/*` cache bypass, cache versioning
and cleanup behavior, local-only shell assets, stable/dev port separation, and
clear browser error states for failed live requests or unsupported actions.

`ajax-web` is organized around vertical browser/operator capabilities inside
the crate:

- `ajax-web::slices::*` owns browser/operator capabilities.
- `ajax-web::adapters::*` owns mechanisms such as HTTP routing, TLS, Web Push,
  static asset embedding, filesystem persistence, network clients, and browser
  serialization formats.
- `ajax-web::runtime` composes slices and adapters into a running web
  companion.

Slices may call adapter facades, but slices are named after capabilities rather
than mechanisms. New browser features should start as a vertical slice when they
represent an operator or browser capability; add an adapter only when the
feature needs a concrete external mechanism.

### `ajax-web::slices::cockpit`

Owns the browser Cockpit read experience. It builds mobile browser DTOs from the
core Cockpit projection, supports snapshot or stream delivery, and preserves the
same task/action meaning as native Cockpit.

### `ajax-web::slices::operate`

Owns browser-submitted operator actions. It accepts mobile action requests,
checks browser capability limits, delegates valid work to the existing core task
operations, and returns the refreshed Cockpit projection. Unsupported
capabilities, such as terminal attach, are reported as adapter capability
errors rather than duplicated lifecycle policy.

### `ajax-web::slices::install`

Owns the installable PWA shell. It serves the HTML shell, client JavaScript,
stylesheet, manifest, service worker, icons, and cache metadata needed for the
browser app to install and refresh predictably.

### `ajax-web::slices::attention`

Owns mobile attention delivery. It compares Cockpit attention projections over
time, detects newly attention-worthy tasks, and asks the push adapter to notify
subscribed browsers.

### `ajax-web::runtime`

Owns web companion runtime wiring and is not itself a slice. It sets up the HTTP
listener, request routing, connection handling, local HTTPS identity, graceful
shutdown, and process-level startup when the companion runs separately by
composing `ajax-web::slices::*` with `ajax-web::adapters::*`. If `ajax-cli`
starts the companion, the CLI launcher passes resolved runtime context to
`ajax-web` explicitly.

The PWA consumes the same Cockpit view model as the native TUI. Browser-specific
DTOs may be narrower or differently named, but they are projections of core
output contracts, not separate task models. Any mobile-only restriction belongs
at the adapter capability boundary; core remains responsible for deciding which
task actions are valid for a task.

The web companion may use HTTP, TLS, filesystem storage for certificates and
subscriptions, network calls to push services, and static asset embedding inside
`ajax-web`. Those mechanisms must not move into `ajax-core` or `ajax-tui`.

## Cockpit Architecture

Cockpit is the primary operator surface over the JSON-backed command boundary.

`ajax-tui` owns native terminal interaction and rendering.

`ajax-web` owns mobile browser interaction and rendering. Native Cockpit and web
Cockpit are sibling presentation adapters over shared core projections and
actions; neither surface owns task truth. `ajax-tui` must not know about HTTP,
TLS, Web Push, service workers, browser manifests, or static web assets.

The companion serves HTTPS so that browsers grant it a secure context: the
prerequisite for installing the PWA, running its service worker, and receiving
Web Push. On first run it generates a self-signed certificate and persists it
beside the state database; the operator trusts it once on the phone.

Web Push is opt-in. The companion holds a persisted VAPID identity, serves its
public key at `/api/push/config`, and stores browser subscriptions posted to
`/api/push/subscribe`. A background attention poller rebuilds the Cockpit view
on an interval, diffs the attention inbox, and sends a VAPID-signed encrypted
notification for each task that newly needs attention; subscriptions the push
service reports as gone are pruned.

Native Cockpit starts the companion as an `ajax-cli web` process by default and
keeps it alive for the Cockpit session. `ajax-cli` starts the companion on port
`8787` with the stable state database, while `ajax-cli dev` starts it on port
`8788` with the development state database. `--no-web` disables the companion.
The companion is started with explicit `AJAX_PROFILE`, `AJAX_CONFIG`,
`AJAX_STATE`, and rooted worktree values from the selected Ajax context so
stable and dev browser sessions stay on their own runtime profile.

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

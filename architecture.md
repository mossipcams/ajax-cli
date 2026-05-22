# Ajax Architecture

Ajax is a native operator cockpit for isolated AI coding tasks. Cockpit is the
primary operator surface. The CLI, JSON contract, and Rust core provide the
deterministic backend used by Cockpit, scripts, and tests.

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
and process supervision stay in `adapters`, `registry/sqlite`, or
`ajax-supervisor`. CLI and Cockpit code are composition and presentation layers;
they call public slice facades and do not reach into private slice modules.

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
- `repo_observer.rs` owns repository file-change observation and Git snapshots.
- `process_observer.rs` owns child process output, exit status, and hang
  detection.
- `event_log.rs` owns optional append-only JSONL event persistence.
- `status.rs` reduces monitor events into observed live status.

## CLI Architecture

`ajax-cli` is the command and rendering shell around `ajax-core`.

- `lib.rs` owns the Clap command tree, parsing, dispatch, and public test
  helpers.
- `context` owns config/state path resolution and load/save behavior.
- `render` owns human, JSON, execution-output, and command-plan rendering.
- `snapshot_dispatch` owns read-only command routing.
- `execution_dispatch` owns mutable command routing.
- `cockpit_backend` owns Cockpit snapshots, watch mode, and TUI backend glue.
  It calls core runtime refresh and explicit cockpit projection rebuilds rather
  than owning substrate refresh logic.
- `web_backend` owns the mobile web presentation shell, HTTP request routing,
  mobile Cockpit JSON serialization, and mobile-safe action dispatch. It serves
  the same core Cockpit projection used by native Cockpit and delegates
  non-interactive task actions to core task operations. It does not own task
  truth, substrate refresh algorithms, or Git/tmux interpretation.
- `agent_status_cache` owns filesystem reads for hook-backed agent status caches
  such as `tmux-agent-status`; core owns the status value interpretation.
- `task_session` owns interactive task PTY entry from Cockpit. Ajax owns the
  foreground task bridge, forwards normal input to the attached tmux client,
  filters Cockpit-owned shortcuts such as Ctrl-Q without installing tmux
  bindings, and resumes Cockpit when the task attach client detaches.

## Cockpit Architecture

Cockpit is the primary operator surface over the JSON-backed command boundary.

`ajax-tui` owns native terminal interaction and rendering.

`ajax-cli::web_backend` owns the mobile browser presentation for Cockpit. The
web layer is intentionally an adapter: it renders a mobile-first HTML shell,
serves a serialized Cockpit view at `/api/cockpit`, and accepts mobile-safe
operator actions at `/api/actions`. Actions that require an attached terminal
session, such as `resume`, remain native-Cockpit only. Git, tmux, SQLite, task
lifecycle, action policy, and projection rebuilding remain owned by the same
core and CLI boundaries used by the terminal Cockpit.

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

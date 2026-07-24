# Ajax Architecture

Ajax is an operator cockpit for isolated AI coding tasks. Native Cockpit and
Web Cockpit are sibling operator surfaces over the same backend contracts. The
CLI, JSON contract, Rust core, TUI, and browser adapter provide deterministic
operator surfaces used by Cockpit, scripts, and tests.

The codebase is a modular monolith organized around vertical slices.

## Documentation Sources

`architecture.md` records durable ownership, boundaries, and invariants. It is
not a parking lot for implementation plans, TDD packets, review notes, or
one-off migration playbooks.

Task-specific plans should live outside the main repo or in an explicitly
temporary planning workspace. If a plan discovers durable architecture, move
only the lasting decision or boundary into `architecture.md` or the nearest
user-facing docs, then retire the plan artifact. Generated graphs, code maps,
and old plan docs are useful for investigation, but source files, tests, and
this architecture document remain the durable references.

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
  facts. These are source-tagged, freshness-aware, and rebuildable from
  external substrates. Observation state distinguishes observed presence,
  observed absence, probe failure, stale evidence, and not-yet-observed facts.
- Task projection — the disposable read model used by CLI, JSON output, and
  Cockpit. It includes the canonical operator status, lifecycle, runtime health,
  live status, annotations, and recommended operator actions. Status is exactly
  `Running`, `Waiting`, `Idle`, or `Error`, with one optional presentation-ready
  explanation; lifecycle and annotations remain separate typed inputs rather
  than additional visible statuses.

SQLite may cache substrate observations and projections so commands and Cockpit
can render quickly. Cached substrate evidence must be treated as staleable
evidence, not authority. Git, tmux, and supervised processes remain the
authoritative sources for their own reality.

### Worktree presence, branch intent, and checkout

Ajax tracks three independent Git facts for each task:

- **Registered-path presence** (`worktree_exists`) — whether the task's
  registered worktree path appears in Git's worktree list. Absence is missing
  substrate, not checkout mismatch.
- **Expected branch intent** (`Task.branch`) — Ajax-owned durable intent for
  which branch the task should use. **Expected-branch existence**
  (`branch_exists`) is observed separately: whether that branch name exists in
  the repo. Intent and existence are independent facts.
- **Observed checkout** (`current_branch`) — the named branch checked out at the
  registered path when the worktree is present, or detached checkout when
  `current_branch` is unset.

**Checkout mismatch** applies only when the worktree is present: the observed
named branch differs from expected intent, or the checkout is detached. A
present worktree on the wrong or detached branch is never classified as missing
substrate.

Reconciliation precedence:

- True physical absence at the registered path remains **missing substrate** and
  follows missing-worktree repair.
- A present but misaligned checkout is **checkout mismatch** with its own
  status explanation and Repair adoption path.
- A refresh that aligns observed checkout with intent clears mismatch without
  changing intent.
- Missing-path repair ignores stale `current_branch` evidence and plans from
  expected-branch existence (`branch_exists`) instead.

Agent runtime snapshots written by the Ajax launch wrapper are trusted process
evidence for terminal exit (`done`/`failed`) and for process liveness only.

Native client hooks and the launch wrapper feed a **canonical agent-event
contract** (facts, not display statuses). Per-client adapters identify what
happened (`TurnStarted`, `ActivityStarted`/`Finished`, `AttentionRequested`,
`TurnSettled`, child lifecycle, heartbeat, session open/close). They do not
choose Running / Waiting / Idle / Error. One helper (`ajax-cli __agent-event`)
ingests stdin native JSON under wrapper identity env (or, for Cursor, a
cwd-index entry published by `__agent-runtime` and keyed by
`CURSOR_PROJECT_DIR` / `workspace_roots`, plus `sessionStart` session `env`
echo-back) and appends a versioned
event envelope; Ajax folds the log into an orthogonal per-run snapshot
(liveness, phase, activity, blocker, outcome, open children/tools/attention)
and projects operator status. Capability profiles mark which facts each client
can supply (`native` / `wrapper` / `unavailable` / `unverified`); absence of an
event must never be treated as absence of a state. Concurrent tools and
subagents use open sets, not last-event-wins. Hooks append versioned JSONL;
`notify.sock` is best-effort transport only — when a listener is bound it
accepts and drains lines with bounded reads but does not yet fan out immediate
status delivery to Cockpit. Durable operator status comes from folding the JSONL
log on runtime refresh.

Native hooks are the primary agent-status evidence. There is one structured
source: the canonical JSONL event log folded per run. `ajax-cli`'s
`AgentStatusSource` reads only the two files Ajax writes per task — the event
log (`agent-events/{stem}.jsonl`) and the launch-wrapper runtime snapshot
(`agent-runtime/{stem}.json`) — and yields reducer-ready `StatusObservation`s
directly to core; there is no status-string round-trip, no pane-text inference,
and no legacy `~/.cache/tmux-agent-status` or scalar `{stem}.json` reads.
Uninstrumented sessions project no confident activity beyond prior state,
process liveness, and confirmed wrapper exit (`done`/`failed`). When sources
disagree, the single reducer (`agent_status::reduce_agent_status`) applies this
precedence:

1. Terminal process exit or fatal runtime error (confirmed wrapper exit, 120s)
2. Structured native lifecycle events folded from the JSONL log (attention and
   open activities persist until cleared or session end; non-terminal phases
   expire after a generous window; terminal outcomes persist until superseded)
3. Process liveness (wrapper `Starting`/`Running`, 30s) — informational only;
   never alone becomes `AgentRunning`

Confirmed wrapper exit is a terminal fallback where native evidence is absent:
`Starting`/`Running` yield only liveness, never activity, and an `Exited*`
observation can only exist once the supervised process has actually ended.
Missing substrate stays authoritative over activity candidates. Ambiguous or
contradictory fresh evidence projects `Unknown`. Parent and delegated runs are
aggregated as a run graph: a parent is not fully complete while non-detached
descendants remain active. Equal-timestamp conflicts across sources on the same
run project `Unknown`; malformed values never participate.

See `.planning/agent-plans/canonical-agent-events.md` for the envelope schema,
client mapping matrix, and migration phases.

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
  without mutating the registry. Start planning uses fresh origin-fetch
  evidence to skip redundant remote fetches when it is recent enough, and the
  task-session launch shell folds husky/bootstrap setup into the agent launch
  line rather than serializing them as standalone critical-path commands.
- Start operation execution records the task, applies named provisioning steps,
  records step receipts for successful provisioning side effects, marks
  provisioning failure in core with failed-step metadata, and opens the task
  after successful setup.
- Single-task command operations plan and execute `resume`, `review`, `repair`,
  and `ship` from core. CLI and Cockpit provide runner and rendering adapters;
  core owns post-execution reducers such as opened, merged, repair/check
  succeeded, and merge/check failure state. When checkout mismatch is present
  (worktree exists, checkout misaligned), Open/Resume, Check, and Review remain
  available; Review diffs `base...HEAD` at the worktree path. Ship and
  Drop/Cleanup are blocked until reconciliation. Repair on mismatch offers a
  zero-command, confirmation-required `BranchAdoptionPlan` carrying the exact
  expected/observed branch pair; core revalidates that pair at execution, updates
  only task branch intent, records a substrate-change event, and preserves task
  identity, path, session, lifecycle, and history. Adoption runs no
  branch-switch command. Detached checkout cannot be adopted; the operator must
  switch to a named branch externally and refresh to clear mismatch without
  changing intent. CLI and Cockpit adapters display the core-provided pair in
  confirmation prompts, retain it between activations, and resubmit it
  unchanged; core rejects stale or altered evidence.
- Drop operation planning starts from fresh substrate observation and produces
  `DropOp`s from observed resources rather than cached registry fields alone.
- Confirmed worktree teardown renames the worktree into a sibling
  `.ajax-trash` entry, prunes, and deletes in the background; `tidy` sweeps
  stale trash entries left behind by interrupted cleanup.
- Drop execution runs teardown ops, records step evidence, re-observes external
  resources, records receipts for successful or already-satisfied cleanup steps,
  and decides `Removed` versus `TeardownIncomplete` from the final observation
  inside core.
- Sweep cleanup (`tidy`) is a batch operation that plans safe cleanup
  candidates, executes each candidate, sweeps stale `.ajax-trash` entries per
  worktree root, marks completed cleanup state, and reports whether an error
  happened after partial state changes. With `--orphans` / `--orphans=ajax`, tidy
  also plans and (when confirmed with `--execute --yes`) force-removes
  unregistered Ajax-shaped leftovers: local `ajax/*` branches and `ajax-*`
  worktrees under the legacy sibling `*__worktrees/` directory or configured
  worktree root. `--orphans=all` also removes unregistered foreign sibling
  worktrees (still never force-deletes non-`ajax/*` branches; skips a `main`
  worktree basename).

Command modules still expose substrate-oriented planning helpers. Task
operations compose those helpers into vertical operator transactions.

## Core Architecture

### Vertical Slices

Ajax is a modular monolith: dependency boundaries point inward, while mutating
feature work is organized around operator capabilities. A slice is a vertical
use-case module inside its owning crate, not a new crate and not a cosmetic
facade over unrelated layered code.

`ajax-core::task_operations` is the core slice layer. Each operator verb is a
file-backed submodule — `start`, `task_command` (resume/review/repair/ship),
`drop_task`, and `sweep_cleanup` — plus `kernel` for shared execution
plumbing. Slice names use operator language, not substrate language such as
Git diff, tmux attach, or process cleanup. `ajax-web::slices` is the sibling
slice layer for browser capabilities.

Each operation slice follows one contract: `plan_*` functions are pure — fresh
evidence in, command plan out, no registry mutation; `execute_*` functions own
external effects and step receipts; post-execution state decisions live in
private reducer functions inside the slice. Slices must not import sibling
slices, with one documented exception: `sweep_cleanup` composes `drop_task`
teardown because tidy sweeps what drop leaves behind.

Slices may depend on the shared kernel: domain models, lifecycle rules, live
status, policy, output contracts, registry traits, and command-spec ports. The
kernel is layered by authority tier — intent, events, observations, projection
— and is never sliced; slicing it would create a second source of task truth.
Every type belongs to exactly one tier, and mutation flows downward: operations
write intent and events, refresh writes observations, and projections are
always derived. Mechanisms remain outside slices: filesystem, terminal, JSON,
subprocess, Git, tmux, networking, SQLite, and process supervision stay in
`adapters`, `registry/sqlite`, `ajax-web`, or `ajax-supervisor`, depending on
the external boundary. CLI, TUI, and browser code are composition and
presentation layers; they consume projections and typed output contracts, not
`Task` internals, and they do not reach into private slice functions.

Hand-rolled architecture tests in each crate's `architecture.rs` enforce slice
isolation and the operation entry-point shape. New operator verbs start as a
new `task_operations` submodule following the same contract; new core items
default to `pub(crate)` unless they are part of a consumed contract.

### Registry

The registry stores Ajax task state and typed task events. It exposes typed
tasks and events to command, output, CLI, and Cockpit boundaries.

Durable registry state is backed by SQLite through `SqliteRegistryStore`.
Transient and test state use `InMemoryRegistry`.

SQLite is the fast read model for Ajax task state. Schema version 9 stores the
registry into focused tables: `registry_tasks` stores durable task intent;
`registry_task_workflow` stores lifecycle, agent runtime status, activity
timestamps, and attention acknowledgment; `registry_task_live_status` stores
the optional live-status kind, summary, and observation timestamp;
`registry_task_runtime_projection` stores reduced runtime health, source,
observed-at, and optional probe error; `registry_task_git_evidence`,
`registry_task_tmux_evidence`, and `registry_task_window_evidence` store the
cached substrate observations; and `registry_events`, `step_receipts`, and
`registry_meta` keep typed history, operation evidence, and revision state.
Both workflow timestamps and observation timestamps use nullable typed
seconds/nanoseconds columns with strict pair validation. `migrate_v7_to_current_schema`
renames the wide v7 task table, copies the data into the normalized tables, and
drops the temporary legacy table in one migration pass. Older migrations still
remain available for databases created before v7, and concurrent acknowledgment
and live-status edits to the same task still surface an explicit revision
conflict rather than a silent overwrite. Git and tmux still own live substrate
reality; Ajax reconciles their observations into SQLite so Cockpit, command
planning, and JSON output can read one coherent task record. Loading legacy
rows normalizes workflow `Waiting` into an active lifecycle with waiting
runtime evidence, and normalizes legacy `Unknown` sentinels into explicit
not-observed evidence.

Registry ghosts are tasks that should not survive SQLite save/load and should
not appear in Cockpit. `ajax-core::ghost_task` is the single classifier for that
decision. Persistence (`registry/sqlite`), Cockpit projection, and visibility
all consult the same rule. Recoverable missing-substrate tasks in operational
lifecycles remain persisted with their events and step receipts. Only
`Removed`, `Stale`, or abandoned provisioning records with no recoverable Git
substrate are pruned as ghosts.

### Lifecycle

Lifecycle state is modeled in `ajax-core::lifecycle`. Lifecycle answers where
the task is in the operator workflow; it does not encode transient agent
attention. Task operations and trusted process terminal events request
lifecycle transitions through the lifecycle boundary. Ordinary pane text,
hooks, prompts, blockers, probe failures, and missing-resource observations
update runtime evidence and attention without changing lifecycle. A trusted
wrapper completion may move an active task to `Reviewable`; waiting or blocked
runtime evidence leaves it `Active`.

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

Managed repos may optionally run a detached `graphify_update` shell command from
each new task worktree after `git worktree add` (for example `graphify extract
--update`). Each task generates its own ignored `graphify-out/` knowledge graph
without committing generated graph data. `ajax doctor` warns when
`graphify-out/` is not gitignored in a repo that configures `graphify_update`.

Runtime reconciliation lives in `runtime`. It compares expected task runtime
state with observed Git, tmux, and task-window evidence, then produces a single
runtime health verdict such as healthy, missing worktree, missing session,
missing task window, wrong task-window path, or unobservable. UI and action
selection consume that verdict instead of reinterpreting individual substrate
fields.

Runtime refresh lives in `runtime_refresh`. It probes Git and tmux, reconciles
runtime evidence, refreshes cached annotations, and recovers missing task
records from observed Ajax worktrees. Core also accepts a small external
agent-status cache port; adapters merge hook-backed status files with Ajax agent
runtime snapshots, attach source/time/freshness metadata, and core reduces the
newest fresh value into a live observation. Probe command failure preserves the
last known substrate value and records an explicit observation error; it never
pretends that a resource was observed missing. Cockpit invokes runtime refresh
through the CLI adapter but does not own the refresh algorithm.

#### Runtime refresh and registry persistence

Ajax keeps one operator-facing task model, but three boundaries apply different
rules:

- **In-memory registry** — authoritative for the running CLI or web process
  between SQLite reloads.
- **SQLite persistence** — stores durable operator intent. Active tasks with
  credible git worktree evidence persist even when tmux/ task window substrate is
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

`refresh_runtime_context_with_tier` also observes GitHub PR checks through
`CommandRunner` and `adapters::github::GithubChecksAdapter`. The adapter runs
`gh pr checks --json name,state,link`. Failed checks reduce to
`LiveStatusKind::CiFailed` with summary `ci failed: <check>`, distinct from
local `check failed` evidence.
Passing or pending checks clear GitHub-sourced CI evidence and drop `TestsFailed`
unless the live status is a local check failure (`CiFailed` with summary
`check failed`). Probes are rate-limited by the per-task `ci_checks_probed_at`
metadata timestamp: 30 seconds while the live status is a GitHub-sourced CI
failure (`ci failed: …`), otherwise 300 seconds, shared by Live and Full tiers. Unobservable probes
(missing `gh`, auth failure, or no PR) record `ci_probe_error` metadata and
never project the task to Error. Notification dedup keys on operator status
class only (`Waiting` / `Error`), so explanation churn inside one class stays
one episode; a class change (Waiting→Error) re-fires.

### Live Status

`agent_status` is the single agent reducer: it maps observations (source,
freshness, confidence, `run_id` / `parent_run_id`, and parent-phase
aggregation) onto one `LiveObservation`. Runtime refresh feeds it the folded
native `RunSnapshot` observations (via `observations_from_run_snapshot`) plus
the confirmed wrapper exit / liveness, and applies the result directly — the
prior string-candidate arbitration reducer is gone. `LiveStatusKind` remains
the presentation projection. `live.rs` keeps only `reduce_live_observation`
(supervisor/application status folding) and the `apply_*` writers.

`live.rs` (`application` submodule) applies reduced observations to task state, agent status,
side flags, activity timestamps, visible live status, and the live evidence's
own durable `observed_at` timestamp. The application path
separates ordinary observations from trusted wrapper/supervisor observations so
only the trusted path may advance lifecycle on process start or successful
completion. Confirmed stop or missing runtime records `Dead`. Uninstrumented
sessions without hook or lifecycle evidence preserve prior credible state;
process liveness alone never fabricates `AgentRunning`.

Trusted wrapper/hook evidence applies immediately. Trusted wrapper completion
advances lifecycle to `Reviewable` only when the run-graph aggregation reports
the parent as fully completed (no active non-detached descendants).

Attention webhooks (`attention::take_attention_transition`) fire on actionable
Waiting and Error operator status after a shared 15-second confirmation dwell
(`NOTIFY_CONFIRMATION_DWELL`) that applies to all actionable attention — a
Waiting→Error flap mid-dwell does not restart the clock. Actionable Waiting is allowlisted to structured
wait/ask explanations only (`Waiting for input`, `Waiting for approval` from
Claude `Notification`, Codex `PermissionRequest`, and legacy provider hook files
that write `wait`/`ask`). Cursor and Pi have no native wait/ask hook today —
they still notify on Error-class evidence (CI/wrapper/substrate). Auth required,
context waits, lifecycle review, rate limits, response-ready settle, and parent
phases that wait on delegated children remain visible as Waiting but do not
phone-ping. Ordinary user waits and approvals still notify once the dwell
confirms sustained attention.

Opening a task persists an attention acknowledgment without changing lifecycle
or deleting evidence. `live::acknowledge_attention` is agent-neutral:
waiting or completion evidence is suppressed only when its durable
`observed_at` is at or before the acknowledgment. Newer same-kind evidence is
accepted and becomes visible normally. Acknowledgment never clears failures,
missing substrate, flags, agent state, or live status, and it never fabricates
shell/process state. Reviewable and mergeable lifecycle also remain intact so
their valid Review or Ship capabilities survive acknowledgment.

Web Cockpit terminal input is a second attention acknowledgment source. The PTY
adapter (`ajax-web::adapters::terminal_pty`) reports only validated input
frames, binary or JSON `input`, through an injected sink; it never mutates
registry or core state. The runtime bridge
(`RuntimeBridge::acknowledge_operator_input`, implemented by the CLI backend)
calls core `mark_task_opened_at` and persists, coalescing per episode by
re-acknowledging only when live evidence is newer than the last acknowledgment.

Agent-deck inspired this status model, but Ajax retains its own lifecycle,
substrate, task-operation, and operator-projection boundaries.

`ui_state::derive_operator_status` is the single operator-facing projector over
lifecycle, expected runtime substrate, GitHub status, the native hook-derived
phase, and acknowledgment. It emits `Running`, `Waiting`, `Idle`, `Error`, or
`Unknown`, plus an optional explanation. Precedence: `TeardownIncomplete` is
always `Error`; terminal/cleanup lifecycle decides whether substrate is still
expected, so a missing tmux session, task window, worktree, or branch is
`Error` only while the lifecycle expects those resources; relevant GitHub
failure or conflict is `Error` and pending checks are `Running` ("CI running"),
while passing checks clear the override and reveal the native phase; otherwise
the native phase applies, with confirmed wrapper exit as a terminal fallback;
and a task no source can prove is `Unknown`. Cleanup/terminal lifecycles
(`Merged`, `Cleanable`, `Removing`, hidden `Removed`) stay idle unless current
error or running evidence overrides them.

Lifecycle remains workflow authority. Annotations remain typed attention and
diagnostic evidence. Operation eligibility and action policy remain capability
authority. Cockpit inbox membership is derived from canonical `Waiting` and
`Error` status, while Review, Ship, Drop, and remediation availability continue
to follow lifecycle, operation eligibility, and policy. CLI and Native Cockpit
consume the canonical pair directly. Compatibility CLI JSON may retain
annotation-based `needs_attention`, but it is not derived from a second
UI-state reducer.

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
- `commands/task_window.rs`
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
- `agent_status_cache` implements core's `AgentStatusSource`: it reads the
  canonical JSONL event log and the launch-wrapper runtime snapshot and yields
  reducer-ready `StatusObservation`s; core owns authority reduction. It performs
  no legacy `tmux-agent-status`, pane, or scalar-snapshot reads.
- `agent_runtime` owns the hidden `__agent-runtime` launch wrapper. Normal task
  start commands run the selected agent through this wrapper, which preserves
  inherited terminal I/O while atomically writing the latest starting/running/
  exited snapshot and appending runtime history under the selected runtime
  cache directory.
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

Web Cockpit is a first-class browser operator surface that is dashboard-first,
with an authenticated raw xterm.js/tmux terminal bridge for existing Ajax task tmux
sessions. Native Cockpit and Web Cockpit consume shared Cockpit projections and
task-operation contracts; neither surface owns task truth. The browser
experience should lead with task state, required decisions, and next actions,
then open the embedded raw terminal for the selected task on both mobile and
desktop. The browser submits only an Ajax task handle; `ajax-web` resolves that
handle to the registered `tmux_session` and attaches to the fixed ` task window`
target. The browser must not accept raw tmux target names or make pane captures,
snapshot viewers, key-send endpoints, or answer routes the default task
interaction path.

The browser shell is not an offline-first Ajax client and must not introduce a
second browser-side task model. Git, tmux, SQLite, supervised processes, and
the Ajax backend remain authoritative for task state and operations. The
primary iPhone target is normal iOS Safari. Web Cockpit does not ship an
installable PWA surface; it has no manifest, service worker, or app icon routes.

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
the HTML shell, the boot client JavaScript (`app.js`), the deferred terminal
chunk (`terminal.js`), and the stylesheet from that directory.
`ajax-web::runtime` owns HTTP transport wiring, local TLS setup, and shell asset
delivery.
`ajax-web::adapters::browser_session` owns browser-session token persistence,
cookie formatting, `Set-Cookie` application, and request-cookie matching.
`ajax-cli` remains a thin native bridge: it resolves stable/dev context paths,
reloads and saves the authoritative SQLite state, and delegates native command
execution for browser-submitted actions.

Manifest, service-worker, icon, offline-cache, push, and Home Screen install
surfaces are unsupported. The browser shell must remain a live same-origin
client for the host-native backend.

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
cached operational/API data. No browser WASM runtime asset is currently shipped;
the shell must not add Yew, Trunk, or a large frontend architecture unless the
project explicitly adopts those elsewhere.

Stable and dev runtime profiles remain separated by the host-native
`ajax-cli web` process and explicit runtime context. Stable uses the stable
state database and default web port, while dev uses the development state
database and dev web port. The browser shell must not merge profile state in
browser storage.

Browser notifications are out of scope. Ajax Web Cockpit must not implement Web
Push, PushManager flows, Notification API prompts, VAPID keys, push
subscriptions, service-worker push handlers, notification click handlers, or
notification infrastructure. Server-side webhook delivery through the CLI
notify adapter (`[notify]` config) is the supported notification channel; the
web runtime only hosts its background poll.

The notify adapter fires once per actionable episode and only for statuses
the operator can act on. Actionable Waiting is allowlisted to `Waiting for
input` / `Waiting for approval` (structured hooks/lifecycle events); all other
Waiting explanations stay inbox-visible but silent. `Error`-class evidence
(CI failed, merge conflict, command failed, blocked, runtime probe failure)
each fire a single webhook after the same shared 15-second confirmation dwell
(`NOTIFY_CONFIRMATION_DWELL`) for every actionable status. Transient `Rate limited` Waiting,
lifecycle-only "Ready for review", turn-settled "Response ready" (`Done` from
Cursor `stop` / Claude·Codex·Pi settle), and auth/context waits do **not**
phone-ping — Cursor has no native wait/ask, so settle must not look like
actionable attention. Episode dedup is status-class only; the webhook body still
includes the agent client and explanation
(`repo/handle: Waiting (codex) — …`). Delivery stays on CLI/cockpit refresh
and the web background tick — hooks only write event files and must stay
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
  When `[notify]` is configured it also spawns a background notify tick that
  reuses the `/api/cockpit` refresh path (same single-flight lock, cache TTL,
  and revision-checked commit) so attention webhooks fire without a browser
  polling; the interval comes from `[notify] poll_seconds`. The tick skips
  webhook delivery while a browser has polled `/api/cockpit` within the last
  90 seconds. Presence is refreshed not only by `/api/cockpit` polls but also
  by recent terminal-WebSocket attaches and operate/action requests that pass
  their origin/JSON-parse gates, so the tick stays suppressed while the operator
  is actively using the PWA terminal or submitting actions.
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
browser-executable action metadata. Unsupported actions, legacy UI states, and
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

Opening a task in the browser is the resume gesture: entering a task route
dispatches the `resume` operation (acknowledging attention through core, exactly
like Enter in the native Cockpit) before attaching the terminal. The browser
renders no separate resume control; the implicit open=resume acknowledgment is
best-effort and never derives task truth in JavaScript. Confirmed operator
actions must echo the exact `branch_adoption` plan core attached to the action;
the slice forwards that payload to core without recomputing branch policy or
comparing branches in the browser.

### `ajax-web::slices::install`

Owns the browser shell. It serves the HTML shell, the boot client JavaScript
(`app.js`), the deferred terminal chunk (`terminal.js`), and the stylesheet. It
must not serve a web manifest,
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
WebSocket route. It builds attach commands only from registered task evidence,
forwards terminal I/O over bounded WebSocket frames, and closes the PTY when
the browser socket disconnects. Browser task terminal WebSocket upgrades require
a same-origin `Origin` that matches the request `Host` in addition to the
normal protected-route session and Cloudflare Access checks.

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
state. `/api/cockpit` adds a short refresh TTL and single-flight gate so
near-simultaneous polls reuse the same refreshed projection, and task mutations
invalidate that window. Terminal bridge cleanup and substrate probes are
bounded so browser disconnects, pane probes, or slow external commands do not
starve lightweight routes. Supervisor cancellation terminates and awaits the
child process before reporting completion, with a bounded wait.

The process-local `OperationCoordinator` is an intentional single-operator
ceiling: only one mutation may be in flight at a time, and per-task mutation
concurrency is outside this alignment and deferred until task-granular
commit semantics exist and measurement justifies it.

## Cockpit Architecture

Cockpit is the operator surface over the JSON-backed command boundary.

`ajax-tui` owns native terminal interaction and rendering.

`ajax-web` owns browser interaction and rendering. Native Cockpit and Web
Cockpit are sibling presentation adapters over shared core projections and
actions; neither surface owns task truth. `ajax-tui` must not know about HTTP,
TLS, browser shell assets, or static web assets.

Web Cockpit serves HTTPS so browsers treat it as a secure context. On first run
it generates a self-signed certificate and persists it beside the state
database; the operator trusts it once on the browser device. HTTPS support does
not imply Home Screen installation, service-worker, or notification support.

Native Cockpit starts `ajax-cli web` by default and keeps it alive for the
Cockpit session. `ajax-cli` starts Web Cockpit on port `8787` with the stable
state database, while `ajax-cli dev` starts it on port `8788` with the
development state database. `--no-web` disables Web Cockpit startup. The web
process is started with explicit `AJAX_PROFILE`, `AJAX_CONFIG`, `AJAX_STATE`,
and rooted worktree values from the selected Ajax context so stable and dev
browser sessions stay on their own runtime profile.

- `actions` owns action and annotation chrome metadata.
- `cockpit_state` owns view state, selectable construction, transitions,
  refresh application, short-lived cockpit response caching, flash state, and
  confirmations.
- `input` owns terminal-event classification.
- `layout` owns pure layout calculations.
- `navigation` owns key classification helpers.
- `rendering` owns status palette, glyph mapping, and screen rendering.
- `runtime` owns terminal mode, polling, refresh timing, the cockpit refresh
  cache window, and the event loop.

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

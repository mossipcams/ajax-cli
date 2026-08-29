# Core Subsystems

Registry, lifecycle, substrate evidence, and live status.

Shared-kernel admission and layer rules live in root `architecture.md`.

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

`refresh_runtime_context_with_tier` also observes GitHub PRs and checks through
`CommandRunner` and `adapters::github::GithubChecksAdapter`. The existing
`ajax_pull_requests` / `PullRequestRef` metadata is the only task-to-PR
association. Full refresh discovers the open branch PR every 300 seconds and
runs `gh pr checks <number> --json name,state,link` only for that associated
open PR when `checks_due` allows: a minimum 10-second gap while pending or
failed, and every 300 seconds after a stable pass. Probes run on
`RefreshTier::Full` only; web `/api/cockpit` is Live. The web background tick
runs Full refresh (including CI probes) and attention delivery every 30 seconds
on the same tick before notifying. A changed head SHA starts a new attempt;
closed or merged PRs retire the attempt.

Failed checks reduce to
`LiveStatusKind::CiFailed` with summary `ci failed: <check>`, distinct from
local `check failed` evidence.
Passing or pending checks clear GitHub-sourced CI evidence and drop `TestsFailed`
unless the live status is a local check failure (`CiFailed` with summary
`check failed`). Probes run on `RefreshTier::Full` only, not `RefreshTier::Live`.
Unobservable probes
(missing `gh`, auth failure, or no PR) record `ci_probe_error` metadata and
never project the task to Error. The CI attempt reducer stores all failed check
names, links, and available run/check identities in deterministic order. It
starts a failed episode and exposes transport-neutral `AgentNotification::CiFailed`
as soon as at least one check is terminally failed, even while sibling checks on
the same attempt are still pending. It emits once per failed episode. A rerun can
emit once more only after a pending transition and a distinct available identity;
incremental completion within one episode does not re-fire.

### Live Status

Run-state ownership is three symbols — no fourth path:

| Layer | Symbol | Module |
| --- | --- | --- |
| Observation → live | `reduce_agent_status` | `agent_status` |
| Writer | `apply_reduced_observation` | `live_application` (via `live::apply_*`) |
| Operator projector | `derive_operator_status` | `ui_state` |

`reduce_agent_status` is the single agent reducer: it maps observations (source,
freshness, confidence, `run_id` / `parent_run_id`, and parent-phase
aggregation) onto one `LiveObservation`. Runtime refresh feeds it the folded
native `RunSnapshot` observations (via `observations_from_run_snapshot`) plus
the confirmed wrapper exit / liveness, and applies the result through the
`live::apply_*` writers — the prior string-candidate arbitration reducer is
gone. `LiveStatusKind` remains the presentation projection. `live.rs` keeps only
`reduce_live_observation` (supervisor/application status folding) and the
`apply_*` entry points.

`live.rs` (`application` submodule) applies reduced observations to task state,
agent status, side flags, activity timestamps, visible live status, and the live
evidence's own durable `observed_at` timestamp. **`apply_reduced_observation` is
the sole live-apply writer** of `agent_status`, agent side flags, visible live
status, and attempt sync (`sync_open_attempts`). Three public entry meanings
converge on it:

| Mode | Entry | Reduction | Lifecycle |
| --- | --- | --- | --- |
| Ordinary | `apply_observation` / `_at` | Yes — `reduce_live_observation` against the stored live row | No |
| Authoritative | `apply_authoritative_observation` / `_at` | No — host-first evidence applied as given | No |
| Trusted | `apply_trusted_observation` / `_at` | No | Yes — `Active` on running-class evidence; `Reviewable` on `Done` |

The non-`_at` helpers are thin `SystemTime::now()` wrappers; the three meanings
stay distinct. Runtime refresh selects the mode from observation source:
`ProcessExit` → trusted, `ProviderLifecycle` → authoritative, otherwise ordinary.

**ACP prohibition:** provisioned chat reports host transitions through
`web_session::session_activity`, which maps each transition to
`ObservationSource::ProviderLifecycle`, runs `reduce_agent_status`, then calls
**`apply_authoritative_observation_at`** on the projection. ACP must not use
trusted apply. ACP `TurnEnded` maps to `LiveStatusKind::Done` between turns of
the same launch; trusted apply would incorrectly mark the task `Reviewable`.
Confirmed wrapper exit remains the trusted path.

Legacy direct writers outside live apply (inventory): SQLite load (`row_codec`),
`mark_resource_missing`, and drop teardown (`mark_drop_agent_stopped`). Stale
running retraction (`runtime_refresh::clear_stale_agent_running`) delegates to
`live::retract_stale_agent_running_at`. No new `agent_status` assignments in
web, CLI, or supervisor production code.

Confirmed stop or missing runtime records `Dead`. Uninstrumented sessions
without hook or lifecycle evidence preserve prior credible state; process
liveness alone never fabricates `AgentRunning`.

Trusted wrapper/hook evidence applies immediately. Trusted wrapper completion
advances lifecycle to `Reviewable` only when the run-graph aggregation reports
the parent as fully completed (no active non-detached descendants).

Attention webhooks (`attention::take_attention_transition`) fire on actionable
Waiting and Error operator status after a shared 15-second confirmation dwell
(`NOTIFY_CONFIRMATION_DWELL`) that applies to all actionable attention — a
Waiting→Error flap mid-dwell does not restart the clock. Actionable Waiting is allowlisted to structured
wait/ask explanations only (`Waiting for input`, `Waiting for approval` from
Claude `Notification`, Codex `PermissionRequest`, Cursor `beforeShellExecution` /
`beforeMCPExecution` hooks plus pane fallback, with Cursor `Notification`
permission/elicitation matchers as best-effort only, and legacy provider hook
files that write `wait`/`ask`). Pi has no native wait/ask hook today — they still notify on
Error-class evidence (CI/wrapper/substrate). Auth required, context waits,
lifecycle review, rate limits, response-ready settle, and parent phases that
wait on delegated children remain visible as Waiting but do not
phone-ping. Ordinary user waits and approvals still notify once the dwell
confirms sustained attention.

`AgentAttempt` rows are **launch-episode history**, not a second run-state
writer and not a chat-turn log. Core opens a row on actual launch
(`AgentCommandSent`; resume only when keys are re-sent). Core closes open rows
only when the launch episode ends: `agent_status` `NotStarted` (never started /
spawn-auth fail), `Dead`, or Drop — via `sync_open_attempts` beside
`AgentAttempt::new`. Rows stay open across ACP `TurnEnded` → `Done` (“Response
ready” between turns), `Waiting`, and `Blocked`. The browser cockpit renders
attempts; it does not open or close them. See GitHub `#1096` `#925`.

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
and a task no source can prove is `Unknown`. The GitHub override yields to an
unacknowledged approval/input gate: a `Running` projection cannot raise
attention, so CI must not mask the operator's only actionable signal. GitHub CI
evidence is also dropped once its probe retires (terminal lifecycle, no branch,
missing worktree) or becomes unobservable, so it never outlives what produced
it. Cleanup/terminal lifecycles
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

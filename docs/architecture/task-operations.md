# Task Operations

Mutable operator transactions, receipts, and per-verb behavior.

Operator slices live under `ajax-core::task_operations` as `start`, `resume`,
`review`, `repair`, `ship`, `drop_task`, and `sweep_cleanup`. See root
`architecture.md` for slice contract and dependency rules.


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
  after worktree/session/agent-send succeed (in-pane husky/bootstrap is not an
  Ajax-blocking setup step). `AgentStartMode::PreparedSession` still creates the
  worktree and detached tmux session, skips agent send-keys, and marks
  lifecycle `Active` without an injected agent CLI.
- Single-task command operations plan and execute `resume`, `review`, `repair`,
  and `ship` from core. CLI and Cockpit provide runner and rendering adapters;
  core owns post-execution reducers such as opened, merged, repair/check
  succeeded, and merge/check failure state. When checkout mismatch is present
  (worktree exists, checkout misaligned), Open/Resume, Check, and Review remain
  available; Review diffs `base...HEAD` at the worktree path (CLI/operate text
  summary). Ship and Clean remain blocked until reconciliation; Drop stays
  available as an escape hatch. Repair on
  mismatch offers a zero-command, confirmation-required `BranchAdoptionPlan`
  carrying the exact expected/observed branch pair; core revalidates that pair
  at execution, updates only task branch intent, records a substrate-change
  event, and preserves task identity, path, session, lifecycle, and history.
  Adoption runs no branch-switch command. Detached checkout cannot be adopted;
  the operator must switch to a named branch externally and refresh to clear
  mismatch without changing intent. CLI and Cockpit adapters display the
  core-provided pair in confirmation prompts, retain it between activations, and
  resubmit it unchanged; core rejects stale or altered evidence.
- Web Diff Review is a separate read-only projection surface (not a mutable
  task operation). Core observes GitHub PRs associated with a task branch,
  merges live `gh` results with durable `PullRequestRef` metadata on the task
  (so a merged PR remains visible after a later PR is opened for the same
  task), and projects structured file/hunk diffs for a selected PR or a local
  `base...HEAD` fallback, plus a deterministic vibe-judgment block (totals,
  signal reading order, and path/hunk heuristic flags). The browser only
  renders that projection; it must not invent PR association, parse `gh`
  output, derive judgment rules, or store a second PR registry.
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
  worktree basename). Drop, clean, and orphan GC also delete matching
  `origin/ajax/*` remote refs when tearing down `ajax/*` branches and prune
  the matching local `refs/remotes/origin/ajax/*` tracking ref so re-observe
  does not treat GitHub-already-deleted heads as still present.

Command modules still expose substrate-oriented planning helpers. Task
operations compose those helpers into vertical operator transactions.

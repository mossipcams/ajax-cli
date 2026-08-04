# Task Authority

Durable task-truth and substrate-evidence rules for Ajax.


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
directly to core; there is no status-string round-trip and no legacy
`~/.cache/tmux-agent-status` or scalar `{stem}.json` reads. Runtime refresh may
also capture the visible pane under AoE-shaped reconcile gates (below) so
mid-turn Working evidence can be corrected to Waiting when permission/input
chrome is on screen. When structured lifecycle evidence is absent, a weaker
capability-gated fallback still applies for clients whose wait facts are
unavailable or unverified. Pane text never invents Running or errors.
Uninstrumented sessions otherwise project no confident activity beyond prior
state, process liveness, and confirmed wrapper exit (`done`/`failed`). When
sources disagree, the single reducer (`agent_status::reduce_agent_status`)
applies this precedence:

1. Terminal process exit or fatal runtime error (confirmed wrapper exit, 120s)
2. Structured native lifecycle events folded from the JSONL log (attention and
   open activities persist until cleared or session end; non-terminal phases
   expire after a generous window; terminal outcomes persist until superseded)
3. Visible-pane permission/input evidence under reconcile gates: when lifecycle
   projects `ActivelyWorking` for Claude, Codex, or Cursor, or when Claude
   projects `FullyCompleted` after prior `AgentRunning` or actionable
   wait (`WaitingForApproval` / `WaitingForInput`) live status, runtime
   refresh may capture the pane and upgrade to Waiting when bottom-anchored
   wait chrome is visible. Soft waits such as `Done` (response ready) do not
   open that gate. Capability-gated pane fallback still applies only
   when structured lifecycle evidence is absent (`Unknown` phase); Unknown
   never clears prior live evidence by applying `LiveStatusKind::Unknown`.
4. Process liveness (wrapper `Starting`/`Running`,
   `PROCESS_LIVENESS_FRESH_FOR` = 30s) — informational only; never alone
   becomes `AgentRunning`

Pane evidence never invents `AgentRunning` from chrome. It may only correct
structured activity toward Waiting under the gates above.

Liveness is supplied separately from observations and is never activity: a fresh
heartbeat rules out `Unknown` (the process demonstrably exists, so the task is
at rest) but can only ever project `Idle`. Refresh stamps
`ui_state::AGENT_PROCESS_ALIVE_KEY` (presence-only marker `"1"`, not a freshness
clock) while the heartbeat is inside its window and removes it once stale, which
keeps `derive_operator_status` a pure projection
with no notion of "now". Confirmed wrapper exit is a terminal fallback where
native evidence is absent: `Starting`/`Running` yield only liveness, never
activity, and an `Exited*` observation can only exist once the supervised
process has actually ended. Missing substrate stays authoritative over activity
candidates. Ambiguous or contradictory fresh evidence projects `Unknown`. Parent
and delegated runs are aggregated as a run graph: a parent is not fully complete
while non-detached descendants remain active. Because every run appends to the
one per-task log, `AgentStatusSource` groups envelopes by `run_id` before
folding and emits one observation per run — a child's events never move the
parent's phase. Malformed values never participate.

See `.planning/agent-plans/canonical-agent-events.md` for the envelope schema,
client mapping matrix, and migration phases.

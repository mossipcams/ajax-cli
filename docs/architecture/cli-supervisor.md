# CLI and Supervisor

Composition shell and supervised agent execution.

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
- `tmux_task_session` owns interactive task PTY entry from Cockpit. Ajax owns the
  foreground task bridge, forwards normal input to the attached tmux client,
  filters Cockpit-owned shortcuts such as Ctrl-Q and Ctrl-T without installing
  tmux bindings, and resumes Cockpit when the task attach client detaches.
  Ctrl-T returns to Cockpit on the create-task screen for the task's project.

Startup behavior should stay inside normal CLI parsing and dispatch. Bare
invocations may choose a default operator surface, and flags may select runtime
profiles, but `main.rs` should not rewrite argv into hidden commands. Public CLI
vocabulary remains operator-facing.

## Native hook agent events

`ajax-cli agent-event` (hidden) translates harness hook payloads into canonical
JSONL under `AJAX_AGENT_EVENTS_DIR`. `run_agent_event` returns typed outcomes:

- `NoIdentity` — no resolved task/run (including missing cwd index); success with
  no write.
- `Ignored` — event not mapped to a canonical kind; success with no write.
- `RejectedByRuntime` — runtime snapshot gate refused the event; success with no
  write.
- `Appended` — one JSONL line was written.

IO or clock failures return `AgentEventError` and fail the hook command with a
non-zero exit — they must not be swallowed as success. Hook installs should treat
write failures as operator-visible (stderr from `run_agent_event_command`).

## Agent watcher

The agent watcher supervises ordinary tasks: it decides whether an agent is still
working toward its task or whether Ajax should nudge it. It needs no `/goal`, no
special mode, and no `ajax supervise`, and it is not part of `ajax-supervisor`.
`ajax web` hosts it (`agent_watcher_runtime.rs`); there is no new daemon.

- **Evidence.** The canonical JSONL stays the durable source. `notify.sock` lines
  only wake the watcher to read the journal; the socket is never a second source
  of truth. Canonical activity events carry a bounded `signature` (tool name plus
  an FNV-1a digest of the canonical JSON of the whole `tool_input` (first 4096
  bytes), never stored raw) and a
  `success` flag so repeated calls can be recognised.
- **Policy.** The pure policy lives in `ajax_core::agent_watcher` (see
  `core-subsystems.md`). Hosted state is per task and bounded; the registry is
  written through `CliRuntimeBridge::refresh_cockpit`, which also publishes
  task frames (objective = task title, harness) to the watcher. Cockpit delivery
  also records the delivery result into the watcher store.
- **Judge.** Ambiguous checkpoints go to an `AgentProgressJudge`. The optional
  implementation (`laya_judge.rs`) drives a persistent Python sidecar
  (`scripts/ajax-laya-sidecar`) around the local Laya decision model. The host
  enforces a timeout; an unavailable, slow, malformed or failing judge means no
  action. Calls are at least 30 seconds apart per task; loop checkpoints fire
  once per repeated-signature episode. Active grace, attention, and operator
  handoff states suppress judgment. Evidence older than five minutes cannot
  justify a judgment or nudge; refresh ticks do not refresh that evidence.
- **Config.** Optional `[watcher]` table: `enabled` (default true), `laya_command`
  (required to start the watcher), `judge_timeout_ms` (default 2000, clamped
  200..=10000). When enabled without a judge command, the host logs once that
  the watcher is idle and uses the plain notification drain listener. No
  watcher journals are read or watcher metadata written in that mode.
- **Delivery.** A nudge becomes `AgentNotification::WatcherNudge` and travels the
  existing cockpit delivery path: the validated tmux path for interactive tasks,
  `TaskSessionDirectory::submit_prompt_with_id` for ACP-backed tasks.
- **V1 limits.** ACP sessions emit no native hook events, so only interactive runs
  are observed (ACP nudge delivery is implemented, detection is not). Events
  in the first journal read rebuild watcher state only, including for a new
  task that reuses an old journal. Events after a restored cursor stamped before
  `ajax web` started also replay as state only; replay never requests a judge
  or queues a nudge.

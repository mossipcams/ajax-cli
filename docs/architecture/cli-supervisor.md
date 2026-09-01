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

# Ajax Architecture

Ajax is a native operator cockpit for isolated AI coding tasks. Cockpit is the
primary operator surface; the `ajax` command, JSON contract, and typed Rust
orchestration library, `ajax-core`, exist to make that surface deterministic,
testable, and scriptable.

## System Boundaries

- `ajax-core` owns task models, orchestration decisions, policy, attention,
  live status projection, command plans, and output contracts.
- `ajax-cli` owns argument parsing, context loading/saving, command dispatch,
  human rendering, JSON rendering, and process execution wiring for Cockpit and
  scripts.
- `ajax-tui` is the primary native Rust operator surface over `ajax-core`
  responses.
- `ajax-supervisor` owns supervised agent execution, process monitoring, and
  translation of live agent/process events into Ajax monitor events.
- External tools remain durable substrates: `git` owns repository truth,
  branches, merges, and worktrees; `tmux` owns durable interactive runtime; and
  agent CLIs remain opaque workers. Ajax owns task lifecycle planning, naming,
  policy, live status projection, and registry state.

## Architectural Direction

Keep the current Rust core plus CLI JSON contract behind Cockpit. This is the
right boundary for a tool that needs deterministic policy, event-driven runtime
state, and scriptable command output while still centering the operator
experience in the native cockpit.

Do not rewrite Ajax into a different application framework. Prefer small
boundary improvements:

- Keep `clap` for the command surface.
- Keep `serde` response structs as the frontend contract.
- Keep Cockpit native to Rust so Ajax has one install/runtime path.
- Keep Ratatui as the current interactive TUI foundation.

## Persistence

The runtime state path is documented as `~/.local/state/ajax/ajax.db`, and the
current durable registry store is SQLite via `SqliteRegistryStore`.

Prefer `rusqlite` for this project because Ajax is local, synchronous, and
Cockpit-first over a CLI/JSON backend. Avoid `sqlx` unless Ajax later needs
async database access or a larger server-style persistence model.

The persistence boundary is:

- Keep `InMemoryRegistry` for tests and transient contexts.
- Keep `RegistryStore` as the load/save abstraction for registry state.
- Back durable state with SQLite tables for tasks, events, and future
  migrations.
- Preserve explicit errors on corrupt, incompatible, or unavailable state.

Legacy JSON state is not migrated. This is a full rewrite of the durable state
format, so pre-SQLite JSON snapshots at the state path should fail with a clear
operator-facing error and can be removed to start with fresh SQLite state.

Registry state records three separate kinds of truth:

- Lifecycle status describes Ajax's current task state.
- Substrate evidence records current external facts such as git, tmux, and
  worktrunk state.
- Operation/live status records what Ajax or a supervised agent is doing now or
  what last failed.

Lifecycle and substrate changes should go through registry-level helpers that
record typed events. SQLite persists typed task fields and event rows directly;
new operation fields or statuses must round-trip through that typed schema and
unsupported schema versions must fail clearly.

## Command Execution

Command planning should stay separate from command execution. `CommandSpec`
should describe what to run, and the runner should decide how to run it.

Ajax needs more than one execution style:

- `CommandMode::Capture` for probes such as `git status` and
  `tmux list-sessions`.
- `CommandMode::InheritStdio` for interactive commands such as
  `tmux attach-session`.
- `CommandMode::Spawn` for detached execution where Ajax should start a
  process without waiting on captured output.

Avoid treating all external commands as captured subprocesses.

Destructive commands may collect the substrate evidence they require at their
own command boundary. For example, cleanup may capture `git status` for the task
worktree before applying cleanup safety policy. That evidence is command-scoped
bookkeeping, not reconciliation: it must not inspect or repair unrelated tmux
state, recreate missing resources, or mutate live status outside the command's
own safety decision.

Merge and cleanup plans must use fresh evidence when their safety decision
depends on git state. Risky or destructive plans must surface confirmation
requirements in the command plan and must not execute without explicit operator
confirmation. Cockpit actions use the same operation policy and confirmation
state as CLI execution.

A partial failure is recoverable state, not disappearance. If an operation
successfully changes registry or substrate state and then a later step fails,
Ajax preserves the completed updates, records visible failure status or
attention, saves durable state, and keeps the affected task visible for
operator recovery.

## CLI Organization

`ajax-cli` should stay thin, but it should not grow into a single catch-all file.
The current `ajax-cli` split is:

- `lib.rs` for the `clap` command tree, parsing, command dispatch, and public
  test helpers.
- `context` for config/state path resolution and load/save behavior.
- `render` for human, JSON, execution-output, and command-plan rendering.
- `snapshot_dispatch` for read-only command routing, state export, and doctor
  path checks.
- `execution_dispatch` for mutable command routing and state-changing execution
  helpers.
- `cockpit_backend` for cockpit snapshots, live refresh, watch mode, and
  interactive TUI backend glue.
- `classifiers` for small operator-facing heuristics that interpret command
  output.

If `lib.rs` becomes difficult to scan, prefer extracting dispatch into a small
module while preserving the public test helpers used by the current suite.

Preserve those helpers unless a task explicitly changes them.

## Cockpit Guidance

Cockpit is the primary operator surface, not the orchestration engine. It
should:

- Call `ajax cockpit --json` or other JSON-backed commands.
- Treat missing or malformed backend data as a recoverable startup/rendering
  issue.
- Present attention, review, safety, and command-plan decisions as first-class
  operator workflows.
- Keep layout behavior tested through layout functions and JSON contracts, not
  brittle source-string assertions.
- Avoid taking dependencies on internal Rust model details outside the JSON
  response schema.

Ratatui is the current interactive TUI foundation because it preserves Ajax's
Rust-only runtime story while keeping orchestration logic in `ajax-core`.

The current `ajax-tui` internals keep small pure helpers out of the main TUI
file:

- `actions` owns action chrome metadata for core `RecommendedAction` values.
- `navigation` owns terminal key classification helpers.
- `rendering` owns status bucket palette and glyph mapping.

## Validation Expectations

Before considering architectural code work complete, run the strongest
applicable checks:

```sh
cargo fmt --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

There is no Python frontend runtime in the supported cockpit path.

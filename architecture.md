# Ajax Architecture

Ajax is a CLI-first orchestration layer for isolated AI coding tasks. The core
product is the `ajax` command and its typed Rust orchestration library,
`ajax-core`; frontends are shells over the same backend contract.

## System Boundaries

- `ajax-core` owns task models, orchestration decisions, policy, attention,
  reconciliation, command plans, and output contracts.
- `ajax-cli` owns argument parsing, context loading/saving, command dispatch,
  human rendering, JSON rendering, and process execution wiring.
- `ajax-tui` is the native Rust cockpit surface over `ajax-core` responses.
- `ajax-supervisor` owns supervised agent execution, process monitoring, and
  translation of live agent/process events into Ajax monitor events.
- External tools remain durable substrates: `workmux` owns task/worktree/session
  lifecycle, `tmux` owns interactive runtime, `git` owns repository truth, and
  agent CLIs remain opaque workers.

## Architectural Direction

Keep the current Rust core plus CLI JSON contract. This is the right boundary
for a tool that needs deterministic policy, testable reconciliation, and
scriptable command output.

Do not rewrite Ajax into a different application framework. Prefer small
boundary improvements:

- Keep `clap` for the command surface.
- Keep `serde` response structs as the frontend contract.
- Keep the cockpit native to Rust so Ajax has one install/runtime path.
- Keep Ratatui as the current interactive TUI foundation.

## Persistence

The runtime state path is documented as `~/.local/state/ajax/ajax.db`, and the
current durable registry store is SQLite via `SqliteRegistryStore`.

Prefer `rusqlite` for this project because Ajax is local, synchronous, and
CLI-first. Avoid `sqlx` unless Ajax later needs async database access or a
larger server-style persistence model.

The persistence boundary is:

- Keep `InMemoryRegistry` for tests and transient contexts.
- Keep `RegistryStore` as the load/save abstraction for registry state.
- Back durable state with SQLite tables for tasks, events, and future
  migrations.
- Preserve explicit errors on corrupt, incompatible, or unavailable state.

Legacy JSON state is not migrated. This is a full rewrite of the durable state
format, so pre-SQLite JSON snapshots at the state path should fail with a clear
operator-facing error and can be removed to start with fresh SQLite state.

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

## CLI Organization

`ajax-cli` should stay thin, but it should not grow into a single catch-all file.
The current `ajax-cli` split is:

- `lib.rs` for the `clap` command tree, parsing, command dispatch, and public
  test helpers.
- `context` for config/state path resolution and load/save behavior.
- `render` for human, JSON, execution-output, and command-plan rendering.

If `lib.rs` becomes difficult to scan, prefer extracting dispatch into a small
module while preserving the public test helpers used by the current suite.

Preserve those helpers unless a task explicitly changes them.

## Cockpit Guidance

The native Rust cockpit is an operator view, not the orchestration engine. It
should:

- Call `ajax cockpit --json` or other JSON-backed commands.
- Treat missing or malformed backend data as a recoverable startup/rendering
  issue.
- Keep layout behavior tested through layout functions and JSON contracts, not
  brittle source-string assertions.
- Avoid taking dependencies on internal Rust model details outside the JSON
  response schema.

Ratatui is the current interactive TUI foundation because it preserves Ajax's
Rust-only runtime story while keeping orchestration logic in `ajax-core`.

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

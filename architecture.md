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
- External tools remain durable substrates: `workmux` owns task/worktree/session
  lifecycle, `tmux` owns interactive runtime, `git` owns repository truth, and
  agent CLIs remain opaque workers.

## Architectural Direction

Keep the current Rust core plus CLI JSON contract. This is the right boundary
for a tool that needs deterministic policy, testable reconciliation, and
scriptable command output.

Do not rewrite Ajax into a different application framework just to simplify the
prototype. Prefer small boundary improvements:

- Keep `clap` for the command surface.
- Keep `serde` response structs as the frontend contract.
- Keep the cockpit native to Rust so Ajax has one install/runtime path.
- Consider Ratatui when the cockpit grows beyond the current text renderer.

## Persistence

The runtime state path is documented as `~/.local/state/ajax/ajax.db`, so the
long-term implementation should use a real SQLite store rather than a JSON
snapshot with a database-looking extension.

Prefer `rusqlite` for this project because Ajax is local, synchronous, and
CLI-first. Avoid `sqlx` unless Ajax later needs async database access or a
larger server-style persistence model.

The intended persistence boundary is:

- Keep `InMemoryRegistry` for tests and transient contexts.
- Add a storage abstraction for loading/saving registry state.
- Back durable state with SQLite tables for tasks, events, reconciliation
  observations, and future migrations.
- Preserve explicit errors on corrupt, incompatible, or unavailable state.

Legacy JSON state is not migrated. This is a full rewrite of the durable state
format, so pre-SQLite JSON snapshots at the state path should fail with a clear
operator-facing error and can be removed to start with fresh SQLite state.

## Command Execution

Command planning should stay separate from command execution. `CommandSpec`
should describe what to run, and the runner should decide how to run it.

Ajax needs more than one execution style:

- Captured output for probes such as `git status` and `tmux list-sessions`.
- Inherited stdio for interactive commands such as `tmux attach-session`.
- Spawned or detached execution for long-running cockpit or agent processes.

Avoid treating all external commands as captured subprocesses.

## CLI Organization

`ajax-cli` should stay thin, but it should not grow into a single catch-all file.
As the command surface expands, split it into modules:

- `cli` for the `clap` command tree and parsing.
- `context` for config/state path resolution and load/save behavior.
- `app` for dispatching parsed commands into core operations.
- `render` for human, JSON, and command-plan rendering.

Preserve the public test helpers used by the current suite unless a task
explicitly changes them.

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

If the cockpit must become a polished interactive TUI, Ratatui is the likely
next step because it preserves Ajax's Rust-only runtime story.

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

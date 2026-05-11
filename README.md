# Ajax

Ajax is a native operator cockpit for isolated AI coding tasks. It is not a
replacement for tmux, git worktrees, workmux, Claude, Codex, or future agent
runtimes. Ajax sits above those tools and tracks what tasks exist, what state
they are in, what needs attention, and which actions are safe to take.

The installed binary is `ajax`. The Rust orchestration library is `ajax-core`.
Cockpit is the primary operator experience; `ajax-core`, the CLI command
surface, and the JSON contracts exist to make that experience deterministic,
testable, and scriptable.

## Install

Ajax is a Rust workspace. Build the local binary with:

```sh
cargo build --release -p ajax-cli
```

The compiled binary is:

```sh
target/release/ajax
```

For local daily use, put that binary on your `PATH` or install it from the
workspace:

```sh
cargo install --path crates/ajax-cli
```

Ajax expects `git`, `tmux`, `workmux`, and an agent CLI such as `codex` to be
available on `PATH`. Run `ajax doctor` after installing to check the local
operator environment.

## Configuration

Ajax reads configuration from `~/.config/ajax/config.toml` unless
`AJAX_CONFIG` points to another file. Runtime state is stored in
`~/.local/state/ajax/ajax.db` unless `AJAX_STATE` points to another SQLite
database path.

Minimal configuration:

```toml
[[repos]]
name = "web"
path = "/Users/matt/projects/web"
default_branch = "main"

[[test_commands]]
repo = "web"
command = "cargo test"
```

Each managed repo should have a matching test command so `ajax check` and
`ajax doctor` can verify the workflow end to end.

## First Run

After installing and writing a config file, start with:

```sh
ajax doctor
ajax repos
ajax tasks
```

Create a task plan before executing it:

```sh
ajax new --repo web --title "fix login" --agent codex
```

When the plan looks right, execute it:

```sh
ajax new --repo web --title "fix login" --agent codex --execute
```

Before changing machines or testing a state migration, export a backup:

```sh
ajax state export --output ~/ajax-state-backup.json
```

Run the deterministic local smoke workflow before release-sensitive changes:

```sh
scripts/smoke.sh
```

## Architecture

Ajax owns orchestration, state, policy, attention, safety, workflow, and the
operator experience. Existing tools keep owning the durable primitives:

- `workmux` owns task, worktree, and session lifecycle.
- `tmux` owns durable interactive runtime.
- `git` owns repository, branch, and worktree reality.
- `worktrunk` is treated as the stable home window inside every task session.
- Claude, Codex, and other agent CLIs are opaque workers running inside task
  environments.

The preferred flow is:

```text
SSH or dev command -> ajax cockpit/CLI -> ajax-core -> workmux/tmux/git/agents
```

Rust owns the orchestration core because safety policy, reconciliation, command
dispatch, and task state benefit from explicit types and testable decisions.
Cockpit owns the operator workflow over those typed decisions.

## Command Surface

The CLI surface backs Cockpit and remains intentionally stable and scriptable:

```sh
ajax repos
ajax tasks
ajax tasks --repo web
ajax inspect web/fix-login
ajax new --repo web --title "fix login" --agent codex
ajax open web/fix-login
ajax trunk web/fix-login
ajax check web/fix-login
ajax diff web/fix-login
ajax merge web/fix-login
ajax clean web/fix-login
ajax sweep
ajax next
ajax inbox
ajax review
ajax status
ajax doctor
ajax reconcile
ajax attach web/fix-login
ajax cockpit
ajax cockpit --watch
```

Commands that feed a UI should support JSON output:

```sh
ajax repos --json
ajax tasks --json
ajax inspect web/fix-login --json
ajax inbox --json
ajax review --json
ajax doctor --json
```

## Native Rust Cockpit

Cockpit is the primary Ajax operator experience. Render it through the `ajax`
command:

```sh
ajax cockpit
```

Cockpit is the place to decide what needs attention, what is safe to do next,
and which command plan should run. It uses a project-first workflow modeled
after the earlier gum flow: choose a project, choose an action, then choose the
task when that action needs one. Project actions include creating a task,
opening or reviewing a task, running checks, viewing diffs, merging, cleaning,
reconciling, and showing project status.

Opening a task from Cockpit attaches to the task's tmux session through Ajax.
While attached, press `Ctrl-q` to detach from the task and return to Cockpit
without typing control text into the agent.

The cockpit remains a Rust operator surface over `ajax-core` command and JSON
contracts. Orchestration logic stays in the core so Cockpit can be tested,
scripted, and recovered without becoming the source of truth.

Use watch mode when you want repeated cockpit frames:

```sh
ajax cockpit --watch
```

## Source And Runtime Layout

Keep source, config, runtime state, logs, cache, managed repos, and task
worktrees separate:

- Source repo: `~/projects/ajax-cli`
- Installed binary: `ajax`
- User config: `~/.config/ajax/config.toml`
- Runtime state: `~/.local/state/ajax/ajax.db`
- Logs: `~/.local/state/ajax/logs`
- Cache: `~/.cache/ajax`
- Managed repos: for example `~/projects/api`, `~/projects/web`, `~/projects/infra`
- Task worktrees: wherever `workmux` already puts them

The `ajax-cli` source repo should not be included in the default managed repo
list at first.

## Repository Structure

```text
ajax-cli/
  Cargo.toml
  crates/
    ajax-core/
    ajax-cli/
    ajax-tui/
```

Planned core modules:

- `config`
- `registry`
- `models`
- `policy`
- `reconcile`
- `attention`
- `commands`
- `adapters`
- `output`

## MVP Phases

Phase 1 builds `ajax-core` and the Rust CLI contract Cockpit will rely on.

Phase 2 makes frontend workflows call `ajax` commands instead of lifecycle
substrates directly.
The native cockpit renders from `ajax-core` responses through `ajax-tui`.

Phase 3 stabilizes JSON output contracts for UI consumption.
Current JSON-backed commands include `repos`, `tasks`, `inspect`, `inbox`,
`next`, `doctor`, and `reconcile`.

Phase 4 makes the native Rust cockpit the primary operator experience over the
same backend. The initial cockpit artifact is the `ajax-tui` crate plus
`ajax cockpit`, which renders an operator dashboard from `ajax-core` responses.
`ajax cockpit --watch` keeps refreshing cockpit frames while still keeping
lifecycle orchestration out of the UI crate.

Phase 5 adds semi-agentic attention, review, and cleanup intelligence.
The first attention layer derives prioritized structured inbox items with
recommended actions from lifecycle state and side flags.

Phase 6 replaces `workmux` internals only if it becomes a real constraint.
No replacement is implemented yet; `workmux` remains the lifecycle substrate
behind the adapter boundary.

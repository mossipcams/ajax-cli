# Ajax CLI

Ajax CLI is a CLI-first orchestration layer for isolated AI coding tasks. It is
not a replacement for tmux, git worktrees, workmux, Claude, Codex, or future
operator dashboards. Ajax sits above those tools and tracks what tasks exist,
what state they are in, what needs attention, and which actions are safe to
take.

The installed binary is `ajax`. The Rust orchestration library is `ajax-core`.
The CLI is the product core; the native Rust cockpit is an operator view over
the same backend instead of the place where orchestration logic lives.

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
SSH or dev command -> ajax CLI/TUI -> ajax-core -> workmux/tmux/git/agents
```

Rust owns the orchestration core because safety policy, reconciliation, command
dispatch, and task state benefit from explicit types and testable decisions.

## Command Surface

The initial CLI surface is intentionally stable and scriptable:

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
ajax repair web/fix-login
ajax next
ajax inbox
ajax review
ajax status
ajax doctor
ajax reconcile
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

Render the native Rust cockpit through the `ajax` command:

```sh
ajax cockpit
```

The cockpit uses a project-first workflow modeled after the earlier gum flow:
choose a project, choose an action, then choose the task when that action needs
one. Project actions include creating a task, opening or reviewing a task,
running checks, viewing diffs, merging, cleaning, repairing, reconciling, and
showing project status. The frontend remains a Rust shell over the same
`ajax-core` command and JSON contracts.

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

Phase 1 builds `ajax-core` and the Rust CLI.

Phase 2 makes frontend workflows call `ajax` commands instead of lifecycle
substrates directly.
The native cockpit renders from `ajax-core` responses through `ajax-tui`.

Phase 3 stabilizes JSON output contracts for UI consumption.
Current JSON-backed commands include `repos`, `tasks`, `inspect`, `inbox`,
`next`, `doctor`, and `reconcile`.

Phase 4 adds a persistent native Rust cockpit over the same backend.
The initial cockpit artifact is the `ajax-tui` crate plus `ajax cockpit`, which
renders an operator dashboard from `ajax-core` responses. `ajax cockpit --watch`
keeps refreshing cockpit frames while still keeping lifecycle orchestration out
of the UI crate.

Phase 5 adds semi-agentic attention, review, repair, and cleanup intelligence.
The first attention layer derives prioritized structured inbox items with
recommended actions from lifecycle state and side flags.

Phase 6 replaces `workmux` internals only if it becomes a real constraint.
No replacement is implemented yet; `workmux` remains the lifecycle substrate
behind the adapter boundary.

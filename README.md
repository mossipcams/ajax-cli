# Ajax

Ajax is a native operator cockpit for isolated AI coding tasks. It is not a
replacement for tmux, git, Claude, Codex, or future agent runtimes. Ajax sits
above those tools and tracks what tasks exist, what state they are in, what
needs attention, and which actions are safe to take.

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

Ajax expects `git`, `tmux`, and an agent CLI such as `codex` to be available on
`PATH`. Run `ajax doctor` after installing to check the local operator
environment.

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
command = "cargo nextest run --all-features"
```

Each managed repo should have a matching test command so `ajax repair` and
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
ajax start --repo web --title "fix login" --agent codex
```

When the plan looks right, execute it:

```sh
ajax start --repo web --title "fix login" --agent codex --execute
```

Before changing machines or testing a state migration, export a backup:

```sh
ajax state export --output ~/ajax-state-backup.json
```

Run the deterministic local smoke workflow before release-sensitive changes:

```sh
scripts/smoke.sh
```

The smoke workflow uses strict fake `git`, `tmux`, and agent tools to validate
the full happy-path journey, state export behavior, and a partial-failure
recovery path where Ajax keeps the task visible with attention.

## Architecture

Ajax owns task lifecycle planning, orchestration, state, policy, annotations,
safety, workflow, and the operator experience. Existing tools keep owning the
durable primitives:

- `git` owns repository, branch, merge, and worktree reality.
- `tmux` owns durable interactive runtime.
- `worktrunk` is treated as the stable home window inside every task session.
- Claude, Codex, and other agent CLIs are opaque workers running inside task
  environments.

The preferred flow is:

```text
SSH or dev command -> ajax cockpit/CLI -> ajax-core -> git/tmux/agents
```

Rust owns the orchestration core because safety policy, live status projection,
command dispatch, and task state benefit from explicit types and testable
decisions.
Cockpit owns the operator workflow over those typed decisions.

## Command Surface

The CLI surface backs Cockpit and remains intentionally stable and scriptable:

```sh
ajax repos
ajax tasks
ajax tasks --repo web
ajax inspect web/fix-login
ajax start --repo web --title "fix login" --agent codex
ajax resume web/fix-login
ajax repair web/fix-login
ajax review web/fix-login
ajax ship web/fix-login
ajax drop web/fix-login
ajax tidy
ajax next
ajax inbox
ajax ready
ajax status
ajax doctor
ajax supervise --task web/fix-login --prompt "implement the approved plan"
ajax cockpit
ajax cockpit --watch
```

Commands that feed a UI should support JSON output:

```sh
ajax repos --json
ajax tasks --json
ajax inspect web/fix-login --json
ajax next --json
ajax inbox --json
ajax ready --json
ajax status --json
ajax doctor --json
ajax cockpit --json
```

## Native Rust Cockpit

Cockpit is the primary Ajax operator experience and native Rust cockpit. Render
it through the `ajax` command:

```sh
ajax cockpit
```

Cockpit is the place to decide what needs attention, what is safe to do next,
and which command plan should run. It uses a project-first workflow modeled
after the earlier gum flow: choose a project, choose an action, then choose the
task when that action needs one. Project actions include starting a task,
resuming or reviewing a task, shipping, dropping, and showing project status.

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
- Task worktrees: sibling directories such as `repo__worktrees/ajax-fix-login`

The `ajax-cli` source repo should not be included in the default managed repo
list at first.

## Repository Structure

```text
ajax-cli/
  Cargo.toml
  crates/
    ajax-core/
    ajax-cli/
    ajax-supervisor/
    ajax-tui/
```

Core modules:

- `config`
- `registry`
- `models`
- `policy`
- `live`
- `attention`
- `commands`
- `adapters`
- `output`

Additional crates keep the boundaries described in `architecture.md`:

- `ajax-cli` owns CLI parsing, command dispatch, context loading, process
  execution wiring, and human/JSON rendering.
- `ajax-core` owns models, registry state, policy, lifecycle decisions, live
  status projection, attention, command plans, and output contracts.
- `ajax-supervisor` owns supervised agent execution and live process status.
- `ajax-tui` owns the Cockpit screen state, input, layout, and rendering.

## Validation

Before release-sensitive changes, run the strongest applicable local checks:

```sh
cargo fmt --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
```

Use `scripts/smoke.sh` for the deterministic end-to-end smoke workflow.

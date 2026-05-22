# Ajax

Ajax is a native operator cockpit for isolated AI coding tasks. It is not a
replacement for tmux, git, Claude, Codex, or future agent runtimes. Ajax sits
above those tools and tracks what tasks exist, what state they are in, what
needs attention, and which actions are safe to take.

The installed binary is `ajax-cli`. The Rust orchestration library is `ajax-core`.
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
target/release/ajax-cli
```

For local daily use, put that binary on your `PATH` or install it from the
workspace:

```sh
cargo install --path crates/ajax-cli
```

Ajax expects `git`, `tmux`, and an agent CLI such as `codex` to be available on
`PATH`. Run `ajax-cli doctor` after installing to check the local operator
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
bootstrap = "npm ci"

[[test_commands]]
repo = "web"
command = "cargo nextest run --all-features"
```

Each managed repo should have a matching test command so `ajax-cli repair` and
`ajax-cli doctor` can verify the workflow end to end.

Set `bootstrap` when a repo needs dependencies or guardrail tooling installed
inside each task worktree before the agent starts. Ajax runs the command from
the newly created worktree after `git worktree add` succeeds and before tmux or
the selected agent CLI are launched.

## First Run

After installing and writing a config file, start with:

```sh
ajax-cli doctor
ajax-cli repos
ajax-cli tasks
```

Create a task plan before executing it:

```sh
ajax-cli start --repo web --title "fix login" --agent codex
```

When the plan looks right, execute it:

```sh
ajax-cli start --repo web --title "fix login" --agent codex --execute
```

Before changing machines or testing a state migration, export a backup:

```sh
ajax-cli state export --output ~/ajax-state-backup.json
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
- Ajax treats a task's tmux window as the stable home window inside that task
  session.
- Claude, Codex, and other agent CLIs are opaque workers running inside task
  environments.

SQLite is Ajax's fast current-state read model. It stores the expected task
runtime, last observed Git/tmux evidence, derived runtime health, and task
events. Git and tmux remain the live substrates; Ajax reconciles their observed
state into SQLite so Cockpit and command planning do not repeat those checks on
every render.

The preferred flow is:

```text
SSH or dev command -> ajax-cli cockpit/CLI -> ajax-core -> git/tmux/agents
```

Rust owns the orchestration core because safety policy, live status projection,
command dispatch, and task state benefit from explicit types and testable
decisions.
Cockpit owns the operator workflow over those typed decisions.

## Command Surface

The CLI surface backs Cockpit and remains intentionally stable and scriptable:

```sh
ajax-cli repos
ajax-cli tasks
ajax-cli tasks --repo web
ajax-cli inspect web/fix-login
ajax-cli start --repo web --title "fix login" --agent codex
ajax-cli resume web/fix-login
ajax-cli repair web/fix-login
ajax-cli review web/fix-login
ajax-cli ship web/fix-login
ajax-cli drop web/fix-login
ajax-cli tidy
ajax-cli next
ajax-cli inbox
ajax-cli ready
ajax-cli status
ajax-cli doctor
ajax-cli supervise --task web/fix-login --prompt "implement the approved plan"
ajax-cli cockpit
ajax-cli cockpit --watch
```

Commands that feed a UI should support JSON output:

```sh
ajax-cli repos --json
ajax-cli tasks --json
ajax-cli inspect web/fix-login --json
ajax-cli next --json
ajax-cli inbox --json
ajax-cli ready --json
ajax-cli status --json
ajax-cli doctor --json
ajax-cli cockpit --json
```

## Native Rust Cockpit

Cockpit is the primary Ajax operator experience and native Rust cockpit. Render
it through the `ajax-cli` command:

```sh
ajax-cli cockpit
```

Cockpit is the place to decide what needs attention, what is safe to do next,
and which command plan should run. It uses a project-first workflow modeled
after the earlier gum flow: choose a project, choose an action, then choose the
task when that action needs one. Project actions include starting a task,
resuming or reviewing a task, shipping, dropping, and showing project status.

The cockpit remains a Rust operator surface over `ajax-core` command and JSON
contracts. Orchestration logic stays in the core so Cockpit can be tested,
scripted, and recovered without becoming the source of truth.

When Cockpit opens a task, Ajax runs a foreground bridge to the task's tmux
session. Normal input is forwarded to tmux. Press `Ctrl+Q` from that bridge to
detach the foreground task client and return to Cockpit. Ajax does not install a
global tmux key binding for this; outside the Cockpit task bridge, tmux keeps
its normal key handling.

Use watch mode when you want repeated cockpit frames:

```sh
ajax-cli cockpit --watch
```

## Stable/Dev Runtime Isolation

Ajax supports runtime profiles so stable daily use and development dogfooding
can run from the same source checkout without sharing state.

Inspect the selected runtime before starting work:

```sh
ajax-cli runtime
ajax-cli --profile stable runtime
cargo run -p ajax-cli -- --profile dev runtime
AJAX_PROFILE=dev cargo run -p ajax-cli -- status
AJAX_HOME=~/.ajax-dev cargo run -p ajax-cli -- runtime
```

The `stable` profile is the default and preserves the existing paths:
`~/.config/ajax/config.toml`, `~/.local/state/ajax/ajax.db`,
`~/.local/state/ajax/logs`, `~/.cache/ajax`, and legacy sibling task
worktrees.

The `dev` profile uses isolated runtime state under `~/.ajax-dev`:
`config.toml`, `ajax.db`, `logs`, `cache`, and `worktrees`. New dev-profile
tasks create worktrees under that runtime worktree root. Existing tasks keep
the concrete worktree paths already stored in their database records.

Use `--home` or `AJAX_HOME` for a fully custom isolated runtime directory.
`--config`, `--state`, `--worktree-root`, `AJAX_CONFIG`, `AJAX_STATE`, and
`AJAX_WORKTREE_ROOT` override profile-derived paths. `ajax-cli runtime --json`
reports those overrides so you can verify which database and worktree root a
command will use.

## Source And Runtime Layout

Keep source, config, runtime state, logs, cache, managed repos, and task
worktrees separate:

- Source repo: `~/projects/ajax-cli`
- Installed binary: `ajax-cli`
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
npm run lint:duplication
```

Use `scripts/smoke.sh` for the deterministic end-to-end smoke workflow.

Releases are managed by Release Please. The repository needs a
`RELEASE_PLEASE_TOKEN` secret so Release Please PRs trigger the real GitHub CI
workflow before they are merged.

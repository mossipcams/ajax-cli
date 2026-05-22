# Ajax

Ajax is a native Rust cockpit for running many isolated AI coding tasks without
losing track of the work. It gives an operator one place to see what is active,
what needs attention, what is ready to review, and which action is safe to take
next.

Use Ajax when you want agents such as Codex to work in separate Git worktrees,
inside durable tmux sessions, while Ajax keeps the task list, live status, and
cleanup path organized. Ajax does not replace Git, tmux, or agent CLIs. It sits
above them as the operator layer.

The installed binary is `ajax`. The primary experience is Cockpit:

```sh
ajax stable
```

## What Ajax Does

- Creates isolated task worktrees from configured repos.
- Starts agent CLIs such as `codex` inside per-task tmux sessions.
- Shows a cross-repo inbox for work that needs operator attention.
- Tracks which tasks are active, review-ready, merged, errored, or safe to
  clean up.
- Lets you resume, repair, review, ship, drop, and tidy tasks from one cockpit.
- Keeps the same state available through stable CLI commands and JSON output
  for scripts, tests, and future UIs.
- Records enough local task history to recover from interrupted provisioning or
  cleanup without treating cached state as more authoritative than Git or tmux.

## Daily Loop

Ajax is built around a project-first workflow: choose a project, choose what you
want to do, then choose a task when the action needs one.

Typical flow:

```sh
ajax stable
```

From Cockpit you can start a task, jump back into an active task, inspect work
that needs attention, review completed work, ship it, or drop stale task
environments.

When native Cockpit starts through `ajax stable` or `ajax dev`, Ajax also starts
the mobile web Cockpit companion. Stable serves on `0.0.0.0:8787`; dev serves
on `0.0.0.0:8788` and uses a separate development state database. Open
`http://<this-machine-ip>:8787` or `http://<this-machine-ip>:8788` from a phone
on the same routed network. Use `--no-web` to keep native Cockpit terminal-only.

The same loop is available from the CLI:

```sh
ajax start --repo web --title "fix login" --agent codex --execute
ajax inbox
ajax resume web/fix-login
ajax repair web/fix-login
ajax review web/fix-login
ajax ship web/fix-login
ajax drop web/fix-login
ajax tidy
```

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
database path. `ajax dev` defaults to `~/.local/state/ajax/ajax-dev.db` and can
be pointed elsewhere with `AJAX_DEV_STATE`; `AJAX_DEV_CONFIG` can point dev at a
separate config file.

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

Each managed repo should have a matching test command so `ajax repair` and
`ajax doctor` can verify the workflow end to end.

Set `bootstrap` when a repo needs dependencies or guardrail tooling installed
inside each task worktree before the agent starts. Ajax runs the command from
the newly created worktree after `git worktree add` succeeds and before tmux or
the selected agent CLI are launched.

## First Run

After installing and writing a config file, check the environment:

```sh
ajax doctor
ajax repos
ajax tasks
```

Open the cockpit:

```sh
ajax stable
```

Start a task from Cockpit, or create a CLI plan before executing it:

```sh
ajax start --repo web --title "fix login" --agent codex
```

When the plan looks right, execute it:

```sh
ajax start --repo web --title "fix login" --agent codex --execute
```

Come back later through Cockpit or the attention queues:

```sh
ajax inbox
ajax ready
ajax status
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

## Native Rust Cockpit

Cockpit is the primary Ajax operator experience and native Rust cockpit. Render
it through the stable or dev Ajax command:

```sh
ajax stable
ajax dev
```

Cockpit uses the project-first workflow: choose a project, choose an action, and
then choose the task when that action needs one. It surfaces the cross-repo
inbox first so work that needs the operator does not disappear inside one repo.

When Cockpit opens a task, Ajax runs a foreground bridge to the task's tmux
session. Normal input is forwarded to tmux. Press `Ctrl+Q` from that bridge to
detach the foreground task client and return to Cockpit. Ajax does not install a
global tmux key binding for this; outside the Cockpit task bridge, tmux keeps
its normal key handling.

Use watch mode when you want repeated cockpit frames:

```sh
ajax cockpit --watch
```

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
ajax stable
ajax dev
ajax cockpit
ajax cockpit --watch
```

Commands that feed a UI support JSON output:

```sh
ajax repos --json
ajax tasks --json
ajax inspect web/fix-login --json
ajax next --json
ajax inbox --json
ajax ready --json
ajax status --json
ajax doctor --json
ajax stable --json
ajax dev --json
ajax cockpit --json
```

## How It Works

Ajax coordinates existing local tools:

- Git owns repository, branch, merge, and worktree reality.
- tmux owns durable interactive sessions.
- Agent CLIs are opaque workers running inside task environments.
- SQLite stores Ajax-owned task intent, task events, runtime evidence, and named
  step receipts.

Ajax observes Git and tmux before deciding what to show or what to do next. The
SQLite database is a fast local record of Ajax task state, not a replacement for
the live substrates. When provisioning, retrying, or cleaning up a task, Ajax
uses fresh substrate observations plus recorded step receipts to avoid repeating
work that already succeeded while still recovering safely from partial failures.

For implementation boundaries, crate ownership, and runtime reconciliation
details, see `architecture.md`.

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

## Validation

Before release-sensitive changes, run the strongest applicable local checks:

```sh
cargo fmt --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features --test-threads=1
cargo test --doc
npm run lint:duplication
```

Use `scripts/smoke.sh` for the deterministic end-to-end smoke workflow.

Releases are managed by Release Please. The repository needs a
`RELEASE_PLEASE_TOKEN` secret so Release Please PRs trigger the real GitHub CI
workflow before they are merged.

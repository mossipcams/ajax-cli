# Ajax

Ajax is a native Rust cockpit for running many isolated AI coding tasks without
losing track of the work. It gives an operator one place to see what is active,
what needs attention, what is ready to review, and which action is safe to take
next.

Use Ajax when you want agents such as Codex to work in separate Git worktrees,
inside durable tmux sessions, while Ajax keeps the task list, live status, and
cleanup path organized. Ajax does not replace Git, tmux, or agent CLIs. It sits
above them as the operator layer.

The installed binary is `ajax-cli`. The primary experience is Cockpit:

```sh
ajax-cli
```

## What Ajax Does

- Creates isolated task worktrees from configured repos.
- Starts agent CLIs such as `codex` inside per-task tmux sessions.
- Shows a cross-repo inbox for work that needs operator attention.
- Tracks which tasks are active, review-ready, merged, errored, or safe to
  clean up.
- Lets you resume, repair, review, ship, drop, and tidy tasks from one cockpit.
- Keeps the same state available through Native Cockpit, Web Cockpit, stable CLI
  commands, and JSON output for scripts and tests.
- Records enough local task history to recover from interrupted provisioning or
  cleanup without treating cached state as more authoritative than Git or tmux.

## Daily Loop

Ajax is built around a project-first workflow: choose a project, choose what you
want to do, then choose a task when the action needs one.

Typical flow:

```sh
ajax-cli
```

From Cockpit you can start a task, jump back into an active task, inspect work
that needs attention, review completed work, ship it, or drop stale task
environments.

### Web Cockpit (PWA)

Web Cockpit is a mobile-first Progressive Web App and full browser operator
surface over Ajax. It runs host-native through `ajax-cli web`, with the same
host authority as SQLite, configured repos, worktrees, tmux sessions, agent
CLIs, and host process state. Docker is no longer part of the Web Cockpit
runtime.

When Native Cockpit starts through `ajax-cli` or `ajax-cli dev`, Ajax also
starts the host-native Web Cockpit server. Stable serves on `0.0.0.0:8787`; dev
serves on `0.0.0.0:8788` and uses the isolated dev runtime profile. Use
`--no-web` to keep Native Cockpit terminal-only.

Persistent Web Cockpit deployments should run `ajax-cli web` under a
host-native supervisor such as launchd, `systemd --user`, tmux, or another
service manager. Ajax does not provide its own daemon manager.

WireGuard or an equivalent private network is the access boundary for Web
Cockpit. Public internet exposure is unsupported. Bind the server to a trusted
interface or restrict access at the network layer before using it from another
device.

Web Cockpit serves HTTPS, which browsers require before they will install a PWA,
run its service worker, or deliver push notifications. Open
`https://<this-machine-ip>:8787` or `https://<this-machine-ip>:8788` from a
browser device on the private network. On first run Ajax generates a self-signed
certificate and stores it beside the state database (`web-tls-cert.pem`); your
browser will warn the first time. To install the app to your home screen and
enable notifications, trust that certificate once. On iOS, open
`web-tls-cert.pem`, install the profile, then enable full trust under Settings,
General, About, Certificate Trust Settings.

From the installed app you can see every repo's tasks, use the attention inbox,
and run browser-capable operations such as `review`, `ship`, `repair`, and
`drop`. Web Cockpit is a triage surface: when an agent stops at a recognized
approval prompt, it shows the command and structured Approve/Deny (or numbered)
choices, and a one-tap answer is routed through the agent adapter as the correct
keystroke. The answer is guarded by a fingerprint of the prompt the operator
saw, so an answer is rejected if the agent has moved on. Anything Web Cockpit
cannot safely structure — free-text composers, low-confidence prompts — is not
given a text box; it escalates to the terminal. Browser `resume` remains
`needs_terminal`.

Tap Alerts to enable Web Push: the phone is then notified when a task newly
needs attention, even when the app is closed. When the waiting task is a
recognized approval, the notification carries Approve/Deny actions you can tap
without opening the app (on platforms that support notification actions; iOS
falls back to opening the task). Web Push on iOS requires iOS 16.4 or later and
the app installed to the home screen.

The PWA renders server-authoritative Cockpit projections and submits typed
operator intents. It does not own offline task mutation state, persist task
operation queues, or replay mutations after reload. The service worker is only
for static app-shell assets and must not cache `/api/*`.

The Web Cockpit HTTP runtime uses Axum inside the host-native `ajax-cli web`
process. Axum is transport only: routing, request extraction, response
construction, static PWA serving, TLS wiring, and future stream/WebSocket
endpoints. Task lifecycle, registry truth, substrate evidence, action policy,
browser DTOs, and operation outcomes remain server-authoritative Ajax
contracts. Bind `--host` to the WireGuard interface address when you want the
Cockpit reachable only on that private network.

The same loop is available from the CLI:

```sh
ajax-cli start --repo web --title "fix navbar" --agent codex --execute
ajax-cli inbox
ajax-cli resume web/fix-navbar
ajax-cli repair web/fix-navbar
ajax-cli review web/fix-navbar
ajax-cli ship web/fix-navbar
ajax-cli drop web/fix-navbar
ajax-cli tidy
```

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
`AJAX_CONFIG` points to another file. Stable runtime state is stored in
`~/.local/state/ajax/ajax.db` unless `AJAX_STATE` points to another SQLite
database path. `ajax-cli dev` uses the isolated dev runtime profile under
`~/.ajax-dev`.

Use `ajax-cli runtime` or `ajax-cli --profile dev runtime` to inspect the
selected config, state DB, logs, cache, and worktree placement before starting
tasks. `AJAX_PROFILE`, `AJAX_HOME`, `AJAX_CONFIG`, `AJAX_STATE`, and
`AJAX_WORKTREE_ROOT` can override profile-derived paths.

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

After installing and writing a config file, check the environment:

```sh
ajax-cli doctor
ajax-cli repos
ajax-cli tasks
```

Open the cockpit:

```sh
ajax-cli
```

Start a task from Cockpit, or create a CLI plan before executing it:

```sh
ajax-cli start --repo web --title "fix navbar" --agent codex
```

When the plan looks right, execute it:

```sh
ajax-cli start --repo web --title "fix navbar" --agent codex --execute
```

Come back later through Cockpit or the attention queues:

```sh
ajax-cli inbox
ajax-cli ready
ajax-cli status
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

## Native Rust Cockpit

Cockpit is the primary Ajax operator experience and native Rust cockpit. Render
it through the stable or dev Ajax command:

```sh
ajax-cli
ajax-cli dev
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
ajax-cli cockpit --watch
```

## Command Surface

The CLI surface backs Cockpit and remains intentionally stable and scriptable:

```sh
ajax-cli repos
ajax-cli tasks
ajax-cli tasks --repo web
ajax-cli inspect web/fix-navbar
ajax-cli start --repo web --title "fix navbar" --agent codex
ajax-cli resume web/fix-navbar
ajax-cli repair web/fix-navbar
ajax-cli review web/fix-navbar
ajax-cli ship web/fix-navbar
ajax-cli drop web/fix-navbar
ajax-cli tidy
ajax-cli next
ajax-cli inbox
ajax-cli ready
ajax-cli status
ajax-cli doctor
ajax-cli supervise --task web/fix-navbar --prompt "implement the approved plan"
ajax-cli
ajax-cli dev
ajax-cli cockpit
ajax-cli cockpit --watch
```

Commands that feed a UI support JSON output:

```sh
ajax-cli repos --json
ajax-cli tasks --json
ajax-cli inspect web/fix-navbar --json
ajax-cli next --json
ajax-cli inbox --json
ajax-cli ready --json
ajax-cli status --json
ajax-cli doctor --json
ajax-cli --json
ajax-cli dev --json
ajax-cli cockpit --json
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
- Installed binary: `ajax-cli`
- User config: `~/.config/ajax/config.toml`
- Stable runtime state: `~/.local/state/ajax/ajax.db`
- Dev runtime state: `~/.ajax-dev/ajax.db`
- Stable logs/cache: `~/.local/state/ajax/logs`, `~/.cache/ajax`
- Dev logs/cache: `~/.ajax-dev/logs`, `~/.ajax-dev/cache`
- Managed repos: for example `~/projects/api`, `~/projects/web`, `~/projects/infra`
- Task worktrees: sibling directories such as `repo__worktrees/ajax-fix-navbar`
- Dev task worktrees: rooted under `~/.ajax-dev/worktrees`

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

Releases are managed by Release Please. If set, `RELEASE_PLEASE_TOKEN` is used;
otherwise the workflow falls back to `github.token` so releases still run.

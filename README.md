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

### Web Cockpit (Safari-first)

Web Cockpit is a mobile-first browser operator dashboard over Ajax. It runs
host-native through `ajax-cli web`, with the same host authority as SQLite,
configured repos, worktrees, tmux sessions, agent CLIs, and host process state.
Docker is no longer part of the Web Cockpit runtime.

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

Web Cockpit serves HTTPS. Open `https://<this-machine-ip>:8787` or
`https://<this-machine-ip>:8788` from Safari on an iPhone connected to the
private network. On first run Ajax generates a self-signed certificate and
stores it beside the state database (`web-tls-cert.pem`); Safari will warn the
first time. On iOS, open `web-tls-cert.pem`, install the profile, then enable
full trust under Settings, General, About, Certificate Trust Settings.

Recommended on iPhone: use a normal Safari tab. Web Cockpit no longer ships a
manifest, service worker, or Home Screen icon surface; the supported browser
path is the live Safari shell. A native app is only a future option if the
browser path stops being sufficient.

From Safari you can see every repo's tasks, use the attention inbox, and run
browser-capable operations such as `review`, `ship`, `repair`, and `drop`. The
main task view is dashboard-first: current status, required decision, best next
action, and recent milestones are primary. When you open a task, the embedded
raw Ghostty/tmux terminal is the default on mobile and desktop. Browser
`resume` uses that authenticated terminal bridge for full interactive attach.

When an agent stops at a recognized approval prompt, Web Cockpit shows guarded
structured actions such as Approve and Deny. The browser sends a typed answer
plus a fingerprint of the prompt the operator saw; the server re-captures the
pane and rejects the answer if the agent has moved on. Free-form input and other
terminal-only interactions use the raw task terminal bridge instead of a browser
composer or read-only snapshot viewer.

Notifications are out of scope. Ajax Web Cockpit does not support Web Push,
PushManager flows, Notification API prompts, VAPID keys, push subscriptions,
service-worker push handlers, notification click handlers, or native/external
notification replacements.

The browser renders server-authoritative Cockpit projections and submits typed
operator intents. It does not own offline task mutation state, persist task
operation queues, replay mutations after reload, or cache operational/API data.
A full page reload should recover the current cockpit state from the server.

The Web Cockpit HTTP runtime uses Axum inside the host-native `ajax-cli web`
process. Axum is transport only: routing, request extraction, response
construction, static browser shell serving, TLS wiring, and future stream/WebSocket
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
graphify_update = "graphify extract --update"

[[test_commands]]
repo = "web"
command = "cargo nextest run --all-features"
```

For this repository, prefer the checked-in task bootstrap so every new
worktree gets Node 22 (CI pin), `npm ci` (husky via `prepare`), and — when
`ajax-model-router` is present locally — the dispatch script symlinks:

```toml
[[repos]]
name = "ajax-cli"
path = "/Users/matt/Desktop/Projects/ajax-cli"
default_branch = "main"
bootstrap = "./scripts/task-bootstrap.sh"
```

Each managed repo should have a matching test command so `ajax-cli repair` and
`ajax-cli doctor` can verify the workflow end to end.

Set `bootstrap` when a repo needs dependencies or guardrail tooling installed
inside each task worktree before the agent starts. Ajax runs the command from
the newly created worktree after `git worktree add` succeeds and before tmux or
the selected agent CLI are launched.

Add an optional `[notify]` block to receive a webhook (for example an
[ntfy](https://ntfy.sh) topic) for actionable attention episodes:

```toml
[notify]
webhook_url = "https://ntfy.sh/your-topic"
# poll_seconds = 30   # background poll interval for `ajax web`; 0 disables
```

Webhooks fire once per episode for `NeedsInput` waiting evidence (waiting
for input, waiting for approval, auth required, rate limited, context limit)
and `Error`-class evidence (CI failed, merge conflict, command failed,
blocked, runtime probe failure). Lifecycle-only "Ready for review" stays
inbox-visible in the Cockpit but does **not** phone-ping. Sustained
`Running`/`Idle` evidence for 30 seconds re-arms the detector so the next
actionable episode delivers one ping; opening a task silences the current
episode so further pings wait for new evidence.

Notifications fire from CLI/cockpit refreshes and from a background poll inside
`ajax web`, so a running web server delivers them even when no browser tab is
open. Webhooks stay quiet while a browser is actively polling Web Cockpit.
`poll_seconds` defaults to 30 when `[notify]` is present.

GitHub CI failure surfacing requires the `gh` CLI installed and authenticated
(`gh auth login`) in the task worktree's repo. When `gh` is missing,
unauthenticated, or the branch has no PR, Ajax keeps the last known task state
and skips CI evidence: no error status, no notification. Checks are polled at
most every 5 minutes per task.

`ajax start` fast-forwards the managed repo's `default_branch` from `origin`
before creating the task worktree so new branches base on current remote `main`
(or your configured default branch).

Set `graphify_update` to generate a knowledge graph in each new task worktree
during start. Ajax runs the configured command from the task worktree after
`git worktree add` and detaches it so graph generation does not block agent
startup. Add `graphify-out/` to the repo's `.gitignore`; `ajax doctor` reports
Graphify-enabled repos where the generated output is not ignored.

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
detach the foreground task client and return to Cockpit. Press `Ctrl+T` from the
bridge or from Cockpit itself to open create-task for the current project (from
inside a task, that is the task's repo). Ajax does not install a global tmux key
binding for these shortcuts; outside the Cockpit task bridge, tmux keeps its
normal key handling.

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
npm run web:check
npm run web:test -- --run
```

Releases are managed by Release Please. If set, `RELEASE_PLEASE_TOKEN` is used;
otherwise the workflow falls back to `github.token` so releases still run.

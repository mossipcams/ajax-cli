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
that needs attention, open Diff Review on a task, ship it, or drop stale task
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
host-native supervisor. On macOS, use the launchd service below. Ajax does not
provide its own daemon manager beyond that.

On macOS, `ajax-cli web service` manages a launchd user agent
(`com.ajax.web.<profile>`) that starts at login and restarts after exits:

```sh
ajax-cli [--profile P] web service install [--host H] [--port N]
ajax-cli [--profile P] web service start
ajax-cli [--profile P] web service stop
ajax-cli [--profile P] web service status
ajax-cli [--profile P] web service uninstall
```

`install` is idempotent: it replaces the job and starts it using the invoking
binary. `stop` stays stopped until `start` or the next login. Logs are in
`~/Library/Logs/ajax-web-<profile>.log`. Dev defaults to port 8788 and stable to
8787. Run `install` from the repo checkout: the job's working directory is the
directory `install` ran from, which is how Restart/Test in Stable find
`scripts/dev-web-restart.sh`. Re-run `install` after changing `PATH` or
toolchains, because the job captures the environment at install time. See
[Server supervision](docs/architecture/web-cockpit.md#server-supervision-launchd).

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

Primary path on iPhone: a normal Safari tab. Full Cockpit works without Home
Screen install. Ajax does not ship classic PWA packaging — no
`manifest.webmanifest` or service worker. Its one 180x180 apple-touch-icon is
served at `/icons/apple-touch-icon.png` for Home Screen installs. Optional Add to Home
Screen (Safari-native; safe `apple-mobile-web-app-*` metadata only) is how you
enable Declarative Web Push phone pings on a capable browser. A native app is
only a future option if the browser path stops being sufficient.

From Safari you can see every repo's tasks, use the attention inbox, and run
browser-capable operations such as `review`, `ship`, `repair`, and `drop`. The
main task view is dashboard-first: current status, required decision, best next
action, and recent milestones are primary. When you open a task, the embedded
raw Ghostty/tmux terminal is the default on mobile and desktop. Browser
`resume` uses that authenticated terminal bridge for full interactive attach.
From a selected task, swipe left opens Diff Review — a read-only PR / file /
hunk viewer for vibe-checking the change before you ship.

When an agent stops at a recognized approval prompt, Web Cockpit shows guarded
structured actions such as Approve and Deny. The browser sends a typed answer
plus a fingerprint of the prompt the operator saw; the server re-captures the
pane and rejects the answer if the agent has moved on. Free-form input and other
terminal-only interactions use the raw task terminal bridge instead of a browser
composer or read-only snapshot viewer.

In the task terminal, **Mic** (after **Paste** in the shortcut bar) starts
host-side continuous speech-to-text: iPhone capture, Mac recognition, auto-insert
into the active shell line through the same paste/PTY path as manual paste. See
[`docs/speech-input.md`](docs/speech-input.md) for Moonshine v2 host
setup, `[stt]` configuration, iOS behavior, and recovery.

Optional phone pings use Declarative Web Push from Web Cockpit Settings after
you add the site to the Home Screen on a capable browser (for example iOS
Safari 18.4+). Cockpit still works without install or notifications; without
enabling push there is no phone ping. Delivery uses `window.pushManager` with
no service worker — see Configuration below for episode rules.

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
`PATH`. Every agent runs in a Docker container, so Ajax also requires
[OrbStack](https://orbstack.dev) as the Docker engine, with `docker` on `PATH`:

```sh
brew install --cask orbstack
```

The `ajax-agent` image builds in the background when the Ajax web server starts,
so start `ajax-cli web` once after installing. Agent launches are refused until
docker is reachable and the image exists. See
[Agent sandbox (Docker)](#agent-sandbox-docker).

Run `ajax-cli doctor` after installing to check the local operator
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

Operator logs append to `{logs_dir}/ajax.log` (see `ajax-cli runtime`). Default
filter is info for Ajax crates. Override with `AJAX_LOG` (else `RUST_LOG`). Set
`AJAX_LOG_STDERR=1` to also mirror to stderr. Opt-in deep traces:
`AJAX_TIMING` (command timing on stderr), `AJAX_TASK_SESSION_TRACE=<path>`
(attach/PTY session), and `AJAX_SUPERVISE_EVENT_LOG=1` to opt in to supervise’s
`{logs_dir}/monitor/events.jsonl` JSONL stream (off by default).

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
inside each task worktree before the agent starts. When bootstrap is
`task-bootstrap.sh`, start also seeds ajax-model-router dispatch symlinks
(including `scripts/analyze-task`) immediately after `git worktree add` (before
tmux/agent, including provisioned ACP starts). Ajax folds husky setup and
optional bootstrap into the task-session launch line (after worktree add and
detached tmux create), so start returns without waiting on npm/husky while the
pane still runs setup before the agent.

Enable Declarative Web Push from Web Cockpit Settings after adding the site to
the Home Screen on a Declarative Web Push–capable browser (for example iOS
Safari 18.4+). Attention episodes deliver OS notifications even when the
installed shell is fully closed. Cockpit still works without install; without
enabling notifications there is no phone ping.

Push notifications fire once per episode for actionable Waiting (`Waiting for
input` / `Waiting for approval` from structured hooks/lifecycle events) and
`Error`-class evidence (CI failed, merge conflict, command failed, blocked,
runtime probe failure). Cursor and Pi have no native wait/ask hook yet; they
still notify on Error-class evidence. Transient `Rate limited` Waiting,
lifecycle-only "Ready for review", and turn-settled "Response ready" stay
inbox-visible but do **not** phone-ping. Dedup is by status class
(`Waiting` / `Error`); the body includes the agent client and explanation
(`repo/handle: Waiting (codex) — …`). Sustained `Running`/`Idle` for 30
seconds re-arms the detector; opening a task silences the current episode.

Notifications fire from the `ajax-cli web` background poll when at least one
push subscription is stored, so a running web server delivers them even when no
browser tab is open. Hooks only write status event files — they never send
push. Delivery stays quiet while a browser is actively using Web Cockpit.

Optional continuous speech-to-text for Web Cockpit uses a `[stt]` block on the
same config file. Defaults apply when the block is omitted; speech stays
unavailable until `provider_command` points at a local sidecar on the Ajax host.
Operator setup, iOS behavior, and recovery are documented in
[`docs/speech-input.md`](docs/speech-input.md).

GitHub CI failure surfacing requires the `gh` CLI installed and authenticated
(`gh auth login`) in the task worktree's repo. When `gh` is missing,
unauthenticated, or the branch has no PR, Ajax keeps the last known task state
and skips CI evidence: no error status, no notification. Checks are polled at
most every 5 minutes per task.

`ajax start` fast-forwards the managed repo's `default_branch` from `origin`
before creating the task worktree so new branches base on current remote `main`
(or your configured default branch).

### Agent sandbox (Docker)

Every Ajax-launched agent runs in a `docker run` container on OrbStack: tmux task
agents (through `__agent-runtime`), Web Cockpit ACP, Claude SDK, Cursor SDK, and
Pi RPC sessions, the sign-in and model-listing probes, and `ajax-cli supervise`.
Launches fail closed. If docker is missing, the daemon is unreachable, or the
image is not built yet, the launch is refused with a message that names the fix.
Ajax never falls back to running unsandboxed. `AJAX_SANDBOX=off` is the only
opt-out. Repo `cargo test`/`cargo run` set it via `.cargo/config.toml`; installed
binaries are sandboxed.

The web server builds the image `ajax-agent:<hash>` in the background at startup,
from the Dockerfile embedded in the binary. The tag is a hash of that Dockerfile,
so upgrading Ajax builds a new image on the next web start.

Agents see:

- The task worktree and the git common dir, each at its own absolute path, so the
  worktree's `.git` file and absolute symlinks resolve unchanged.
- Per-task `target/` and `node_modules/` volumes, so Linux builds never touch the
  host's macOS artifacts.
- The `ajax-agent-home` volume at `$HOME`, which holds agent config and sign-ins.
  Claude, Codex, and Cursor keep their settings and MCP config there, so host
  hooks and MCP servers (macOS binaries) do not reach agents. Shared
  `ajax-rustup`, `ajax-cargo-registry`, and `ajax-cargo-git` volumes hold the Rust
  toolchain and caches.
- Tooling at the same host paths, skipped when missing. Read-only:
  `~/Projects/ajax-model-router`, the `skills` dirs under `~/.claude`, `~/.codex`,
  `~/.agents`, and `~/.cursor`, `~/.claude/skills-src`, the instruction files and
  `scripts` in `~/Projects` (`AGENTS.md`, `agents.md`, `CLAUDE.md`, `.claude`,
  `.codex`, `scripts`), `~/.claude/CLAUDE.md`, `~/.claude/RTK.md`, and
  `~/.codex/AGENTS.md`. Read-write: `~/.claude/projects` (memory and transcripts),
  `~/.ajax-router`, `~/.pi/agent`, `~/.local/share/opencode`, and
  `~/.config/opencode`.
- The ssh agent socket at `~/.config/ajax/ssh/agent.sock`, with `SSH_AUTH_SOCK`
  pointing at it.
- `GH_TOKEN`, only when it is set in Ajax's own environment.

Agents do not see `~/.ssh`, `~/.orbstack`, `~/.docker`, the Docker and OrbStack
sockets, the host tmux socket, the macOS keychain, or Ajax state
(`~/.local/state/ajax`, `~/.ajax-dev`, and `~/.config/ajax`, apart from the ssh
agent socket above).

The container uses host networking. Loopback services stay reachable: Postgres on
`127.0.0.1:5432`, the Ajax web server, and host Chrome on `:9222`. Secure those
services with authentication. The web session cookie mint guard (#1413) is what
stops an agent from minting a Web Cockpit session through the loopback server.

At each launch Ajax verifies the linked worktree and records its git admin dir and
common dir under `~/.config/ajax/trusted-worktrees`. Host-side git that Ajax runs
in that worktree is pinned to those paths with `GIT_DIR`, `GIT_COMMON_DIR`, and
`GIT_WORK_TREE`, and runs with `core.hooksPath=/dev/null` and
`core.fsmonitor=false`. An agent that rewrites the worktree's `.git` file or its
hooks cannot make Ajax run its git config.

Each container is named `ajax-<task key>-<instance>` and labelled with
`ajax.task_key`, `ajax.boot`, `ajax.worktree`, and `ajax.server`. The server label
is `cli` or `supervisor` for those processes, and otherwise a hash of the web
server's state directory, so the stable and dev web servers sharing one engine
never touch each other's containers. At startup, containers left by a previous
boot of the same server are removed for sessions that recovery marks interrupted;
adoptable sessions are never swept. Dropping a task removes its containers and its
`target/` and `node_modules/` volumes.

Agents can run `gh` (with `GH_TOKEN`), `CI=1 npm run web:smoke` and Playwright,
and `ps` and `pgrep` inside their own container. Agents cannot use the iOS
Simulator (iOS-only repros run outside Ajax), drive the host tmux server, or edit
other repos on the host; cross-repo work is a separate task in that repo.

The mounts prevent accidents, not a hostile agent. Agents can still edit the task
worktree, its `.git` (hooks and config included), and the read-write mounts above.

Codex's own sandbox is turned off for a binary named `codex`, because the container
is the boundary. `codex-acp` cannot take that override. If its Linux sandbox fails
inside the container, set `sandbox_mode = "danger-full-access"` in the container's
`~/.codex/config.toml`.

Private keys in `~/.ssh` are not mounted. To let agents use your keys, run a
dedicated `ssh-agent` on a socket outside `~/.ssh` (for example
`~/.config/ajax/ssh/agent.sock`, kept alive by a launchd job that loads your keys).
Ajax mounts that socket into agent containers, so agents can log in wherever those
keys work, but cannot copy the keys. To use the same agent from your shell, point
`~/.ssh/config` at it with `IdentityAgent ~/.config/ajax/ssh/agent.sock` under
`Host *`. To push workflow files over HTTPS instead of SSH, run
`gh auth refresh -s workflow` once.

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

## Agent Client Protocol adapters

Browser sessions drive each harness over its own transport: Cursor speaks the
Agent Client Protocol natively (`agent acp`), Codex needs its ACP adapter, Pi is
driven directly through its RPC mode, and Claude over the Claude Agent SDK.

```bash
npm install -g @agentclientprotocol/codex-acp
```

```bash
npm install -g @earendil-works/pi-coding-agent
```

Claude additionally needs Node.js 20 or newer on `PATH` and the Agent SDK:

```bash
npm install -g @anthropic-ai/claude-agent-sdk
```

`ajax doctor` reports these as `acp:codex`, `rpc:pi`, and `sdk:claude`. Without
an ACP adapter, Ajax falls back to `npx -y <package>`, which works but pays a
fetch on the first session.

## Validation

Before release-sensitive changes, run the strongest applicable local checks:

```sh
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo nextest run --all-features
cargo test --doc
npm run web:check
npm run web:lint
npm run web:sg
npm run web:test -- --run
```

Releases are managed by Release Please. If set, `RELEASE_PLEASE_TOKEN` is used;
otherwise the workflow falls back to `github.token` so releases still run.

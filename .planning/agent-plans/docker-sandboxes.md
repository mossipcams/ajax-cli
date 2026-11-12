# Replace the nono agent sandbox with Docker (OrbStack)

Approval: **APPROVED 2026-11-11** by Matt (revised the same day: "plan better").

## Goal and non-goals

Goal: every Ajax-launched agent runs in a `docker run` container on OrbStack's
engine. nono is removed completely: no fallback, and no release that ships
both backends. `AJAX_SANDBOX=off` stays the only opt-out.

Non-goals: containment of a hostile agent (the threat model is still accident
prevention), Docker Sandboxes (`sbx`), a per-task unsandboxed option, and new
features on frozen tmux/supervisor paths beyond what the swap needs.

## Why

nono kept breaking agents: browsers (WebKit and Chromium cannot nest
Seatbelt), keychain OAuth revocation (#1417), git config and `packed-refs`
(#1409, #1403), `/bin/ps` exec (#1429), `launchctl`, loopback router routes,
and each new deny broke something else (#1423).

## Decisions (all 2026-11-11)

1. `docker run` on OrbStack, not `sbx`. `sbx` cannot reach host loopback,
   has undocumented stdio over `exec`, and keeps a VM and image cache per
   sandbox.
2. nono is removed fully.
3. Skills and scripts keep their current symlinks and resolve through
   same-path mounts.
4. Sandboxed agents lose the iOS Simulator. iOS-only repros run outside Ajax.

## The constraint that shapes the plan

All work happens in nono-sandboxed sessions, which cannot reach Docker. So
the plan never depends on an agent or a hand-run script reaching Docker
before cutover. Evidence comes from three places, in order:

| Layer | Where it runs | Proves |
| --- | --- | --- |
| Unit tests | inside nono (delegates) | exact docker argv, mount list, fail-closed errors, program resolution |
| Dev canary | branch deployed to :8788 with **Test in dev**; its agents are the first in Docker | everything real: image build, stdio, signals and orphans, git, OrbStack loopback, ssh socket, symlinks, sign-ins, web:smoke, live chat |
| Stable cutover | Matt's normal upgrade of :8787 | day-to-day use; nono stays installed for rollback |

Matt's actions: tap **Test in dev**, sign each harness in from Settings,
approve the merges, and do the normal stable upgrade. No Terminal scripts.

## Design

### One seam

Every launch already goes through
`ajax_core::adapters::sandbox::sandbox_command(...) -> (program, argv)`,
called from `ajax-web/src/adapters/program.rs` (ACP via `acp_launch.rs`, Pi RPC
via `jsonl_process.rs`, the Claude SDK sidecar), from
`ajax-supervisor/src/process_observer.rs`, and from
`ajax-cli/src/agent_runtime.rs` (tmux; needs `-t`). The function keeps its
role and returns a docker argv:

```
docker run --rm -i [-t] --init --user <uid>:<gid> --network host
  --name ajax-<task-key> --label ajax.boot=<boot_id> --label ajax.worktree=<cwd>
  -e HOME=$HOME -w <cwd>
  -v ajax-agent-home:$HOME
  -v <cwd>:<cwd>  -v <git common dir>:<git common dir>
  -v <extra_rw>:<extra_rw>                       (tmux agent-events dir)
  -v ajax-target-<task-key>:<cwd>/target
  -v ajax-node-modules-<task-key>:<cwd>/node_modules
  <built-in shared tooling mounts>  [-e GH_TOKEN when set]
  ajax-agent:<dockerfile-hash> <bare program> <args>
```

- `HOME` is the host `$HOME` path on a volume, with host binds on top at the
  same paths, so absolute symlinks, `~` paths, the worktree `.git` file, and
  Claude's cwd-keyed memory all resolve unchanged.
- `target/` and `node_modules` volumes keep Linux builds away from the host's
  macOS artifacts (`dev-web-restart.sh` builds from the worktree). Ajax
  creates them with a `chown` init run on first use (new volumes are
  root-owned).
- Nothing else is mounted: no Docker or OrbStack socket, `~/.ssh`, keychain,
  `~/.config/ajax`, or registry DB. That replaces the whole denylist.
- `trusted_git` (pinned `GIT_DIR`, hooks off) stays: the worktree and common
  dir are still agent-writable. Its store moves from
  `~/.config/ajax/nono/trusted-worktrees` to `~/.config/ajax/trusted-worktrees`.
- Fail closed: missing `docker`, daemon down, or image not ready refuses the
  launch, and the message names the fix.

### Program resolution moves into the image

Callers currently resolve host paths, which a Linux container cannot run. In
the switch PR: bare program names are passed through; `claude_sdk_spawn` uses
the image's npm root for `--sdk-module` (the sidecar source is already inline
`node -e`); and ACP takes the image's adapter without probing the host (deviation
2026-11-12: no exit-127 fallback; with the sandbox on, Codex always uses
`codex-acp`, never native `codex acp`). The Claude and Cursor SDK sidecars load
`/opt/ajax/claude-agent-sdk/sdk.mjs` and `/opt/ajax/cursor-sdk/dist/esm/index.js`,
symlinked in the image.

### Image: embedded and self-building

`docker/agent.Dockerfile` is embedded in the binary (`include_str!`), and its
hash is the image tag. At startup the web server (host-side, unsandboxed)
runs `docker build` in the background if `ajax-agent:<hash>` is missing.
Launches fail closed with "sandbox image is building" until it finishes.
Upgrading Ajax therefore rebuilds the image automatically, with no manual
verb.

Image contents: `mcr.microsoft.com/playwright:v1.62.1-noble` (the same base
as CI web-e2e, so browsers match); git, openssh-client, python3,
build-essential, tmux, ripgrep, jq, gh; rustup with no default toolchain
(`rust-toolchain.toml` installs into a shared `ajax-rustup` volume);
cargo-nextest; global npm pinned to the host's versions: claude-code,
claude-agent-sdk 0.3.302, claude-agent-acp 0.93.0, codex 0.163.0,
codex-acp 2.2.2, acpx 0.21.1, pi-coding-agent, @cursor/sdk; Linux installs of
opencode and cursor-agent. A user with the host uid whose passwd home is the
host `$HOME` (ssh reads passwd, not `$HOME`), plus a pre-created, chowned
home skeleton (`.claude .codex .agents .cursor .config .ajax-router`) so the
home volume initialises writable.

`pi-acp` carries a local patch and must never be `npm i -g`'d. The builder
adds the host's `$(npm root -g)/pi-acp` (pure JS, 7 MB) to the build context.
Drop this once the patch is upstreamed.

A prototype of this image and of the checks is at `~/ajax-docker-spike/`
(`Dockerfile`, `spike.sh`). Packet 1 ports the Dockerfile from it; the folder
is deleted once the canary passes.

### Shared tooling mounts (built-in list; missing host paths skipped)

Inventory of symlinks taken 2026-11-11:

| Symlink location | Target |
| --- | --- |
| `~/.claude/skills`, `~/.codex/skills`, `~/.agents/skills`, `~/.cursor/skills` (model-router, `*-delegate`) | `~/Projects/ajax-model-router/skills/*` |
| repo `.claude/ .codex/ .cursor/ .agents/` `skills/*` | `~/Projects/ajax-model-router/skills/*` |
| worktree `scripts/{run-delegate,analyze-task,run-transaction,delegate-*,check-report,extract-report,router-log*}`, `~/Projects/scripts/*` | `~/Projects/ajax-model-router/scripts/*` |
| `~/.claude/skills/handoff` | `~/.agents/skills/handoff` |
| `~/.claude/skills/no-ai-slop` | `~/.claude/skills-src/no-ai-slop/...` |

- Read-only: `~/Projects/ajax-model-router` (`run-delegate` follows
  `realpath` to its `libexec/`), the four skill dirs, `~/.claude/skills-src`,
  `~/Projects/{AGENTS.md,agents.md,CLAUDE.md,.claude,.codex,scripts}`,
  `~/.claude/{CLAUDE.md,RTK.md}`, and `~/.codex/AGENTS.md`.
- Read-write: `~/.claude/projects` (memory and transcripts), `~/.ajax-router`
  (route health), `~/.pi/agent` (Pi auth, models, extensions, context-fold),
  `~/.local/share/opencode` and `~/.config/opencode`.
- Container-owned (in the home volume, never shared): Claude
  `settings.json`/`.claude.json`, `~/.codex/config.toml`, and Cursor state.
  Their host hooks and MCP servers point at macOS binaries. The iwdp and
  opensafari MCPs drop out.

There is no new config section (cut 2026-11-11). `~/.config/ajax/nono/local.json`
is replaced by built-in defaults: the scoped ssh agent socket
`~/.config/ajax/ssh/agent.sock` is in the mount list (same path, with
`SSH_AUTH_SOCK`, skipped if missing), and `GH_TOKEN` is passed through when
set in the web server's environment (same exposure as the old `~/.config/gh`
grant). Add a config section only when a second machine needs different
mounts.

### Sign-in

Claude, Codex and Cursor credentials live in the home volume. The Settings
status probes (`slices/agent_auth.rs`), `cursor_login.rs` and
`session_models.rs` run through the same image and volume, so a Settings
sign-in lands where agents read it, and Claude rotates its own refresh token
(the #1417 failure class goes away). Pi and opencode reuse the host's files
through the read-write mounts. If a harness has no Settings sign-in flow, the
canary adds one or documents a one-time `docker run -it` login; the canary
decides which.

### Process observation

The host now sees only the `docker` client:

- `web_session/background_jobs.rs` finds the task's containers with
  `docker ps --filter label=ajax.task_key=<key>` and reads `docker top`.
- `orphan_sweep.rs` runs `docker rm -f` only on this server's
  (`ajax.server`) prior-boot containers of sessions that startup recovery
  marks interrupted (never adoptable ones), because a SIGKILLed client may
  leave the container running. Stable and dev web servers share one engine.
- Cancel signals the client (`docker run` proxies to `--init`), with
  `docker kill` as the fallback.
- The session shim keeps holding the client's stdio, so durable adoption is
  unchanged.
- `drop_task.rs` removes the task's `target`/`node_modules` volumes next to
  `trusted_git::forget`.

## Work packets (one branch: `ajax/docker-sandboxes`)

Cut 2026-11-11: no CI `sandbox-image` job (the canary tests the real thing;
add the job only if the image starts breaking). All work lands on this branch
through the model router, one packet at a time, committed when each passes
the gate. It is split into PRs only at PR time, to fit the 1,500-lines-per-PR
and 600-per-file gate; keep each file's changes in one PR. Only the last PR
uses a releasing title, so no release ships both backends.

- [x] **Packet 1: image and docker launch module, not wired.** (accepted 2026-11-12; Dockerfile at `crates/ajax-core/src/adapters/agent.Dockerfile`, not yet built)
  `docker/agent.Dockerfile` (ported from the prototype) and a new
  `ajax-core/src/adapters/docker_sandbox.rs`: argv builder, built-in mount
  list, image tag from the embedded Dockerfile hash, image ensure (inspect,
  else background build), and volume chown init. Accept when exact-argv tests
  cover: linked vs plain worktree, tmux `-t` plus events mount, the codex
  override, missing host paths skipped, `GH_TOKEN` passed only when set, and
  assertions that the tmux socket, Docker/OrbStack sockets, `~/.ssh`,
  `~/.config/ajax` and the registry DBs are never mounted (keeps #1396 and
  #1415 covered). Also fail-closed tests for missing docker, daemon down, and
  image building.
- [ ] **Packet 2: web launch sites, sign-in and observation.** Slice 2a (launch sites: `program.rs`, `acp_launch.rs`, `jsonl_process.rs`, `claude_sdk_spawn.rs`, `session_client.rs`, `cursor_sdk_spawn.rs`) accepted 2026-11-12. Slice 2b (sign-in probes, Cursor login, model listing in the container from a dedicated empty probe dir under the OS temp dir; unique container names `ajax-<task_key>-<pid>-<seq>` with an `ajax.task_key` label) accepted 2026-11-12. Slice 2c (observation: `ajax.server` label from the state dir, boot/server ids set in `ExecutionOwnership::new`, background jobs via `docker ps` + `docker top`, orphan containers removed only for prior-boot, non-adoptable `recovered_handles` and never another server's) accepted 2026-11-12. Remaining: + startup (`background_jobs.rs`, `orphan_sweep.rs`, `process_tree_tests.rs`, image build at web startup, wiring `set_sandbox_boot_id`), 2d docs. `program.rs`,
  `acp_launch.rs`, `jsonl_process.rs`, `claude_sdk_spawn.rs`, `agent_auth.rs`,
  `cursor_login.rs`, `session_models.rs`, `background_jobs.rs`,
  `orphan_sweep.rs`, `process_tree_tests.rs` fixtures, image ensure at
  web-server startup, and `web-cockpit.md`/`durable-execution.md`. Retarget
  (never delete) the `program.rs` launch tests.
- [ ] **Packet 3: CLI/supervisor switch and nono deletion.**
  `agent_runtime.rs`, `process_observer.rs`, `drop_task.rs` volume cleanup,
  the `trusted_git` store move; delete `nono_agent_profile.json`, the nono
  code and tests in `sandbox.rs`, `resolve_nono*`, and
  `.planning/agent-plans/nono-sandboxing.profile.json`; docs: README sandbox
  section and prerequisites, `architecture.md` security bullet,
  `cli-supervisor.md`, `playwright-mcp.md`, `pull-requests.md` (agents can run
  web:smoke). Done when `grep -rni nono` outside `CHANGELOG.md` is empty.

## Dev canary (before any merge)

1. Matt taps **Test in dev** on this task. :8788 runs the top of the stack and
   builds the image on startup. Its `PATH` must include `/usr/local/bin`;
   check this first.
2. Matt signs Claude, Codex, Cursor and Pi in from Settings on :8788.
3. Matt starts one Chat task on :8788 with the prompt "run the canary
   checklist in `.planning/agent-plans/docker-sandboxes.md` and record
   results". That agent is inside Docker and checks:
   - [ ] `curl` to 127.0.0.1:18000, :8788 and :9222 works (host network)
   - [ ] `ssh -T git@github.com` through the mounted agent socket
   - [ ] no dangling links (`find <skill dirs> scripts -xtype l`); one real
     `scripts/run-delegate` round completes
   - [ ] `gh auth status`; commit and push the canary branch
   - [ ] `CI=1 npm run web:smoke` passes; workspace nextest time noted
   - [ ] a `sleep 60 &` job shows in the background-jobs indicator (proves the
     `docker ps` label format and `docker top -eo pid,ppid,comm` parsing)
   - [ ] stdio stays clean over a long chat (no garbled or dropped events)
   - [ ] from inside the container, minting a web session cookie on :8787 or
     :8788 fails (host networking makes #1413 load-bearing)
   - [ ] the image built (pins, gh apt repo, nextest download, cursor install
     path were unverifiable in nono); pin `opencode-ai` to the built version
   - [ ] `/opt/ajax/cursor-sdk/dist/esm/index.js` exists in the image
   - [ ] a Codex chat works through `codex-acp`; if its Linux sandbox fails in
     Docker, set `sandbox_mode = "danger-full-access"` in the container's
     `~/.codex/config.toml` (codex-acp cannot take the `-c` override)
4. Matt checks from the phone: a chat in each harness answers; Cancel stops
   the container; restarting dev web (Test in dev again) leaves no orphan
   containers; dropping a task removes its volumes.

Failures become fixes on the stack, then the canary runs again.

## Cutover and rollback

- Merge the PRs back to back (each retargeted to `main`). Then gate on a
  detached `origin/main` (merging does not run CI on `main`).
- Matt upgrades stable :8787 as usual. Rollback is reinstalling the previous
  release, which still uses nono. Keep nono installed until 7 days of clean
  stable use.
- Then `brew uninstall nono`, move `~/.config/ajax/nono/trusted-worktrees` to
  `~/.config/ajax/trusted-worktrees` (records are also re-written at each
  launch), and remove `~/.config/ajax/nono` and `~/.nono`.

## Verification gate (every PR)

`cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`env -u RUST_LOG cargo nextest run --workspace`, and for web changes
`npm run web:check && npm run web:lint && npm run web:test`.

## Capability changes

Gained: Playwright and web:smoke locally, `ps`, git config writes, writable
caches, Claude-managed OAuth, and loopback routes.
Unchanged: `launchctl` and `open` (already blocked), dev deploy (host-side via
API), and host Chrome CDP (host network).
Lost: the iOS Simulator (decision 4); macOS-only `cfg` branches in
`os_process.rs`, `session_shim/spawn.rs` and `web_service/mod.rs` run only in
Matt's host gate (CI is Linux too); and editing other host repos (cross-repo
work stays "start a task in repo B").

## Risks

- OrbStack specifics (host-network loopback, unix-socket bind mounts,
  VirtioFS speed) are proven only at the canary. If host networking fails,
  fall back to `host.internal` with the affected URLs overridden by env.
- `--network host` shares the Mac's ports: two agents' vite servers on 5173
  collide (already true under nono). Smoke uses `--strictPort`.
- Disk: per-task volumes plus the image. Drop cleanup is required (packet 3),
  and an `ajax.boot` sweep also prunes dangling `ajax-target-*` volumes.
- OrbStack VM memory caps concurrent cargo builds; note the limit at the
  canary.
- Postgres trust on 127.0.0.1:5432 stays reachable, as under nono; this is
  out of the threat model.

## Status

- 2026-11-11: plan approved and revised. Prototype at `~/ajax-docker-spike/`
  (not runnable from nono). 2026-11-11: cut the CI job, the `[sandbox]`
  config section, and the fixed four-PR stack. Next: packet 1 via the model
  router.
- 2026-11-12: packet 1 accepted (Haiku; Qwen route down). Gate re-run by the
  orchestrator: fmt ok, workspace clippy ok, `env -u RUST_LOG cargo nextest
  run -p ajax-core` 1174/1174. Packet 2 must build the image from a small
  dedicated context dir (the Dockerfile copies its whole context) and pass
  `git_grants(cwd).read.first()` as `git_common_dir`.
- 2026-11-12: slice 2a accepted (Haiku, two rounds: the first stopped
  correctly at out-of-scope Pi and Cursor SDK callers). Orchestrator gate:
  fmt ok, workspace clippy ok, `env -u RUST_LOG cargo nextest run -p ajax-web
  -p ajax-core` 2825 passed, 1 skipped.
- 2026-11-12: slice 2b accepted (Haiku, plus one revise round to reuse the
  Cursor SDK constants instead of duplicating them). Orchestrator gate: fmt ok,
  workspace clippy ok, 2836 passed, 1 skipped. Known overhead: every sandboxed
  probe runs `docker info` and `image inspect` first.
- 2026-11-12: slice 2c accepted (Haiku). Orchestrator reviewed the sweep set
  (`recovered_handles` excludes adoptable sessions in both recovery paths)
  and re-ran the gate: 2848 passed, 1 skipped. File sizes: `program.rs` 860,
  `docker_sandbox.rs` 741 lines (under the 1,000 hard limit; do not grow them
  further without splitting). Untested live: `docker ps` label format and
  `docker top -eo` output; add both to the canary.

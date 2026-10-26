# nono sandboxing for agent launches

Approval: **APPROVED 2026-10-22** — on by default + fail closed; all four launch paths; spike first.

## Goal

Run every Ajax-launched agent process under [nono](https://github.com/nolabs-ai/nono)
(Seatbelt on macOS, Landlock on Linux) so an agent can write only to its task
worktree plus the few paths Ajax and git need.

## Non-goals (phase 1)

- Network allowlists / credential proxy (`--network-profile`, `--credential`).
  Network stays open, as in nono's agent profiles.
- nono rollback, audit, Sigstore trust, and publishing profiles to the registry.
- Sandboxing tmux, git, or Ajax itself. Only the agent process tree is sandboxed.
- Web UI for toggling the sandbox.

## Launch chokepoints (from discovery)

| Family | Chokepoint | Today |
| --- | --- | --- |
| tmux interactive (CLI, native + web terminal) | `ajax-cli/src/agent_runtime.rs:117` `Command::new(program)` inside `__agent-runtime` | inherits env; cwd = worktree |
| Web ACP (codex) | `ajax-web/src/adapters/program.rs:105` `harness_command` | cwd = worktree |
| Web Claude SDK / Cursor SDK / Pi RPC | `ajax-web/.../web_session_acp/jsonl_process.rs:59` `JsonlProcess::spawn` | node sidecars + `pi --mode rpc`; cwd = worktree |
| Supervisor (`ajax-cli supervise`) | `ajax-supervisor/src/process_observer.rs:71` | **no cwd set** (inherits ajax-cli cwd) |

## Design

1. **Switch**: on by default. `AJAX_SANDBOX=off` is the only off switch, read by
   one `ajax-core` fn at spawn time. *Deviation from the first draft (config.toml
   field)*: spawn sites span three crates and only some load `Config`; deciding at
   spawn time keeps plan-shape tests unchanged. Repo test runs set
   `AJAX_SANDBOX=off` via `.cargo/config.toml` `[env]` (CI has no nono).
   ponytail: no per-agent/per-repo knobs or extra_allow list until a real need appears.
2. **Fail closed**: when `enabled = true` and `nono` is missing or exits 1
   (nono's own error code), the launch fails with a clear error. It never
   silently runs unsandboxed.
3. **One Ajax-owned profile, no registry packs.** `nono pull nolabs-ai/claude`
   json-merges a plugin into `~/.claude/settings.json` (observed in the spike),
   and no Cursor pack exists. Ajax instead embeds one profile
   (`include_str!`, spiked copy: `nono-sandboxing.profile.json`), writes it
   to the Ajax cache dir (`<cache>/nono/ajax-agent.json`) on EVERY launch
   (overwrites any tampering; the dir is outside the sandbox grants), and passes it by path.
   One profile for every agent.
   ponytail: one profile for all agents; split per agent if one agent's grants
   ever need to be withheld from another.
4. **One pure argv builder in `ajax-core`** (policy owner):
   `sandbox_argv(profile_path, worktree, extra_rw, argv) -> Vec<String>` =
   `nono -s run --allow-cwd --no-rollback-prompt --profile <path> --workdir <wt>
   --allow <git-common-dir> --allow <agent-events-dir> [--allow extra…] -- <argv>`.
   The process cwd must be the worktree (`--allow-cwd` + profile
   `workdir.access = readwrite` is the worktree grant).
   - Grants: git common dir (rw; worktree commits write `.git/worktrees/<n>` and
     objects), Ajax agent-events dir (rw; `ajax-cli __agent-event` hooks append JSONL).
   - **Codex**: when wrapped, force Codex's own sandbox off
     (`--sandbox danger-full-access` for the CLI; `-c sandbox_mode="danger-full-access"`
     for ACP). Seatbelt cannot nest: Codex `workspace-write` fails with
     `sandbox_apply: Operation not permitted`. nono becomes the boundary.
5. **Call sites** wrap argv at the four chokepoints above. *Correction*: `ajax
   supervise` is not task-bound and intentionally runs in the caller's cwd; the
   supervisor takes an optional worktree and sandboxes the current dir otherwise.
   Cursor login (temp-dir helper, not an agent) uses `JsonlProcess::spawn_unsandboxed`.
   Capability probes (`--help`/`--version`/model listing) stay unwrapped.
6. **Status**: unchanged task truth. A sandbox denial shows up as the agent's
   own error/exit, which existing runtime snapshots already report.

## Tasks

- [x] 0. Spike (live, no repo writes): `brew install nono`; in a scratch worktree
      run claude, codex, pi, cursor-agent, and both node sidecars under candidate
      flags; confirm env passthrough (`AJAX_*`), stdio/JSONL passthrough, TTY
      in tmux, keychain/OAuth login, git commit from worktree, hooks writing
      agent-events. Record the working flag set here.
- [x] 1. `ajax-core`: `SandboxConfig` + `sandbox_argv` + unit tests (argv shape,
      grants, profile map, disabled = passthrough).
- [x] 2. tmux path: `__agent-runtime` wraps when enabled; fail-closed test.
- [x] 3. Web path: `harness_command` + `JsonlProcess::spawn` callers wrap.
- [x] 4. Supervisor: wrap + set `current_dir(worktree)`.
- [x] 5. Docs: `docs/architecture/cli-supervisor.md` (+ README config section,
      security assumptions in `architecture.md`).
- [~] 6. Gate (web-session e2e not run; see log): clippy `--all-targets --all-features`, fmt, nextest, docs gate;
      manual end-to-end with `enabled = true` for each harness.

## Risks

- nono is pre-1.0; flags have already been renamed (`--net-block` →
  `--block-net`, `--allow-command` deprecated). Pin a minimum version and check
  `nono --version` at launch.
- Claude auto-update fails in-sandbox (install dir is read-only). Acceptable:
  the agent cannot rewrite its own binary; update outside Ajax.
- Keychain: profile grants the whole `~/Library/Keychains` dir read-write
  (allow + bypass_protection) because keychain writes need temp files in that
  dir. Without it sandboxed Claude could not persist rotated OAuth refresh
  tokens, the stale token was reused and the login revoked (`OAuth token
  revoked`, first seen 2026-10-23). Residual: agents can read and modify any
  keychain item file in that dir (items stay encrypted). Accepted under the
  accident-prevention threat model.
- Network is open (non-goal for phase 1); exfiltration of worktree contents is not prevented.
- Default-on: `cargo run`/tests are unsandboxed via the `.cargo/config.toml`
  `[env]` opt-out; installed binaries (`cargo install`) are not affected by it.
- Not spiked (verify in task 6): Cursor SDK sidecar, `codex acp` over stdio.

## Validation log

Spike 2026-10-22, nono 0.80.0, macOS 26, scratch repo /tmp/nono-spike (main + worktree):
- git add/commit from worktree with common-dir grant: OK. Write to ~ denied; ~/.zshrc read denied.
- `claude -p`: OK after keychain allow + bypass_protection (without: "Not logged in").
- interactive `claude` in tmux (-L nonospike): TTY, trust prompt, resize, reply OK.
- `codex exec`: OK. Codex tool write with `-s workspace-write`: FAILS (nested Seatbelt);
  with `danger-full-access`: worktree write OK, ~ write denied by nono.
- `pi -p`: OK. `cursor-agent -p --force` write: OK after read grant on ~/.local/share/cursor-agent.
- Claude Agent SDK via `node --input-type=module -e` (sidecar shape): OK.
- `ajax-cli __agent-event` in-sandbox: runs; events dir writable only with explicit grant.
- AJAX_* env passes through `nono run`.
- Security-review follow-up: dropped claude_code_macos/codex_macos groups and the
  Keychains dir grant; keychain dir listing now denied, claude -p and codex exec still OK.

Implementation 2026-10-22 (router: Haiku delegates for code slices, Qwen for docs; all accepted after parent review):
- `cargo fmt --check`: pass. `cargo clippy --locked --all-targets --all-features -D warnings`: pass.
  `RUSTDOCFLAGS=-D warnings cargo doc --no-deps --all-features`: pass.
- `cargo nextest run --all-features --no-fail-fast`: 3091/3092 pass. 1 FAIL
  `ajax-web slices::install::tests::stylesheet_preserves_the_safari_first_visual_language`
  (CSS palette tokens) — unrelated: no CSS/asset touched; HEAD is #1383 palette change.
- E2E with built `target/debug/ajax-cli`, AJAX_SANDBOX unset, scratch worktree:
  `__agent-runtime -- claude -p …` wrote worktree file, ~ write blocked, profile written
  to ~/.cache/ajax/nono/ajax-agent.json; PATH without nono → refused with message, agent
  not run; AJAX_SANDBOX=off → unsandboxed; `supervise --prompt …` (codex) worktree write OK,
  ~ write blocked.
- NOT run e2e: Web Cockpit sessions (codex ACP, Claude SDK, Cursor SDK, Pi RPC); covered
  by unit tests + the spike of the same process shapes. Cursor SDK sidecar never spiked.
- File-size warnings (under 1000 hard cap): agent_runtime.rs 704, process_observer.rs 796.
- Spike cleanup: `nono remove nolabs-ai/claude` reversed its ~/.claude wiring.

## Code review follow-up (PR #1385, 2026-10-22)

Review findings fixed (router: Qwen core, Haiku callers, DeepSeek docs/CI):
- Profile tamper: profile moved to `~/.config/ajax/nono` (verified ungranted; `~/.cache` is granted);
  written atomically (temp + rename) and only when content differs (also fixes concurrent-launch race).
- Git escape: whole `.git` rw grant replaced by validated `git_grants`: read common dir; rw only the
  worktree admin dir + objects/refs/logs. Validation runs git with GIT_* stripped and requires
  common=`.git`, gitdir under `common/worktrees`, and the gitdir back-link == `<worktree>/.git`;
  otherwise no git grants (forged `.git` cannot widen grants). nono cannot protect the `.git` file
  itself (deny blocks reads; rename still allowed), hence validation instead.
- Exploratory CI: `start-server.sh` defaults `AJAX_SANDBOX=off`.
- Supervised cancel: SIGTERM to `nono` (verified: nono forwards TERM/INT to the agent), 2s grace,
  then SIGKILL. Process-group approach rejected: it breaks terminal Ctrl-C.
- Duplication/lookup drift: all three call sites use `ajax_core::adapters::sandbox::sandbox_command`;
  `resolve_nono` also checks /opt/homebrew/bin, /usr/local/bin, ~/.cargo/bin, ~/.nix-profile/bin.
- Accepted/documented, not fixed: codex-acp/npx fallbacks need `sandbox_mode = "danger-full-access"`
  in ~/.codex/config.toml (codex-acp has no -c flag); ACP candidate fallback on inner exec failure
  is lost under nono; repo-wide `.cargo/config.toml` AJAX_SANDBOX=off kept for test runs.
- Residual (out of scope): host-side git in a worktree whose `.git` an agent redirected still reads
  agent-controlled config (fsmonitor etc.). Needs host git to use trusted GIT_DIR — follow-up.

Validation: fmt OK; clippy --all-targets --all-features OK; nextest --all-features 3097/3098
(same unrelated stylesheet failure). E2E (home-dir scratch repo, real binary): commit OK; hooks,
git config, profile writes denied; forged `.git` → no git grants, ~/.ssh unreachable.

## Issue #1387 — host-side git trusts worktree `.git` (2026-10-22)

Exploit reproduced: agent inside nono rewrote `<wt>/.git` → `gitdir: <wt>/evil/.git` with
`core.fsmonitor`; unpinned host `git status` ran it.
Fix (uncommitted, awaiting ship decision):
- `ajax-core::adapters::trusted_git`: at sandboxed launch (before the agent runs) record verified
  worktree → admin dir + common dir under `~/.config/ajax/nono/trusted-worktrees/` (atomic, FNV key,
  exact-match lookup, stale records ignored unless admin dir still lives in `common/worktrees`).
- Host git pinned via GIT_DIR/GIT_COMMON_DIR/GIT_WORK_TREE: process runner (direct `git`) and
  supervisor `git_snapshot`. Shell-wrapped git targets the main repo via `$1` (not pinned; safe).
- Sandboxed launch refused in a non-linked checkout (its `.git` would be agent-writable).
- Security-review fix: `.git` must be a regular file (no symlink); back-link compared to
  canonical(worktree)/.git.
Validation: fmt/clippy/rustdoc OK; nextest 3105/3106 (known stylesheet failure). Regression test
`trusted_env_defeats_forged_worktree_link_that_runs_fsmonitor` proves exploit fires unpinned and is
blocked pinned. E2E with real binary: record written, store write-denied in sandbox, pinned git safe.
Blocker: adding to #1385 breaks size gates (1766/1500 PR lines; sandbox.rs 628/600 changed) →
needs stacked PR `fix/sandbox/trusted-worktree-git` (BASE_BRANCH=feat/sandbox/nono-agent-sandboxing).
- Security-review follow-up (fail-open): parent verified a sandboxed agent CAN `rm -rf` its own
  admin dir (Seatbelt checks the target path), which made the H2 "stale record" check ignore the
  record and fall back to the forged `.git`. Fixed: lookup always pins when a record exists
  (missing admin dir → git fails closed); `forget` removes the record after a successful drop
  `EnsureWorktreeAbsent`; pinned git also gets `core.fsmonitor=false` via GIT_CONFIG_COUNT.
  Gate: fmt/clippy/rustdoc OK, nextest 3108/3109 (known stylesheet failure). E2E: forge + admin-dir
  deletion → `fatal: not a git repository`, planted fsmonitor not run.

## Agent tooling access (2026-10-22, in-process at Matt's request)

Found from inside a sandboxed Ajax session: `gh` (no read on ~/.config/gh), git push over HTTPS
(gh credential helper), `ps` (process info), and the model router (workspace repo not granted) all
fail. Fix: profile reads `$HOME/.config/gh`; macOS `process_info_mode: allow_all`; machine-specific
grants via user-owned `~/.config/ajax/nono/local.json`, merged into the written profile (invalid →
launch refused). `~/.ssh` stays denied (no ssh-agent; would expose keys) — use
`gh auth refresh -s workflow` for workflow-file pushes over HTTPS.
Validation (inside the sandbox): profile validates (`Process info: AllowAll`); sandbox tests 16/16
incl. local.json merge + invalid-file refusal; fmt/clippy/rustdoc OK. Full nextest inside the
sandbox: 150 env failures (ps denied, home writes) unrelated to this change — CI is the gate.
Not verifiable in-sandbox: real `gh`/`ps`/router under the new profile (no nested nono).
- Security-review follow-up: gh read and `process_info_mode: allow_all` moved OUT of the shipped
  profile into the documented per-machine `local.json` opt-in (gh may expose a hosts.yml token;
  allow_all exposes other processes' argv). Shipped profile is byte-identical to #1390's. Matt's
  local.json (gh + ps + router) validated as a merged profile (`Process info: AllowAll`).
- SSH for sandboxed agents (Matt's VMs), verified with a real `ajax-cli __agent-runtime` launch:
  dedicated `ssh-agent -D -a ~/.config/ajax/ssh/agent.sock` (launchd `com.matt.ajax-ssh-agent`,
  keys loaded at start), `IdentityAgent` under `Host *` in ~/.ssh/config, local.json grants
  `unix_socket` + read_file/bypass_protection for config, known_hosts and every `.pub` named by
  `IdentityFile` (ssh refuses agent keys it cannot match to an unreadable .pub). sayso-llm,
  proxmox, debian101, llm-gpu reachable; private keys still denied. `gh api user` works;
  router runnable. `/bin/ps` is setuid → nono refuses exec regardless of process_info_mode;
  `pgrep` works with allow_all (empty without).

## Issue #1396 — tmux socket escape (2026-10-22)

Sandboxed agents (any launch path, incl. Web Cockpit chat) could reach the host tmux server at
`/private/tmp/tmux-$UID/default` (covered by the `/private/tmp` system write grant) and run
`tmux new-window` unsandboxed. Fix: profile `filesystem.deny` on `/private/tmp/tmux-$UID` (macOS)
and `/tmp/tmux-$UID` (Linux). Host spike: `tmux ls` / `tmux -S … new-window` → Operation not
permitted, `/tmp` still writable, `claude -p` OK. Regression test
`profile_denies_host_tmux_socket_issue_1396`. Not covered: custom TMUX_TMPDIR / -S sockets.

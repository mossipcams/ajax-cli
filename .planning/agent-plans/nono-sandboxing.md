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
- Keychain: profile grants ONLY `login.keychain-db` + `metadata.keychain-db`
  (allow_file + bypass_protection; Claude/Codex OAuth). Narrower than nono's own
  `claude_code_macos` group (whole Keychains dir), which we deliberately do not
  include. Residual: the encrypted login keychain file is readable by agents.
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

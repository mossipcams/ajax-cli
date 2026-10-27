# Agent sandbox: switch the nono profile to a denylist

Approval: **approved 2026-10-26** (Matt: "Yes", including removing `sandbox_command_refuses_non_linked_checkout`). On 2026-10-26 Matt chose the "Denylist" option over "Off by default" after
rejecting worktree-only writes twice as "still too strict".

## Problem

The profile in `crates/ajax-core/src/adapters/nono_agent_profile.json` is an allowlist: agents
read and write only granted paths. That containment rule doesn't fit the accident-prevention
threat model, and it keeps producing blockers. Agents can't read other repos or sibling
worktrees, `~/.config/ajax/nono/local.json`, LaunchAgents, or vitest's `~/package.json`. They
can't install tools globally. A read-only `.git/config` makes `git switch -c X origin/main`
half-apply (ref created, then an error).

## Rule

Agents may read and write everything except:

1. **Credentials**: nono's default deny list (`extends: "default"`): `~/.ssh` private keys,
   `~/.aws`, `~/.gnupg`, `~/.kube`, `.netrc`, shell history and rc files, browser and 1Password
   stores. The existing keychain `bypass_protection` entries stay.
2. **Ajax's own state**, listed file by file. `~/.ajax-dev` also holds dev worktrees and the
   agent-events dir, so it can't be denied wholesale:
   - `$HOME/.config/ajax/nono` (profile plus trusted-worktree records), so an agent can't
     rewrite its own policy, even by accident;
   - `$HOME/.config/ajax/config.toml`, `$HOME/.ajax-dev/config.toml`;
   - `$HOME/.local/state/ajax/ajax.db`, `$HOME/.ajax-dev/ajax.db`, each with `-wal` and `-shm`.

   Agents have never had access to these files: they aren't granted today, and hooks write only
   to the agent-events dir. Denying them removes nothing that works now.
3. **Host-control sockets**: tmux, Docker/OrbStack/Colima/Lima/Rancher/Podman, Postgres. These
   are unchanged, because these are the "kill someone else's work" accidents.

## Changes

- [x] Profile: `filesystem.allow` becomes `$HOME` plus `/opt/homebrew` (macOS). Remove the
      per-tool grants it makes redundant (`~/.claude`, `~/.codex`, `~/.pi`, `~/.cursor`,
      `~/.npm`, `~/.cargo/*`, the `read` list). Keep the keychain `allow_file`/`bypass_protection`
      entries, the runtime groups, `unlink_protection`, and every existing deny. Add the
      Ajax-state denies.
- [x] `sandbox.rs`: remove the non-linked-checkout refusal. It only existed to keep `.git`
      read-only, and `.git` is now writable everywhere, so it only blocks agents launched in a
      main checkout. Remove its test `sandbox_command_refuses_non_linked_checkout` (needs
      Matt's sign-off). The linked-worktree git grants and `trusted_git` recording stay: they are
      now redundant for the sandbox, but they still pin host git to verified dirs.
- [x] Tests in `sandbox.rs`: the profile allows `$HOME` and denies each Ajax-state path. The
      existing socket and tmux deny tests stay.
- [x] Docs: README "Agent sandbox (nono)", `docs/architecture/cli-supervisor.md`,
      `architecture.md` where it describes sandbox grants, and
      `.planning/agent-plans/nono-sandboxing.md` (threat model plus this decision).

## Non-goals

- Nested sandboxes stay blocked: WebKit/`web:smoke`, nono inside nono. Codex keeps the
  `danger-full-access` override.
- No change to network (already unrestricted), to `local.json` merging, or to `AJAX_SANDBOX=off`.
- #1413 (web-cockpit session for any local caller) is separate.

## Risks

- nono might refuse a whole-`$HOME` grant, or a parent grant might override a deny. Its docs
  say deny wins. This can't be checked from inside nono (running `nono` inside nono kills the
  session), so it is verified on the first session after install. Fallback: grant the
  top-level `$HOME` children instead.
- Accidents now possible: an agent can edit other repos, global tool installs, and `.git`
  hooks/config. Matt accepted this when he chose the denylist.

## Deviations (2026-10-26)

Found by the independent Codex review and checked by the parent against nono v0.80.0 source:

- nono refuses a grant that is a parent of its own state root (`~/.nono`). The profile sets
  `allow_parent_of_protected: true`, which nono honours on macOS only. On Linux, Landlock
  rejects the parent grant even with the flag and cannot deny inside an allowed directory,
  so `$HOME` is macOS-only and Linux keeps the previous per-tool allowlist.
- The `unlink_protection` group is dropped. With it, nono emits `allow file-write-unlink` for
  every writable path after the denies, so agents could delete denied Ajax state (for
  example `ajax.db-wal`). nono's default profile doesn't include it, and with `$HOME`
  writable it protected nothing useful.
- Added credential denies that nono's default list misses: `~/.pgpass`, `~/.pypirc`,
  `~/.cargo/credentials{,.toml}`, and OrbStack's private key `~/.orbstack/ssh/id_ed25519`.
- The edited issue-1423 test now asserts the `$HOME` grant and the OrbStack key deny,
  in place of the removed `read_file` grant. Matt signed off on this edit (2026-10-26).
- Known limit: Ajax-state denies cover the default and dev-profile paths only. A custom
  `AJAX_HOME`/`AJAX_STATE` location is not denied.

## Verification

- Inside nono: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features
  -- -D warnings`, and `cargo nextest run -p ajax-core sandbox`. CI on the PR.
- Live: Matt runs `cargo install --locked` and restarts the web server, then starts a new
  sandboxed session, which probes:
  - **allowed**: read and write in another worktree's scratch file, `~/package.json` read,
    `git switch -c` with tracking, `ssh -G llm`;
  - **denied**: `~/.ssh/id_ed25519`, writing `~/.config/ajax/nono/ajax-agent.json`, reading
    `~/.ajax-dev/ajax.db`, the tmux and Docker sockets.

## Delivery

New branch `fix/sandbox/denylist-profile` off `origin/main`, separate from PR #1429. Delegated
via model-router, independently reviewed, PR via `scripts/gh-pr-create`.

## Validation results (2026-10-26, inside nono)

- Profile JSON validates against nono v0.80.0 `nono-profile.schema.json` (the `main` profile also
  validates, as a control).
- `cargo fmt --all --check`: pass.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: pass.
- `env -u RUST_LOG cargo nextest run --workspace -E 'test(sandbox) | test(nono) | test(sandboxed) |
  test(launch) | test(trusted_git) | test(profile)'`: 90/90 pass.
- Independent review: Codex (gpt-6-astra). Its findings are recorded under Deviations; all were fixed.
- Pending: CI on the PR. Live macOS check after `cargo install --locked` and a web restart
  (see Verification); nono cannot be run from inside nono.

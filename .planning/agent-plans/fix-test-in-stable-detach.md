# fix: Test in Stable must not die with its own log pipe

## Problem

`Test in Stable` spawned `scripts/dev-web-restart.sh --profile stable` as a
child of the running stable web server. That child inherited the server's
stdout: the `tee -a stable-web.log` pipe inside tmux session `ajax-web-stable`.

Sequence that left stable down:

1. Button -> `schedule_test_in_stable()` spawns the restart script, server exits.
2. Sync + `web:build` + `cargo install` all succeed (reached 7c765e3).
3. Script runs `tmux kill-session -t ajax-web-stable`, which kills `tee`.
4. Next `echo` -> SIGPIPE -> script dies before `start_web`.
5. Nothing listening on :8787.

Log ends at `Stopping tmux session ajax-web-stable ...` with no `Starting ...`.

## Scope

- New `scripts/test-in-stable.sh`: hands the stable restart to its own detached
  tmux session (`ajax-test-in-stable`) with its own log, so nothing in the
  restart path can kill the process doing the restarting.
- `crates/ajax-web/src/adapters/server.rs`: `Test in Stable` spawns the new
  script (sibling of `AJAX_WEB_RESTART_SCRIPT`) instead of `dev-web-restart.sh`.
  Button availability gated on the new script existing.

## Non-goals

- The plain `Restart server` button (`schedule_process_restart`) has the same
  bug class when it restarts its own profile. Not touched here.
- No change to `Test in Dev` (`dev_deploy.rs`), which targets a different tmux
  session than the one it runs under and so never kills its own pipe.
- No frontend change: `/api/server/test-in-stable` contract is unchanged.

## Why tmux and not setsid/nohup

`setsid` does not exist on macOS. `nohup` only ignores SIGHUP, it does not
detach stdout from the inherited pipe. tmux is already a hard requirement of
`dev-web-restart.sh`, and a new tmux session is genuinely independent: it is
owned by the tmux server daemon, not by the pane being killed.

## Delegation decision

`Delegation decision: not delegated because the change is smaller than the work
order needed to describe it` (one ~30-line shell wrapper plus a path-derivation
function and its spawn site).

## Tasks

- [x] `scripts/test-in-stable.sh` — detached wrapper, own log, refuses to run
      concurrently with itself.
- [x] `server.rs` — `test_in_stable_script()` derivation, spawn site, env gate.
- [x] Unit tests for the derivation and the gate.
- [x] `cargo fmt --check`, `cargo clippy -p ajax-web`, `cargo nextest run -p ajax-web`.

## Test notes

The shell wrapper has no automated test: reproducing the failure requires a live
tmux-hosted server, a real `cargo install`, and killing the session under it.
Rust-side path derivation and gating are unit tested; the wrapper itself is
validated by deploying and pressing the button.

## Deployment note

The currently running stable binary still points at `dev-web-restart.sh`, so the
first deploy of this change must be done from a terminal:

    scripts/dev-web-restart.sh --profile stable

After that the button uses the new path.

## Validation results

- `cargo fmt --check` — pass
- `cargo clippy -p ajax-web --all-targets --all-features -- -D warnings` — pass
- `cargo nextest run -p ajax-web` — pass (184 tests, 0 failed)
- `bash -n scripts/test-in-stable.sh` — pass
- `shellcheck` — not run, not installed locally

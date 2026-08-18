# Fix #873: Test in Stable must not take :8787 down

## Approval

Approved 2026-08-18 by Matt: implement immediately. Do not wait.

## Goal

Test in Stable must leave `https://127.0.0.1:8787` serving throughout the
rebuild. The live stable process must not `exit(0)` when the button is pressed.
Git reset/build must use a dedicated main worktree, never the Settings-host
checkout. If cutover `start_web` fails, bring the previous binary back.

Fixes #873. Related #944 (stale pid / cargo rollback) — include equivalent
safety so a later abort cannot leave the port empty.

## Non-goals

- Blue/green on a second port or reverse proxy
- Changing Test in Dev (`--worktree`) semantics
- Merging or closing #944 (mention overlap in the report)
- Resetting or switching the operator’s current ajax-cli branch as a side effect

## Scope

- `crates/ajax-web/src/adapters/server.rs` (keep near 600; do not grow past 1000; prefer new tests in `runtime/tests`)
- `crates/ajax-web/src/runtime/mod.rs` (POST `restarting` JSON only if needed)
- `crates/ajax-web/src/runtime/tests/suite_3.rs` and existing server tests
- `scripts/dev-web-restart.sh`
- `scripts/dev-web-restart.test.mjs`
- `scripts/test-in-stable.sh` (comments only if the wrapper contract is unchanged)
- `docs/architecture/web-cockpit.md` (Test in Stable process model)
- Settings tests only if `restarting` / wait behavior must change

## Behavior

1. **Keep the listener.** `schedule_test_in_stable` must not `process::exit`.
   The wrapper still detaches into `ajax-test-in-stable`. Stable POST still
   returns `{ok:true,restarting:true}` so Settings waits for cutover (version
   change or down-edge then two healthy checks). Dev POST stays
   `restarting:false`.
2. **Dedicated main worktree.** Default
   `${AJAX_STABLE_MAIN_WORKTREE:-$HOME/.ajax-dev/worktrees/<repo-basename>-main}`.
   Fetch `origin/main` from `REPO_ROOT`. If the path is missing, `git worktree
   add --detach` at `origin/main`. `git reset --hard origin/main` only that
   tree. Never reset `REPO_ROOT` when it is on another branch.
   `RUN_DIR` (pid/logs) stays `$REPO_ROOT/.ajax-dev-web`.
3. **Cutover.** Build/install while the old tmux session still serves. Then
   stop + start. If `start_web` fails on stable, restore the previous
   `~/.cargo/bin/ajax-cli` (snapshot before `--force` install) and start it.
   A stale pid file after tmux stop must warn and continue, not `exit 1`.

## Verification

- `node --test scripts/dev-web-restart.test.mjs`
- `cargo nextest run -p ajax-web` focused on `test_in_stable` / `adapters::server`
- `bash -n scripts/dev-web-restart.sh scripts/test-in-stable.sh`
- Do not run a live Test in Stable against this clone (it is not on `main`)

## Checklist

- [x] Stable Test in Stable does not `process::exit` the live listener
- [x] Stable POST still returns `{ok:true,restarting:true}`
- [x] Dedicated detached main worktree; never reset the host checkout
- [x] `RUN_DIR` stays `$REPO_ROOT/.ajax-dev-web`
- [x] Stale pid after tmux stop warns and continues
- [x] Failed `start_web` restores the previous cargo bin
- [x] `docs/architecture/web-cockpit.md` describes this process model
- [x] Focused tests lock git -C MAIN_WORKTREE reset/clean, not GIT_DIR

## Stop

Do not `git reset --hard` this checkout. Do not switch branches or commit.
Do not merge or close #944.

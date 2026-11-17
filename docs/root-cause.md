# root-cause reference: ajax-cli

## Logs and evidence
- `~/.local/state/ajax/logs/ajax.log`: every web operation. `grep "action=<name>"` shows `operate begin/end` with status.
- `~/.local/state/ajax/web-session/<url-encoded-worktree-path>.shim.log`: raw harness stream (JSON lines) for the current agent process. It is rewritten on respawn, so its mtime shows restarts. `.shim.stderr` gains one line per process.
- `ajax.db` is denied inside the sandbox. Read live state through the web server (see the `ajax-new-task` skill).
- "Still broken" web reports: compare the served `app.js` with `crates/ajax-web/web/dist` first, then check that the PWA Settings version matches `/api/version`. `:8787` is TLS.

## Repro commands
- One Rust test: `env -u RUST_LOG cargo nextest run --all-features -p <crate> <test-name>`
- All Rust tests: `env -u RUST_LOG cargo nextest run --workspace --all-features`
- Web unit: `npm run web:test -- --run <path>`
- Web e2e, always on a fresh server: `pkill -f '[v]ite'; CI=1 npx playwright test --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit crates/ajax-web/web/e2e/<file>.test.ts`
- Web typecheck: `npm run web:check`. Plain `tsc` prints about 240 test-only errors; ignore it.
- Full gate: `npm run verify`
- Flake check: Playwright `--repeat-each=8 --retries=0`, or nextest `--stress-count`. Repeat on a baseline (`git archive <base> <paths> | tar -x`) before blaming the change.

## Known quirks (rule these out first)
- Agent shells export `RUST_LOG=off`, so four logging tests fail unless run with `env -u RUST_LOG`.
- A reused vite server makes Playwright pass or fail against stale code. Kill it and use `CI=1`.
- Process tests that write a fake executable and then wait on a short timer flake under load, because macOS delays the first exec by seconds. Warm the script before the timed wait instead of raising the timeout.
- Tests that go through `run_with_context*` read the ambient `$TMUX`. Inject `OpenMode` instead, and check with and without `TMUX` set.
- `crates/ajax-web/web/e2e/session-chat-regression.test.ts:344` flakes 1-2 times in 8 on main.
- Under heavy load, `web:test` times out on 1s `findBy` waits. Rerun the failing files with `--maxWorkers=1 --no-file-parallelism`.
- SQLite "unable to open database file" often means a full disk (`target/` growth), not a code bug.
- A mid-task agent reset can be a branch switch re-keying the task ID, not a crash.

# Fix ProcessCommandRunner timed capture pipe deadlock

Mode: Behavior Change / Small Fix.
Status: in progress.

## Symptom (stable)

Diff Review stays on “Loading pull requests…” then fails. Authenticated repro:

- `GET .../pull-requests` → 200 in ~0.4s
- `GET .../diff?local=1` → **502 `git timed out after 30s`** (~30.0s)
- Manual `git diff main...HEAD` in the same worktree → ~0.02s, ~792KB stdout

## Root cause

`run_capture` with timeout pipes stdout/stderr but only reads after `try_wait` sees exit.
When stdout exceeds the OS pipe buffer (~64KB), the child blocks on write → parent waits forever → 30s kill → TimedOut.

## Delegation decision

`Delegation decision: delegated via model-router`

## Scope

- `crates/ajax-core/src/adapters/process.rs` only (plus tests in same file)

## Non-goals

- Diff Review UI copy
- Soft-fail policy for Unobservable
- Changing GH/git timeouts

## Task checklist

- [x] Failing test: timed capture of >64KB stdout succeeds
- [x] Fix: drain stdout/stderr while waiting (reader threads); stdin null
- [x] Existing timeout tests still pass
- [x] Parent validation + reinstall/restart stable

## Validation results

```bash
cargo test -p ajax-core --lib adapters::process -- --test-threads=4  # 8 passed
cargo test -p ajax-core --lib diff_review -- --test-threads=4        # 16 passed
# stable after install+restart:
# GET .../diff?local=1 → 200 in 0.095s (was 502 @ 30s)
# GET .../diff?pr=726 → 200 in 1.6s
```

Status: complete (fix uncommitted; deployed to `~/.cargo/bin` + restarted stable).

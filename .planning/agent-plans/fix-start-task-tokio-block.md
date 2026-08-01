# Fix: New-task create blocks Tokio workers (stable blank / unreachable)

## Scope

- Keep `/api/health` and TLS accepts responsive while `POST /api/tasks` runs
  sync start work (worktree + agent launch).
- Non-goals: Diff Review (already #721), Test-in-Stable blue/green, `/diff`
  slug collision (separate follow-up), frontend timeouts.

## Root cause

Same class as #721 / `fix-diff-review-tokio-block.md`.

`axum_start_task` holds `control_lane` and runs `execute_start_task` via
`run_optimistic` **inline on the Axum worker** — no `spawn_blocking`.

Creating a task from Web Cockpit does long sync git/tmux/agent work on a Tokio
worker. That starves the multi-thread runtime: phone shows blank / “not
loading” / backend unreachable while the process stays alive (health may still
look fine from the Mac when the wedge clears).

Live symptom (user): **Ajax stable broken after creating a new task through
web** — blank screen not loading.

Diff Review got the offload in #721; start (and operate) still block.

## Secondary bug (out of scope here)

Any task whose slug is exactly `diff` (`title: Diff` → `ajax-cli/diff`) is
stolen by `handle.strip_suffix("/diff")` in `axum_task_get` and 404s on detail.
Fix separately: only treat `/diff` as Diff Review when the remaining handle
still contains `/`.

## Delegation decision

`Delegation decision: delegated via model-router`

## Checklist

- [x] Packet: `.planning/packets/fix-start-task-tokio-block.md`
- [x] Characterization: `axum_task_start_does_not_block_health`
- [x] Wrap `axum_start_task` and `axum_action` in `spawn_blocking` + `blocking_lock`
- [x] Parent review + focused validation
- [ ] Optional later: `/diff` slug collision
- [ ] Deploy to stable `:8787` (not done — installed binary still pre-fix)

## Validation (parent re-ran)

```bash
cargo test -p ajax-web --lib axum_task_start -- --nocapture
# 4 passed
cargo test -p ajax-web --lib axum_diff_review_does_not_block_health -- --nocapture
# 1 passed
cargo test -p ajax-web --lib runtime::tests::axum_ -- --test-threads=4
# 35 passed
```

## Deviations

1. GLM monthly usage limit → escalated to cursor-delegate / composer-2.5.
2. Cursor report schema invalid; accepted via delta + parent re-verify.
3. Residual LOW: JoinError path skips `operations().finish` (panic-only).

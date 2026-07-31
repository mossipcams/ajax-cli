# Fix: Diff Review blocks Tokio workers (today's unreachable regression)

## Scope

- Keep `/api/health` and TLS accepts responsive while Diff Review runs `gh`/`git`.
- Non-goals: terminal reconnect client rewrite, Test-in-Stable blue/green, CSP, swipe UX.

## Root cause (updated)

Today's Diff Review (#712+) added `GET .../pull-requests` and `GET .../diff` that call
sync `ProcessCommandRunner` (`gh`/`git`, up to 30s) **directly on Axum worker threads**
via `run_optimistic` with no `spawn_blocking`.

That starves the multi-thread runtime. Phone symptoms match: TLS handshake eof storms,
terminal cleanup timeouts, PWA "backend unreachable", while the process stays alive.

Live catch at 17:18 CDT: same pid, health OK from Mac, phone thrashing TLS/terminals.

## Delegation decision

`Delegation decision: delegated via model-router`

## Checklist

- [x] Failing/characterization test: health (or other async GET) completes promptly while a Diff handler holds a slow command
- [x] Wrap `axum_task_pull_requests` and `axum_task_diff` work in `spawn_blocking`
- [x] Parent review + focused validation
- [ ] Optional follow-up (out of this packet): skip duplicate `observe_task_pull_requests` on `/diff`

## Validation

```bash
cargo test -p ajax-web --lib axum_diff_review -- --nocapture
# ok: does_not_block_health + runner_panic_returns_internal_server_error
cargo test -p ajax-web --lib runtime::tests::axum_ -- --test-threads=4
# ok: 34 passed
```

## Review gate

`VERDICT: ACCEPT` (Codex report schema failed; parent verified diff + tests).
Scope: `crates/ajax-web/src/runtime.rs` only.

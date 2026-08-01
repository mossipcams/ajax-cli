PACKET_STATUS: READY
TASK_KIND: behavior
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
dispatch_level: compact

## Task

Stop Diff Review HTTP handlers from blocking Tokio worker threads.

Today's #712+ routes `GET /api/tasks/{handle}/pull-requests` and
`GET /api/tasks/{handle}/diff` run sync `gh`/`git` via `run_optimistic` on the
async Axum worker. That starves TLS accept/handshake and cockpit polls, so the
phone shows backend unreachable / TLS eof while the process is still alive.

Move the Diff Review handler bodies onto `tokio::task::spawn_blocking` so the
async runtime stays free. Preserve existing status codes and JSON shapes.

## Scope

### Allowed

- `crates/ajax-web/src/runtime.rs`
- `.planning/agent-plans/fix-diff-review-tokio-block.md` (checklist only)

### Forbidden

- Frontend / DiffReview.tsx / swipe / polling timeout changes
- `ajax-core` Diff Review domain rewrite
- Skipping or weakening Diff Review observation behavior (except as required to
  compile the spawn_blocking move)
- Commits, pushes, branch changes
- Unrelated refactors

## Acceptance

1. `axum_task_pull_requests` and `axum_task_diff` run their `run_optimistic` /
   command work inside `tokio::task::spawn_blocking` (or an equivalent off-runtime
   blocking pool), not inline on the async worker.
2. A focused multi-thread test proves `/api/health` (or another lightweight async
   GET already used in this file's tests) completes within a short timeout while
   a Diff Review request is blocked inside a slow `CommandRunner::run`.
3. Existing Diff Review JSON status mapping is unchanged (200 / 404 / 502 shapes).
4. Join/panic from `spawn_blocking` maps to a 500-style web error response, not a
   handler panic.

## Constraints

- Mirror existing test helpers in `runtime.rs` (`app_with`, `get`, cookies,
  `TestBridge`, gated runners / Notify+Condvar patterns like
  `axum_cockpit_returns_current_projection_while_control_lane_is_busy`).
- Keep handler generics `Send + 'static` (add `Sync` only if required to compile).
- Smallest diff; do not rewrite `run_optimistic`.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: cargo test -p ajax-web --lib axum_diff -- --nocapture
      expected: new blocking-isolation test passes; related runtime tests pass
    - type: build
      command: cargo check -p ajax-web
      expected: success
  broader_checks:
    - cargo test -p ajax-web --lib runtime::tests -- --nocapture
  reason: Proves health stays responsive under a blocked Diff handler; typechecks the handler bounds.
```

## Stop if

- Fix requires changing Diff Review domain semantics or frontend
- Cannot gate `CommandRunner` for Diff routes without touching many crates
- Patch would exceed ~400 changed lines

PACKET_STATUS: READY
TASK_KIND: behavior
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
dispatch_level: compact

## Task

Stop task-create and operate HTTP handlers from blocking Tokio worker threads.

Today `POST /api/tasks` (`axum_start_task`) and `POST /api/operations`
(`axum_action`) run sync `run_optimistic` / bridge work **inline on the async
Axum worker** while holding `control_lane`. Creating a new task from Web Cockpit
does long sync git/tmux/agent work and starves TLS accept + health — phone shows
blank / “not loading” while the process stays alive.

Diff Review already offloaded via `spawn_blocking` in #721
(`axum_diff_review_does_not_block_health`). Mirror that for start + operate.

## Scope

### Allowed

- `crates/ajax-web/src/runtime.rs`
- `.planning/agent-plans/fix-start-task-tokio-block.md` (checklist only)

### Forbidden

- Frontend / NewTaskSheet / api.ts timeout changes
- `/diff` slug-collision routing change (separate follow-up)
- `ajax-core` start/operate domain rewrites
- Skipping or weakening operate/start gate / idempotency behavior
- Commits, pushes, branch changes
- Unrelated refactors

## Acceptance

1. `axum_start_task` and `axum_action` run their `run_optimistic` / bridge work
   inside `tokio::task::spawn_blocking` (or equivalent off-runtime blocking
   pool), not inline on the async worker.
2. Keep `control_lane` serialization: either
   - `blocking_lock()` inside the `spawn_blocking` closure, **or**
   - clone `state`, `lock().await` on the original, then `spawn_blocking` with
     the clone (same Arc mutex). Do not drop lane protection.
3. A focused multi-thread test proves `/api/health` completes within a short
   timeout (~150ms) while a `POST /api/tasks` is blocked inside a slow
   `TestBridge::execute_start_task` (add `start_delay: Duration` mirroring
   `operate_delay` / `refresh_delay` if needed).
4. Join/panic from `spawn_blocking` maps to a 500-style web error response (same
   shape as Diff Review’s `"… worker failed"`), not a handler panic.
5. Existing start/operate JSON status mapping and idempotency tests stay green
   (`axum_task_starts_are_idempotent_by_request_id`, operate tests).

## Anchors

- Pattern to copy: `axum_task_pull_requests` / `axum_task_diff` +
  `axum_diff_review_does_not_block_health` in `runtime.rs` (~1006–1110,
  ~3385–3418).
- Fix targets: `axum_start_task` (~1229–1278), `axum_action` (~1320–1390).
- `TestBridge::execute_start_task` (~1723–1737) — add sleep on `start_delay`
  like `operate_delay` / `refresh_delay`.

## Constraints

- Mirror existing test helpers (`app_with`, `post_json`, `get_public`, cookies,
  `TestBridge`).
- Keep handler generics `Send + 'static` (`Sync` only if required to compile).
- Smallest diff; do not rewrite `run_optimistic` or the operate gate.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: cargo test -p ajax-web --lib axum_task_start -- --nocapture
      expected: new blocking-isolation test passes; related start tests pass
    - type: test
      command: cargo test -p ajax-web --lib axum_diff_review_does_not_block_health -- --nocapture
      expected: still passes (no regression)
    - type: build
      command: cargo check -p ajax-web
      expected: success
  broader_checks:
    - cargo test -p ajax-web --lib runtime::tests::axum_ -- --test-threads=4
  reason: Proves health stays responsive under a blocked start handler; typechecks bounds.
```

## Stop if

- Fix requires changing start/operate domain semantics or frontend
- Cannot gate `TestBridge` start delay without touching many crates
- Patch would exceed ~400 changed lines

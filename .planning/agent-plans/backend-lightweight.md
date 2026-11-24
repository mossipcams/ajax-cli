# Backend lightweight pass (Batch 3)

Approval: **approved** by Matt on 2026-11-24. S1 in progress; S2-S4 not started.
Open questions below still need answers before S2 and S3.

Related: Batch 1 (release profile + aws-lc-rs removal, config only) shipped as
its own PR (`perf/deps/lighter-release-binary`). Batch 2 (cap `gh` work per refresh tick so `refresh_flight` is not
held for 5-15 s, `ajax-core/src/runtime_refresh/ci_monitor.rs:93`) is proposed
but not part of this plan.

## Problem

The web server does repeated work on every request or tick that does not grow
with what changed:

- Every SQLite call builds a new `SqliteRegistryStore`, opens a new
  connection and runs `migrate()` (`ajax-core/src/registry/sqlite/mod.rs:71-110`,
  `migrations.rs:7`). `SessionStorage::registry_store()`
  (`ajax-web/.../session_storage.rs:21`) is called from 47 sites, including
  `persist_log_batch` on every streamed transcript append
  (`web_session/task_session.rs:351`). No `busy_timeout` is set, so concurrent
  writers (CLI verbs, web server) fail fast with `SQLITE_BUSY`.
- The 10 s execution-ownership heartbeat does that open + migrate + `UPDATE`
  synchronously on a tokio worker (`ajax-web/src/runtime/mod.rs:242-249` →
  `execution_recovery.rs:355`).
- Each open chat wakes 40 times a second while idle: the session actor ticks
  every 50 ms to drain a `std::sync::mpsc` receiver
  (`task_session.rs:517`, `client.rs:352`), and each websocket asks the actor
  for outbound events every 50 ms (`ws_bridge.rs:13,215`).
- Every refresh clones the full registry 4-6 times, including a second full
  copy kept in `ContextSaveState.loaded_registry`
  (`ajax-cli/src/context.rs:215`, `web_backend.rs:467`,
  `runtime/task_routes/cockpit.rs:155`).

## Non-goals

- No switch to WAL. The rollback journal is a deliberate confidentiality choice
  (`docs/architecture/durable-execution.md` §8); WAL would keep old pages in
  `-wal` until checkpoint.
- No change to the optimistic revision/merge model, `refresh_flight`, or
  `control_lane` semantics.
- No change to the frozen TUI, tmux, or terminal-agent paths.
- No new dependencies.

## Slices (each ≤ 4 files, builds and verifies alone, in this order)

### S1. Heartbeat off the async worker — low risk
- [ ] Run `directory.heartbeat_execution_ownership()` inside
      `tokio::task::spawn_blocking` in `ajax-web/src/runtime/mod.rs:242-249`.
- Verify: existing execution-ownership tests; workspace gate.

### S2. Reuse one SQLite connection per store — high risk (persistence)
- [ ] `SqliteRegistryStore` keeps a lazily opened connection behind a mutex,
      runs `migrate()` and the 0600 permission check once per connection, and
      sets `busy_timeout` (proposed 5 s).
- [ ] Reopen when the file is replaced: compare `(dev, inode)` of the path with
      the open connection's before each use (one `stat`), so `state reset`,
      restores, or test fixtures that swap the file never write to an unlinked
      inode.
- [ ] `SessionStorage` holds one shared store instead of building one per call.
- [ ] Keep `secure_delete=ON` on every connection.
- Acceptance: a regression test proves N consecutive calls on one store open
  one connection and migrate once; a test proves replacing the DB file makes
  the next call reopen; existing durable-execution, crash-scenario, and
  sqlite tests pass unmodified.
- Docs: `durable-execution.md` §2 Storage (connection lifetime, busy timeout).

### S3. Event-driven chat wakeups — high risk (session)
- [ ] Session actor: replace the 50 ms drain tick with an awaitable channel
      from the ACP reader thread (`tokio::sync::mpsc::UnboundedSender` is
      callable from a std thread), and drive the retry/backoff work from
      `sleep_until(next_deadline)` instead of a fixed tick.
- [ ] Websocket bridge: wake on log append (`tokio::sync::Notify` owned by the
      session actor) instead of polling `CollectOutbound` every 50 ms.
- Acceptance: no output-latency regression (first event reaches the socket
  within the current 50 ms bound in a test); idle open chat performs zero
  periodic wakeups apart from the existing 1 s / 5 s background-job throttle;
  chat e2e (`npm run web:smoke`, `session-chat-regression`) passes, remembering
  that `:344` flakes on baseline too.
- Docs: `web-session-behavior.md` / web-cockpit "Task-session module ownership".

### S4. Registry clones — measure first, likely skip
With 9 tasks and 12 registry events a full clone costs microseconds. Do this
only if a timing of `refresh_cockpit_and_cache_locked` shows clones above 1 ms.
If it does: share the save baseline (`loaded_registry`) with the live registry
behind `Arc` and clone on write, instead of keeping a second deep copy.

## Validation (per slice)

```
cargo fmt --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
env -u RUST_LOG cargo nextest run --workspace --all-features
cargo test --doc
RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --all-features
pkill -f '[v]ite'; CI=1 npm run web:smoke      # S3 only
```

## Open questions

- Is a 5 s `busy_timeout` acceptable, or should CLI verbs fail faster?
- S3 touches the live chat path. Ship S3 behind the normal PR flow only after
  S1 and S2 have run on the dev profile for a day?

## Deviations

None yet.

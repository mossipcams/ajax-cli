# Packet: agent-events notify socket listener (Phase 3b)

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

When serving Web Cockpit, bind `{cache_dir}/agent-events/notify.sock` and
accept connections in a background thread, reading and discarding envelope
lines (wake signal). Refresh already folds JSONL; the listener makes the
3a writer’s socket path real. Idempotent bind: remove stale sock file on
start. No daemon process — lives in the existing `ajax web` / serve path.

## Allowed files

- `crates/ajax-cli/src/agent_event_notify.rs` (new) — bind/accept helpers
- `crates/ajax-cli/src/lib.rs` — `mod agent_event_notify;`
- `crates/ajax-cli/src/web_backend.rs` — start listener when serving
- `crates/ajax-cli/src/agent_event.rs` — only if exporting `notify_socket_path`
  as `pub(crate)` for reuse (preferred over duplicating path logic)

## Forbidden changes

- Do not change fold/translate semantics.
- Do not add tokio dependency; use `std::thread` + `UnixListener`.
- No ajax-core edits. No commits.

## Context evidence

- Writer: `agent_event.rs` `notify_socket_path` /
  `try_notify_socket` (post-3a); default `events_dir.join("notify.sock")`.
- Serve entry: `web_backend.rs` `serve_mobile_web` /
  `serve_mobile_web_with_paths` (~164+).
- Cache dir: `context.runtime_paths.cache_dir` → `agent-events/`.

## Code anchors

1. Export `pub(crate) fn notify_socket_path(events_dir: &Path) -> PathBuf`
   from `agent_event.rs` (make existing fn public to crate; keep test
   override).
2. New `agent_event_notify.rs`:
   - `start_agent_event_notify_listener(events_dir: PathBuf) -> io::Result<()>`
   - create_dir_all events_dir; `let _ = fs::remove_file(sock)`; bind
     `UnixListener`; `thread::spawn` loop: `accept`, read to newline or drop
     stream, continue. Log-free / swallow errors after bind.
   - `#[cfg(unix)]` only; non-unix stub returns Ok(()).
3. `serve_mobile_web_with_paths`: after bridge ready, call
   `start_agent_event_notify_listener(cache_dir.join("agent-events"))` —
   ignore error (bind fail must not prevent serve) or map to warning string;
   prefer ignore Err so serve still works.

## Test-first instructions

Red: `cargo test -p ajax-cli agent_event_notify -- --nocapture`

1. `listener_accepts_writer_line` (unix): start listener on temp dir; connect
   with UnixStream write `{"schema_version":1}\n`; assert accept thread read
   it (use mpsc channel from accept loop in test variant, or
   `start_listener_for_test` that sends lines on a channel). Prefer a
   test-only function `spawn_notify_listener_with_sink(events_dir, tx)` used
   by production with sink=None/discard.

Implement production with discard sink; test uses channel sink.

## Edit instructions

Smallest thread+listener. Do not block serve on accept.

## Verification commands

```bash
cargo test -p ajax-cli agent_event_notify
cargo test -p ajax-cli agent_event
cargo clippy -p ajax-cli --all-targets -- -D warnings
cargo fmt -p ajax-cli -- --check
```

## Acceptance criteria

- Listener binds notify.sock; test proves round-trip with writer path.
- Serve still starts if bind fails.

## Stop conditions

- Need async runtime.
- Patch > ~250 lines.

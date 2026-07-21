# Packet: Unix socket try + durable JSONL (Phase 3a)

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

On each accepted agent event, **always** append the JSONL spool (durable), and
**additionally** try a best-effort Unix socket send of the same envelope line
for immediate delivery when a listener is present. No permanent daemon
required: connect failure is silent success for the write path. Socket path:
`{AJAX_AGENT_EVENTS_DIR}/notify.sock` (or `AJAX_AGENT_EVENTS_SOCKET` env
override). No web listener in this packet (3b).

## Allowed files

- `crates/ajax-cli/src/agent_event.rs`

## Forbidden changes

- Do not edit web_backend, cockpit, ajax-core, hooks installers.
- Do not require a listener for tests of the JSONL path.
- No new dependencies (use `std::os::unix::net::UnixStream`).
- No commits.

## Context evidence

- Write path: `run_agent_event` → `append_agent_event_jsonl` + optional legacy
  snapshot in `agent_event.rs`.
- Plan: try socket then spool; JSONL already exists — keep JSONL always for
  durability; socket is additive notify.
- Identity env already provides `AJAX_AGENT_EVENTS_DIR`.

## Code anchors

1. `fn agent_events_socket_path(events_dir: &Path) -> PathBuf` — env
   `AJAX_AGENT_EVENTS_SOCKET` if set and non-empty, else
   `events_dir.join("notify.sock")`.
2. `fn try_send_socket(path: &Path, line: &[u8])` — `UnixStream::connect`,
   `write_all(line)`, `write_all(b"\n")`, ignore all errors (including
   non-unix cfg: no-op compile on non-unix with `#[cfg(unix)]`).
3. After building the envelope JSON bytes for JSONL append, call
   `try_send_socket` with the same line (without requiring trailing newline
   twice — JSONL writer should write `bytes + \n`; socket gets the same).
4. Order: append JSONL first (durable), then try socket (or socket then JSONL —
   prefer **JSONL first** so a crash mid-socket still persisted).

## Test-first instructions

Red: `cargo test -p ajax-cli agent_event -- --nocapture`

1. `socket_send_delivers_line_when_listener_present` — bind temp
   `UnixListener`, spawn thread accept+read one line; set
   `AJAX_AGENT_EVENTS_SOCKET` to that path; `run_agent_event` with identity;
   assert listener received JSON containing `schema_version` and matching
   kind; also assert `.jsonl` grew. Use `#[cfg(unix)]`.
2. Existing JSONL tests remain green when no socket exists.

## Edit instructions

Keep changes localized around `append_agent_event_jsonl` / `run_agent_event`.
Use `std::env::set_var` only in tests; consider `mutex` if parallel tests race
on env — or pass socket path into an internal function for testability
(`append_agent_event_jsonl_with_socket_path`). Prefer injectable path to avoid
global env races:

```rust
fn notify_socket_path(events_dir: &Path) -> PathBuf
fn try_notify_socket(path: &Path, line: &[u8])
```

Tests call `try_notify_socket` directly plus one integration through
`run_agent_event` with env override under a lock or serial_test-free unique
socket path per test (env override is ok if unique path and test restores
env).

## Verification commands

```bash
cargo test -p ajax-cli agent_event
cargo clippy -p ajax-cli --all-targets -- -D warnings
cargo fmt -p ajax-cli -- --check
```

## Acceptance criteria

- JSONL always written; socket best-effort; unix test proves delivery.
- Non-unix builds still compile (cfg).

## Stop conditions

- Need tokio/async runtime for socket.
- Patch > ~200 lines.

PACKET_STATUS: READY
DISPATCH_LEVEL: compact
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Task

In `crates/ajax-web/src/adapters/stt_provider.rs`, function `bridge_task_stt_socket`,
the `SttClientMessage::Stop` arm runs a nested blocking drain loop:

```rust
let deadline = Instant::now() + Duration::from_millis(finalization_timeout_ms.max(1));
while Instant::now() < deadline {
    for event in drain_provider_events(session, &session_id) { ... }
    tokio::time::sleep(Duration::from_millis(STT_EVENT_POLL_MS)).await;
}
```

This loop owns the task for the full timeout and never calls `socket.recv()`, so
every stop costs a fixed `finalization_timeout_ms` (default 5000 ms) and any
`stt.cancel`, `Ping`, or `Close` frame sent during that window goes unread.

Delete the nested loop and hoist the deadline into the existing outer loop, which
already drains provider events each pass and already bounds `socket.recv()` with a
`STT_EVENT_POLL_MS` timeout.

Required shape:

1. Declare `let mut finalize_deadline: Option<std::time::Instant> = None;` alongside
   the existing `provider_session` and `active_session_id` locals.
2. In the `Stop` arm: keep the version/session validation and the `session.finalize()`
   call with its existing `finalize_failed` error path, then set
   `finalize_deadline = Some(Instant::now() + Duration::from_millis(finalization_timeout_ms.max(1)));`
   and remove the nested `while` loop entirely.
3. In the outer loop's existing drain block, note whether any drained event matched
   `SttServerEvent::Final { .. }`. After the drained events are sent, if
   `finalize_deadline` is `Some(deadline)` and either a `Final` was drained this pass
   or `Instant::now() >= deadline`, then take and `cancel()` the provider session, set
   `active_session_id = None`, and set `finalize_deadline = None`.
4. Clear `finalize_deadline` to `None` in the `Cancel` arm and on a successful `Start`.

Behavior after the change: a stop completes as soon as the final transcript is
drained, the socket keeps servicing client frames throughout finalization, and an
absent final still tears the session down at the deadline.

## Allowed files

- `crates/ajax-web/src/adapters/stt_provider.rs`

## Forbidden changes

- Any file other than the one allowed file.
- Changing the wire protocol: do not add, rename, or remove any `SttClientMessage`
  or `SttServerEvent` variant or field.
- Changing `finalization_timeout_ms` plumbing, its config source, or its default.
- Altering `MoonshineSession::finalize` / `cancel` semantics, `BoundedAudioBuffer`,
  the `SttProvider` / `SttProviderSession` traits, or the sidecar frame encoders.
- Renames, formatting sweeps, import reordering, or drive-by cleanup.
- Commits, branches, pushes, merges, rebases.

## Acceptance

- The `while Instant::now() < deadline` loop no longer exists in the `Stop` arm; a
  grep for `while Instant::now()` in the file returns nothing.
- `socket.recv()` continues to be reached every outer-loop pass while a finalization
  deadline is pending, so `stt.cancel`, `Ping`, and `Close` are handled during
  finalization instead of being deferred until the timeout expires.
- The session is torn down (session cancelled, `active_session_id` cleared) on the
  first pass after a `Final` event is drained following `stt.stop`, without waiting
  for the remaining timeout.
- With no `Final` forthcoming, the session is still torn down once the deadline
  passes.
- `finalize_deadline` is cleared on `Cancel` and on a successful `Start`, so a
  subsequent session never inherits a stale deadline.
- Existing behavior for `Start`, `Cancel`, binary audio frames, oversized control
  frames, and malformed control frames is unchanged.

## Verification

Run and report actual results for:

- `cargo clippy -p ajax-web --all-targets --all-features` — must pass with no new warnings.
- `cargo test -p ajax-web stt` — existing tests must pass.

Add a test only if you can do so within the allowed file and it genuinely
demonstrates the early-teardown or non-blocking behavior. If a socket-level test
would require new test infrastructure outside the allowed file, skip it and say so
in `CONCERNS` rather than adding scaffolding.

## Stop if

- The change cannot be made without editing a file outside `Allowed files`.
- The outer loop cannot be made to service `socket.recv()` during finalization
  without a protocol change.
- The patch would exceed roughly 120 changed lines.
- Borrow-checker constraints force restructuring `provider_session` ownership beyond
  the four steps described in `## Task`.

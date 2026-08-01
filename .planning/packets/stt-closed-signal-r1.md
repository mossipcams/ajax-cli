PACKET_STATUS: READY
DISPATCH_LEVEL: compact
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Task

After `stt.stop`, the server now tears the session down as soon as the final
transcript is drained, but it never tells the client. The browser therefore waits
out a hardcoded `FINALIZATION_TIMEOUT_MS = 5_000` in
`speechTransport.ts` `stop()` before calling `onClosed()`, so the UI shows
"Finalizing…" for a fixed 5 seconds regardless of how fast the transcript arrived.

Add a terminal server event and make the client resolve on it.

1. In `crates/ajax-web/src/slices/stt.rs`, add a variant to `SttServerEvent`:

```rust
#[serde(rename = "stt.closed")]
Closed {
    version: u32,
    #[serde(rename = "sessionId")]
    session_id: String,
},
```

2. In `crates/ajax-web/src/adapters/stt_provider.rs`, in `bridge_task_stt_socket`,
   in the finalization-teardown block that currently runs when
   `finalize_deadline` is `Some` and either a `Final` was drained or the deadline
   passed: send `SttServerEvent::Closed { version: STT_PROTOCOL_VERSION, session_id }`
   to the socket immediately before cancelling the session and clearing
   `active_session_id`. Clone the session id into an owned `String` before the
   teardown if the borrow checker requires it. Use the existing `send_stt_event`
   helper and follow the existing convention: if the send fails, cancel the session
   and `return`.

3. In `crates/ajax-web/web/src/shared/lib/speechTransport.ts`:
   - Extract the body of the `finalizationTimer` callback in `stop()` into a single
     `completeFinalization()` function (clear the finalization timer, detach socket
     listeners, close and clear the socket, clear `activeSessionId`, call
     `callbacks.onClosed()`). Make it idempotent — a second call must be a no-op.
   - Have the `stop()` timer call `completeFinalization()`.
   - In `handleServerMessage`, add a `case "stt.closed":` that calls
     `completeFinalization()`.
   - Keep `FINALIZATION_TIMEOUT_MS` and the timer as a **fallback** for the case
     where the server never sends `stt.closed`. Do not delete the timer.

Result: a stop resolves as soon as the server signals completion, with the existing
timeout demoted to a safety net.

## Allowed files

- `crates/ajax-web/src/slices/stt.rs`
- `crates/ajax-web/src/adapters/stt_provider.rs`
- `crates/ajax-web/web/src/shared/lib/speechTransport.ts`
- `crates/ajax-web/web/src/shared/lib/speechTransport.test.ts`

## Forbidden changes

- Any file outside `Allowed files`. In particular do not touch
  `crates/ajax-core/src/config.rs`, `speechState.ts`, `TaskTerminal.tsx`, or
  `TerminalComposer.tsx`.
- Do not change `STT_PROTOCOL_VERSION`.
- Do not add, remove, or rename any other `SttServerEvent` or `SttClientMessage`
  variant or field.
- Do not delete `FINALIZATION_TIMEOUT_MS` or the fallback timer.
- Do not change the `pendingFrames` logic, the RMS/VAD logic, the audio framing, or
  `BoundedAudioBuffer`.
- Renames, formatting sweeps, import reordering, drive-by cleanup.
- Commits, branches, pushes, merges, rebases.

## Acceptance

- `SttServerEvent::Closed` serializes with `"type": "stt.closed"` and a camelCase
  `sessionId`, matching the existing variants.
- The server sends exactly one `stt.closed` for the finalized session, before the
  session is cancelled and `active_session_id` is cleared.
- `stt.closed` is sent on both teardown paths: final-drained and deadline-expired.
- On receiving `stt.closed` for the active session, the client immediately clears
  the finalization timer, closes the socket, clears `activeSessionId`, and fires
  `onClosed()` exactly once.
- Calling `completeFinalization()` twice fires `onClosed()` only once.
- If `stt.closed` never arrives, the existing timeout path still fires `onClosed()`
  exactly as before.
- A `stt.closed` carrying a non-active `sessionId` is ignored, consistent with the
  existing session-id guard in `handleServerMessage`.

## Verification

Run and report actual results for:

- `cargo clippy -p ajax-web --all-targets --all-features` — must pass.
- `cargo test -p ajax-web stt` — must pass.
- `npx vitest run src/shared/lib/speechTransport.test.ts` from
  `crates/ajax-web/web` — must pass.

Add tests to `speechTransport.test.ts` covering: (a) `stt.closed` resolves the stop
without advancing timers, (b) `onClosed` fires exactly once when both `stt.closed`
and the fallback timer would fire. Extend the round-trip test in `stt.rs` to include
the new variant.

## Stop if

- The change cannot be made without editing a file outside `Allowed files`.
- Sending `stt.closed` inside the teardown block cannot satisfy the borrow checker
  without restructuring `provider_session` ownership.
- The patch would exceed roughly 200 changed lines.
- Making `completeFinalization()` idempotent would require new module-level state
  beyond a single boolean or a nulled reference.

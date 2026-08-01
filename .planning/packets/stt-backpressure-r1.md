PACKET_STATUS: READY
DISPATCH_LEVEL: compact
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Task

`onSamples` in `crates/ajax-web/web/src/shared/lib/speechTransport.ts` pretends to
apply backpressure but does not:

```ts
if (pendingFrames >= MAX_PENDING_FRAMES) return;
...
pendingFrames += 1;
socket.send(frame);
pendingFrames = Math.max(0, pendingFrames - 1);
```

`pendingFrames` is incremented and decremented synchronously around a synchronous
`send()`, so it is always `0` by the time the guard is evaluated. The guard can never
fire. A stalled or slow socket therefore accumulates unbounded audio in the
browser's send buffer.

Replace the fake counter with the real signal the WebSocket API already provides.

1. Add `bufferedAmount: number` to the `SpeechTransportSocket` interface.
2. In `wrapNativeSocket`, expose it as a getter that reads
   `socket.bufferedAmount`, matching the existing `readyState` getter style.
3. In `onSamples`, replace the `pendingFrames` guard with a single check that drops
   the frame when the socket's send buffer is already over budget:

```ts
if (socket.bufferedAmount > MAX_BUFFERED_AUDIO_BYTES) return;
```

4. Define `const MAX_BUFFERED_AUDIO_BYTES = 64_000;` next to the other module
   constants, with a short comment noting it is roughly two seconds of 16 kHz mono
   PCM16, matching the server's `max_buffered_audio_ms` default of 2000.
5. Delete `MAX_PENDING_FRAMES` and every remaining use of the `pendingFrames`
   variable, including its declaration and its resets in `start()` and
   `releaseCapture()`.

Dropping a frame must not tear down the session: the guard returns early and capture
continues, exactly as the dead guard was written to do.

## Allowed files

- `crates/ajax-web/web/src/shared/lib/speechTransport.ts`
- `crates/ajax-web/web/src/shared/lib/speechTransport.test.ts`

## Forbidden changes

- Any file outside `Allowed files`. In particular do not touch `speechState.ts`,
  `TaskTerminal.tsx`, `TerminalComposer.tsx`, or any Rust file.
- Do not change the RMS / voice-activity logic (`rms`, `noteLocalSpeech`,
  `SPEECH_RMS_THRESHOLD`, `SPEECH_END_SILENCE_MS`) — a separate packet covers it.
- Do not change `completeFinalization`, `FINALIZATION_TIMEOUT_MS`, the `stt.closed`
  handling, `sendControl`, or any session-lifecycle function.
- Do not change `floatSamplesToPcm16`, `quantizePcm16`, or
  `encodeSpeechAudioFrame`.
- Do not change the sequence-number logic (`nextSequence`).
- Renames, formatting sweeps, import reordering, drive-by cleanup.
- Commits, branches, pushes, merges, rebases.

## Acceptance

- A grep for `pendingFrames` and `MAX_PENDING_FRAMES` in the repository returns no
  hits.
- `SpeechTransportSocket` exposes `bufferedAmount`, and `wrapNativeSocket` forwards
  it from the underlying `WebSocket`.
- When `socket.bufferedAmount` exceeds `MAX_BUFFERED_AUDIO_BYTES`, `onSamples` sends
  nothing and returns without calling `callbacks.onError` or closing the session.
- When `socket.bufferedAmount` is at or under the threshold, frames are sent exactly
  as before, and `nextSequence` still advances once per sent frame.
- A dropped frame does not advance `nextSequence` — the counter must only move when a
  frame is actually sent.
- Existing tests in `speechTransport.test.ts` continue to pass; the fake socket is
  extended with a `bufferedAmount` field defaulting to `0` so existing cases behave
  unchanged.

## Verification

Run and report actual results for:

- `npx vitest run src/shared/lib/speechTransport.test.ts` from `crates/ajax-web/web`
  — must pass.
- `npx tsc --noEmit -p tsconfig.json` from `crates/ajax-web/web` (or the project's
  existing typecheck script if one is defined in `package.json`) — must pass.

Add tests covering: (a) frames are dropped while `bufferedAmount` is over the
threshold and resume once it drops back, and (b) a dropped frame leaves
`nextSequence` unchanged, observable via the sequence prefix of the next sent frame.

## Stop if

- The change cannot be made without editing a file outside `Allowed files`.
- The existing fake socket in the test file cannot express `bufferedAmount` without
  restructuring the whole test harness.
- The patch would exceed roughly 120 changed lines.

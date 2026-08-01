PACKET_STATUS: READY
DISPATCH_LEVEL: compact
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Task

`speechTransport.ts` raises `onSpeechStarted` / `onSpeechEnded` from two independent
sources:

- a local RMS gate in `noteLocalSpeech`, using a fixed `SPEECH_RMS_THRESHOLD` of
  `0.08` with no hysteresis and a `SPEECH_END_SILENCE_MS` timer, and
- the server's `stt.speech_started` / `stt.speech_ended` events, produced by the
  sidecar's real voice-activity detector.

The reducer treats `speech_started` as the signal that resumes a `pause_pending`
session. With two sources, any ambient noise above a fixed RMS threshold cancels a
pending pause. A fixed threshold with no hysteresis is strictly worse than the
provider's VAD, so remove the local one and keep the server as the single source of
truth.

Delete from `crates/ajax-web/web/src/shared/lib/speechTransport.ts`:

- the `SPEECH_RMS_THRESHOLD` and `SPEECH_END_SILENCE_MS` constants,
- the `rms()` helper,
- the `noteLocalSpeech()` function and its call at the top of `onSamples`,
- the `speechActive` and `silenceTimer` variables, the `clearSilenceTimer()` helper,
  and every remaining reference to them, including inside `releaseCapture()` and
  `start()`.

Keep the `onSpeechStarted` / `onSpeechEnded` entries in `SpeechTransportCallbacks`
and keep dispatching them from the `stt.speech_started` / `stt.speech_ended` cases in
`handleServerMessage`. Only the local emission is removed, not the callback contract.

`onSamples` keeps its existing responsibilities: the `readyState` guard, the
`bufferedAmount` backpressure guard, PCM conversion, framing, send, and sequence
advance.

## Allowed files

- `crates/ajax-web/web/src/shared/lib/speechTransport.ts`
- `crates/ajax-web/web/src/shared/lib/speechTransport.test.ts`

## Forbidden changes

- Any file outside `Allowed files`. Do not touch `speechState.ts`,
  `TaskTerminal.tsx`, `TerminalComposer.tsx`, or any Rust file.
- Do not remove `onSpeechStarted` or `onSpeechEnded` from
  `SpeechTransportCallbacks`, and do not stop dispatching them from the server
  events.
- Do not change `completeFinalization`, `FINALIZATION_TIMEOUT_MS`, `stt.closed`
  handling, `onReady` / `pauseGracePeriodMs` parsing, `sendControl`, or the session
  lifecycle.
- Do not change `bufferedAmount` backpressure, `MAX_BUFFERED_AUDIO_BYTES`,
  `floatSamplesToPcm16`, `quantizePcm16`, `encodeSpeechAudioFrame`, or `nextSequence`
  handling.
- Do not change `createBrowserAudioCapture` or the platform interface.
- Renames, formatting sweeps, import reordering, drive-by cleanup.
- Commits, branches, pushes, merges, rebases.

## Acceptance

- A repository grep for `SPEECH_RMS_THRESHOLD`, `SPEECH_END_SILENCE_MS`,
  `noteLocalSpeech`, `speechActive`, and `silenceTimer` returns no hits.
- The `rms` helper no longer exists.
- Feeding `onSamples` loud audio no longer triggers `onSpeechStarted` by itself; the
  callback fires only in response to a server `stt.speech_started` message.
- A server `stt.speech_started` for the active session still calls
  `onSpeechStarted` exactly once, and `stt.speech_ended` still calls
  `onSpeechEnded`.
- Audio frames are still sent for the same inputs as before — removing the RMS gate
  must not change which frames get transmitted, since the gate never suppressed
  sending in the first place.
- `releaseCapture()` still stops capture and releases media tracks, with no dangling
  reference to the deleted timer.
- Existing tests are updated where they asserted local RMS behavior, not deleted
  wholesale: if a test covered "loud samples raise onSpeechStarted", convert it to
  assert the server-driven path instead.

## Verification

Run and report actual results for, from the repository root:

- `npm run web:check` — must pass.
- `npm run web:lint` — must pass.
- `npm run web:test -- --run` — must pass.

Add or adapt a test proving `onSpeechStarted` fires from a server
`stt.speech_started` message and does **not** fire from loud local samples alone.

## Stop if

- The change cannot be made without editing a file outside `Allowed files`.
- Removing `speechActive` or `silenceTimer` turns out to affect session lifecycle or
  capture teardown in a way not described above — report what and stop.
- The patch would exceed roughly 150 changed lines.

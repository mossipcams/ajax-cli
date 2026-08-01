PACKET_STATUS: READY
DISPATCH_LEVEL: compact
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Task

`pause_grace_period_ms` is the last `SttConfig` key that nothing reads. The browser
hardcodes its own copy in `DEFAULT_SPEECH_CONFIG.pauseGracePeriodMs` in
`speechState.ts`. Both happen to be `9000`, so it looks correct until someone
customises the config, at which point the setting silently does nothing.

Carry the server value to the reducer on `stt.ready`, the same way
`finalization_timeout_ms` reaches the client today is *not* yet done — `stt.ready`
currently carries only `version` and `sessionId`, so add the field.

1. `crates/ajax-web/src/slices/stt.rs`: add to `SttServerEvent::Ready` a field
   `pause_grace_period_ms: u64` serialized as `pauseGracePeriodMs`.

2. `crates/ajax-web/src/runtime.rs`: carry `config.stt.pause_grace_period_ms` onto
   `WebAppState` as `stt_pause_grace_period_ms: u64`, exactly the way the existing
   `stt_phrase_end_silence_ms` and `stt_language` fields are carried (field
   declaration, `Clone` impl, both constructors), and pass it into
   `bridge_task_stt_socket`.

3. `crates/ajax-web/src/adapters/stt_provider.rs`: add a
   `pause_grace_period_ms: u64` parameter to `bridge_task_stt_socket` and populate
   the new `Ready` field with it.

4. `crates/ajax-web/web/src/shared/lib/speechTransport.ts`: change the `onReady`
   callback signature in `SpeechTransportCallbacks` from `() => void` to
   `(config: { pauseGracePeriodMs: number }) => void`, and pass the value parsed from
   the `stt.ready` payload. If the field is missing or not a number, fall back to
   `9000` rather than throwing.

5. `crates/ajax-web/web/src/shared/lib/speechState.ts`:
   - Add `pauseGracePeriodMs: number` to `SpeechInputModel`, initialised to `9000` in
     `createSpeechInputModel()`.
   - Add `pauseGracePeriodMs: number` to the `provider_ready` action, and store it on
     the model in that case arm.
   - In the `final` case arm, compute `pauseDeadlineMs` from
     `model.pauseGracePeriodMs` instead of `DEFAULT_SPEECH_CONFIG.pauseGracePeriodMs`.
   - Delete the now-unused `phraseEndSilenceMs` entry from `DEFAULT_SPEECH_CONFIG`;
     the server owns it and passes it to the sidecar. Keep
     `DEFAULT_SPEECH_CONFIG.pauseGracePeriodMs` as the documented default and use it
     as the initial model value.

6. `crates/ajax-web/web/src/features/task/TaskTerminal.tsx`: update the `onReady`
   callback passed to `createSpeechTransport` so it forwards the received
   `pauseGracePeriodMs` into the `provider_ready` dispatch. Change nothing else in
   this file.

## Allowed files

- `crates/ajax-web/src/slices/stt.rs`
- `crates/ajax-web/src/adapters/stt_provider.rs`
- `crates/ajax-web/src/runtime.rs`
- `crates/ajax-web/web/src/shared/lib/speechTransport.ts`
- `crates/ajax-web/web/src/shared/lib/speechTransport.test.ts`
- `crates/ajax-web/web/src/shared/lib/speechState.ts`
- `crates/ajax-web/web/src/shared/lib/speechState.test.ts`
- `crates/ajax-web/web/src/features/task/TaskTerminal.tsx`

## Forbidden changes

- Any file outside `Allowed files`. Do not touch `TerminalComposer.tsx`,
  `TaskTerminal.test.tsx`, `config.rs`, `styles.css`, or `docs/speech-input.md`.
- **In `TaskTerminal.tsx`, change only the `onReady` callback.** Do not modify
  `CONTROL_KEYS`, the terminal toolbar, the Mic or Cancel buttons, the pause
  countdown effect, `activateMic`, `cancelSpeechInput`, or any layout or JSX
  structure. The absence of a `⌃C` toolbar entry is intentional — do not re-add it.
- Do not change `STT_PROTOCOL_VERSION`.
- Do not change `stt.closed`, `completeFinalization`, the finalization deadline
  logic, the writer thread, `bufferedAmount` backpressure, or the RMS/VAD logic.
- Do not change any other reducer action or state transition.
- Renames, formatting sweeps, import reordering, drive-by cleanup.
- Commits, branches, pushes, merges, rebases.

## Acceptance

- `stt.ready` serializes with a `pauseGracePeriodMs` field alongside `version` and
  `sessionId`.
- Setting `pause_grace_period_ms = 4000` in `[stt]` results in `4000` appearing in
  the `stt.ready` payload.
- After `provider_ready`, a spoken standalone `pause` produces
  `pauseDeadlineMs === nowMs + <server value>`, not `nowMs + 9000`, when the server
  value differs from the default.
- `createSpeechInputModel()` still yields `pauseGracePeriodMs: 9000` so a reducer used
  without a ready event behaves exactly as before.
- `DEFAULT_SPEECH_CONFIG` no longer contains `phraseEndSilenceMs`; a repository grep
  for `phraseEndSilenceMs` returns hits only in Rust sidecar code, not in the browser
  bundle.
- A malformed or absent `pauseGracePeriodMs` on `stt.ready` falls back to `9000`
  without throwing.
- Existing reducer tests still pass; `speechState.test.ts` is updated rather than
  weakened where the `provider_ready` action gains a field.

## Verification

Run and report actual results for, from the repository root:

- `cargo clippy -p ajax-web -p ajax-core --all-targets --all-features` — must pass.
- `cargo test -p ajax-web --lib` — must pass.
- `npm run web:check` — must pass.
- `npm run web:lint` — must pass.
- `npm run web:test -- --run` — must pass.

Add a reducer test proving a non-default server grace period drives
`pauseDeadlineMs`, and a transport test proving `onReady` receives the parsed value
and falls back to `9000` when the field is absent.

## Stop if

- The change cannot be made without editing a file outside `Allowed files`.
- Changing the `onReady` signature would require touching a file outside
  `Allowed files`.
- Any change to `TaskTerminal.tsx` beyond the `onReady` callback proves necessary —
  report what and stop.
- The patch would exceed roughly 220 changed lines.

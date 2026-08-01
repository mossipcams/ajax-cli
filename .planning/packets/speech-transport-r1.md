# Browser speech transport — implementation packet

## Scope

Add `crates/ajax-web/web/src/shared/lib/speechTransport.ts` with one-shot
session ownership for microphone capture, authenticated task STT WebSocket
transport, PCM16 framing, lightweight local speech-start VAD, lifecycle
interruption handling, bounded reconnects, and deterministic resource release.

## Tests

- One `start()` requests microphone permission once and creates one UUID/session
  connection; repeated starts while active do not duplicate it.
- The control frame uses the versioned `stt.start` contract and binary audio
  frames carry sequence-prefixed PCM16 without JSON/base64 wrapping.
- Provider partial/final/speech events reach callbacks; local speech activity
  fires independently of delayed transcript text.
- Permission denial, unsupported capture, WebSocket failure/reconnect limit,
  visibility/background interruption, and audio suspension become recoverable
  errors with finalized callbacks preserved.
- Stop/cancel release tracks, processor/context, socket, and timers; no second
  microphone stream is created during reconnect.

## Constraints

- May edit only `crates/ajax-cli/web/src/shared/lib/speechTransport.ts`, its
  focused test, and this plan. (Repository path is `crates/ajax-web/web`; use
  that actual path if the packet typo is encountered.)
- Use browser-native `getUserMedia`, Web Audio, and WebSocket. No model,
  WebGPU, service worker, or new dependency.
- Start only from the caller's user gesture. Do not write to xterm or execute
  terminal input. Keep session IDs stable within one transport instance and
  reject stale socket messages.
- Keep audio queues bounded; use the existing `Config.stt` timing/limits at
  integration boundaries rather than scattering constants.

## Verification

```text
npm run web:test -- --run crates/ajax-web/web/src/shared/lib/speechTransport.test.ts
npm run web:check
```

## Stop conditions

Stop after focused transport tests and `web:check` pass. React composer,
shortcut UI, and full state integration belong to later packets.

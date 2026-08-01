# Frontend speech state — implementation packet

## Scope

Add a pure reducer/controller model for continuous speech input in
`crates/ajax-web/web/src/shared/lib/speechState.ts`, with focused Vitest tests
in `speechState.test.ts`. This slice owns deterministic state, transcript
ordering, standalone `pause` recognition, and session/timer identity checks.

## Tests

- Only one start request can create an active session; duplicate activation is
  ignored while connecting/listening/finalizing.
- Valid transitions cover connecting, listening, pause_pending, finalizing,
  idle, cancellation, and error.
- Standalone normalized `pause` enters pause_pending and does not enter final
  text; sentence uses remain ordinary transcript content.
- Speech-start during pause_pending immediately returns to listening and makes
  an old timer token harmless.
- Nine-second elapsed action enters finalizing only for the active session and
  active timer token.
- Partial text replaces the previous partial; final segments deduplicate and
  sort by sequence, including out-of-order arrivals.

## Constraints

- May edit only `crates/ajax-web/web/src/shared/lib/speechState.ts`, its focused
  `speechState.test.ts`, and this plan.
- Do not add WebSocket/audio/provider code, React components, PTY writes, or
  dependencies.
- Keep the model pure and framework-independent. Timer scheduling belongs to a
  later integration layer; reducer actions carry session IDs and timer tokens.
- Preserve finalized text on cancellation/error; unstable partial text may be
  cleared.

## Verification

```text
npm run web:test -- --run crates/ajax-web/web/src/shared/lib/speechState.test.ts
npm run web:check
```

## Stop conditions

Stop after focused tests and `web:check` pass. Report any need for React,
WebSocket, or browser API scope expansion instead of implementing it here.

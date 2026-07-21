PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

When the iOS Home Screen app mounts while `document.hidden` is true and its
first Cockpit request fails, retry automatically on the short active cadence
until the first projection loads. Resume quiet hidden polling after success.

## Allowed files

- `crates/ajax-web/web/src/app/App.tsx`
- `crates/ajax-web/web/src/app/App.test.tsx`

## Forbidden changes

- Do not edit API transport, polling helpers, Cockpit resource/in-flight guard,
  server/Rust code, generated `dist`, dependencies, service-worker/manifest
  surfaces, terminal/WebSocket code, or files under a `tests/` directory.
- Do not weaken or replace the existing successful hidden-launch test.
- Do not add a retry library, backoff abstraction, or new persistent state.

## Context evidence

- `App.tsx:125-149` derives the hidden cadence as 60 seconds and its interval
  callback skips every Cockpit load while `document.hidden` is true.
- `useCockpitResource.ts:82-107` intentionally permits the one mount load while
  hidden and converts a failure into an error state with `cockpit.data === null`.
- `cockpitPoll.ts:20-54` already collapses overlapping interval attempts while
  a request is in flight, so a one-second startup cadence cannot create
  concurrent fetches.
- `polling.ts:3-6` already defines the one-second active cadence and 60-second
  hidden cadence; no new timing constant is needed.
- `App.test.tsx:548-578` models the iOS hidden mount and proves a successful
  initial request remains quiet through two hidden minutes.

## Code anchors

- Add the failing behavior test beside
  `loads the cockpit on mount while hidden, but skips the background poll` in
  `App.test.tsx`.
- In `App.tsx`, anchor on the `pollingInput`, `cockpitIntervalMs`, and Cockpit
  interval effect. Use the scalar fact `cockpit.data === null`; do not add a
  hook or state machine.

## Test-first instructions

Add `retries a failed hidden PWA launch until the first cockpit projection loads`.

1. Use fake timers and set both `document.hidden` and
   `document.visibilityState` to hidden.
2. Mock the first `/api/cockpit` fetch to reject with a network error and the
   second to resolve with the Cockpit fixture; keep `/api/version` successful.
3. Render `App`, wait for exactly one initial Cockpit call, advance 1,000 ms,
   and require a second Cockpit call and loaded task status without dispatching
   focus/pageshow/visibility events or clicking Retry.
4. Advance two hidden minutes after success and require the Cockpit call count
   to remain two, preserving quiet hidden polling.

Run RED before production edits:

```bash
npm run web:test -- --run crates/ajax-web/web/src/app/App.test.tsx -t 'retries a failed hidden PWA launch until the first cockpit projection loads'
```

Expected RED: only the failed mount request occurs after advancing one second.

## Edit instructions

- Derive one boolean indicating that no Cockpit projection has loaded.
- While that boolean is true, use the existing one-second active cadence even
  if the document is hidden, and allow that interval callback to call
  `loadCockpit` while hidden.
- Include the scalar boolean in the interval effect dependencies so success
  replaces the startup timer with the existing route/visibility cadence.
- Once `cockpit.data` is non-null, preserve the exact existing hidden callback
  suppression and 60-second hidden cadence.

## Verification commands

```bash
npm run web:test -- --run crates/ajax-web/web/src/app/App.test.tsx -t 'retries a failed hidden PWA launch until the first cockpit projection loads'
npm run web:test -- --run crates/ajax-web/web/src/app/App.test.tsx -t 'loads the cockpit on mount while hidden, but skips the background poll|retries a failed hidden PWA launch until the first cockpit projection loads'
npm run web:test -- --run crates/ajax-web/web/src/app/App.test.tsx
npm run web:check
```

## Acceptance criteria

- The new test is observed RED for one call and GREEN for automatic recovery.
- Hidden startup retries use the existing one-second constant and in-flight
  guard, with no concurrent-fetch mechanism or new timer abstraction.
- A successful hidden launch stops startup retries and remains quiet for two
  hidden minutes.
- Existing App behavior and type checking remain green.
- Only the two allowed files change.

## Stop conditions

- Stop if implementation requires API/server changes, a new dependency,
  service worker, manifest, browser-owned task state, or changes outside the
  two allowed files.
- Stop if existing hidden-success behavior must be weakened, or if the focused
  RED failure is unrelated to the missing retry.
- Stop on unrelated baseline failures, changed anchors, or a patch approaching
  400 changed lines.

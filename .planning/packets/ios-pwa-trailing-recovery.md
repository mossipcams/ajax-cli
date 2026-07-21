PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Preserve one Cockpit recovery attempt when iOS lifecycle or network recovery
signals arrive during an in-flight request. Focus, pageshow, visible-state, and
online signals must coalesce into exactly one trailing fetch.

## Allowed files

- `crates/ajax-web/web/src/app/App.tsx`
- `crates/ajax-web/web/src/app/App.test.tsx`

## Forbidden changes

- Do not edit the Cockpit resource, in-flight guard, polling helpers, API
  transport, server/Rust code, generated `dist`, dependencies,
  service-worker/manifest surfaces, terminal/WebSocket code, or files under a
  `tests/` directory.
- Do not change polling cadences or the hidden-startup retry from Task 2.
- Do not add a retry abstraction, event queue, persistent state, or dependency.

## Context evidence

- `App.tsx:94-110` routes focus/pageshow and visible-state recovery through
  `loadCockpit()` without `{ trailing: true }`.
- `App.tsx:112-124` registers focus, pageshow, and visibilitychange, but has no
  native online listener.
- `useCockpitResource.ts:82-107` already accepts
  `loadCockpit({ trailing: true })`; no hook change is needed.
- `cockpitPoll.ts:20-54` already coalesces any number of overlapping trailing
  calls into one follow-up fetch after the current request settles.
- `useCockpitResource.test.tsx:92-117` and `cockpitPoll.test.ts:68-88` directly
  prove that trailing/coalescing primitive. This task only wires shell recovery
  signals to the existing contract.

## Code anchors

- Add the failing App behavior test near `keeps one focus listener across
  re-renders` in `App.test.tsx`.
- In `App.tsx`, modify only `onShellResume`, the visible branch of
  `onShellVisibilityChange`, and the mount-once shell listener effect.

## Test-first instructions

Add `coalesces overlapping shell recovery signals into one trailing cockpit load`.

1. Use fake timers. Mock the first `/api/cockpit` fetch as a manually rejected
   pending promise and the second as a manually resolved pending promise. Keep
   `/api/version` successful and count Cockpit calls.
2. Render `App` and wait for exactly one in-flight Cockpit call.
3. Before settling it, dispatch `focus`, `pageshow`, `online`, and a visible
   `visibilitychange`. Require the count to remain one while the first request
   is in flight.
4. Reject the first request and flush microtasks without advancing the one-second
   dashboard interval. Require one trailing Cockpit call. The current handlers
   should remain at one and fail here.
5. Resolve the trailing request with the Cockpit fixture, require connected
   state, and require the final Cockpit count to be exactly two despite the four
   recovery signals.

Run RED before production edits:

```bash
npm run web:test -- --run src/app/App.test.tsx -t 'coalesces overlapping shell recovery signals into one trailing cockpit load'
```

Expected RED: after the pending first request rejects, no immediate second
Cockpit request runs because all non-trailing overlap calls were discarded.

## Edit instructions

- Change the Cockpit calls in `onShellResume` and the visible branch of
  `onShellVisibilityChange` to `loadCockpit({ trailing: true })`.
- Register `online` on `window` using the same stable `onResume` listener used
  for focus/pageshow, and remove it in the effect cleanup.
- Preserve `checkVersion()` behavior and the mount-once subscription pattern.
- Rely only on the existing in-flight guard to coalesce multiple signals.

## Verification commands

```bash
npm run web:test -- --run src/app/App.test.tsx -t 'coalesces overlapping shell recovery signals into one trailing cockpit load'
npm run web:test -- --run src/app/App.test.tsx
npm run web:test -- --run src/shared/hooks/useCockpitResource.test.tsx src/shared/lib/cockpitPoll.test.ts
npm run web:check
npm run web:lint
```

## Acceptance criteria

- The new test is observed RED for the missing follow-up and GREEN for exactly
  one trailing fetch.
- Focus, pageshow, visible-state, and online all request trailing recovery.
- Multiple signals during one request coalesce through the existing guard.
- Existing hidden-startup cadence, App tests, hook/guard tests, type checking,
  and lint remain green.
- Only the two allowed files change.

## Stop conditions

- Stop if implementation requires changes to the resource/guard/API/server,
  a new dependency, a new retry queue, or files outside the two allowed files.
- Stop if the focused RED failure is caused by the one-second interval rather
  than the missing trailing recovery, or if fake timers cannot distinguish the
  paths reliably.
- Stop on unrelated baseline failures, changed anchors, or a patch approaching
  400 changed lines.

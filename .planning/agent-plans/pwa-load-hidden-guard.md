# PWA load: cockpit never fetches when the document mounts hidden

## Scope

Fix the first-load connection stall in the web cockpit when launched as an iOS
home-screen PWA.

Non-goals: WebSocket session renewal, TLS/cert trust, service worker (there is
none), any layout or terminal change.

## Root cause

`useCockpitResource.loadCockpit` opened with `if (document.hidden) return;`.

That guard sat in the shared loader every caller routes through — mount,
resume (`focus`/`pageshow`), `visibilitychange`, pull-to-refresh, Retry, and
the background interval — but only the background interval wants it.

iOS launches a home-screen PWA with the document still reporting hidden behind
the splash screen. So on cold launch:

1. `onShellMount` calls `loadCockpit()` → swallowed by the guard. No fetch, no
   state change.
2. Initial `documentVisibility` reads `"hidden"`, so the poll interval is
   `REFRESH_INTERVAL_HIDDEN_MS` (60s).
3. `connection` stays `"checking"` and `cockpit.status` stays `"loading"` until
   a `visibilitychange` fires or the 60s timer elapses.

Result: the PWA appears to fail to connect on load.

## Change

- `useCockpitResource.ts` — delete the `document.hidden` early return.
- `App.tsx` — apply the skip at the only caller that wants it, the background
  `setInterval`.

No new option or signature change; the guard moved to its correct owner.

## Tasks

- [x] Move the guard from the shared loader to the background poll.
- [x] Retarget the existing hidden-document unit test at the new behavior
      (assertion preserved, not weakened: it now asserts the mount load *does*
      fetch).
- [x] Add an `App.test.tsx` regression test covering both halves: mount fetches
      while hidden, background interval does not.
- [x] Reset `document.hidden` / `visibilityState` in the App suite `beforeEach`
      — `unstubAllGlobals` does not undo `defineProperty`, so a faked hidden
      document would leak into later tests.

## Validation

- Reverted the fix and confirmed both new tests fail (2 failed), then restored
  it — the tests are load-bearing.
- `npx vitest run` in `crates/ajax-web/web` — 394 passed, 0 failed.
- `npm run web:smoke` (mobile-webkit) — 95 passed, 2 skipped.

## Deviations

Plan written alongside the fix rather than strictly before editing source.

## Fix 2: terminal socket never renewed a stale session

### Root cause

`connectTaskTerminal` had no session-renewal path. A 401 on the WebSocket
upgrade closes the socket immediately, and the browser WebSocket API does not
expose the handshake status — it is indistinguishable from any other failed
dial. After `IMMEDIATE_FAILURE_LIMIT` (5) dials the connection latched to
`"unavailable"` permanently, recoverable only by a manual `reconnectNow`. The
HTTP transport self-heals a stale cookie via `/api/session`; the socket never
did.

### Change

- `api.ts` — export the existing `renewBrowserSession` (already dedupes
  concurrent callers through one in-flight promise; reused rather than
  reimplemented).
- `terminalConnection.ts` — on a close whose dial never opened, renew the
  session once and redial immediately. Renewal failure falls back to the normal
  backoff.

Two flags carry it: `dialOpened` (per dial) distinguishes a rejected handshake
from an established socket dropping, so a backgrounded socket never spends a
renewal; `sessionRenewTried` (reset on every open) caps it at one renewal per
disconnected episode.

The retry preserves `lastDialSeeded` — a dial that never opened received no
history, so the redial must still ask for the seed.

### Tasks

- [x] Export `renewBrowserSession`.
- [x] Renew once and redial on a dial that never opened.
- [x] Update the three tests that drive consecutive failed dials — the renewal
      retry legitimately adds one dial before give-up. Assertions preserved;
      the extra dial is made explicit via `exhaustSessionRenewalRetry()`.
- [x] Cover the new behavior: renew-and-redial, once per episode, backoff
      fallback on renewal failure, no renewal on an established socket drop,
      re-arm after a successful open.

### Validation

- Removed the renewal branch and confirmed 4 of the new tests fail, then
  restored it.
- `npx vitest run` — 399 passed, 0 failed.
- `npm run web:check` / `web:lint` / `web:sg` — clean.
- `npm run web:smoke` (mobile-webkit) — 95 passed, 2 skipped.

## Still not addressed

Neither fix has been confirmed against a live reproduction of the reported
symptom — both were found by tracing the load path. The web server serves HTTPS
with a self-signed cert; an iOS home-screen PWA has no UI to accept a
certificate exception, which is a separate candidate worth ruling out if the
symptom persists.

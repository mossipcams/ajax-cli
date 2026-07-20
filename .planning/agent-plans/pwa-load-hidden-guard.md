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

## Fix 3: `immutable` claimed for non-content-addressed URLs

### Root cause

`5902dab` (today) long-caches shell assets as
`public, max-age=31536000, immutable`, but the cache-busting is in the query
string (`?v=<app_version()>`) while the header was applied to the path
unconditionally. Bare `/app.js`, `/app.css` and `/terminal.js` are not
content-addressed, so the promise is false there.

`immutable` suppresses revalidation even on explicit reload. Anything fetching
a bare path pins that bundle for a year with no user-recoverable escape; on an
installed PWA that means clearing site data.

### Change

`static_asset_response` takes `version_busted`; the immutable cache applies only
when the request carries a non-empty `?v=`, otherwise it falls back to the
existing `bytes_axum_response` (no-store). The three asset handlers read it from
the request `Uri`.

### Validation

- Reverted the branch and confirmed the test fails
  ("bare /app.js must not claim immutability"), then restored.
- `cargo test -p ajax-web` — 163 passed.
- `cargo clippy -p ajax-web --all-targets -- -D warnings` — clean.
- `cargo fmt --check` — clean.

## Why the symptom started today

Fixes 1 and 2 are latent — weeks old — which did not explain a regression that
appeared today. Investigating today's caching/perf work (`5902dab`) resolved
that.

Ruled out with direct evidence against the live server:

| Suspect | Result |
| --- | --- |
| `CompressionLayer` breaking the WS upgrade | clean `101` under gzip |
| `terminal.js` / `app.js` version-bust rewrite no-op | working, verified live |
| missing `Vary: Accept-Encoding` → corrupt cached JS | present on all assets |
| cockpit contract drift vs a stale bundle | shape matches |
| stale server process | started 12:57, after the last release |
| regenerated TLS cert | May 22, SAN covers the current LAN IP |
| boot-paint commit `8b42360` | benign |

What gzip actually changed:

```
/app.js       303189 ->  95527 bytes  (3.2x)
/terminal.js  356280 ->  91786 bytes  (3.9x)
/app.css       54871 ->  10750 bytes  (5.1x)
```

Fix 1 only bites when the app mounts before iOS flips the document to visible.
Before today the PWA spent long enough pulling ~700KB uncompressed that the
splash had dismissed by mount, so the guard never fired. Compressed and
long-cached, the app now mounts inside the hidden window and the mount load is
swallowed. Old bug, new trigger.

Reported symptom ("progress page that changed as we load") is consistent with
this: the cockpit stranded on its loading state.

This explanation fits the evidence but is not a confirmed device reproduction.

## Still not addressed

Neither fix has been confirmed against a live reproduction of the reported
symptom — both were found by tracing the load path. The web server serves HTTPS
with a self-signed cert; an iOS home-screen PWA has no UI to accept a
certificate exception, which is a separate candidate worth ruling out if the
symptom persists.

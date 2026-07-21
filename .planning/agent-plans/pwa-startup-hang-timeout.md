# PWA startup: hung cockpit fetch bricks the app until manual reload

## Symptom

Slow, inconsistent PWA startup; sometimes never starts at all (persists after
#608 / 0.51.2, which fixed the hidden-mount guard, socket session renewal, and
false immutable cache claims).

## Root cause (primary, client)

A single hung HTTP GET permanently bricks the app:

1. `crates/ajax-web/web/src/shared/lib/api.ts` — no fetch anywhere carries an
   `AbortSignal` / timeout (`GET_OPTIONS`, `SESSION_RENEW_OPTIONS`).
2. `crates/ajax-web/web/src/shared/lib/cockpitPoll.ts` — `createInFlightGuard`
   drops every overlapping poll; `trailing` re-runs only *after* the in-flight
   promise settles. A fetch that never settles (classic on iOS after network
   transitions / WKWebView suspend: TCP connect to a LAN IP stalls for
   minutes, TLS hangs) means the mount `loadCockpit` hangs → every interval,
   focus, `pageshow`, `visibilitychange`, pull-to-refresh, and even Retry poll
   no-ops forever.
3. UI sits on `checking` / dashboard skeleton until `location.reload()`.

That is "sometimes doesn't start at all". "Slow, inconsistent" is the same
mechanism with the fetch eventually erroring after OS-level TCP timeouts.

## Contributing cause (server, OUT OF SCOPE here)

`/api/cockpit` cache TTL is 750 ms; every cache miss runs a *synchronous*
full refresh (git/tmux subprocess work) serialized on the single global
`control_lane` (`crates/ajax-web/src/runtime.rs` `refresh_cockpit_and_cache`),
shared with task starts, mutations, and the background notify tick. Any slow
operation stalls cockpit responses for all clients. Architecture-level;
flagged as follow-up, not changed here.

## Scope

Bounded client fix: bound the idempotent GET transport and the session-renewal
POST with a timeout (`AbortSignal.timeout`), so a stalled request rejects as a
network error within ~10 s and the existing 1 s dashboard interval recovers
automatically. Mutation POSTs (`/api/operations`, `/api/tasks`,
`/api/server/restart`, dev-deploy) stay unbounded — task starts legitimately
take longer.

## Non-goals

- Server `control_lane` / blocking-refresh rework (architecture change; needs
  its own approved plan)
- WebSocket / terminal changes
- Cert trust strategy (still unverified as a factor; ruled out only by live
  device testing)
- Poll cadence changes

## Delegation decision

`Delegation decision: delegated via model-router`

## Task checklist

- [x] RED: tests proving GETs/session-renewal carry an abort signal
      (3 failed on `signal` undefined, as predicted)
- [x] Implement `GET_REQUEST_TIMEOUT_MS = 10000` in `polling.ts`; per-call
      `getOptions()` / `sessionRenewOptions()` factories with
      `AbortSignal.timeout` in `api.ts` (constants would have started the
      timer at module load)
- [x] Verify: focused vitest 24/24, full suite 412/412, `web:check` +
      `web:lint` clean — all re-run by parent independently
- [x] Parent review of diff vs anchors; independent validation

## Outcome

- Delegate: pi-delegate / glm-5.2, round 1. Report failed runner schema
  validation (INVALID_STRUCTURED_REPORT); accepted via delta inspect +
  independent verify (same as cockpit-connection-hardening round).
- Changed: `polling.ts` (+2 lines), `api.ts` (factories + 3 call sites),
  `api.test.ts` (4 new tests + `signal: expect.any(AbortSignal)`
  accommodations on existing exact-equality assertions). No other files.
- Review finding (LOW, follow-up): the 401 retry reuses the same signal, so
  renewal time eats the retry's 10 s budget; self-recovers via the 1 s poll.

## Follow-ups (not done here)

- Server: `/api/cockpit` runs a synchronous full refresh on the single global
  `control_lane` with only a 750 ms cache TTL — any slow git/tmux mutation or
  notify tick stalls all clients. Architecture change; needs its own plan.
- Cert trust on the iOS device remains unverified as a factor.
- Local verify gate (`sh .husky/pre-commit`) PASSED end to end, exit 0:
  web:build (dist staged), fmt/check/clippy, nextest 1706/1706, doc tests,
  web:check/lint/sg, web:test 412/412, release build, cargo install.
  First attempt failed on `No space left on device`; freed ~30G by deleting
  the bare main checkout's stale `target/` (14G) and three clean, unused
  worktree `target/` dirs (ajax-worktree-checkout-state, ajax-status-refactor,
  fix-web-pwa-boot-paint). Note: `cargo install` replaced the globally
  installed ajax-cli 0.51.4 (main worktree) with this branch's 0.51.3 build.
- Committed as fe2791a (went through husky pre-commit on commit, exit 0),
  pushed to ajax/connections, PR opened: https://github.com/mossipcams/ajax-cli/pull/616
  (`fix(web): bound cockpit GETs with a 10s timeout so a hung fetch cannot stall PWA startup`).
- PR #616: CI 10/10 green; merged and deployed by user (2026-07-20). Live
  bundle on :8787/:8788 confirmed serving `AbortSignal.timeout`.

## Validation commands

```bash
cd crates/ajax-web/web && npx vitest run src/shared/lib/api.test.ts
cd crates/ajax-web/web && npx vitest run
npm run web:check
npm run web:lint
```

## Deviations

(none yet)

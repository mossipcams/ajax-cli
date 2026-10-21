# Implementation Plan

## Goal

When Cloudflare Access expires while Web Cockpit remains open as a Home Screen
app, take the operator to the Cloudflare sign-in flow without requiring them to
close and reopen the app. Track the reported defect in [issue #1378](https://github.com/mossipcams/ajax-cli/issues/1378).

## Architecture

- **Design decisions:** Handle unrecoverable session expiry in the shared web
  API transport so Cockpit, training, and terminal callers use the same
  recovery behavior. Navigate the top-level page to its current URL so
  Cloudflare Access can run its normal sign-in flow. Bound retries to avoid a
  reload loop.
- **Interfaces and contracts:** Keep `Cf-Access-Jwt-Assertion` verification
  and the HttpOnly browser-session cookie server-side. Keep `POST /api/session`
  as browser-cookie bootstrap/renewal; it does not authenticate a public
  client.
- **Constraints and invariants:** Do not store Access tokens, add service
  workers/offline behavior, or replay failed mutations. Preserve same-origin
  requests and existing API error handling when top-level navigation is not
  available. Auth behavior changes require user approval before implementation.

## Tasks

### T001: Recover Cloudflare sign-in after session renewal fails
- Status: complete
- Depends on: none
- Files:
  - `crates/ajax-web/web/src/shared/lib/api.ts`
  - `crates/ajax-web/web/src/shared/lib/api.test.ts`
  - `crates/ajax-web/web/src/shared/lib/terminalConnection.ts` (caller review only; edit only if required)
  - `crates/ajax-web/web/src/features/training/trainingApi.ts` (caller review only; edit only if required)

**Steps**
1. Review every caller of `renewBrowserSession` and the shared protected-request recovery path.
2. On unrecoverable auth expiry, navigate the top-level page to the current URL once; retain the stale-session error if navigation cannot proceed or the retry guard is already set.
3. Add a focused regression test for expiry navigation and the retry guard.

**Acceptance Criteria**
- [x] An expired Access session in an open Home Screen app reaches the Cloudflare sign-in flow without closing the app.
- [x] Repeated unauthorized responses cannot cause an automatic reload loop.
- [x] Failed mutations are not replayed by the auth recovery path.
- [x] Existing browser-cookie renewal behavior remains covered.

**Verify**
- `rtk npm run web:test -- --run crates/ajax-web/web/src/shared/lib/api.test.ts`
- `rtk npm run web:check`

### T002: Document the session-expiry recovery contract
- Status: complete
- Depends on: T001
- Files:
  - `docs/architecture/web-cockpit.md`

**Steps**
1. Update the Web API access section to describe top-level Cloudflare reauthentication after session renewal fails.
2. Preserve the documented server-side auth and no-replay boundaries.

**Acceptance Criteria**
- [x] The architecture doc matches the implemented expiry behavior and its retry bound.
- [x] No browser token storage or offline mutation behavior is documented or introduced.

**Verify**
- `rtk git diff --check`

## Plan Status

- User approval for auth behavior changes: pending.
- Issue #1378 filed; duplicate searches returned no matches.
- T001 implemented in `api.ts` (`attemptSessionExpiryNavigation`, sessionStorage + in-memory one-shot guard, cleared on successful renewal) and wired into the protected transport, `renewBrowserSession` failure path, `trainingApi.ts`, and `terminalConnection.ts`. Verified: full `pnpm web:test` (165 files / 1653 tests passed), `pnpm web:check` clean, `pnpm web:lint` clean. (No committed lockfile in this worktree; deps installed ad hoc and the generated `pnpm-lock.yaml` was removed.)
- T002 implemented in `docs/architecture/web-cockpit.md` (Web API access section documents the one-shot top-level reauthentication, sessionStorage + in-memory reload-loop guard, stale-session retention on navigation failure, and no-replay / server-side auth boundaries). Verified: `rtk git diff --check` clean.
- PR preparation (2026-07): per explicit user instruction to not delegate, commit/PR ran in-process. Branch renamed from `ajax/buggy` to `fix/web/cloudflare-session-expiry`. Verification before PR: `rtk npm run web:test -- --run` exit 0 (165 files / 1653 tests passed, 2 files / 9 tests skipped), `rtk npm run web:check` exit 0, `rtk npm run web:lint` exit 0. PR created via `scripts/gh-pr-create`.

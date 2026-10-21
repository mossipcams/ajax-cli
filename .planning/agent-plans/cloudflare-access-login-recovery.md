# Implementation Plan

## Goal

Make protected API requests recognizable as AJAX to Cloudflare Access so an
expired Access session can recover to the app login page. Clear the one-shot
navigation guard after successful reauthentication and cover the expiry flow
with regression tests.

## Architecture

- Design decisions
  - Keep the existing API client, renewal flow, login route, and one-shot
    sessionStorage guard as the owners of their current responsibilities.
  - Add the XMLHttpRequest marker to protected API requests and direct expiry
    failures to the existing login navigation.
  - Reset the guard only after the current app flow confirms successful
    reauthentication.
- Interfaces and contracts
  - Protected API calls send `X-Requested-With: XMLHttpRequest`.
  - The app performs at most one login navigation for a failed session recovery
    until a protected request succeeds after authentication; session renewal
    alone does not clear the guard.
- Constraints and invariants
  - Preserve existing API error and renewal behavior outside expiry recovery.
  - Do not modify unrelated tests or authentication flows.
  - User requested regression tests; the repository's default test-file
    restriction is explicitly overridden for these focused tests.
  - Implementation requested by the user; proceed within this bounded behavior.

## Tasks

### T001: Recover Cloudflare Access expiry in the API client
- Status: complete
- Depends on: none
- Files: `crates/ajax-web/web/src/shared/lib/api.ts`; `crates/ajax-web/web/src/features/training/trainingApi.ts`

**Steps**
1. Inspect the API client, all callers, focused tests, and relevant web/auth architecture docs.
2. Send `X-Requested-With: XMLHttpRequest` on protected API requests.
3. Route expired-session outcomes, including rejected fetches and failed/repeated 401 recovery, through the existing one-time login navigation.
4. Clear the one-shot navigation guard after successful reauthentication.

**Acceptance Criteria**
- [x] Protected API requests include the AJAX marker header.
- [x] Cloudflare Access expiry and recovery failures navigate to login once.
- [x] A successful protected response clears the one-shot navigation guard;
      session renewal alone does not.

**Verify**
- `npm run web:test -- --run src/shared/lib/api.test.ts` — pass (48 tests).
- `npm run web:check` — pass.
- `npm run web:build` — pass; existing bundle-size warning only.
- iOS Home Screen check — skipped; no booted simulator and paired iPhone unavailable.

### T002: Add regression coverage for expiry and guard reset
- Status: complete
- Depends on: T001
- Files: `crates/ajax-web/web/src/shared/lib/api.test.ts`

**Steps**
1. Add coverage for the protected request header and expiry recovery.
2. Add coverage that successful reauthentication clears the one-shot guard.
3. Run focused tests and applicable web build/typecheck.

**Acceptance Criteria**
- [x] Regression coverage fails if the header is absent or expiry bypasses login recovery.
- [x] Regression coverage confirms the guard resets only after protected success.

**Verify**
- `npm run web:test -- --run src/shared/lib/api.test.ts src/features/task/desiredModel.test.ts src/features/settings/diagnostics.test.ts` — pass (70 tests).
- `npm run web:check` — pass.
- `npm run web:build` — pass.

### T003: Refresh the checked-in web asset
- Status: complete
- Depends on: T001, T002
- Files: `crates/ajax-web/web/dist/app.js`

**Steps**
1. Regenerate the checked-in web bundle from the updated client.

**Acceptance Criteria**
- [x] The checked-in app bundle includes the API recovery behavior.

**Verify**
- `npm run web:build` — pass.

### T004: Document the web session recovery contract
- Status: complete
- Depends on: T001, T002
- Files: `docs/architecture/web-cockpit.md`

**Steps**
1. Update the web architecture documentation if it describes API session recovery.

**Acceptance Criteria**
- [x] Documented session recovery behavior matches the implemented guard and AJAX header contract.

**Verify**
- Review the documentation change against the implementation.

### T005: Route remaining protected fetch callers through shared recovery
- Status: complete
- Depends on: T001
- Files: `crates/ajax-web/web/src/features/task/desiredModel.ts`; `crates/ajax-web/web/src/features/task/desiredModel.test.ts`; `crates/ajax-web/web/src/features/settings/diagnostics.ts`; `crates/ajax-web/web/src/features/settings/diagnostics.test.ts`; `crates/ajax-web/web/src/shared/lib/api.ts`; `crates/ajax-web/web/src/shared/lib/api.test.ts`; `crates/ajax-web/web/dist/app.js`

**Steps**
1. Use the shared protected API transport for `/api/session/models`, protected diagnostics routes, and the restart-time `/api/version` probe.
2. Keep public `/api/health` diagnostics and health polling outside the protected recovery path.
3. Add focused coverage for the AJAX header and caller behavior.

**Acceptance Criteria**
- [x] Protected direct API callers send the AJAX header and recover through shared session handling.
- [x] `/api/health` remains a public reachability check and does not trigger login navigation on network failure.
- [x] Restart-time version polling uses protected session recovery.

**Verify**
- `npm run web:test -- --run src/features/task/desiredModel.test.ts src/features/settings/diagnostics.test.ts` — pass (22 tests).
- `npm run web:check` — pass.
- `npm run web:build` — pass.

## Approval, status, and risks

- Approval status: implementation requested; bounded work is authorized.
- Checklist status: T001–T005 complete.
- Risk: installed iOS Home Screen app verification could not run because no simulator was booted and the paired iPhone was unavailable. GitHub issue #1378 exists and remains open for this defect.
- Deviations: review expanded scope to two protected direct callers and restart-time version polling; `/api/health` remained public and unchanged.
- Final validation: combined focused tests passed (70 tests); web typecheck, web build, and `git diff --check` passed.

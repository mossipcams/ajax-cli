# Playwright JWT leak scan

## Scope

Add a Playwright e2e harness that explores Web Cockpit surfaces and **fails if any compact JWT** appears in:

- `localStorage`
- `sessionStorage`
- URL / query parameters
- rendered HTML
- console logs
- unexpected API response fields (any string leaf / body text in JSON)
- WebSocket messages

Reuse the existing Vite + mocked-fetch e2e setup under `crates/ajax-web/web/e2e/`. No production auth/runtime changes.

## Non-goals

- Changing Cloudflare Access JWT verification or browser-session cookies
- Hitting a live `ajax-cli web` server
- Scanning HttpOnly cookies via `document.cookie`
- Broader security audit beyond JWT-shaped token leakage

## Delegation decision

`Delegation decision: delegated via model-router` (GLM / opencode-delegate `test-only` after Codex packet critique PASS)

## Task checklist

- [x] Task 1 — JWT probe helper + exploratory e2e that fails on JWT findings
  - Test: `crates/ajax-web/web/e2e/jwt-leak.test.ts` (+ `jwtLeakScan.ts`)
  - Implementation: scan helpers only under `e2e/` (tests-only)
  - Verification: focused Playwright run for the new file
- [x] Review gate + parent validation
- [x] Update this plan with command results

## Approval status

Not required (tests-only security regression harness; no auth architecture change).

## Deviations

- Packet critique BLOCK ×4 then PASS: fixture escape hatch removed; order-robust re-armer; per-route snapshots; all-surface canaries; scope git checks.
- `JwtFinding` fields are `label`/`snippet` (vs packet `detail`/`sample`) — behavior equivalent.

## Validation commands

```bash
rtk npx playwright test e2e/jwt-leak.test.ts --config crates/ajax-web/web/playwright.config.mts --project=desktop-chromium
rtk git diff -- crates/ajax-web/web/e2e/fixtures.ts
```

## Results

- Parent Playwright: **2 passed** (canary all surfaces + clean exploration)
- `fixtures.ts`: empty diff
- Changed/untracked code: only `e2e/jwtLeakScan.ts`, `e2e/jwt-leak.test.ts` (+ planning artifacts)

# Playwright exploratory UI testing

## Scope

Add a Playwright **exploratory UI** suite that walks Web Cockpit operator surfaces
(dashboard filter, task open/back, terminal, settings, new-task sheet) and fails on:

- `pageerror` / uncaught exceptions
- `console` errors (not warnings/logs)
- JWT leaks (reuse `jwtLeakScan.ts`)

## Non-goals

- Production UI changes
- Editing `fixtures.ts` or existing smoke/actions suites
- Visual regression / screenshot baselines
- Live backend (keep mocked fetch/WS)

## Delegation decision

`Delegation decision: delegated via model-router` (Cursor / test-only — frontend e2e)

## Task checklist

- [x] Add `e2e/explore-ui.test.ts`
- [x] Parent review + Playwright verification
- [x] Update results below

## Validation

```bash
rtk npx playwright test e2e/explore-ui.test.ts --config crates/ajax-web/web/playwright.config.mts --project=desktop-chromium
```

## Results

- Parent: `explore-ui` + `jwt-leak` — **3 passed** (desktop-chromium)
- Forbidden files unchanged
- Plan complete

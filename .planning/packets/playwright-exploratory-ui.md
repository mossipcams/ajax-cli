# TDD implementation packet: Playwright exploratory UI

```yaml
PACKET_STATUS: READY
TASK_KIND: tests-only
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
BLOCKERS: []
```

## 1. Status and task contract

READY. Tests-only exploratory UI e2e.

## 2. Goal

One Playwright test (or small suite) that **explores** Web Cockpit UI flows and fails if the session produces page errors, console errors, or JWT-shaped leaks.

## 3. Allowed files

- `crates/ajax-web/web/e2e/explore-ui.test.ts` (new)
- `crates/ajax-web/web/e2e/exploreUi.ts` (new, optional helper ≤150 lines — prefer inline in test if smaller)

## 4. Forbidden changes

- `fixtures.ts`, `jwtLeakScan.ts`, `jwt-leak.test.ts`, other existing e2e files
- `crates/ajax-web/web/src/**`, Rust crates, `package.json`
- commits / deps / production edits

## 5. Context evidence

- Graphify / Serena / ast-grep: `NOT_REQUIRED` — reuse existing e2e anchors.
- Anchors:
  - mocks: `fixtures.ts` `mockFetch`, `mockTerminalWebSocket`, `terminalPanel`, `terminalToolbar`, `waitForTerminalSocket`
  - JWT: `jwtLeakScan.ts` `installJwtLeakProbe`, `snapshotBrowserSurfaces`, `collectContinuousFindings`, `assertNoJwts`
  - flows: `actions.test.ts` settings link / back / bottom-nav; `smoke.test.ts` project pill, new-task, terminal Esc
  - config: `playwright.config.mts` `desktop-chromium`

## 6. Code anchors

Install order before first `goto`: `mockFetch` → `mockTerminalWebSocket` → `installJwtLeakProbe`.

Exploration steps (interactive, not only `goto`):

1. Dashboard `/app.html` — wait `web/fix-login`; click project pill `web`; expect `api/add-auth` hidden
2. Open task via UI if practical (click handle / card) **or** `goto` detail hash then click `← Back` to prove navigation — prefer click-through when selector is stable (`getByText('web/fix-login')` click into detail)
3. Task detail — terminal visible, toolbar Esc, snapshot JWT surfaces
4. Settings — via `button.settings-link` or `#/settings`; expect `outlet-settings`
5. New-task — bottom-nav `data-bottom-action='new-task'`; `#new-task-title-input` visible; Cancel if present
6. Throughout: collect `page.on('pageerror')` and `page.on('console')` where `type()==='error'`

After exploration: `assertNoJwts(...)` and `expect(pageErrors).toEqual([])`, `expect(consoleErrors).toEqual([])`.

Filter known-benign console noise only if an existing e2e already documents it; default is fail on any console error.

## 7. Test-first instructions

`NOT_APPLICABLE: tests-only.`

## 8. Edit instructions

`PRODUCTION_EDIT: FORBIDDEN`. Add explore test (+ optional helper). Desktop-chromium only required.

## 9. Verification commands

```bash
rtk npx playwright test e2e/explore-ui.test.ts --config crates/ajax-web/web/playwright.config.mts --project=desktop-chromium
rtk git diff --name-only
rtk git diff -- crates/ajax-web/web/e2e/fixtures.ts crates/ajax-web/web/e2e/jwtLeakScan.ts
```

## 10. Acceptance criteria

- Explore test passes on clean mocked app
- Fails closed on pageerror / console error / JWT finding
- Only new allowed files in delegate delta

## 11. Stop conditions

- Need to edit fixtures/JWT helper/production
- Clean run fails on pre-existing console noise — STOP and report the exact messages (do not broaden filters without parent)
- Diff >~300 lines or outside allowed files
- Playwright tooling missing — report command + exit

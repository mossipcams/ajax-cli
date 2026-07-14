# Adversarial Playwright JWT defect hunt

## Scope

Plant JWTs in API/WS payloads the UI consumes; fail if they appear in client surfaces
(localStorage, sessionStorage, URL, console, rendered HTML). Do not fail on
intentional transport capture. Keep clean jwt/explore suites unchanged.

## Delegation decision

`Delegation decision: not delegated because approved plan is a complete executable work order with exact files/anchors; parent executes to finish todos without another critique round.`

## Checklist

- [x] `assertNoJwtsOnSurfaces` / `CLIENT_JWT_SURFACES` in `jwtLeakScan.ts`
- [x] `jwt-adversarial.test.ts`
- [x] Validate adversarial + clean suites; record findings

## Results

### Adversarial (defect found)

```bash
rtk npx playwright test e2e/jwt-adversarial.test.ts --config crates/ajax-web/web/playwright.config.mts --project=desktop-chromium
```

**FAIL (exit 1)** — JWT canary visible in **rendered HTML**:

- `[html] (dashboard)` — canary from hostile `status_explanation`
- `[html] (task-detail)` ×2 — canary from hostile detail `status_explanation` / `title`

No findings on localStorage, sessionStorage, URL, or console in this run.

### Clean regression (still green)

```bash
rtk npx playwright test e2e/jwt-leak.test.ts e2e/explore-ui.test.ts --config crates/ajax-web/web/playwright.config.mts --project=desktop-chromium
```

**PASS (3)** — exit 0.

### Follow-up (out of scope for this plan)

Production redaction (or stop rendering JWT-shaped API/WS text into the DOM) is a separate fix when requested.

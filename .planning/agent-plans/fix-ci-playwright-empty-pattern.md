# Fix CI: Playwright empty-pattern beforeEach

## Scope

Restore Playwright-compatible `test.beforeEach(({}, testInfo) => …)` in two e2e
files. Slice 12 renamed the unused fixture to `_fixtures` to satisfy
`no-empty-pattern`, which broke Playwright's fixture contract in CI.

## Non-goals

- No behavior changes to the tests themselves
- No weakening of `no-empty-pattern` globally

## Delegation decision

`Delegation decision: not delegated because smaller than the work order
(two-line restore + eslint-disable).`

## Checklist

- [x] Diagnose: Web smoke failed on `_fixtures` arg
- [x] Restore `({}, testInfo)` + scoped eslint-disable
- [x] `npm run web:lint` + smoke load check
- [ ] Commit / push
- [x] Update PR 587 description for slices 1–12

## Validation

```bash
npm run web:lint   # exit 0
npx playwright test --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit --list
# Total: 96 tests in 7 files
```

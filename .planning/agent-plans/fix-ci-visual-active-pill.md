# Fix CI: visual e2e expects teal active pill

## Scope

- Update Playwright visual test that still asserts mustard for `.project-pill.is-active`.
- Expect teal (`rgb(54, 112, 105)` / `--teal`) to match the selection color change.

## Non-goals

- No production CSS changes.
- No snapshot regenerations unless the test requires them.

## Delegation decision

`Delegation decision: not delegated because smaller than a work order (one assertion + comment).`

## Checklist

- [ ] Update `e2e/visual.test.ts` active-pill expectation to TEAL
- [ ] Push to PR branch
- [ ] Confirm Web CI / note remaining risks

## Validation

- Local: inspect assertion; full smoke optional (Playwright heavy)

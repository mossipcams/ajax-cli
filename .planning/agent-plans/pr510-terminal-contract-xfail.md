# PR 510 terminal contract expected failures

## Scope

Keep PR 510 as the behavior-contract and legacy-removal change while making its
intentionally absent terminal explicit to Playwright. Do not add implementation
code, skip a test, or change an assertion.

## Approval and delegation

- Approval status: approved 2026-07-15.
- Delegation decision: not delegated because the model-router classifies this
  as a one-line test-metadata edit; delegating it would cost more than the edit.
- Approved exception: the 27 contract cases temporarily expect failure in PR
  510 and execute normally. The stacked implementation PR removes this marker.

## Task

- [x] Test: use failing PR 510 Web CI as RED (27 cases fail at the intentionally
  absent `task-terminal-panel`).
- [x] Implementation: add one file-scoped `test.fail` annotation to
  `e2e/terminal-behavior.test.ts`.
- [x] Verification: focused 27-case run reports expected failures with exit 0;
  full mobile-WebKit smoke and `npm run verify` pass; push and wait for PR CI.

## Validation ledger

- PR 510 Web CI before change: exit 1; all 27 terminal cases fail because the
  replacement surface is intentionally absent. Aggregate CI fails only through
  Web.
- Focused terminal contract after annotation: exit 0; all 27 cases executed and
  were reported as passed expected failures in 34.1s.
- `npm run web:smoke -- --project=mobile-webkit`: exit 0; 54 passed, one
  existing visual test skipped. The 27 terminal cases all executed.
- `npm run verify`: exit 0; 1,579 nextest tests, doc tests, web type/Svelte
  checks, and 245 Vitest tests passed.

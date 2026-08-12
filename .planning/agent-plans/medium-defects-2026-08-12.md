# Medium defects — 2026-08-12

## Scope

Fix the confirmed open medium Web Cockpit and CLI defects identified after PR
#849. Keep the changes small, preserve the existing task-authority boundaries,
and update the current PR branch.

## Exclusions

- #846 is fixed by PR #849.
- #848 is already covered by the root-level result panel; no reproduction or
  code change is currently needed.
- #842 conflicts with the intentional swipe-only dashboard action design.

## Tasks

- [x] #818/#808 — strict Diff route and PR query parsing. Focused route tests: 10 passed.
- [x] #814 — reject empty and whitespace-only CLI task titles. Focused CLI test: 1 passed.
- [x] #843 — surface incompatible task-detail responses as recoverable errors. Focused hook tests: 13 passed.
- [x] #835 — revalidate cockpit connection after returning to dashboard. Focused polling suite: 8 passed.
- [x] #805 — add ErrorBoundary recovery action. Focused boundary tests: 5 passed.
- [x] #804 — clear update banner when the server rolls back to boot version. Focused version-monitor tests: 6 passed.
- [x] Run focused and full verification; update PR description and issue links.

## Verification

Focused tests run after each task, followed by the repository verify gate and
the pre-commit release build/install checks before updating the PR.

Results so far:

- Focused web tests: 42 passed; focused CLI regression: 1 passed.
- `npm run web:check`: passed.
- `npm run verify`: passed — 1,850 Rust tests; 755 web tests passed and 9 skipped.
- Release build/install checks: passed in the commit hook for commit `6704be5`.

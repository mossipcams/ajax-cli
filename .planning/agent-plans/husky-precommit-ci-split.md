# Slim pre-commit; CI owns the suite; strip Cursor PR footers

Approval: user requested immediate implementation (2026-08-18), after
deciding local work goes straight to PR with no intermediate commits.

## Scope

- Shrink `.husky/pre-commit` to commit-honesty checks only.
- Add a CI stale-`dist/` check so `--no-verify` cannot ship #593.
- Add `scripts/gh-pr-create` that strips Cursor footer lines from the PR body.
- Mirror that strip in a Husky commit-msg hook for commit messages.
- Update `docs/agent/pull-requests.md` to match.

## Non-goals

- Playwright, clippy, nextest, release build, or `cargo install` on commit.
- Changing CI job graph besides the dist freshness step.
- Commit, push, or open a PR unless the user asks.

## Implementation

- [x] `.husky/pre-commit`: staged LOC, `cargo fmt --check`, and `web:build` +
      `git add dist` only when staged paths under the embedded web/embed
      surface changed. Never `web:build:check`. No `npm run verify`, no
      release build, no `cargo install`.
- [x] CI `web` job: `npm run web:build` then
      `git diff --exit-code crates/ajax-web/web/dist`. Update
      `scripts/verify-ci-workflows.mjs` if it must guard the new step.
- [x] `scripts/gh-pr-create`: wrap `gh pr create "$@"`, strip exact Cursor
      footer lines, `gh pr edit` when the body changed, print the PR URL.
- [x] Husky `commit-msg`: same strip on `$1` (COMMIT_EDITMSG).
- [x] Focused test for the stripper.
- [x] `docs/agent/pull-requests.md`: local hook is the slim pre-commit;
      delegates create PRs via `scripts/gh-pr-create`.

## Verification

- [x] `node --test scripts/strip-cursor-attribution.test.mjs` — pass
- [x] `node scripts/verify-ci-workflows.mjs` — pass
- [ ] Full `npm run verify` — not run (CI owns the suite on this flow)

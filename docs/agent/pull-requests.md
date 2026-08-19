# Pull Requests, CI, and Releases

Read this document before creating or retitling a pull request, changing CI or
release behavior, or preparing a release-sensitive change.

## Commit and pull-request titles

Ajax uses Conventional Commits. Pull-request titles are enforced by CI; commit
messages should use the same vocabulary so Release Please can build
`CHANGELOG.md`.

Keep this section aligned with:

- `.github/workflows/ci.yml` → `pr-title` job `types`;
- `release-please-config.json` → `changelog-sections`;
- `release-please-config.json` → `pull-request-title-pattern`
  (`chore: release ajax-cli <version>`).

| Type | Allowed in PR title | Changelog section | Use for |
| --- | --- | --- | --- |
| `feat` | yes | Features | user-visible feature |
| `fix` | yes | Bug Fixes | bug fix |
| `perf` | yes | Performance Improvements | performance improvement |
| `refactor` | yes | Code Refactoring | behavior-preserving restructure |
| `revert` | yes | Reverts | revert of a prior change |
| `chore` | yes | none, intentionally | tooling, tests-only cleanup, docs/agent hygiene |

Use `type(optional-scope): summary`, for example `fix(web): ...` or
`chore(test): ...`.

- Do not use `test:`, `docs:`, `ci:`, `build:`, `style:`, or any type outside
  the table. The PR Title check fails with `Unknown release type` and skips the
  rest of CI.
- Tests-only or local-suite cleanup uses `chore:` or `chore(test):`, never
  `test:`.
- `chore:` passes PR-title validation but does not bump a version or open a
  Release Please release PR. Use `feat:`, `fix:`, `perf:`, or `revert:` when a
  product release is required.
- `chore: release ajax-cli <version>` is reserved for Release Please's own PRs.
- Prefer a scope when it helps. Confirm the type is in the table before
  `gh pr create` or retitling.

## CI trigger contract

`scripts/verify-ci-workflows.mjs`, run by `npm run verify`, guards this matrix.
Change the workflows and that script together.

| Event | Runs |
| --- | --- |
| Normal PR opened or updated | path-filtered CI lanes once per head; superseded runs cancelled; CodeQL |
| Normal PR merged to `main` | Release Please only; no CI run |
| Release Please updates its PR | Release Candidate job only; superseded runs cancelled; CodeQL |
| Release Please PR merged | tag and GitHub release; no test run |

There is no `push: main` CI run. This is safe only while the `CI` repository
ruleset has `strict_required_status_checks_policy: true`, so the exact up-to-date
PR tree must pass before merge. If that rule is relaxed, restore the
`push: main` trigger in the same change.

The generated Release Please PR skips expensive jobs because every commit it
releases already passed the full suite. Its Release Candidate job checks out
the exact head SHA and verifies:

- a clean merge into current `main` with `git merge-tree --write-tree`;
- one version across `.release-please-manifest.json`, `version.txt`,
  `crates/ajax-cli/Cargo.toml`, and the `ajax-cli` entry in `Cargo.lock`;
- `cargo check --locked -p ajax-cli`.

`release-please-config.json` bumps `Cargo.lock` in place through `extra-files` so
the release PR reaches its final SHA in one update. Its Cargo.lock jsonpath is
`$.package[?(@.name.value=="ajax-cli")].version`; `.value` is required because
Release Please's TOML reader wraps scalars in `{start, end, value}` spans.
`release-type` remains `simple`: `ajax-cli`, sibling crates, and
`workspace.package` intentionally use different versions, while the Rust
strategy would unify them.

CodeQL uses GitHub default setup and cannot exclude the release branch. The
duplicate release-PR scan is accepted; excluding it would require a manually
maintained advanced-setup workflow.

The Web lane runs in the pinned Playwright container image matching
`@playwright/test` in `package-lock.json`. `web-unit` runs tsc, eslint,
ast-grep, vitest, and fails when committed `crates/ajax-web/web/dist` is stale
after `npm run web:build`. `web-e2e` runs the full mobile-webkit Playwright
suite with four CI workers and two retries.

Normal PR jobs:

- `pr-title`, `changes`, `file-loc`, and `invariants` always run.
- `scripts/ci-changed-paths.mjs` emits `rust`, `web`, `lockfile`, and `full`
  from the PR diff. CI-script or workflow diffs, missing SHAs, and
  `workflow_dispatch` set `full=true`.
- `rust-lint`, `rust-test`, and `rust-docs` run when `rust` or `full`.
- `web-unit` and `web-e2e` run when `web` or `full`.
- `audit` runs when `lockfile` or `full`.
- The aggregate `CI` job fails when a needed lane job is skipped or not
  success. Docs/agent-only diffs skip the expensive lanes.

## Who opens the PR

The model-router-selected delegate runs `scripts/gh-pr-create`, not raw
`gh pr create`. After an explicit parent-local bypass, the active agent runs
the same wrapper. The wrapper creates the PR, strips Cursor footer /
co-author lines from the body, and prints the URL. The orchestrator reviews
the delta and reports that URL.

## Local verification gate before a PR

Work goes straight to the PR. There is no separate local full-suite step.
Husky on the PR-creating commit only:

1. Rebuilds and stages `crates/ajax-web/web/dist` when web sources are staged
   (`#593`).
2. Checks staged Rust file size.
3. Runs `cargo fmt --check`.
4. Strips Cursor attribution lines from the commit message (`commit-msg`).

Install Husky with `npm prepare` or `npx husky` so those hooks run. Do not use
`--no-verify` or `--no-gpg-sign` to skip them. CI runs the path-filtered lanes
that the diff can affect and fails if `dist/` is stale.

Focused tests for the code you touched still belong in the worktree before the
PR commit. They do not replace CI.

Record the commands and exit statuses in the persistent plan when one exists
and in the final report. The final report must include:

- what changed;
- persistent-plan path and checklist status, when used;
- verification commands and results;
- failed or skipped commands;
- remaining risks or follow-up work.

Do not claim the repository is clean unless status was checked.

For focused validation commands, use [`architecture.md`](../../architecture.md#validation-fast--slice-local).
For the broader command catalog, see [`README.md`](../../README.md#validation).

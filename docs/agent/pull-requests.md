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
| Normal PR opened or updated | full CI suite once per head; superseded runs cancelled; CodeQL |
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

## Local verification gate before a PR

Do not create a pull request until local tests have passed in the worktree.

Before `gh pr create`:

1. Install Husky with `npm prepare` or `npx husky` so `.husky/pre-commit` runs.
2. Either commit through the Husky hook successfully or run its equivalent
   local suite successfully. The current hook rebuilds and stages the embedded
   web bundle, checks staged Rust file size, runs `npm run verify`, builds
   `ajax-cli` in release mode, and installs it from the locked workspace.
3. If `prek` is available and configured for this repository, it may satisfy
   the gate only when it runs the equivalent suite successfully.

Do not use `--no-verify`, `--no-gpg-sign`, or another bypass merely to open a
PR. Do not open a PR after a failed verification run; fix the failure and rerun
until green. Focused crate tests alone do not satisfy this PR gate.

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

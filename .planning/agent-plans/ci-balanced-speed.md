# CI: balance speed with quality

## Approval

Approved by user request on 2026-08-19: implement a real CI redesign (not a
small trim) that keeps merge quality while cutting wall-clock wait.

## Problem

Successful PRs finish in ~4–5.5 minutes. That wait is two serialized long
poles on every PR, not a bloated job graph:

- Web Playwright e2e: ~190s, one worker, full suite mislabeled as “smoke”
- Nextest: ~210s with `--test-threads=1`

Every PR also pays for a redundant `cargo check` (~80–100s billed, no wall-clock
help next to Clippy) and skips the JS unit/lint layer that `npm run verify`
already runs locally.

## Goal

Keep the merge bar at least as strong as today, and faster when the diff
cannot affect a lane.

- Never skip the tests that can catch the change.
- Close the JS quality gap (tsc / lint / ast-grep / vitest belong in CI).
- Parallelize the expensive suites instead of deleting them.
- Keep the single required aggregate check named `CI`.
- Keep the no-`push: main` + Release Please cheap-path contract.

## Non-goals

- Do not add a `push: main` test run.
- Do not move full Playwright off PRs onto a nightly-only gate.
- Do not weaken Clippy, nextest, rustdoc `-D warnings`, or Playwright retries.
- Do not change exploratory-testing.yml (already off the PR path).
- Do not change Release Please’s cheap candidate job beyond leaving it intact.

## Design

### Path classes

Add `scripts/ci-changed-paths.mjs` plus `scripts/ci-changed-paths.test.mjs`.
Compute from `GITHUB_BASE_SHA` / `GITHUB_HEAD_SHA` (same pair `file-loc` uses).
On `workflow_dispatch` or missing SHAs, set `full=true`.

| Output | True when the diff touches |
| --- | --- |
| `rust` | `*.rs` under `crates/` except `crates/ajax-web/web/**`; any `Cargo.toml`; `Cargo.lock`; `rust-toolchain*`; `rustfmt.toml`; `clippy.toml`; `.config/nextest.toml` |
| `web` | `crates/ajax-web/web/**`; `package.json`; `package-lock.json`; Playwright configs |
| `lockfile` | `Cargo.lock` |
| `full` | `.github/workflows/**`; `scripts/verify-ci-workflows.mjs`; `scripts/ci-changed-paths.mjs`; `scripts/ci-changed-paths.test.mjs`; or SHA resolution failed |

`crates/ajax-web/web/**` is **not** rust. Web-only PRs must not compile Rust.
`crates/ajax-web/src/**` is rust, not web: ajax-web server tests live in nextest,
and current e2e mocks the API.

`merge_group` uses the merge-candidate base/head pair, so combined queue diffs
union correctly. Do not force `full` on every merge-group run.

### Job graph

```
pr-title          always
changes           always (emits rust/web/lockfile/full)
file-loc          always except release-please
invariants        always except release-please; npm ci + npm run ci:verify
rust-lint         rust|full; fmt + clippy --locked --all-targets --all-features -D warnings
rust-test         rust|full; nextest (default threads) + cargo test --doc
rust-docs         rust|full; cargo doc --no-deps --all-features -D warnings
audit             lockfile|full
web-unit          web|full; tsc, lint, ast-grep, vitest, web:build + dist freshness
web-e2e           web|full; current Playwright container + full mobile-webkit suite
release-candidate release-please only (unchanged)
ci                aggregate required check (unchanged name)
```

Delete standalone `format`, `check`, `clippy`, `test`, `docs`, `web`, `audit`
job IDs. The aggregate `needs` list and `scripts/verify-ci-workflows.mjs` must
move with them.

Release Please still skips every expensive job. Candidate still runs
`cargo check --locked -p ajax-cli`.

### Quality additions (not just cuts)

- **web-unit** is new. Today CI never runs `web:check`, `web:lint`, `web:sg`,
  or `web:test`. Those become a required web-lane job.
- **web-e2e** stays the full e2e tree (not a 5-test smoke). Rename the step so
  the log matches reality. Keep retries at 2. Raise CI workers from 1 to 4
  (`playwright.config.mts`).
- **Clippy absorbs `cargo check`** via `--locked`. Lockfile drift still fails
  the rust lane.
- **Nextest keeps the same tests.** Drop `--test-threads=1` from CI,
  `package.json` `verify`, and README. Do not add a serial `nextest.toml`
  unless a focused run shows a real shared-resource collision.

### Aggregate `CI` rules

`pr-title` must succeed.

Release Please branch: `release-candidate` must succeed; path jobs may skip.

Normal PR:

- `changes`, `file-loc`, `invariants` must succeed.
- If `rust` or `full`: `rust-lint`, `rust-test`, `rust-docs` must succeed.
- If `web` or `full`: `web-unit`, `web-e2e` must succeed.
- If `lockfile` or `full`: `audit` must succeed.
- A needed job that is `skipped` or not `success` fails the aggregate.
- An unneeded job that is `skipped` is success.
- If `changes` failed, fail the aggregate (do not treat missing outputs as
  docs-only).

Docs/agent-only diffs: title + file-loc + invariants + green aggregate.
That is the intended fast path.

### Invariant script

Rewrite `scripts/verify-ci-workflows.mjs` in the same change:

- No `push` trigger; keep `pull_request` + `merge_group`.
- New job IDs exist; old heavy IDs do not.
- Release skip still uses `!startsWith(github.head_ref, release-please…)`.
- Path-filtered jobs key off `needs.changes.outputs.*`.
- Aggregate still named `CI` and still branches release vs normal.
- `web-unit` fails on stale `crates/ajax-web/web/dist`.
- `web-e2e` stays on `mcr.microsoft.com/playwright:v1.61.1-noble` with
  `--ipc=host`, `safe.directory`, `HOME=/root`, timeout 20, no extra
  Playwright install/cache.
- Clippy invocation includes `--locked`.
- Nextest invocation does not include `--test-threads=1`.
- Playwright config CI workers are 4, not 1.

### Docs

Update `docs/agent/pull-requests.md` CI trigger contract to the new matrix
and job list. Update README validation so it no longer documents
`--test-threads=1` or a standalone `cargo check` as the CI shape.

## Expected wall clock

| Diff | Today | After (approx) |
| --- | --- | --- |
| Web-only | 4–5.5 min (rust+e2e) | ~1–2 min (unit + 4-worker e2e) |
| Rust-only | 4–5.5 min (e2e+serial nextest) | ~2 min (parallel nextest, no e2e) |
| Full stack | 4–5.5 min | ~2–3 min (both lanes, parallelized) |
| Docs/agent | 4–5.5 min | ~20–40 s |
| Release Please | ~40 s | unchanged |

## Checklist

- [x] `scripts/ci-changed-paths.mjs` + tests
- [x] Playwright CI workers 4
- [x] Rewrite `.github/workflows/ci.yml`
- [x] Rewrite `scripts/verify-ci-workflows.mjs` for the new graph
- [x] Drop `--test-threads=1` from CI / `package.json` / README
- [x] Update `docs/agent/pull-requests.md`
- [x] `node --test scripts/ci-changed-paths.test.mjs`
- [x] `npm run ci:verify`

## Verification

```sh
node --test scripts/ci-changed-paths.test.mjs
npm run ci:verify
```

Do not claim a GitHub Actions time-down until a real PR run exists.

Parent re-ran both commands on 2026-08-19: pass (12 path tests; `ci:verify` including 72 script tests).

This change set itself touches workflow and classifier scripts, so the first PR will set `full=true` and run every lane. That is intentional. Parallel nextest and 4 Playwright workers should still cut that run versus today’s serial poles.

## Risks

- Path-filter mistakes skip a needed lane. Mitigate with unit tests, `full` on
  CI-script diffs, and aggregate checks that fail when a needed job skipped.
- Nextest parallelism may expose leftover shared-temp flakes. Fix the test or
  mark that case serial; do not put the whole suite back on one thread.
- Playwright workers=4 may flake more. Keep retries=2. Drop to 2 workers only
  if a real CI run shows contention, not preemptively.

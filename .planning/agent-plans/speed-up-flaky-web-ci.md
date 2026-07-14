# Shorten Web CI

## Scope

Shorten normal pull-request CI without dropping checks: cache Playwright's
browser download and make terminal-test screenshots local diagnostics only.
Keep both browser projects, retries, and all assertions.

Approval status: approved by user on 2026-07-14.

## Non-goals

- Do not remove browser coverage, reduce retries, or weaken assertions.
- Do not alter Rust checks, tests, or dependency auditing.

## Delegation decision

Delegation decision: not delegated because model-router selected the local lane
for this one-file, under-ten-line workflow-cache edit.

```yaml
ROUTING_DECISION:
  ACTION: LOCAL
  LANE: local
  MODE: NONE
  MODEL: NONE
  PACKET_STATUS: NOT_REQUIRED
  ALLOWED_SCOPE: [".github/workflows/ci.yml", "crates/ajax-web/web/e2e/terminal-scroll-garble.test.ts"]
  REASON: One-file, under-ten-line workflow-cache edit; user approved implementation.
  ESCALATE_IF: ["cache is ineffective", "diagnostic removal affects an assertion"]
```

## Tasks

- [x] Task 1 — cache Playwright browsers in the Web job.
  - Test first: none; this is a workflow-cache-only change with no local
    behavior assertion to add.
  - Implementation: cache `~/.cache/ms-playwright`, keyed by runner OS and
    `package-lock.json`, before `playwright install`.
  - Verify: CI Web job passes; a subsequent matching run reports a cache hit
    and skips the ~101-second browser download.

- [x] Task 2 — retain assertions but skip artifact-only screenshots in CI.
  - Test first: none; existing marker-contiguity assertions are the behavior
    checks and remain unchanged.
  - Implementation: guard the four diagnostic `page.screenshot` calls so
    local debugging retains them but CI cannot time out after assertions pass.
  - Verify: `CI=1 npm run web:smoke`; terminal-scroll tests pass without
    screenshot timeouts.

## Expected outcome

The normal Web critical path should fall from about 5m38s to roughly 4m,
primarily by avoiding the 101-second browser install. The 18-minute flaky
tail is removed without reducing coverage.

## Validation

- `CI=1 npm run web:smoke -- --grep 'terminal scroll garble repro'` — passed:
  8 tests in 15.2s.
- `CI=1 npm run web:smoke` — passed: all 96 tests.
- `npm run web:check` — passed: no errors or warnings.
- `npm run web:test -- --run` — passed: 560 tests.
- `npm run web:build:check` — passed.
- Ruby YAML parse and `git diff --check` — passed.
- Remote CI still must establish the first cache entry; the following matching
  run must report a cache hit.

## Evidence

- Run 29350657313: full CI 5m56s; Web 5m38s; browser installation 1m41s;
  smoke tests 3m02s.
- Run 29348104812: Web 18m12s; six Chromium screenshot timeouts retried twice
  after their assertions had passed.

## Deviations

- A local two-worker benchmark was not run. The safer cache-and-diagnostics
  changes provide the measured win without increasing browser concurrency.
- Before `npm ci`, `CI=1 npx playwright test --config
  crates/ajax-web/web/playwright.config.mts --workers=2` failed with
  `ERR_MODULE_NOT_FOUND` for `@playwright/test`; this was an environment setup
  failure, not a code failure.
- Task 1 validation: Ruby parsed `.github/workflows/ci.yml` successfully and
  `git diff --check` passed. Cache-hit behavior requires a remote CI run.
- `npm ci` installed lockfile-pinned development dependencies locally for
  validation only; it left no tracked changes.

## Review gate

Accepted locally: the diff is limited to the browser cache and the four
post-assertion diagnostic screenshots; all behavior assertions remain intact.

## Revision: Playwright-only Web CI

Revision approval status: approved by user on 2026-07-14.

### Scope

Keep the existing pre-commit `npm run verify` for Web type and unit checks.
Make the CI Web job run only the mobile-WebKit Playwright smoke suite,
retaining its WebKit browser cache and removing its duplicated checks.
Run that project with Playwright's `iPhone 15 Pro` device preset.

### Non-goals

- Do not change the pre-commit hook or Rust CI jobs.
- Do not remove Playwright smoke coverage or test assertions.

### Routing decision

```yaml
ROUTING_DECISION:
  ACTION: LOCAL
  LANE: local
  MODE: NONE
  MODEL: NONE
  PACKET_STATUS: NOT_REQUIRED
  ALLOWED_SCOPE: [".github/workflows/ci.yml", "crates/ajax-web/web/playwright.config.mts"]
  REASON: Each configuration edit is a one-file, mechanical change under ten lines.
  ESCALATE_IF: ["the Web job runs a check other than mobile-WebKit Playwright smoke", "mobile-WebKit coverage is removed"]
```

### Tasks

- [x] Task 3 — make the Web job mobile-WebKit-Playwright-only.
  - Test first: none; this is workflow-only removal explicitly requested by
    the user. The existing local hook retains `web:check` and `web:test`.
  - Implementation: remove Web type checking, unit tests, and build checking;
    cache/install WebKit only; run `npm run web:smoke -- --project=mobile-webkit`.
    Change the project's device preset from `iPhone 12` to `iPhone 15 Pro`.
  - Verify: parse the workflow YAML; run the same mobile-WebKit command;
    confirm the project uses the iPhone 15 Pro preset and the Web job contains
    no other check or browser project.

### Expected outcome

The Web job has one purpose: iPhone 15 Pro mobile-WebKit smoke coverage. Type
and unit checks stay in the local pre-commit hook; Rust CI remains unchanged.

### Revision validation

- `CI=1 npm run web:smoke -- --project=mobile-webkit` — passed: 46 tests,
  with 2 desktop-only skips.
- Workflow YAML parsed successfully; an assertion confirmed the Web job has
  only WebKit installation and the mobile-WebKit smoke command.
- The configured `iPhone 15 Pro` preset exists in the pinned Playwright
  version; `git diff --check` passed.

### Deviation

- The CI run exposed an existing workflow-contract test that required the
  removed Web build check. Its assertion must change to the explicitly
  requested mobile-WebKit smoke contract.

### Follow-up task

- [x] Task 4 — update the Web CI workflow contract test.
  - Test first: `cargo test -p ajax-cli ci_web_job_runs_web_build_check` fails
    because it requires the removed build check.
  - Implementation: assert the mobile-WebKit smoke command instead.
  - Verify: rerun the renamed focused test and `cargo nextest run -p ajax-cli`.

Task 4 validation: the focused contract test passed; `cargo nextest run -p
ajax-cli` passed 334 tests.

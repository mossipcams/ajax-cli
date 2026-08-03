# Plan: Fix PR #745 Web CI — stale “Task started” e2e

## Status

**Approved** — implementing.

## Delegation decision

`Delegation decision: not delegated because` single-file mechanical e2e assertion update with exact anchors already in the approved plan (~20 lines); smaller than a useful work order.

## Failure summary

| Check | Run | Role |
| --- | --- | --- |
| **Web** (root cause) | [job 91731361836](https://github.com/mossipcams/ajax-cli/actions/runs/30827060189/job/91731361836) | Playwright e2e failed |
| **CI** (cascade) | [job 91732497048](https://github.com/mossipcams/ajax-cli/actions/runs/30827060189/job/91732497048) | Aggregate gate failed because Web ≠ success |

Head SHA: `e46a21a6` · Branch: `ajax/toasts` · PR: [#745](https://github.com/mossipcams/ajax-cli/pull/745)

### Root cause

`crates/ajax-web/web/e2e/actions.test.ts:166` — `new task sheet Start submits and reports the task started` still expects:

```ts
await expect(resultPanel(page)).toContainText("Task started");
```

That toast was **removed** on purpose (open the task is the confirmation). Unit tests were updated; this e2e assertion was not.

Retries: failed 3× with the same expectation.

### Out of scope / noise

- Socket Security: success (external URLs only if needed)
- Early `ECONNREFUSED 127.0.0.1:8788` during web-server warmup: not the failing assertion
- Skipped e2e tests in the same run: unrelated

## Proposed fix

### Task 1: Align e2e with no-toast start  [Behavior Change]

- [x] Update `actions.test.ts` test (rename + assertions)
- [x] Grep e2e for other stale success-toast expectations — only this one
- [x] Validate: `npx playwright test … -g "new task sheet Start"` → PASS
- [ ] Commit + push to PR #745

### Non-goals

- Restoring the toast
- Changing ResultPanel / ActionBar further
- Touching Rust CI jobs

## Delegation decision (after approval)

`Delegation decision: delegated via model-router` — tiny e2e assertion update (≤2 files).

## Validation commands

```bash
# Prefer the same Playwright project CI uses for this file, e.g.:
npx playwright test crates/ajax-web/web/e2e/actions.test.ts -g "new task sheet Start"
# or repo npm script equivalent used by the Web job
```

Then `gh pr checks 745` after push.

## Risks

- Fixture may not include `web/add-logout` card after start — navigation assert may need to check hash `#/task/...` rather than list text. Inspect `startTaskHandle` + mock cockpit before coding.

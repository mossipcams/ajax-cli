# Plan: Reduce exploratory Composer token usage

## Scope

Keep cloud-only Ajax Web exploratory testing (GHA + Composer 2.5 + Playwright MCP
+ oracles + memory + validate + issue filing). Cut token use by shortening the
default budget, capping agent relaunches, adding marginal-value stop criteria,
and running WebKit only.

## Non-goals

- Deterministic Playwright regression suite
- Planner/executor two-call architecture
- Model change away from Composer 2.5
- Changing product e2e (`web:smoke`, `playwright.config.mts` desktop-chromium)

## Implementation

- [x] Default budget 25 → 12 minutes; keep `workflow_dispatch` override; budget is a maximum
- [x] `run-agent.sh` MAX_ATTEMPTS 8 → 2; second launch only on premature clean exit with time left and no stop-reason
- [x] Charter + prompt: stop criteria, info-per-turn, campaign-over-time; remove “minimum / keep going until stopped”
- [x] Playwright MCP + GHA install/cache: WebKit only; no Chromium/Firefox fallback
- [x] Docs, `verify-ci-workflows.mjs`, and exploratory tests updated

## Verification

```bash
node scripts/verify-ci-workflows.mjs
node --test scripts/exploratory-helpers.test.mjs scripts/exploratory-file-issues.test.mjs scripts/exploratory-memory.test.mjs scripts/exploratory-oracles.test.mjs
```

Results (2026-08-12): `verify-ci-workflows.mjs` exit 0; 27/27 exploratory tests pass.

# Plan: Nightly exploratory defect discovery

## Approval

- Approved by the user on 2026-08-26.
- Supersedes `web-exploratory-testing.md` for future exploratory work.

## Scope

- Keep the isolated Ajax server, disposable git remote, WebKit MCP, read-only
  checkout guard, artifacts, Actions memory cache, and issue integration.
- Run one scheduled workflow and exactly one Cursor explorer process nightly.
- Select one deterministic, change-aware mission with one fallback.
- Reuse the checked-in fake ACP fixture so session behavior is runnable without
  live harness credentials.
- Validate evidence and classify findings as novel, known, or regression before
  filing issues or updating memory.

## Non-goals

- Live ACP exploration.
- A second verifier agent.
- Base/head replay.
- Product fixes found by exploration.

## Implementation

- [x] Add failing contract tests for one invocation, mission selection,
      fake-ACP preflight, strict confirmation, novelty, and workflow ordering.
- [x] Implement deterministic mission planning, structured memory/cooldowns,
      seeded fixtures, and fake-ACP preflight through the `agent acp` stub.
- [x] Reduce the workflow to one nightly explorer process and tighten its
      permissions and completion reporting.
- [x] Enforce Ajv-backed schemas/evidence, remove fabricated reproduction success,
      classify novel/known/regression findings, file issues before memory update.
- [x] Update exploratory documentation.
- [x] Run focused tests and the fixture-backed local pipeline.
- [x] Inspect the first scheduled nightly artifact.

## Verification

- `node --test scripts/exploratory-helpers.test.mjs scripts/exploratory-file-issues.test.mjs scripts/exploratory-memory.test.mjs scripts/exploratory-oracles.test.mjs scripts/exploratory-missions.test.mjs`
- `node scripts/verify-ci-workflows.mjs`
- Fixture-backed preparation, server startup, seed, mission preflight, strict
  validation, classification, memory update, and teardown.

## Validation results

- 2026-08-26: exploratory contract tests pass (46/46).
- 2026-08-26: `node scripts/verify-ci-workflows.mjs` pass.
- 2026-08-26: local fixture pipeline — prepare-instance, plan-mission,
  validate-run --fixture.
- 2026-08-26: built `ajax-cli` release and ran start → wait-ready → seed →
  preflight-fake-acp (wrapper + seeded task verify) → validate-run --fixture →
  classify-findings → stop-server.
- 2026-08-28: inspected scheduled run
  [33099612279](https://github.com/mossipcams/ajax-cli/actions/runs/33099612279)
  (2026-08-27, `main` @ `42805355`, old workflow — redesign is still local).
  Agent completed Happy path in ~3.5 minutes. Two confirmed findings were
  auto-filed: [#1095](https://github.com/mossipcams/ajax-cli/issues/1095)
  (new terminal overlay pointer block, 2/2) and
  [#1096](https://github.com/mossipcams/ajax-cli/issues/1096) (duplicate of
  open [#925](https://github.com/mossipcams/ajax-cli/issues/925), 1/1
  reproductions). Garbage-hashes has been “mandatory next run” for many
  consecutive nights; CI ACP auth still blocks session/composer probes.

## Deviations

- Mission seeding authenticates via public `POST /api/session` (browser session
  cookie) before `POST /api/tasks`; no product-source auth bypass was required.
- Preflight verifies an existing seed from `seed.json` rather than re-seeding.

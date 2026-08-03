# CI per-file LOC limits

## Scope

- Add a CI check that measures **total lines per changed source file**.
- Emit a GitHub Actions warning when a changed file is 600+ lines.
- Fail when a changed file is 1000+ lines.
- Wire the job into the aggregate `CI` required check for normal PRs.

## Non-goals

- PR diff-size limits.
- Failing CI on untouched legacy large files.
- Local `npm run verify` gate (logic is covered by script tests).

## Delegation decision

Not delegated: bounded CI script + workflow wiring; smaller than a delegate packet.

## Checklist

- [x] `scripts/check-file-loc.mjs` with exported pure helpers and CLI
- [x] `scripts/check-file-loc.test.mjs`
- [x] `file-loc` job in `.github/workflows/ci.yml`
- [x] Aggregate `ci` job requires `file-loc` on normal PRs
- [x] `scripts/verify-ci-workflows.mjs` asserts `file-loc` wiring
- [x] Run `node --test scripts/check-file-loc.test.mjs` — pass
- [x] Run `node scripts/verify-ci-workflows.mjs` — pass

## Validation

```bash
node --test scripts/check-file-loc.test.mjs
node scripts/verify-ci-workflows.mjs
```

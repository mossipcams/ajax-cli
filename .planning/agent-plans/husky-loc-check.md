# Husky LOC check

## Scope

Run the existing LOC checker from Husky against staged source changes, with
thresholds for PR total changes, changed lines per file, and total production
lines per file.

## Checklist

- [x] Add tested thresholds and staged diff parsing.
- [x] Read staged file contents for total production LOC.
- [x] Run the checker from `.husky/pre-commit`.
- [x] Verify focused tests and repository CI verification.
- [x] Run the full `npm run verify` suite.

## Validation

- `node --test scripts/check-file-loc.test.mjs` — passed, 13 tests.
- `node scripts/check-file-loc.mjs --staged` — passed; no staged source files
  were present at runtime.
- `npm run ci:verify` — passed, 51 tests.
- `npm run verify` — passed, including Rust checks/tests, web checks, 763 web
  tests passed with 9 skipped, and the 51 Node tests.

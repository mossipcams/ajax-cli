# TypeScript 7 Migration

## Scope

- Migrate the web toolchain from TypeScript 5.x to TypeScript 7.0.2, the current npm `latest`.
- Keep the change as small as possible: package manifests first, source fixes only when TypeScript 7 reports real incompatibilities.
- Do not modify files in a top-level or nested `tests/` directory.

## Non-Goals

- No broad dependency upgrades unless TypeScript 7 peer constraints force them.
- No Svelte, Vite, Vitest, or Playwright migration unless validation proves it is required.
- No UI behavior changes.
- No Rust behavior changes.

## Delegation Decision

- Delegation decision: not delegated for the initial package/lock update because it is a mechanical manifest change smaller than a useful handoff.
- If TypeScript 7 produces nontrivial multi-file source fallout, pause before source edits and route the first bounded compatibility fix through `model-router` with a TDD implementation packet.

## Current Baseline

- `npm view typescript dist-tags version --json` reports `latest: 7.0.2`.
- `npm run web:check` currently fails before checking code because `node_modules` is absent: `svelte-check: command not found`.
- `npm run web:test -- --run` currently fails before running tests because `node_modules` is absent: `vitest: command not found`.
- `npm ls typescript --depth=0` reports no installed dependency because `node_modules` is absent.

## Task Checklist

- [x] Task 1: Install current dependencies for a clean baseline.
  - Test to write: none; this is environment setup, not behavior.
  - Code to implement: none.
  - Verify: run `npm install` if needed, then `npm run web:check` and `npm run web:test -- --run` on the existing TypeScript lock state.

- [x] Task 2: Move the package manifests to TypeScript 7.0.2.
  - Test to write: none; the failing test-first signal is the existing typecheck/test suite under the TypeScript 7 dependency.
  - Code to implement: update `package.json` and `package-lock.json` so the root dev dependency resolves to TypeScript 7.0.2.
  - Verify: run `npm run web:check` and `npm run web:test -- --run`; record the expected failure if TypeScript 7 exposes incompatibilities, or mark green if it passes with no source changes.

- [x] Task 3: Fix TypeScript 7 compatibility errors, only if Task 2 turns red.
  - Test to write: no new tests unless a runtime behavior regression appears; use the failing `svelte-check` or Vitest output as the red test.
  - Code to implement: the smallest source/config edits needed for the exact TypeScript 7 diagnostics.
  - Verify: rerun the focused failing command until green, then rerun both `npm run web:check` and `npm run web:test -- --run`.

- [x] Task 4: Final validation.
  - Test to write: none.
  - Code to implement: none unless validation exposes a migration issue.
  - Verify: run `cargo fmt --check`, `cargo check --all-targets --all-features`, and the web checks. Run broader cargo checks only if the web build affects embedded assets or Rust compile surfaces.

## Deviations

- `svelte-check` 4.1.4 and 4.7.2 both crash under `typescript@7.0.2` because TS7 no longer exposes the legacy `typescript.sys` compiler API shape. Added a `typescript-5` alias and a tiny wrapper for `svelte-check` while keeping root `typescript` on 7.0.2.
- Added `tsc -p crates/ajax-web/web/tsconfig.check.json --noEmit` to `web:check` so TS7 is actually exercised by the normal check command.
- Added `declare module "*.css";` because TS7 reports side-effect CSS imports without an ambient module declaration.

## Validation Results

- `npm install`: passed; installed 163 packages, 0 vulnerabilities.
- `npm run web:check`: passed on existing TypeScript lock state; 0 errors, 0 warnings.
- `npm run web:test -- --run`: passed on existing TypeScript lock state; 31 files / 381 tests.
- `npm install --save-dev typescript@7.0.2`: passed.
- `npm run web:check`: failed after TypeScript 7 bump because `svelte-check` could not read `typescript.sys`.
- `npm run web:test -- --run`: passed after TypeScript 7 bump; 31 files / 381 tests.
- `node_modules/.bin/tsc -p crates/ajax-web/web/tsconfig.check.json --noEmit`: failed before CSS declaration with TS2882 for `./styles.css`.
- `npm install --save-dev svelte-check@4.7.2`: passed, but `svelte-check` still failed on the same TS7 compiler API incompatibility.
- `npm install --save-dev typescript-5@npm:typescript@5.9.3`: passed.
- `npm run web:check`: passed after TS7 tsc step, CSS declaration, and `svelte-check` wrapper; 0 errors, 0 warnings.
- `npm run web:test -- --run`: passed after final changes; 31 files / 381 tests.
- `cargo fmt --check`: passed.
- `cargo check --all-targets --all-features`: passed.
- `npm run web:build`: passed; Vite emitted its large chunk warning.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo nextest run --all-features --test-threads=1`: passed; 1543 tests.
- `cargo test --doc`: passed; 0 doctests.

## Approval

- Status: approved by user: "implement until finished".

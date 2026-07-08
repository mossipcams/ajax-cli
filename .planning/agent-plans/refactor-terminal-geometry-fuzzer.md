# Refactor terminal geometry fuzzer

Date: 2026-07-08

## Scope

- Refactor `crates/ajax-web/web/src/terminalGeometry.fuzz.test.ts`.
- Make the fuzzer more accurate and thorough for `terminalGeometry` pure helpers:
  `flooredCols`, `clampPan`, `fitCapFontSize`, `pinchActivated`, and `pinchFontSize`.
- Keep the run deterministic and replayable under Vitest.
- Do not add dependencies.
- Do not modify the root `tests/` directory.

## Non-goals

- Do not change production geometry behavior unless the stronger fuzzer exposes a real bug.
- Do not introduce a property-testing framework.
- Do not broaden this into component/browser gesture tests.
- Do not touch unrelated web tests or generated `dist/` assets.

## Approval

- Status: approved by user on 2026-07-08.

## Current context

- Existing focused command passes:
  `rtk npm run web:test -- --run crates/ajax-web/web/src/terminalGeometry.fuzz.test.ts`
- Current fuzzer is one 500-iteration invariant loop with a seeded LCG and adversarial scalar values.
- Implementation should happen under the web lane after approval; repo guidance prefers `cursor-delegate` for bounded web changes. I will review any delegate diff before accepting.

## Task checklist

### Task 1: Make generated cases explicit and replayable

- Status: complete.
- Test to write:
  - Add a failing Vitest assertion that the fuzz harness exposes labeled cases including seed, iteration, and chosen input family/corpus labels.
  - Add a failing assertion that the corpus includes required adversarial values: `NaN`, `Infinity`, `-Infinity`, signed zero area, exact boundaries, sub-pixel values, huge finite values, and ordinary phone-sized values.
- Code to implement:
  - Extract scalar generation into small helpers inside `terminalGeometry.fuzz.test.ts`.
  - Add labeled deterministic seeds and case metadata used in assertion messages.
  - Keep the existing LCG or an equivalent no-dependency deterministic RNG.
- Verification:
  - First run should fail on the new expectations before helpers exist.
  - After implementation, run:
    `rtk npm run web:test -- --run crates/ajax-web/web/src/terminalGeometry.fuzz.test.ts`

### Task 2: Replace loose invariants with exact independent oracles

- Status: complete.
- Test to write:
  - Add failing expectations comparing each production helper against an independent expected-value oracle for generated and corpus cases.
  - Include oracle checks for custom clamp bounds on `fitCapFontSize` and `pinchFontSize`.
- Code to implement:
  - Add local expected-value functions in the fuzz file:
    `expectedFlooredCols`, `expectedClampPan`, `expectedFitCapFontSize`,
    `expectedPinchActivated`, and `expectedPinchFontSize`.
  - Assert exact equality where the production functions are specified as exact math.
  - Keep finite/range assertions as secondary diagnostics, not the main proof.
- Verification:
  - First run should fail because oracle assertions reference missing harness/oracle code.
  - After implementation, run:
    `rtk npm run web:test -- --run crates/ajax-web/web/src/terminalGeometry.fuzz.test.ts`

### Task 3: Cover interaction-shaped geometry scenarios

- Status: complete.
- Test to write:
  - Add failing assertions for composed terminal scenarios that mirror real use:
    host-width font cap followed by floored column sizing, pan clamp after content/viewport changes, and pinch scaling against a dynamic max font cap.
- Code to implement:
  - Add generated scenario records using existing helpers rather than new abstractions.
  - Assert cross-helper invariants:
    font cap stays within configured bounds, column floor never drops below the active floor, pan never escapes the new scroll range, invalid pinch distances preserve base size.
- Verification:
  - First run should fail on missing composed scenario support.
  - After implementation, run:
    `rtk npm run web:test -- --run crates/ajax-web/web/src/terminalGeometry.fuzz.test.ts`

### Task 4: Run focused and nearby validation

- Status: complete.
- Test to write:
  - No new test file; this task validates the completed fuzzer and nearby deterministic examples.
- Code to implement:
  - Only fix issues discovered by the stronger fuzzer. If production code changes are needed, stop and update this plan before editing production source.
- Verification:
  - Run:
    `rtk npm run web:test -- --run crates/ajax-web/web/src/terminalGeometry.fuzz.test.ts`
  - Run:
    `rtk npm run web:test -- --run crates/ajax-web/web/src/terminalGeometry.test.ts crates/ajax-web/web/src/terminalGeometry.fuzz.test.ts`
  - Run:
    `rtk npm run web:check`

### Task 5: Convert the fuzzer from parity-checking to defect-seeking

- Status: complete.
- Test to write:
  - Add failing fuzz/regression assertions for geometry contracts not already guaranteed by the copied oracle helpers:
    integer column counts, finite/readable pinch font results for invalid base sizes, inactive pinch activation for invalid thresholds, safe pan for impossible layout sizes, and finite fit caps for invalid clamp bounds.
- Code to implement:
  - First add test-only assertions that fail against current production behavior.
  - Then minimally fix production helpers only for failures that represent real terminal geometry defects.
  - Remove or reduce any self-referential oracle checks that only prove implementation parity if they obscure defect coverage.
- Verification:
  - Red: `rtk npm run web:test -- --run crates/ajax-web/web/src/terminalGeometry.fuzz.test.ts` must fail on real production behavior, not missing test scaffolding.
  - Green: run focused fuzzer, nearby geometry tests, and `rtk npm run web:check`.

## Deviations discovered during execution

- Task 1 stayed within `terminalGeometry.fuzz.test.ts`; no production behavior changes.
- Task 2 was delegated to Cursor chat `6b9bcef1-29ed-40ca-8c5a-21c9b11067f4` in red/green steps; Cursor stayed within `terminalGeometry.fuzz.test.ts`.
- Task 3 was delegated to the same Cursor chat in red/green steps; Cursor stayed within `terminalGeometry.fuzz.test.ts`.
- Concern raised after Task 4: the fuzzer found no defects despite known defects existing. Task 5 corrects this by adding defect-oriented properties instead of copied implementation oracles.

## Validation results

- `rtk npm run web:test -- --run crates/ajax-web/web/src/terminalGeometry.fuzz.test.ts` passed before changes.
- Task 1 red:
  `rtk npm run web:test -- --run crates/ajax-web/web/src/terminalGeometry.fuzz.test.ts`
  failed as expected on unlabeled replay metadata.
- Task 1 green:
  `rtk npm run web:test -- --run crates/ajax-web/web/src/terminalGeometry.fuzz.test.ts`
  passed with 2 tests.
- Task 2 red:
  `rtk npm run web:test -- --run crates/ajax-web/web/src/terminalGeometry.fuzz.test.ts`
  failed as expected with `ReferenceError: expectedFlooredCols is not defined`.
- Task 2 green:
  `rtk npm run web:test -- --run crates/ajax-web/web/src/terminalGeometry.fuzz.test.ts`
  passed with 3 tests.
- Task 3 red:
  `rtk npm run web:test -- --run crates/ajax-web/web/src/terminalGeometry.fuzz.test.ts`
  failed as expected with `ReferenceError: sampleComposedGeometryScenarios is not defined`.
- Task 3 green:
  `rtk npm run web:test -- --run crates/ajax-web/web/src/terminalGeometry.fuzz.test.ts`
  passed with 4 tests.
- Task 4 focused validation:
  `rtk npm run web:test -- --run crates/ajax-web/web/src/terminalGeometry.fuzz.test.ts`
  passed with 4 tests.
- Task 4 nearby validation:
  `rtk npm run web:test -- --run crates/ajax-web/web/src/terminalGeometry.test.ts crates/ajax-web/web/src/terminalGeometry.fuzz.test.ts`
  passed with 36 tests.
- Task 4 type validation:
  `rtk npm run web:check`
  passed with 0 errors and 0 warnings.
- Final whitespace check:
  `rtk git diff --check`
  passed.
- Task 5 red:
  `rtk npm run web:test -- --run crates/ajax-web/web/src/terminalGeometry.fuzz.test.ts`
  failed as expected on `Number.isInteger(flooredCols(80.9, 80))`.
- Task 5 green:
  `rtk npm run web:test -- --run crates/ajax-web/web/src/terminalGeometry.fuzz.test.ts`
  passed with 5 tests.
- Task 5 nearby validation:
  `rtk npm run web:test -- --run crates/ajax-web/web/src/terminalGeometry.test.ts crates/ajax-web/web/src/terminalGeometry.fuzz.test.ts`
  passed.
- Task 5 type validation:
  `rtk npm run web:check`
  passed.

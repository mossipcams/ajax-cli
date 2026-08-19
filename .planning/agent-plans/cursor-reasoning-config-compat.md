# Cursor reasoning config compatibility

## Approval

Immediate implementation is authorized by the ongoing request to resolve issue #989.
The operator supplied a fresh failing repro after PR #992 was opened.

## Scope

- Treat Cursor ACP config option `reasoning` as the current wire name for Ajax's
  semantic effort axis while retaining compatibility with legacy `effort`.
- Reconstruct and match the applied model with its reasoning level.
- Let Task Details decode an ACP bracket-form live snapshot and expose the
  catalog's effort controls.
- Add focused backend and frontend regressions and update the web-session
  behavior contract.

## Non-goals

- Redesign the model picker.
- Infer unavailable catalog variants.
- Change ACP process ownership or teardown.
- Change non-Cursor harness model behavior.

## Checklist

- [x] Add failing Rust regression for `reasoning` config reconstruction/apply.
- [x] Add failing frontend regression for `gpt-5.6-sol[fast=false]` in Task Details.
- [x] Implement the smallest backend `reasoning`/`effort` compatibility mapping.
- [x] Decode ACP bracket snapshots in the Cursor picker.
- [x] Update `docs/architecture/web-session-behavior.md`.
- [x] Run focused Rust and frontend tests.
- [x] Run formatting and file-size checks.
- [x] Commit reasoning compatibility fix (`c1fa55c4`).
- [ ] Merge `origin/main` (PR #992 landed as `21db6dee` before `c1fa55c4` was pushed).
- [ ] Resolve `web-session-behavior.md` merge conflict (preserve #992 spawn contract + reasoning additions).
- [ ] Push follow-up PR (PR #992 already merged; this branch carries the remaining #989 fix).

## Evidence and changed assumptions

- Live `agent --model gpt-5.6-sol-high acp` reports `model=gpt-5.6-sol`,
  `reasoning=high`, and `fast=false`.
- Ajax currently reads only config id `effort`, producing
  `gpt-5.6-sol[fast=false]`; that cannot satisfy the High pin.
- The browser parser does not decode bracket-form ACP snapshots, so it cannot
  associate that live value with the `gpt-5.6-sol` catalog row.
- PR #992 (`fix(web): pass explicit Cursor catalog ids on spawn argv`) merged to
  `origin/main` as `21db6dee` before commit `c1fa55c4` was pushed to this branch.
  The remaining reasoning/effort compatibility work requires a follow-up PR.

## Validation

Completed on branch before merge with main:

- `cargo fmt --check` — pass
- `cargo nextest run -p ajax-web reasoning --no-fail-fast` — pass (6 tests)
- `npm run web:test -- --run crates/ajax-web/web/src/features/session/sessionModel.test.ts crates/ajax-web/web/src/features/session/ModelPicker.test.tsx` — pass (44 tests)
- `FILE_LOC_BASE=$(git rev-parse origin/main) FILE_LOC_HEAD=$(git rev-parse HEAD) node scripts/check-file-loc.mjs` — pass

Pending after merge with `origin/main`:

- Re-run the above verification on the merged branch.
- Push and open follow-up PR via `scripts/gh-pr-create`.

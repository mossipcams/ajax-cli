# Cursor reasoning config compatibility

## Approval

Immediate implementation is authorized by the ongoing request to make PR #992
resolve issue #989. The operator supplied a fresh failing repro after the PR was
opened.

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
- [ ] Run formatting, file-size, and full affected validation.
- [ ] Commit and push the accepted fix to PR #992.

## Evidence and changed assumptions

- Live `agent --model gpt-5.6-sol-high acp` reports `model=gpt-5.6-sol`,
  `reasoning=high`, and `fast=false`.
- Ajax currently reads only config id `effort`, producing
  `gpt-5.6-sol[fast=false]`; that cannot satisfy the High pin.
- The browser parser does not decode bracket-form ACP snapshots, so it cannot
  associate that live value with the `gpt-5.6-sol` catalog row.

## Validation

Focused regressions pass after mapping `reasoning` ↔ effort semantics in
`cursor_config.rs` and decoding bracket-form snapshot ids in `sessionModel.ts`.
Full validation pending commit gate.

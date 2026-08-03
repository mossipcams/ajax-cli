# Clean up AGENTS.md — remove TDD requirement

## Scope

- Remove mandated TDD / test-first workflow from `AGENTS.md`.
- Align verification language with `model-router` (evidence required; TDD not required).
- Light cleanup of related plan/delegation wording.

## Non-goals

- Do not delete the `tdd-implementation-packet` skill or rewrite model-router.
- Do not change product code, CI, or CONTRIBUTING.
- Do not rewrite unrelated PR/CI sections.

## Delegation decision

- Delegation decision: not delegated because this is non-code documentation
  updating repo agent instructions (allowed exception).

## Tasks

- [x] Remove Behavior Change / Refactor test-first mandates
- [x] Replace TDD section with light verification/testing guidance
- [x] Update Persistent Plans + Delegation to drop required TDD packet/loop
- [x] Soften final-response test reporting; verify no remaining TDD mandates

## Validation

- `rtk rg -n 'TDD|tdd-implementation|failing .*test first|TEST_FIRST|red-green' AGENTS.md`
  → only explicit “TDD is not required” wording remains
- Manual read of Task Modes, Plans, Delegation, Testing, PR final response: ok
- Line count: 571 → 550

## Deviations

- Restored required `tdd-implementation-packet` in Delegation after user
  clarification: the packet skill was already redone for outcome-based
  verification; keep that handoff, do not drop the packet step.
- Left explicit “TDD is not required” wording in Testing and Behavior Change.

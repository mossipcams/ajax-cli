# Update AGENTS Delegation Default

## Scope

Update `AGENTS.md` so bounded code-changing Small Fix and Behavior Change work
uses delegation as the default path, and so agents must record the delegation
decision before editing source.

## Non-Goals

- Do not change executable code.
- Do not modify files in `tests/`.
- Do not duplicate architecture guidance.
- Do not expand `AGENTS.md` beyond the requested delegation workflow wording.

## Approval

- Approved by user: "implement until finished"

## Delegation Decision

- Delegation decision: not delegated because this is non-code documentation work
  updating repo agent instructions, an allowed exception.

## Tasks

- [x] Tighten `Task Modes` delegation wording.
  - Test: no automated test; documentation-only wording change.
  - Implementation: replace the weak delegation sentence with the requested
    default-execution-path wording.
  - Verification: inspect the `Task Modes` section and confirm it says
    delegation is the default execution path before editing source.

- [x] Rewrite the opening of `Delegation` as a strict default workflow.
  - Test: no automated test; documentation-only wording change.
  - Implementation: add the default rule, normal-request authorization wording,
    persistent-plan decision requirement, and ordered workflow bullets.
  - Verification: inspect the section and confirm the required decision strings
    and workflow steps are present.

- [x] Narrow direct-implementation exceptions while preserving lane rules.
  - Test: no automated test; documentation-only wording change.
  - Implementation: replace the broad direct-implementation sentence with the
    requested exception list, preserving lane selection, Cursor modes, work-order
    rules, review rules, and do-not-delegate list.
  - Verification: inspect the final `Delegation` section and confirm no lane
    selection behavior or safety stop condition was removed.

- [x] Final validation and report.
  - Test: no automated test; documentation-only wording change.
  - Implementation: none beyond inspection.
  - Verification: run focused text inspection and `git diff -- AGENTS.md`, then
    confirm the requested validation bullets.

## Deviations

- None.

## Validation

- `rtk sed -n '/## Task Modes/,/## Persistent Plans/p' AGENTS.md` passed.
- `rtk sed -n '/## Delegation/,/## Non-Negotiable Rules/p' AGENTS.md` passed.
- `rtk git diff -- AGENTS.md` passed.
- Confirmed the changed wording includes:
  - the word "default" in the delegation rule
  - normal "fix/implement/change/add/update" authorization for delegation
  - required persistent-plan delegation decision before editing source
  - direct implementation limited to recorded allowed exceptions
  - preserved lane selection, Cursor modes, work-order rules, review rules, and
    do-not-delegate list

# Refactor AGENTS Model Routing

## Scope

Update `AGENTS.md` so model and lane decisions are owned by the `model-router`
skill, and implementation delegation always goes through `model-router`.

## Non-Goals

- Do not change executable code.
- Do not modify files in `tests/`.
- Do not duplicate model rankings, model names, or lane-selection rules in
  `AGENTS.md`.
- Do not weaken TDD, persistent-plan, review, validation, or no-commit rules for
  delegates.

## Approval

- Approved by user: "implement until finished. Hold up add an exclusion where it
  doesnt delegate if explicitly told do not delegate by the user"

## Delegation Decision

- Delegation decision: not delegated because this is non-code documentation work
  updating repo agent instructions, an allowed exception.

## Tasks

- [x] Remove model decision policy from `AGENTS.md`.
  - Test: no automated test; documentation-only wording change.
  - Implementation: replace the model-ranking/model-application section with a
    compact rule that model and lane choices belong to `model-router`.
  - Verification: inspect the section and confirm no hardcoded model rankings or
    named model preferences remain.

- [x] Refactor `Delegation` to force `model-router` for implementation routing.
  - Test: no automated test; documentation-only wording change.
  - Implementation: remove the local lane-selection list and direct lane names
    from the workflow; require implementation delegation through `model-router`;
    keep an explicit no-delegation exception when the user says not to delegate.
  - Verification: inspect the section and confirm implementation delegation goes
    through `model-router` and no in-file lane picker remains.

- [x] Preserve safety workflow.
  - Test: no automated test; documentation-only wording change.
  - Implementation: keep persistent-plan decision recording, TDD packet
    requirement, primary-agent planner/reviewer/final-approver role, personal
    validation, resume/reject path, delegate branch restrictions, and
    do-not-delegate list.
  - Verification: inspect the final `Delegation` section against the prior safety
    rules.

- [x] Final validation and report.
  - Test: no automated test; documentation-only wording change.
  - Implementation: none beyond inspection.
  - Verification: run focused `sed` inspections, search for removed model names,
    and review `git diff -- AGENTS.md`.

## Deviations

- None.

## Validation

- `rtk sed -n '/## Model Routing/,/## Non-Negotiable Rules/p' AGENTS.md`
  passed.
- `rtk rg -n "gpt|sonnet|opus|fable|Haiku|Cursor|OpenCode|Composer|MiniMax|GLM|cursor-delegate|opencode-delegate|codex-delegate|Pick the lane|first match wins|model parameter" AGENTS.md`
  returned no matches.
- `rtk git diff --check` passed.
- `rtk git diff -- AGENTS.md` passed.

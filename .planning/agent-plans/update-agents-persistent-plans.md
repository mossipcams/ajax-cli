# Update AGENTS Persistent Plans

## Scope

Update `AGENTS.md` so future code changes save a Markdown plan file and keep it
checked off as work progresses.

## Non-Goals

- Do not change executable code.
- Do not modify files in `tests/`.
- Do not add broader planning workflow machinery.

## Approval

- Approved by user: "implement until finished"

## Tasks

- [x] Add persistent plan artifact rule.
  - Test: none; documentation-only change.
  - Implementation: add a compact `Persistent Plans` section to `AGENTS.md`.
  - Verification: inspect the edited section and review `git diff`.

- [x] Define better plan contents and progress tracking.
  - Test: none; documentation-only change.
  - Implementation: require scope, non-goals, task checklist, approval status,
    deviations, and validation results.
  - Verification: confirm wording explains how to keep plans current as work
    progresses.

- [x] Update completion reporting expectations.
  - Test: none; documentation-only change.
  - Implementation: require final responses to include the persistent plan path
    and checklist completion status.
  - Verification: inspect the Pull Request Expectations section and `git diff`.

## Deviations

- The initial user-visible plan was shown in chat before the new repo rule
  existed. This file was added during implementation so the completed change
  still leaves behind a durable plan ledger.

## Validation

- `rtk sed -n '50,120p' AGENTS.md` passed.
- `rtk sed -n '360,410p' AGENTS.md` passed.
- `rtk git diff -- AGENTS.md` passed.
- `rtk git status --short` passed and showed only `AGENTS.md` plus the new
  `.planning/` plan artifact changed.
- Final section inspection command passed for `AGENTS.md` and this plan file.

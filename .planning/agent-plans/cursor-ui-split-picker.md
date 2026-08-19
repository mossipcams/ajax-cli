# Plan: Cursor split model picker in New Task and Switch

## Scope

Show Cursor Fast (and effort, when the catalog has it) as separate controls in
**New Task** and **Switch**, matching the ACP parameterized split. Persist
composed Ajax catalog ids (`cursor-grok-4.6-high`, `composer-2.5-fast`). Both
surfaces already share `ModelPicker`.

GitHub: [#979](https://github.com/mossipcams/ajax-cli/issues/979).

Non-goals: changing ACP handshake, spawn argv, or `session/set_config_option`
apply; adding filesystem/terminal capabilities; native `<select>` (#936);
browser as a second catalog source.

## Approval

Approved in chat 2026-08-19 (“add the split to the ui in both task creation
and model switch”).

## Contract

- Cursor catalog rows that differ only by `-fast` (or Fast in the label)
  collapse to one model. A Fast Off/On row appears; default Off.
- Effort suffixes on Cursor catalog ids (`-high`, `-xhigh`, …) appear as an
  Effort row when more than one level exists for the selected base. Bridges
  keep the existing `catalog.reasoning` picker.
- `onChange` still emits a persistable catalog id (or `auto`). New Task and
  Switch must not send `grok-4.6|fast=false` as the stored `session_model`.
- Auto remains a model choice. Auto does not imply Fast.
- Shortlist does not list Fast and non-Fast as two models.
- Unknown live snapshot ids still show as the current unknown option.

## Task checklist

- [x] Cursor Fast + Effort split in `ModelPicker` (used by New Task and Switch)
- [x] Compose catalog ids on change (`composer-2.5` vs `composer-2.5-fast`,
      `cursor-grok-4.6-high` vs `cursor-grok-4.6-high-fast`)
- [x] Shortlist dedupes Fast siblings
- [x] Tests: ModelPicker, sessionModel helpers, NewTaskSheet and/or HarnessSwap
- [x] Update `web-session-behavior.md` / `web-cockpit.md`

## Validation

- `npm run web:test -- --run ModelPicker sessionModel modelShortlist NewTaskSheet HarnessSwap`

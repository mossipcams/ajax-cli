# Swipe-back must not show new-task sheet

**Date:** 2026-08-05  
**Mode:** Small Fix / Behavior Change

## Scope

After creating a task and swiping right back to the dashboard/project list,
the new-task sheet must not be visible.

## Non-goals

- Sheet dismiss animation polish (button-nav T3)
- Changing when the New button is available
- Swipe gesture math / terminal swipe suppression

## Root cause

`sheetOpen` is App-local React state and is not tied to the route. After Start,
`onClose` usually clears it, but a late reopen (e.g. iOS click-through onto
New) or a missed close can leave `sheetOpen === true` while the hash is already
on the task. Swipe-back to the dashboard then remounts the list under an still-
open sheet.

## Fix

1. Force `sheetOpen` closed whenever the route is `task`/`diff` **and**
   `sheetOpen` is true (deps include both so late reopens clear).
2. Do not mount `NewTaskSheet` on task/diff even if state briefly races true.
3. Lock with App tests: start → back; sheet open → task → late New click → back.

## Delegation decision

`Delegation decision: delegated via model-router` → composer-2.5 implement.
Parent review tightened the effect deps and extended the regression test.

## Checklist

- [x] Persistent plan + READY packet
- [x] Delegate implement
- [x] Parent review diff (+ effect deps fix)
- [x] Parent re-run focused verification
- [x] Record results

## Validation

```bash
rtk npm run web:test -- --run src/app/App.test.tsx src/features/task/NewTaskSheet.test.tsx
rtk npm run web:check
```

## Deviations

- Delegate reported SUCCESS in prose but runner marked FAILED
  (`MISSING_STRUCTURED_REPORT`). Diff was present; parent verified.
- Parent added `sheetOpen` to the effect dependency list so a New click while
  already on a task cannot leave the sheet latched for swipe-back.

## Validation results

- `rtk npm run web:test -- --run src/app/App.test.tsx src/features/task/NewTaskSheet.test.tsx` → **69/69 PASS**
- `rtk npm run web:check` → **PASS**
- No commit made

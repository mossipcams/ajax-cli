# Fix Drop leave-latch (still yanks to dashboard)

**Date:** 2026-08-11  
**Issue:** https://github.com/mossipcams/ajax-cli/issues/785  
**Mode:** Behavior Change

## Scope

After Drop shell confirm, navigating away (including confirm-before-Confirm and
dashboard → other task) must not force `#/` when Drop finishes.

## Non-goals

- Drop undo timing, toast placement, paste recovery
- Remembering project-filter Back destinations

## Root cause

#783 checked `location.hash` only at Drop API completion. That still races
operator navigation (confirm-time leave, dashboard intermediate, swipe/Back
settle delaying the hash). Need a sticky leave latch from the moment Drop
confirm opens.

## Task checklist

- [x] Open GitHub defect #785
- [x] Leave latch on Drop pendingConfirm + route effect flip
- [x] Route-safe `onDismiss` re-check before `go(#/)`
- [x] Regression tests: confirm-after-navigate; dashboard→other-task
- [x] Focused `App.drop-confirm.test.tsx` — 5/5 pass

## Validation

```bash
npx vitest run src/app/App.drop-confirm.test.tsx
# 5 passed
```

## Remaining

- Rebuild/install ajax-cli and re-check on device
- PR with `Fixes #785` when ready to land

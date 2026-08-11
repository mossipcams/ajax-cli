# Web Cockpit open defects (drop nav, toast, paste link)

**Date:** 2026-08-11  
**Mode:** Behavior Change (three bounded UI fixes)

## Scope

1. After Drop confirm, if the operator navigates to another task (or any non-dropped
   route) before Drop finishes, stay on that route — do not force `#/`.
2. Move the Drop/result toast and its buttons to a thumb-reachable position
   (above bottom nav), not far top-left.
3. Restore native paste of links into the task terminal (empty `clipboardData` +
   `insertText` recovery path).

## Non-goals

- Drop undo timing, registry/lifecycle semantics
- Architecture or public API changes
- Toolbar Paste / clipboard.readText redesign

## Root causes

1. Shell confirm + delayed Drop live in App (`commitConfirmedAction`). That path
   does not pass `isMounted`, so `isMounted?.() !== false` always dismisses to
   dashboard even after navigate-away. (#741’s ActionBar mounted check no longer
   covers the Drop confirm path after #773.)
2. `.result-panel` is `position: fixed; top: …` — Confirm/Undo/Dismiss sit at the
   top of the phone, far from the Drop control and bottom nav.
3. Empty sync `clipboardData` arms `pasteExpectRef`, but `onTextareaInput`
   early-returns on `insertText` and clears the expect flag without sending —
   Safari often delivers paste recovery as `insertText`.

## Desired behavior

| Situation | Outcome |
| --- | --- |
| Drop finishes while still on dropped task (or its diff) | Leave detail → dashboard (or project) |
| Drop finishes after switch to another task / elsewhere | Keep current route |
| Drop confirm / undo toast | Anchored above bottom nav; actions easy to tap |
| Native link paste with empty clipboardData + textarea insert | URL reaches PTY |

## Task checklist

- [x] T1 — route-aware Drop `onDismiss` + App test (confirm → switch task → undo window → stay)
- [x] T2 — result-panel bottom placement + action row layout; CSS/unit pin
- [x] T3 — pasteExpect + insertText recovery; unit/e2e pin
- [x] T4 — focused web:test / web:check

## Validation

```bash
npm run web:test -- --run src/app/App.drop-confirm.test.tsx src/shared/ui/ResultPanel.test.tsx src/features/task/TaskTerminal.test.tsx src/app/App.test.tsx src/features/task/ActionBar.test.tsx src/shared/lib/clipboard.test.ts
# 126 passed
npm run web:check
# passed (exit 0)
npm run web:build
# passed
```

## Deviations

- None. Impeccable findings on pre-existing styles.css bounce/color tokens left unchanged.

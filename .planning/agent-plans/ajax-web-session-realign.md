# Ajax Web Session: banner + task column realign

## Problem

Banner horizontal inset doesn’t match task chrome; without Dashboard/New the
bottom of the task page sits on the home indicator and needs safe-area.

## Spatial thesis

- One `--task-inset: 12px` column: header, interact, session, banner card edge
- Banner: status + actions in one compact toast aligned to that column
- Bottom: meta-details / session keys clear `safe-area-inset-bottom` when nav is gone

## Delegation

`Delegation decision: not delegated because visual craft / layout coherence`

## Checklist

- [x] Shared `--task-inset`; banner rail matches detail-header column
- [x] Compact banner status|actions alignment
- [x] Safe-area bottom for meta (nav-less task)
- [x] Tests + build

## Validation

- App + SessionAttentionBanner + keyboardBandPin: 61 passed
- web:check / web:build: pass
- detect layout: clean

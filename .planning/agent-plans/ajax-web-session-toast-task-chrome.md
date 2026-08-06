# Ajax Web Session: minimal attention toast + task chrome

## Problem

Attention toast is too copy-heavy and weak on actions; task pages still show
Dashboard/New bottom nav, stealing ~72px from the session.

## Scope

- Minimal top toast: status line + kind actions (incl. Open) on the banner
- Hide bottom nav on task + diff routes; reclaim route-scroll bottom padding
- Docs/DESIGN note

## Non-goals

- Protocol / hub / attention derivation changes
- Hiding cockpit top chrome
- Classic PWA packaging

## Delegation

`Delegation decision: not delegated because visual craft / layout coherence`

## Checklist

- [x] Minimal SessionAttentionBanner (status + actions; Open for review/all)
- [x] Hide bottom-nav on task/diff; zero task route-scroll bottom pad
- [x] Tests + DESIGN / web-cockpit notes
- [x] web:check / web:build

## Validation

- SessionAttentionBanner + App + keyboardBandPin: 60 passed
- web:check / web:build: pass

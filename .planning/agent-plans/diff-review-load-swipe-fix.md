# Diff Review load + swipe fixes

Mode: Small Fix / Behavior Change.
Status: in progress.

## Delegation decision

`Delegation decision: not delegated because` live user-reported load/gesture
bugs need a coherent fix across API timeout, soft-fail projection, and capture-
phase swipe; one parent pass is smaller than a multi-packet round-trip.

## Scope

1. Soft-fail Diff Review observation/diff (empty PRs / local fallback) instead of
   hard 502/500 when `gh` is slow/unauth; never fail the HTTP response solely
   because metadata persist failed.
2. Longer client timeout for PR/diff fetches; surface server error body text.
3. Task-detail swipe: capture-phase listeners over the whole page (including
   terminal), visual drag feedback, smooth commit.

## Non-goals

- Approve/comment posting
- Changing terminal ownership model beyond gesture capture for navigation

## Checklist

- [ ] Soft-fail core/web projection + persist non-fatal
- [ ] API timeout + error body for diff fetches
- [ ] Capture-phase smooth swipe anywhere on task detail
- [ ] Tests + focused validation

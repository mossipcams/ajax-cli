# Drop must not yank you off a switched-to task

**Date:** 2026-08-01  
**Mode:** Behavior Change (one bounded fix)

## Scope

After confirming Drop on task A, if the operator navigates to another task (or
any non-A route) before Drop finishes, leave them on that route. Do **not** call
the post-Drop `onDismiss` navigation that currently forces `#/` (main dashboard).

Still leave the detail page when Drop completes while the operator is still
viewing the dropped task (refreshing that detail would 404).

## Non-goals

- Changing Drop undo timing, confirmation taps, or “Drop survives unmount” commit.
- Remembering project-filter context for Back / dismiss destinations.
- Dashboard Drop affordances, architecture, or registry semantics.
- Other action UX nits (track separately in this workstream).

## Root cause

`ActionBar.run` always calls `onDismiss?.()` on successful Drop. App wires that
to `go(dashboardHash())`. Drop’s timer intentionally survives ActionBar unmount
so a late commit still runs — and that late commit still dismisses to dashboard,
yanking the operator off the task they switched to.

## Desired behavior

| Situation when Drop API succeeds | Navigation |
| --- | --- |
| Still mounted on dropped task detail | `onDismiss` (leave detail; today → dashboard) |
| Already unmounted (switched task / back / elsewhere) | no `onDismiss` — keep current route |

## Delegation decision

`Delegation decision: delegated via model-router`

- Round 1: `pi-delegate` / MiniMax → DISCARD (monthly Go usage limit)
- Round 2: `cursor-delegate` / `composer-2.5` → ACCEPT

## Approval

User clarified: Drop then switch to another task → stay on the switched-to task;
do not auto-return to the dashboard.

## Task checklist

### T1 — skip dismiss after navigate-away

- [x] Packet: `.planning/packets/drop-stay-on-switched-task.md`
- [x] Test: confirm Drop → unmount → undo window elapses → API called, `onDismiss` **not** called; still-mounted path still dismisses
- [x] Impl: call `onDismiss` after successful Drop only when ActionBar is still mounted
- [x] Verify focused ActionBar vitest (parent re-ran)

## Validation

```bash
rtk npm run web:test -- --run src/features/task/ActionBar.test.tsx
rtk npm run web:check
```

## Deviations

- Round 1 MiniMax (`opencode-go/minimax-m3`) hit monthly Go usage limit (429) —
  empty diff, DISCARD. Escalated to `cursor-delegate` / `composer-2.5`.
- Delegate structured report schema failed (`INVALID_STRUCTURED_REPORT`); parent
  gated on git delta + re-ran verification personally.

## Validation results

- Parent: `npm run web:test -- --run src/features/task/ActionBar.test.tsx` — 13/13 PASS
- Parent: `npm run web:check` — PASS (exit 0)
- Checklist: T1 complete
- No commit made — tree left dirty for review

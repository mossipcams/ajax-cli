# Button navigations finish like swipe

**Date:** 2026-08-01  
**Mode:** Behavior Change (+ small architecture.md wording update)

## Scope

Make **button** navigations that parallel swipe routes feel finished: exit slide
off-screen, then destination enter animation — same path swipe already uses.

Primary pain (from code + Matt): Back / open-task taps are instant hash cuts
while swipe gets exit + enter. Buttons feel “not done.”

## Non-goals

- Redesigning action pills / Drop confirm / ResultPanel motion
- Changing Diff Review content or terminal gesture ownership
- Full shared navigation framework / React Router
- Forcing bottom-nav Dashboard/Settings into a long exit choreography on every
  chrome hop (enter-only or stay instant — decide in T2)
- Sheet dismiss polish (optional follow-up T3)

## Root cause

| Path | Exit | Enter direction | Feel |
| --- | --- | --- | --- |
| Swipe (task ↔ Diff / Back) | `useSwipePageTransition.animateTo` | `setSwipeEnterDirection` | Finished |
| Button Back / open task / bottom-nav / Drop dismiss | none — `go(hash)` | none (App clears leftover) | Instant cut |

`architecture.md:661-662` currently documents button/bottom-nav as instant.
That sentence is amended in this work to match the intended operator feel:
**swipe-parallel button navigations use the same exit+enter contract.**

## Desired behavior

1. **Task ← Back button** — same as swipe-right: exit right, enter dashboard/project.
2. **Diff ← Back button** — same as swipe-right: exit right, enter task.
3. **Open task (row tap)** — destination slides in from the right (enter-only;
   list has no swipe exit surface today).
4. Bottom-nav / Settings — deferred to T2 (likely enter-only or remain instant).

## Architecture note

Update `architecture.md` Web Cockpit paragraph:

- Before: “button and bottom-nav navigations stay instant.”
- After: Swipe-parallel button navigations (task/Diff Back) use the same
  exit+enter contract as swipe; other chrome navigations may stay instant or
  use enter-only.

## Delegation decision

`Delegation decision: delegated via model-router` — sequential packets T1 → T2;
T3 only if requested.

## Approval

User: diagnose → “Plan then delegate” after confirming page/button transitions
are the real pain. Architecture wording change is in-scope for that ask.

## Task checklist

### T1 — Back buttons reuse swipe exit+enter

- [x] Packet: `.planning/packets/button-nav-swipe-parity-back.md`
- [x] Test: TaskDetail Back waits for commit animation then calls `onBack`;
      sets enter direction `"right"`; DiffReview Back same
- [x] Impl: expose programmatic commit from `useSwipePageTransition`; wire Back
      buttons; update `architecture.md` sentence
- [x] Verify focused vitest + parent re-run (45/45, web:check pass)

### T2 — Open-task enter animation

- [x] Packet: `.planning/packets/button-nav-swipe-parity-open-task.md`
- [x] Test: opening a task sets enter direction so outlet gets
      `ajax-swipe-enter-*`; Settings stays without enter class
- [x] Impl: `navigateHashWithEnter` + App `openTask`
- [x] Verify focused tests (parent: swipeEnter+App 53/53, web:check)

### T3 — Sheet dismiss exit (optional)

- [ ] Only if Matt asks after T1/T2
- [ ] Cancel/backdrop/Escape animate down before unmount

## Validation

```bash
# T1
rtk npm run web:test -- --run src/shared/hooks/useSwipePageTransition.test.tsx \
  src/features/task/TaskDetail.test.tsx \
  src/features/diff/DiffReview.test.tsx
# T2
rtk npm run web:test -- --run src/shared/lib/swipeEnter.test.ts src/app/App.test.tsx
rtk npm run web:check
```

## Deviations

- T1/T2: Cursor `INVALID_STRUCTURED_REPORT`; gated on delta + parent re-verify.
- MiniMax unavailable (monthly limit) — Cursor used for T1/T2.

## Validation results

- T1 parent: swipe/TaskDetail/DiffReview vitest **45/45 PASS**, `web:check` **PASS**
- T2 parent: swipeEnter+App vitest **53/53 PASS**, `web:check` **PASS**
- Checklist: T1+T2 complete; T3 optional pending
- No commit made — tree left dirty for review

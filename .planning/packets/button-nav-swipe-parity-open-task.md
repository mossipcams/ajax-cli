# Packet: Open-task enter animation

PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior

```yaml
dispatch_level: compact
estimated_changed_lines: 50
estimated_files: 3
```

## Goal

When the operator opens a task from the dashboard/project list (or after New
Task creates one), the task outlet must play the same one-shot enter animation
swipe already uses (slide in from the right). Set enter direction `"left"`
before changing the hash — matching swipe-left → Diff enter semantics.

## Allowed files

- `crates/ajax-web/web/src/shared/lib/swipeEnter.ts`
- `crates/ajax-web/web/src/shared/lib/swipeEnter.test.ts`
- `crates/ajax-web/web/src/app/App.tsx`
- `crates/ajax-web/web/src/app/App.test.tsx`

## Forbidden changes

- useSwipePageTransition / TaskDetail / DiffReview Back (done in T1)
- Bottom-nav Dashboard/Settings choreography (stay instant unless a one-liner enter is trivial and tested)
- Sheet dismiss exit animation (T3)
- ActionBar / Drop
- architecture.md (already amended in T1)
- Commits, pushes, branch changes

## Code anchors

- `App.tsx` `go(hash)` (~98–100) and `onOpenTask={(handle) => go(taskHash(handle))}` (~344, ~375)
- `swipeEnter.ts` `setSwipeEnterDirection` / `consumeSwipeEnterDirection`
- CSS: `[data-outlet].ajax-swipe-enter-left` → `swipe-enter-from-right`
- App already consumes enter direction on route change (~182–185)

## Implementation sketch

Prefer a tiny helper in `swipeEnter.ts`, e.g. `navigateHashWithEnter(hash, direction)` that sets direction then assigns `location.hash`, **or** set direction at the two `onOpenTask` call sites before `go`. Do not animate list exit (no swipe surface on the list today).

## Acceptance criteria

1. Invoking open-task navigation sets swipe-enter direction `"left"` before the hash becomes the task route.
2. Focused unit/integration test proves `setSwipeEnterDirection("left")` (or sessionStorage key) is set when opening a task via the App open-task path; existing swipe Back enter `"right"` still works.
3. Bottom-nav / Settings paths are unchanged (still instant — no enter direction).
4. Drop dismiss path unchanged in this packet.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: rtk npm run web:test -- --run src/shared/lib/swipeEnter.test.ts src/app/App.test.tsx
      expected: pass, including new open-task enter coverage
    - type: typecheck
      command: rtk npm run web:check
      expected: exit 0
  broader_checks: []
  reason: Enter direction is set before hash change; App already consumes it for outlet class.
```

## Stop conditions

- Requires list exit animation or App-level transition stack
- Changes bottom-nav to animated without explicit tests
- Exceeds Allowed files

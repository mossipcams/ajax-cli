# Web Cockpit page-switch motion (issue #1070)

Status: **approved for implementation** — user approved in-session ("do the latter":
the two correctness fixes plus the cross-slide, measured before and after).
Branch/worktree: `ajax/page-switching` @ `b88ec0a9`.
Issue: [#1070](https://github.com/mossipcams/ajax-cli/issues/1070).

## Why

The reported problem is the page-switching *experience* — how smooth and how fast
it feels — not a routing bug. `DESIGN.md` already sets the binding motion law:

> Motion is short state feedback (≈140–220ms), never page choreography.

A committed swipe today spends a 220ms exit, then a mount, then a separate 220ms
enter, with a gap in the middle where neither surface is on screen. That is
roughly double the budget and is structurally two movements where the design
system allows one. So the target is not "a faster animation" but **one
continuous movement, finger-led, inside ~220ms**.

## Causes (code evidence)

1. **48px pickup snap.** `navigateSwipeMove` will not engage until
   `Math.abs(dx) >= ENGAGE_MIN` (48, `navigateSwipe.ts`), but once engaged
   `navigateSwipeTranslateX` returns the full `dx`. The page holds still for 48px
   of travel then jumps 48px in one frame before tracking the finger.
2. **Ajax Chat has no compositor hint.** `ChatSurface` applies `is-diff-swiping`,
   but `app-shell/shell-layout.css` only defines `.task-detail.is-diff-swiping`
   and `.diff-review.is-diff-swiping`. The heaviest surface swipes without
   `will-change: transform` or `touch-action: pan-y`.
3. **Serialized exit → enter.** `animateTo` in `useSwipePageTransition.ts` runs
   the exit, waits for `transitionend` (260ms fallback timer), then navigates;
   the destination outlet then plays its own 220ms `swipe-enter-from-*` keyframe.

Related: `swipeVelocity` is computed in `animateTo` but only feeds the
`ajax_swipe` telemetry event. The commit animation ignores release velocity and
always runs the fixed `SWIPE_PAGE_COMMIT_MS`.

## Scope

- `crates/ajax-web/web/src/shared/gestures/navigateSwipe.ts`
- `crates/ajax-web/web/src/shared/hooks/useSwipePageTransition.ts`
- `crates/ajax-web/web/src/styles/app-shell/shell-layout.css`
- `crates/ajax-web/web/src/app/App.tsx` (outlet structure for the cross-slide)
- tests beside the above, and `crates/ajax-web/web/e2e`
- `docs/architecture/web-cockpit.md` — the navigation contract there currently
  documents "commits by sliding the page off-screen, then navigates with a
  one-shot CSS enter on the destination outlet". The cross-slide changes that
  contract, so the doc must be updated in the same change.

## Non-goals

- Issue #1064 history containment and the task-switcher drawer. Separate work;
  #1064 is not reproducible on committed code and is pending reclassification.
- Diff Review's horizontal pan exclusions (`isDiffPanGestureTarget`) and terminal
  selection suppression (`shouldSuppressPageSwipe`). Both must keep working
  unchanged.
- Header Back semantics and destination routing.
- The per-frame React re-render (`setDragX` on every `touchmove`) and the
  per-frame `root.clientWidth` read. Both are real, but impeccable's `optimize`
  playbook is explicit that these get decided by measurement, not suspicion.
  Deferred until the before/after numbers say whether they still matter.

## Implementation tasks

1. **[x] Pickup from zero.** Engagement keeps its 48px arming threshold (it
   exists to reject accidental iOS PWA touches — keep that), but travel must
   start at 0 at the moment of engagement and track the finger 1:1 from there.
   Pure change in `navigateSwipe.ts`.
2. **[x] Chat compositor hint.** Give the chat surface the same
   `will-change: transform` / `touch-action: pan-y` treatment the other two
   surfaces get while swiping.
3. **[x] One continuous movement.** Keep the outgoing surface mounted while the
   incoming one enters so both move together as a single gesture-led motion, and
   bring the total committed switch inside the ~220ms budget rather than
   220 + mount + 220.
4. **[x] Velocity carries through.** Use the release velocity already computed in
   `animateTo` so a hard flick finishes proportionally faster instead of always
   spending the fixed duration.
5. **[x] Update `docs/architecture/web-cockpit.md`** to describe the new
   transition contract.

## Verification tasks

- **[x]** Unit coverage for the pickup change in the existing `navigateSwipe`
  tests: crossing the engage threshold produces 0 translate, not 48.
- **[x]** A deterministic before/after timing check for the committed switch, so
  the improvement is a number rather than a claim. Record the before value
  (440ms serial budget) and after value (≤220ms cross-slide commit).
- **[x]** Existing vitest swipe suites stay green — `navigateSwipe`,
  `useSwipePageTransition`, `TaskTerminalView`, `ChatSurface`, `DiffReview` —
  with no weakened assertions.
- **[x]** `npm run web:test -- --run` (1352 passed, 9 skipped),
  `npm run web:check` (clean), `npm run web:lint` (clean). Re-run independently
  by the orchestrator, not just reported by the delegate.
- **[x]** App boots in a real browser: `smoke.test.ts` under
  `--project=desktop-chromium`, 6 passed. This exercises the new
  `PageCrossSlideProvider` at the React root.
- **[ ]** **Playwright `e2e` swipe specs did NOT run locally.** They are
  `@playwright/test` specs, not vitest, so `web:test` never included them
  despite an earlier delegate report implying otherwise. They are gated to the
  `mobile-webkit` project, and on this machine WebKit hangs during iPhone
  context creation (`Test timeout of 30000ms exceeded while setting up "page"`).
  Confirmed environmental, not caused by this change: unrelated `smoke.test.ts`
  fails identically under `mobile-webkit` while passing under
  `desktop-chromium`, and `webkit.launch()` on its own succeeds (v26.5).
  Installing matching browsers via `npx playwright install` did not fix it.
  CI's `web-e2e` lane runs the full mobile-webkit suite in a pinned container,
  so that lane is the real gate for these specs.
- **[ ]** Device pass on iPhone Safari for the actual feel; the `ajax_swipe`
  event already records `settle_ms`, `duration_ms`, and velocity for real-user
  before/after.

Note on the timing number: `swipeCommitTiming.test.ts` compares constants
(`SERIAL_SWIPE_COMMIT_BUDGET_MS` is `SWIPE_PAGE_COMMIT_MS * 2`). It is a budget
regression guard, not a profiled measurement. Real before/after still depends on
the device pass and `ajax_swipe` telemetry.

## Risks

- The cross-slide changes the shell's outlet structure, which is the highest-risk
  part. Mounting two task surfaces at once must not double-mount a terminal PTY
  or a chat ACP session. **Resolved on review:** no swipe path connects the chat
  and terminal surfaces to each other. `TaskTerminalView` and `ChatSurface` both
  swipe left to Diff and right to Back, and `DiffReview` swipes right to Back,
  so the only pairs that are ever co-mounted are task+diff and session+diff.
  Outside a cross-slide, non-visible surfaces render `null` inside `Activity`,
  so steady state still mounts exactly one.
- A back-swipe leaves the workspace stack entirely, so no entering pane exists
  inside it and `onEnteringTransitionEnd` never fires. The unconditional
  `commitMs + 40` fallback timer in `beginCommit` is what releases the gesture
  gate in that case; removing it would strand `gestureBusyGate`.
- Diff Review and terminal gesture exclusions are easy to break from the shared
  hook; they have existing tests and those must not be relaxed.
- `prefers-reduced-motion` must keep collapsing the transition per `DESIGN.md`.

## Deviations

- Velocity scaling uses average gesture velocity
  (`swipeVelocity(distance_px, duration_ms)`), not instantaneous release
  velocity. That is what the codebase already computed for telemetry; the
  architecture doc was corrected to say so.
- `scheduleCrossSlideAnimatingFlip` selects its timer path by testing
  `navigator.userAgent` for jsdom. Real browsers always take the double-rAF
  path, but the sniff is fragile for any non-jsdom test environment.
- One revision round was required. The first implementation flipped the
  cross-slide from `armed` to `animating` inside `setTimeout(0)`, which does not
  guarantee a paint between the two style commits; the browser could coalesce
  them, run no transition, and let the fallback timer finish the slide — a
  silently non-animating page switch invisible to jsdom. Now double-rAF.
- Scope expanded by one file not in the original list:
  `crates/ajax-web/web/src/shared/lib/styleSources.ts`, whose CSS byte baselines
  had to move because the stylesheet grew.

## Follow-ups

- `App.tsx` is now 946 lines. `scripts/check-file-loc.mjs` warns at 800 and
  hard-fails at 1000 for `.tsx`, so this passes with ~54 lines of headroom and
  the next change touching it will likely trip the gate. Splitting it was
  deliberately excluded from this change.
- `Activity` currently wraps children that render `null` when hidden, so it
  provides no state preservation today. It is load-bearing only during a
  cross-slide. Worth revisiting if hidden-surface state retention is ever wanted.
- Issue #1064 remains open describing behavior only reproducible against
  uncommitted local work; it should be reclassified or closed.

# Web Cockpit page stack: interactive, native-feel navigation

Status: **Approved 2026-11-12 ("Continue the plan"). D1–D4 decided. Slice 1 done. Slice 2 (App.tsx) waits for #1613 to merge.**
Issues: [#1614](https://github.com/mossipcams/ajax-cli/issues/1614) (interactive back), [#1615](https://github.com/mossipcams/ajax-cli/issues/1615) (push/pop moves both pages), [#1608](https://github.com/mossipcams/ajax-cli/issues/1608) (commit threshold).
Branch: slice 1 is on `fix/web/swipe-commit-rule`, from `origin/main` at 5b81e40f. It doesn't touch any #1613 file. Slices that touch `App.tsx` start after #1613 merges.

## Why

The 2026-11-12 end-to-end audit ran Playwright CDP against the production `dist` at 390×844 with touch emulation. It judged native feel by structure, not frame rate: the probe Chrome ran at about 13fps under load 19.

- **Dragging back reveals the app background.** During a back-swipe from Chat, `elementsFromPoint` returns `app-shell`, and `outlet-dashboard` is not mounted. Only the workspace surfaces for one handle (task/session/diff) live in the `page-cross-slide-host`.
- **Arming lag.** Finger +70px leaves the page at 0, and finger +200px puts it at +150px. That is the deliberate 48px arming from #1074, plus `translate = dx - engageDx`.
- **Release to the dashboard drops the page.** The dragged page unmounts on release and the dashboard plays `swipe-enter-from-left` from -100%. Chat → Diff only shows Diff after release.
- **Tap push/pop moves one page.** The incoming outlet plays a one-shot `swipe-enter-*` keyframe (220ms). The outgoing outlet unmounts in the same commit.

## Prior decisions this plan must respect or explicitly revise

| Source | Rule |
|---|---|
| `DESIGN.md:172` | "Motion is short state feedback (≈140–220ms), never page choreography." Added with #1074. |
| `.planning/agent-plans/web-page-switch-motion.md` (#1074) | Keep the 48px arming "to reject accidental iOS PWA touches". Travel starts at 0 on engagement. One continuous movement inside ~220ms. Release velocity shortens the commit. |
| `docs/architecture/web-cockpit.md:645-655` | Swipe finger-follows. On commit, the outgoing workspace surface stays mounted (`Activity`) while the destination slides in beside it, in ≈220ms. Button Back uses the same cross-slide path. List-to-task uses the one-shot CSS enter class. Other chrome navigations may stay instant. |
| `docs/architecture/web-cockpit.md:100-114` | Pointer policy. Swipe-right back is capped on `#/` and `#/p/...`. Inner scrollers (`.session-tool-output`, `.session-diff-body`, `.md-block`, `.md-table-wrap`) never arm the page swipe. Terminal selection and Diff pans keep their exclusions. |
| `docs/architecture/web-cockpit.md:74-87`, `web-session-behavior.md:1335` | `html[data-session-viewport="owned"]` is set while session chat is mounted. Task and terminal surfaces use the `.app-viewport` fixed pin. Don't nest a second fixed pin. |
| #1600 / #1601 (PR #1605) | Exactly one motion per navigation. No `route-fade-in` restart after a slide. No replayed swipe-enter after a cross-slide. |

## Decisions

The reference point is Ionic's iOS swipe-back (`ionic-framework` `core/src/utils/gesture/swipe-back.ts`, read 2026-11-12). It is the most widely used web copy of the native gesture:
- the gesture only starts within 50px of the edge;
- it arms after 10px of movement;
- it completes when `velocity >= 0 && (velocity > 0.2 || deltaX > width / 2)`;
- the finish takes `remaining / velocity` ms, capped at 540ms.

Today's code, by comparison (`navigateSwipe.ts`, `useSwipePageTransition.ts:571-640`):
- **Commit:** it commits whenever the raw `dx` at the last move is ≥56px. Engagement needs 48px, so a release with the page moved only 8px still commits.
- **Velocity:** it is the average over the whole gesture (`swipeVelocity(distance, duration)`), not the speed at release.

- **D1 timing: keep the 220ms ceiling (`SWIPE_PAGE_COMMIT_MS`).**
  - **Rationale:** the audit gaps (#1614/#1615) are structural, not about duration: nothing underneath, one page moving, a jump on release. #1074 approved the 220ms ceiling. With the front-loaded `--ease-spring` curve, the first ~60% of the travel happens early, much like Ionic's 540ms curve.
  - **Change:**
    - The page underneath moves in the same transform pass as the top page, so it is still one movement per navigation.
    - Add one sentence to `DESIGN.md:172`: "Stack navigation moves the outgoing and incoming page together in that one movement; this is not choreography."
  - **When to revisit:** if the side-by-side recording in the iOS Simulator (S6) reads as abrupt next to a native app. `SWIPE_PAGE_COMMIT_MS` is the only setting to change.
- **D2 arming: edge starts arm after 10px. Everything else stays at 48px.**
  - **Edge rule:** a back swipe (rightward, on routes where back is allowed) that starts with `touchstart.x <= 50` arms after 10px of movement, with `LOCK_RATIO` still required. This matches Ionic's numbers.
  - **Unchanged:** mid-screen starts and forward (leftward) swipes keep `ENGAGE_MIN` 48. That keeps #1074's protection against accidental touches where it matters.
  - **Exclusions still win:** inner scrollers, terminal selection and Diff pans.
- **D3 commit (#1608): Ionic's rule, using the speed at release.**
  - **Commit when:** `releaseVelocity >= 0 && (releaseVelocity > 0.2px/ms || travel > 50% of width)`. Otherwise the page springs back. A release moving back toward the start always cancels.
  - **Release velocity:** measured from the last 100ms of touch samples.
  - **Finish speed:** the same release velocity feeds `computeSwipeCommitDurationMs` for both commit and cancel, so the finish continues at the finger's speed.
  - `SWIPE_COMMIT_VELOCITY_FLOOR` keeps its current job (a floor for the duration calculation). It is not reused as the flick threshold.
- **D4 the page underneath: mount it when needed, outside `route-scroll`. Don't keep it mounted hidden.**
  - **Why not keep it mounted:** `route-scroll` is the only vertical scroll container (`styles/app-shell/layout.css:1`). The dashboard and the workspace would have to share one scroll position. `RouteScroll.tsx` already saves and restores each route's offset, so keeping the dashboard mounted would not preserve anything new. It would also re-render the hidden list on every cockpit poll while you chat.
  - **The layer:** a fixed, `inert`, `aria-hidden`, non-scrolling layer placed behind `route-scroll`. It renders the destination list route, shifted up by `RouteScroll`'s saved offset for that route.
  - **When it mounts and unmounts:**
    - It mounts on an edge `touchstart`, so it is ready before the 10px arming. On a mid-screen swipe it mounts when the swipe engages.
    - It mounts during the `armed` phase of a tap push or pop.
    - It unmounts when the motion settles or the swipe is cancelled.
  - **Handing over:** after the move settles, the real outlet takes over inside `route-scroll`, which restores the same offset. That swap happens while nothing is moving.
  - **Known limit:** `TaskList` has to mount when the gesture starts. If the iOS Simulator shows a stutter there, the fix is to keep the layer mounted in a hidden `<Activity>`.

## Design (per D1–D4)

- **Two layers, one controller.** Generalize `PageCrossSlideProvider` (`shared/hooks/useSwipePageTransition.ts`). Its leaving/entering panes become *route layers*: any route can be `top` or `below`, not only the workspace surfaces of one handle. Its armed → animating → settle phases, settle fallback and `afterPendingHistoryBack` coupling are reused unchanged. No new navigation framework or router.
- **Below layer.**
  - For `task`, `session` and `diff` routes, the `below` layer is the route Back lands on, normally the dashboard or project list (`swipeEnter.ts` `ajaxPrevHash`). It is rendered per D4.
  - Diff's `below` is the workspace surface it already cross-slides to. That surface is already mounted in its own `<Activity>`, so it just switches to visible when the swipe arms.
- **Leaving layer.** During a pop's settle, the workspace stack stays rendered in `route-scroll` through `leavingRoute`, rather than unmounting when `taskOpenHandle` turns null. The PTY and chat stay attached until the settle finishes.
- **Gesture.**
  - The top layer follows `dx` from engagement (D2).
  - The below layer sits at `translateX(-30% + 30%·progress)` under a dim overlay, with opacity going from 0.1 to 0.
  - The top layer gets a leading-edge shadow.
  - Everything is transform/opacity only, with `will-change` set only while armed.
- **Release.**
  - The commit or spring-back is decided by D3.
  - Both layers animate from their current offsets, with duration from `computeSwipeCommitDurationMs(remaining, velocity)`, within D1's budget.
  - History moves at commit as it does today (`navigateHashBack`).
- **Taps.**
  - Row-tap push and ← Back pop run the same two-layer animation from rest.
  - For stack routes this replaces the one-shot `ajax-swipe-enter-*` classes and the `route-fade-in` entrance. That keeps the #1600/#1601 rule of one motion per navigation.
  - Bottom-nav and Settings stay as they are (out of scope).
- **Forward swipe (Chat → Diff).** The entering pane mounts at arm time and follows the finger, instead of appearing on release.
- **Reduced motion.** Drags still track the finger. Releases and taps switch instantly.

## Blast radius

- **`app/App.tsx`** is 977/1000 lines, and the file-LOC gate fails at 1000. Layer rendering must move out to a new `app/pageStack/` module, and App.tsx must shrink. `renderWorkspaceStack`, `workspacePaneStyle`, `workspaceSurfaceVisible`, `routeOutlet` and the swipeEnter effect move or change.
- **`shared/hooks/useSwipePageTransition.ts`**
  - Imported by App.tsx, `ChatSurface.tsx`, `TaskTerminalView.tsx` and `DiffReview.tsx`.
  - Tests: `useSwipePageTransition.test.tsx` (11 cases), `swipeCommitTiming.test.ts` (4), `ChatSurface.test.tsx`, `TaskTerminalView.test.tsx`, `DiffReview.test.tsx` and `App.route-entrance.test.tsx`.
- **`shared/gestures/navigateSwipe.ts`**
  - Imported by `useSwipePageTransition.ts` and `DiffReview.tsx`.
  - Tests: `navigateSwipe.test.ts`, which pins 56px commit and the dead zone (D2/D3 change these), and `swipeCommitTiming.test.ts`.
- **`shared/lib/swipeEnter.ts`**
  - Imported by App.tsx, `useSwipePageTransition.ts` and `TrainingModal.tsx`.
  - Tests: `swipeEnter.test.ts` and `swipeEnter.history.test.ts` (`ajaxPrevHash` / `history.back`; unchanged).
- **App-level tests pinning today's contract.** These are rewritten to the new contract, with assertions of equal strength:
  - `App.back-transition.test.tsx`: dashboard and task enter via swipe-enter classes.
  - `App.cross-slide-back.test.tsx` (#1601) and `App.route-entrance.test.tsx` (#1600).
  - Also touched by outlet structure: `App.test.tsx`, `App.session.test.tsx`, `App.skeleton.test.tsx`, `App.sheet.test.tsx`, `App.drop-confirm.test.tsx` and `App.pull-refresh.test.tsx`.
- **e2e:** `diff-review-swipe-repro.test.ts`, `shell-characterization.test.ts` (dashboard pull), `layout-scroll.test.ts` (`[data-outlet]` heights), `visual.test.ts` and `actions.test.ts`. CI runs these on mobile-webkit; local WebKit hangs on macOS 26.
- **`app/RouteScroll.tsx`** (D4: its offsets map becomes readable). Its scroll-restore behaviour does not change.
- **CSS:** `styles/app-shell/shell-layout.css` (`.page-cross-slide-*`, `[data-outlet]` entrance, `.ajax-swipe-enter-*`) and the `styleSources.ts` BASELINE ledger.

## Risks

- **Keyboard band.** While Chat is on top, `data-session-viewport="owned"` changes `.app-viewport` pinning. The below dashboard must render correctly under it. Blur the composer when a back gesture arms, as iOS does.
- **Terminal.** The PTY websocket lifetime is unchanged: a popped terminal still unmounts after the settle. Terminal selection and pan exclusions must keep winning.
- **Below-layer copy.**
  - It must never attach `pullToRefreshRef`, because the copy is `inert`.
  - It must match the real outlet when they swap at settle: same data and same offset. When cockpit data is missing, both show the skeleton.
  - `RouteScroll` restores the offset in a layout effect, before paint.
- **App.tsx LOC gate.** It is at 977/1000. Every slice touching it must shrink it.

## Implementation slices

Each slice builds, tests and is reviewed on its own. Delegated through the model router.

1. [x] **Gesture math (D2, D3).** Done 2026-11-12 (see Deviations / results).
   - Edge-start 10px arming for back swipes.
   - Release velocity from the last 100ms of samples.
   - Ionic's commit rule, with the release velocity fed into the commit and cancel durations.
   - Files: `navigateSwipe.ts`, `useSwipePageTransition.ts`, `navigateSwipe.test.ts`, `useSwipePageTransition.test.tsx` and `swipeCommitTiming.test.ts`.
   - The 56px-commit assertions are rewritten to the new rule with equal strength. Approving this plan approves that rewrite.
   - This slice is worth shipping on its own, for #1608.
2. [ ] **Extract layers.** Move workspace-stack rendering out of App.tsx into `app/pageStack/`, with no behaviour change. App.tsx shrinks.
3. [ ] **Below layer (D4).**
   - Move `RouteScroll`'s offsets map to module scope and export a read function.
   - Add the fixed, inert below layer behind `route-scroll`. Mount it on edge touchstart, on swipe engage, or in the `armed` phase of a pop; unmount it on settle or cancel.
   - Keep the leaving workspace stack in `route-scroll` through the pop settle.
   - Tests: the below layer exists during a drag with the correct offset; no pull-to-refresh attaches to it; it unmounts when the swipe is cancelled.
4. [ ] **Interactive back.** Reveal below with parallax, dim and edge shadow. Release from the current offsets with velocity. Rewrite the `App.back-transition` contracts.
5. [ ] **Tap push/pop.** Same two-layer animation from rest. Remove the one-shot enter classes for stack routes. Keep the #1600/#1601 invariants (rewritten tests).
6. [ ] **Forward interactive Chat/Terminal → Diff.** The entering pane follows the finger.
7. [ ] **Docs.** Update `web-cockpit.md` (swipe navigation at 645-655, pointer policy at 100-114) and `DESIGN.md:172` per D1.

## Verification

- Per slice: `npm run web:test -- --run`, `web:check`, `web:lint`, `web:sg`, `web:build`, and `env -u RUST_LOG cargo nextest run -p ajax-web -p ajax-cli`.
- Structure (Playwright MCP over CDP, production `dist`, mocked data):
  - the below layer is present during the drag;
  - the top-layer offset equals `dx - slop`;
  - both layers carry transform transitions on release;
  - no `route-fade-in` follows any stack navigation;
  - no layout-property animation.
- Real WebKit: iOS Simulator Safari through the iwdp MCP (timeline/animation tracking) for frame timing on push, pop and interactive back. This is the only valid frame-rate check, because the CDP Chrome is too slow under load.
- CI: Web E2E (mobile-webkit) green.

## Out of scope

- #1616 sheets, #1617 list row animation, #1618 pull-to-refresh compositor rework and #1612 polish. These are separate, smaller follow-ups.
- Bottom-nav and Settings transitions.
- History containment (#1064).

## Deviations / results

- **Slice 1, deviation (superseded by the PR #1619 review).** The commit direction first came from the raw displacement, so that a single-move flick commits. The review found that a swipe flicked back past its origin then committed the opposite navigation. The direction is now locked when the swipe arms (the sign of `engageDx`). The 50% distance rule measures visible travel toward that direction.
- **Slice 1, review fixes (#1619).**
  - Only a backward flick faster than 0.2px/ms cancels, so jitter at lift no longer cancels a swipe that is past 50%.
  - Velocity samples use `event.timeStamp`.
  - A second finger resets the swipe.
  - The edge zone is measured from the swipe root's left edge.
  - Edge arming is skipped for text fields, `.terminal-keys` and `.interact-panel .action-row`, and expires 300ms after touchstart (the terminal's long-press).
  - `diff-review-swipe-repro.test.ts` dispatches each gesture in one page call, with its assertions unchanged.
  - Docs updated in `web-cockpit.md`: pointer policy and swipe navigation.
  - Not changed: the telemetry still reports average velocity (that is a schema change), and the 50% rule stays on visible travel.
- **Slice 1, scope.** `swipeCommitTiming.test.ts` imported `NAVIGATE_SWIPE_TRIGGER`. It now uses the literal `-64` (the old 56 + 8), and its assertions are unchanged. `ChatSurface`, `DiffReview`, `TaskTerminalView` and `App.route-entrance` tests pass without edits.
- **Slice 1, verification.**
  - `web:test` 1988 passed, 9 skipped. `web:check`, `web:lint` and `web:sg` are clean.
  - Chromium at 390px, touches dispatched in the page:
    - an edge start tracks 1:1 after 10px;
    - a slow 70px drag from mid-screen springs back (the old rule committed it);
    - a slow drag past 50% commits;
    - short flicks at 0.43–0.54px/ms commit;
    - a release at −0.22px/ms after passing 50% springs back.
  - CDP touches starting on `xterm-screen` reached the page 450ms late on the loaded host, which the terminal reads as a long-press selection. Start probes off the terminal.

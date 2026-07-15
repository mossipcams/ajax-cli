# Ghostty terminal smoothness — calm geometry, fit-font rendering, native scroll proxy

Branch: `ajax/ghostty-smooth-ess`. Diagnosis session 2026-07-14: scroll judder,
refit storms, scroll snapping, and resize-induced scrollback duplication in the
default Ghostty surface (`TerminalRawView.svelte`). wterm Surface V2 (deleted in
#499) set the smoothness bar via (a) native iOS scrolling and (b) zero refit
churn. This plan ports both properties to Ghostty in three bounded tasks.

## Scope

- Task 1 — Calm geometry: dedupe no-op fits; one-step font restore (kill the
  +1px staircase).
- Task 2 — Fit-font rendering: in scale mode (host fits < 80 cols), render at
  the (possibly fractional) font size that fits the 80-col floor to the host
  width instead of rendering at the operator font and CSS-downscaling. Visual
  size is unchanged (today's visual cell width is already `min(cellW, hostW/80)`);
  the change removes the ~5× overdraw, the GPU downscale blur, and the
  `scaledLogicalRows` row inflation.
- Task 3 — Native scroll proxy: replace synthetic touch/fling/wheel scrolling
  with a native `overflow-y: auto` scroller (spacer sized
  `(scrollback + rows) × cellHeight`, canvas pinned via `position: sticky`,
  passive scroll listener mapping `scrollTop` → `scrollLines()` + sub-line
  translate). Deletes drag-notch/fling/wheel machinery from
  `terminalGestures.ts`.

## Non-goals

- No change to the 80-column PTY floor or agent-visible geometry (the
  "geometry mode < 80 cols" question stays open and unneeded after Task 2).
- No change to the tmux bridge protocol (reconnect reseed = separate task 4,
  not in this plan).
- No pinch-zoom + horizontal-pan revival on phones (pinch above the fit font is
  visually inert today; Task 2 preserves that; revival is a possible follow-up).
- No changes under `crates/ajax-web/src/` (server side).

## Key facts (verified against code/bundle this session)

- ghostty-web = rcarmo fork v0.9.4; `Terminal.resize` no-ops on identical dims;
  `scrollLines` clamps `viewportY` (integer lines from bottom); instance
  `scrollToBottom` is blinded by Ajax.
- `fitNow` today: runs side effects (`setScrollOffsetPx(0)`, snap-if-pinned)
  even when geometry is unchanged; grows font +1px per refit toward
  `chosenFontSize`, each step scheduling another refit.
- Refit triggers: ResizeObserver, window resize, orientationchange,
  visualViewport resize + scroll → `scheduleDebounced` (fit per rAF, PTY resize
  behind 100 ms debounce, deduped by `createResizeDedupe`).
- Fling (`flingFrames`/`startFling` in terminalGestures.ts) steps integer lines
  with empty frames and never emits sub-cell offsets → judder. Superseded by
  Task 3 (fling code is deleted, not fixed).
- Visual cell width on a phone is already `hostW/80`; rendering at the fitted
  font is visually equivalent to today.

## Delegation decision

`Delegation decision: delegated via model-router` (per task; cursor-agent
available). Packets via `tdd-implementation-packet`. I remain planner,
reviewer, validator.

## Task checklist

### Task 1 — Fit dedupe (TerminalRawView.svelte + TerminalRawView.test.ts)

DEVIATION 2026-07-14: font one-step restore moved from Task 1 into Task 2.
The staircase's cap-drop trigger is unreachable in jsdom (needs the
padding-boundary zone where hostFitCols ≥ 80 but usable-width cap < current),
so it cannot get a clean red test here, and Task 2 rewrites the scale-branch
grow logic anyway. Task 1 = dedupe only.

- [x] Test: a refit with unchanged fit inputs (proposal, host width, font)
      performs no side effects (no `term.resize`, no bottom snap).
- [x] Test: a refit with a changed proposal still resizes.
- [x] Test: sub-cell drag offset survives a no-op refit (vv scroll event).
- [x] Impl: dedupe key (proposal cols/rows, hostFitCols, clientWidth,
      fontSize) checked in `fitNow` before side effects; keyboard-decision
      path still runs; crop branch keeps its offset reset.
      Delegate deviation (accepted): when the key matches but the proposal is
      invalid (pre-layout), the old fitAddon.fit fallback still runs so
      pre-existing post-layout tests keep their contracts.
- [x] Validate: red exit 1 (delegate evidence) → green; 153/153 file,
      568/568 suite, web:check 0 errors — re-run independently by parent.
      GATE: ACCEPT (cursor-delegate, composer-2.5, first round).

### Task 2 — Fit-font rendering (TerminalRawView.svelte, terminalGeometry.ts + tests)

DONE 2026-07-14 (cursor-delegate composer-2.5, one round + parent-applied
mechanical revise). Deviations: (1) packet's expected fit font for a 384px
host was an arithmetic error (13·384/640 = 7.75, not 6.25) — delegate blocked
correctly, parent fixed six test constants; (2) two additional pre-existing
tests pinned the row-inflation contract ("uses agent-sized floor…" rows 50→31
with the 0.99 residual scale at 390px; "fits columns to the full host
width…" rows 50→30) — contracts updated intentionally, this IS the feature.
One-step font restore + cap+1 tolerance landed here as planned.
Validation: vitest 575/575, web:check clean, web:smoke 93 passed with 1
pre-existing load flake in terminal-scroll-garble (fails on clean HEAD too,
different case — screenshot-diagnostic timeout under suite load).
Known cost: one extra mount-time PTY resize while the font converges
(pass 1 sends inflated rows, pass 2 the real ones) — acceptable, once per
mount. GATE: ACCEPT.

- [ ] Test (terminalGeometry): new pure helper `fitFontSize(hostWidthPx,
      cols, cellWidthPx, currentFontSize)` → largest font at which `cols`
      columns fit `hostWidthPx` (linear cell-width scaling), fractional,
      guarded against invalid measurements.
- [ ] Test (component): when the host fits < 80 cols, the live font converges
      to the fit font (not 13) and the scale-layer transform is ~1 (no
      `scale(` entry or scale ≥ 0.98).
- [ ] Test (component): wide hosts unchanged (13px, no scale).
- [ ] Impl: in `fitNow`'s `usingScale` branch, target font = fit font
      (quantized, convergence deadband ~0.25px to prevent oscillation);
      `applyTerminalScale` remains as residual corrector (≈1). Non-scale
      branch untouched.
- [ ] Impl: dedupe key from Task 1 must tolerate fractional font (fixed
      precision).
- [ ] Validate: vitest focused + suite; **mobile-webkit Playwright e2e** run
      (fractional-font rendering in real WebKit is the risk); iOS Simulator
      spot check if e2e is inconclusive.
- [ ] Risk: ghostty-web may round fontSize internally → fallback is
      `floor(fitFont)` + residual `fitScale` (still ≥ ~0.9, still a win).

### Task 3 — Native scroll proxy (TerminalRawView.svelte, terminalGestures.ts, terminalOutputPolicy.ts + tests)

DONE 2026-07-15 (cursor-delegate composer-2.5, two rounds + one parent gate
fix). Round 1 landed the proxy but hit stop conditions: 9 legacy unit tests
and the e2e swipe helpers still encoded the synthetic model (packet
replacement list was incomplete). Round 2 closed them: 5 tests deleted
(fling/momentum/capture — behavior no longer exists), 4 rewritten to
native-driver equivalents, and both e2e `swipeIntoScrollback` helpers now
scroll via trusted `scrollTop` writes instead of untrusted WheelEvents.

PARENT GATE FIX (found via probe during independent verification): a
one-frame race — scroll events are delivered a frame after a scrollTop
write, so an output flush landing in that gap saw stale-pinned state and
`snapScrollbackToBottom` erased the user's scroll (reproduced ~50% on
desktop-chromium; the exact "can't scroll while output streams" class).
Fix: the write batcher's onFlush reconciles pin/viewportY from the real DOM
position (calls onNativeScroll) before choosing the pinned path, guarded to
scrollable hosts so pre-layout zeros can't force-pin. Regression test
"reconciles a pending native scroll before following output" — red proven
by disabling the reconcile (snap fired), green with it.

Validation (all re-run by parent): vitest 555/555; web:check 0 errors;
mobile-webkit e2e 46/46; desktop-chromium terminal-scroll:77 solo 3/3 after
the fix (was 1/3 before); full desktop-chromium green except the KNOWN
pre-existing terminal-scroll-garble parallel-load screenshot timeout (all 4
garble cases pass serially in 13.4s; same flake fails on clean HEAD).
cargo nextest -p ajax-web 133/133 and -p ajax-cli 335/335 with rebuilt dist.
TERMINAL.md ownership table updated. GATE: ACCEPT.

- [ ] Design contract: `.terminal-host` becomes the native scroller
      (`overflow-y: auto`, `touch-action: pan-x pan-y`, `overscroll-behavior:
      contain`); spacer div height `(scrollbackLines + rows) × visualCellHeight`;
      scale layer `position: sticky; top: 0`.
- [ ] Mapping: passive `scroll` listener → target line
      `clamp(round(SB − scrollTop/cellH), 0, SB)`; `scrollLines(currentY −
      target)`; sub-line remainder → existing `setScrollOffsetPx` translate.
      Top-anchored: output growth while reading requires NO scrollTop
      compensation (delete `scrollbackGrowthCompensation` call path); pinned →
      set `scrollTop` to max on flush; scrollback-cap eviction compensation
      deferred to `scrollend` (timer fallback).
- [ ] Pin policy: pinned ⇔ scrollTop within ε of max (replaces
      viewportY-based `setPinnedFromViewport` feed; policy module unchanged).
- [ ] Gestures: DELETE drag-notch scroll, fling (flingFrames/startFling/
      velocity), wheel handler, atTop/atBottom from `terminalGestures.ts`.
      KEEP pinch (2-finger preventDefault), long-press selection (cancelled by
      first scroll event), touchBegan focus.
- [ ] Textarea: must not live in the scroll flow (iOS scroll-chases focused
      elements) — `position: sticky` band or relocated node; native Paste
      long-press targeting must keep working (see web-paste memory).
- [ ] Keyboard/expand: cropToBottom + expand snap paths rewritten in terms of
      native scrollTop.
- [ ] Tests: component tests updated (scrollTop + scroll-event driven);
      e2e probe (`__ajaxTerminalProbe`) unchanged; full mobile-webkit suite;
      iPhone bake-off checklist from TERMINAL.md before merge.
- [ ] Update `crates/ajax-web/web/TERMINAL.md` ownership notes (scroll-follow
      description changes).

### Shipping

- [x] `npm run web:build` — dist/app.css, dist/app.js, dist/terminal.js
      regenerated and staged in the working tree.
- [x] `cargo nextest run -p ajax-web` 133/133; `-p ajax-cli` 335/335.
- [x] `npm run web:check` clean; full vitest 555/555.
- [ ] Real-iPhone bake-off (TERMINAL.md checklist) before merge — needs a
      live backend + device/simulator; not runnable in this session.
- [ ] Commit/PR — not done; awaiting Matt's go-ahead.

## Deviations

All recorded inline in the task sections above: Task 1 dropped the font
staircase half (moved to Task 2); Task 2 packet arithmetic fix (7.75) + two
extra row-inflation test contracts; Task 3 replacement-list gap (round 2) +
parent-fixed flush pin race.

## Validation log

Final state (2026-07-15, all run by parent): vitest 555/555 · web:check 0
errors · mobile-webkit Playwright 46/46 · desktop-chromium green modulo the
pre-existing garble parallel-load screenshot flake (passes serially and on
every solo run; also fails on clean HEAD) · cargo nextest ajax-web 133/133,
ajax-cli 335/335 (dist rebuilt).

## Post-PR CI fix (2026-07-15, commit 45c07b3)

PR #504's first CI run failed the Web job: terminal-zero-lag e2e on Linux
WebKit (overlay not found / only last char). Root cause: the fractional fit
font oscillated on renderers that round glyph advances per font size —
7.75px fit >80 cols, 8px overflowed, flipping the fit branch every pass into
an endless refit/resize/render loop that pegged the runner; the zero-lag
300ms idle backstop then cleared the overlay before stalled polls observed
it. Fix: fitFontSize returns whole pixels (round; undefined <1px) — integer
targets cannot exceed the current font while the floor overflows, so
convergence strictly decreases with no cycles; residual stays with the
shrink-only fitScale (0.97-1.0). Expectations: 7.75→8px, rows 30→31 at
384px. Verified: local suite 556/556, zero-lag e2e 3/3, mobile-webkit 46/46,
CI fully green (Web job 2m55s, down from 3m55s).

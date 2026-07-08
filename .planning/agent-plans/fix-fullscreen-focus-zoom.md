# Fix: fullscreen (⛶) button zooms the whole PWA on iOS

## Root cause (corrected — #375 fixed the wrong thing)

#375 re-fit the terminal *grid* after the viewport settled. That never touched
the zoom mechanism, so the bug still ships.

Device symptoms (confirmed by user 2026-07-08): tapping ⛶ zooms the **whole
page (buttons included), instantly on tap**, and it **latches** until the field
is blurred (close keyboard → dashboard → reopen task). That is the exact
signature of **iOS Safari focus-zoom**: the expand button's `onclick` calls
`focusTerm()` (programmatic focus of the ghostty textarea), and iOS zooms the
page to "reveal" the focused field.

Why existing guards miss it:
- `viewport.ts` preventDefaults pinch/gesture/double-tap zoom — but focus-zoom
  fires no gesture/touch event, so those guards never run.
- The textarea is pinned to 16px (`hardenMobileTextarea`), but 16px alone is
  known-insufficient to stop focus-zoom in iOS Safari when the viewport meta
  permits scaling.
- The viewport meta (`app.html:5`) has **no `maximum-scale`** — styles.css:565
  notes they rely on JS instead. `maximum-scale=1` is the only reliable stop
  for focus-zoom, and the app already fully suppresses browser zoom, so capping
  it costs nothing (terminal pinch-to-resize-font is JS, unaffected).

## Fix

Add `maximum-scale=1` to the viewport meta in `app.html`, then rebuild dist so
production (`dist/index.html`, embedded via `include_str!` in
`ajax-web/src/adapters/assets.rs`) actually carries it. Editing `app.html`
without `npm run web:build` ships nothing — the "forgot to rebuild dist" trap.

## Scope / non-goals

- One-line meta change + dist rebuild. No JS/Svelte logic change.
- Do NOT remove the viewport.ts pinch guards (they feed terminal pinch-to-font).
- Rust asset test `web_backend.rs:421` only asserts `width=device-width` +
  `name="viewport"` — stays green.

## Verification

- `npm run web:build:check` → dist regenerated, shape valid.
- `grep maximum-scale crates/ajax-web/web/dist/index.html` → present.
- `cargo nextest run -p ajax-web -p ajax-cli` (asset/snapshot tests) green.
- `npm run web:test -- --run` green.
- **On-device (only reliable check): user confirms ⛶ no longer zooms on iPhone.**
  Playwright webkit CANNOT reproduce iOS focus-zoom, so no e2e can prove this.

## Delegation decision

Delegated to cursor-delegate (Composer 2.5) per AGENTS.md lane 2 (web frontend).

## Deviations / results

- Fix landed: `app.html:5` viewport meta now has `maximum-scale=1`; dist rebuilt,
  `dist/index.html` (production shell) carries it and survives a fresh build.
- `dist/app.js` changed by 5 lines — minifier var-name churn from the rebuild,
  no source logic change (no .ts/.svelte edited). Benign.
- Verified by me: `cargo nextest -p ajax-web -p ajax-cli` 455/455 pass;
  `npm run web:build:check` EXIT 0; vitest 356 pass; fullscreen e2e 4 pass.
- OUTSTANDING: on-device iOS confirmation. Headless webkit cannot reproduce
  focus-zoom, so no automated test proves the user-facing fix.

## Note: false-green in fullscreen-refit.test.ts

The `visualViewport.scale === 1` assertion I added passes in headless webkit
regardless of the live bug — it does not guard the zoom. Removing it so the
suite doesn't imply coverage it lacks.

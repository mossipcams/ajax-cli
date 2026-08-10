# Session chat — keyboard spacing and snapping

Report: "the UX is broken for ajax web chat, specifically the spacing and
snapping when the keyboard opens."

## Scope

The `/session` route only: dead space around the composer, and the transcript
moving under the reader. Not the shell redesign (open decision 1 in
`HANDOFF-session-chat.md` — still Matt's call, untouched here).

Non-goals: chrome budget on the session head/header, transcript durability,
`DESIGN.md` §5, the dashboard/task routes' own spacing.

## Evidence

Measured on mobile-webkit (iPhone 15 Pro, 393×659) with a throwaway probe,
at rest and under a simulated 460px keyboard band.

| # | Defect | Measurement |
| --- | --- | --- |
| 1 | `body { padding-bottom: 72px }` clears a `position: fixed` nav that lives inside `#app`. Under `height: 100dvh; box-sizing: border-box` it only buys the document 72px of overflow — the pan budget iOS's focus scroll consumes and `resetDocumentScroll()` then yanks back. | `body.scrollHeight` 731 vs `clientHeight` 659 |
| 2 | The session route keeps the generic route-scroll block padding on a route that renders **no** bottom nav and no page lead. | composer bottom 587 in a 659 viewport → 72px dead below it; under the 460px band, 88px of 460 (19%) is padding and the transcript is squeezed to 150px |
| 3 | `.session-composer` pads `env(safe-area-inset-bottom)` while the keyboard is open, but the home indicator is under the keyboard. | ~34px more gap between the composer and the keys |
| 4 | Growing the composer shrinks the transcript out from under a pinned reader; nothing re-pins, so the next agent message snaps it back. | a 4-line draft grows the composer 67→129px and slides the transcript **62px** off the live edge |

Also: `textarea { max-height: 30vh }` measures the *layout* viewport, which iOS
never shrinks for the keyboard — a grown composer can eat the whole band.

## Tasks

- [x] 1. Delete `body { padding-bottom: 72px }`. Verify document overflow is 0
      and nothing moves (`route-scroll` still owns nav clearance).
- [x] 2. Zero the session route's route-scroll block padding, keeping the
      horizontal safe-area insets for landscape.
- [x] 3. Drop the composer's safe-area bottom pad under `html.keyboard-open`.
- [x] 4. Cap the textarea against `--app-height` instead of `vh`.
- [x] 5. Re-pin the transcript on any thread resize (`ResizeObserver`), which
      covers composer growth, the head gaining a decision, and band changes.
- [x] 6. Regression tests: geometry under the simulated band (e2e) + the
      re-pin (unit).

## Result

Measured the same way, mobile-webkit, before → after:

| | before | after |
| --- | --- | --- |
| body scrollable overflow | 72px | 0 |
| gap below the composer, at rest | 72px | 0 |
| gap between the composer and the keyboard | 72px | 0 |
| transcript height under a 460px band | 150px | 238px |
| transcript drift from a 4-line draft | 62px | 0 |
| textarea cap under a 460px band | 198px (`30vh`) | 138px |

All three new tests were confirmed failing on the base (`git stash` of the two
source files) with those exact numbers, so none of them is vacuous.

## Validation

- `web:test` — 763 passed, 74 files
- `session-chat` + `layout-scroll` e2e — 34 passed, desktop-chromium and
  mobile-webkit
- `pkill -f vite; CI=1 web:smoke` — 123 passed, 2 flaky (`terminal-behavior`
  bracketed paste, `visual` settings sections). Both are 60s load timeouts that
  pass in ~1s in isolation, and neither touches anything this change edits;
  the handoff already records this suite's flakes.
- `web:lint`, `web:check`, `web:sg`, `web:build` — clean
- `cargo test -p ajax-web` — 275 passed (guards the embedded asset snapshots,
  since `dist/` is rebuilt here)

## Deviations

None.

## Left alone

- The impeccable hook reports five findings in `styles.css` (L851 `#000`,
  L2091, L2698–99, L3250). All predate this change and sit on lines it does not
  touch; L851 is the mask-stop false positive the handoff already documents.
  Not suppressed, not "fixed" — they want Matt's call, not a drive-by.
- The session route's own 12px gutter. Removing route-scroll's extra 20px
  narrows the content margin, but 12px is the value the page already declared;
  inventing a new number here would be shell design, which is open decision 1.

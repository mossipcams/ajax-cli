# Fix Tap-to-confirm Drop pill clipping

## Scope

Mobile interact-panel Drop confirm label (`Tap to confirm`) and pill look cut
off on both ends. Raise horizontal padding so text clears the stadium curve.

## Non-goals

- Do not change confirm flow, Drop undo window, or ActionBar JS.
- Do not redesign the interact panel row.

## Root cause (IWDP)

`.interact-panel .action` uses `min-height: 28px`, `border-radius: 999px`, and
`padding: 2px 8px`. Half of 28px is 14px; with only 8px inset the label sits
inside the curved ends and reads as clipped on both sides. Confirmed on
simulator: confirming Drop → `padding: 2px 8px`, width ~110px, text flush to
pill ends.

## Delegation decision

`Delegation decision: not delegated because` model-router `R-LOCAL-TINY` —
one CSS file, ~5 lines, no new control flow.

## Checklist

- [x] Task 1 — Raise interact-panel action horizontal padding (≥ 14px) + nowrap
- [x] Task 2 — Update CSS characterization test if assertions need it
- [x] Task 3 — IWDP recheck confirming Drop pill
- [x] Task 4 — Inset action-row scrollport + bump panel pad so pill caps aren’t clipped

## Validation

```bash
rtk npm run web:test -- --run TaskDetail.test.tsx ActionBar.test.tsx  # PASS
```

IWDP after deploy (confirming):
- `padding: 2px 14px`, `white-space: nowrap`
- action-row inset room L/R = 3px; panel gap to right = 12px
- `clearsStadiumCurve: true`

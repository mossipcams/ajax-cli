# Fix Drop button color on task detail

## Scope

When Drop is first/only action it gets `.primary` (accent blue fill) plus
`data-destructive` (danger text/border) → blue pill with red label. Destructive
actions must not receive primary styling.

## Non-goals

- Do not change confirm/undo Drop flow or action ordering from the API.

## Root cause

`actionClassName` always adds `primary` for `index === 0`. IWDP: Drop alone →
`bg rgb(135,175,215)` + `color rgb(215,135,135)`.

## Delegation decision

`Delegation decision: not delegated because` R-LOCAL-TINY — one behavior line
in ActionBar plus a focused test and a CSS safety override.

## Checklist

- [x] Skip `primary` when `action.destructive`
- [x] CSS: `.action.primary[data-destructive="true"]` cannot keep accent fill
- [x] Test: sole Drop is not `.primary`; Review+Drop keeps Review primary
- [x] IWDP recheck Drop colors

## Validation

```bash
rtk npm run web:test -- --run ActionBar.test.tsx  # PASS 13
```

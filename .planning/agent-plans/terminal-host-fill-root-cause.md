# Root cause: host not filling flex wrap

## Misdiagnosis

Earlier work flex-filled the panel/wrap and chased Copy e2e races. That did not
fix the reported gap: on a new task the xterm **entry sits in a short host**
while the wrap/panel is tall, leaving empty space between the entry
(textarea at host bottom) and the hotbar.

## Root cause

`.terminal-host { height: 100% }` only works when the wrap has a **definite**
height. With flex layout, percentage height often fails to resolve on WebKit,
so the host stays content/`min-height` sized while the wrap grows.

## Fix

1. Flex chain all `flex: 1 1 0%` (outlet → detail → panel → wrap).
2. Make the interaction wrap a column flex container; host `flex: 1 1 0%`
   (not `height: 100%`) so it consumes the wrap’s used height.
3. Keep spacer as `flex: none` sibling for scrollback.
4. Keep equal-width hotbar keys + selection-safe ambient fit.

## Checklist

- [ ] CSS/host flex fill
- [ ] Unit contracts
- [ ] Build + focused vitest
- [ ] Push

# Root cause: host not filling flex wrap

## Misdiagnosis

CSS-only `height: 100%` and `display:flex` host-in-wrap both failed: the
former leaves a gap on WebKit; the latter broke sticky scroll / resize settle
and Copy e2e.

## Root cause

The interaction wrap flexes tall, but the sticky host did not get a definite
used height, so the entry stayed in a short host above empty space over the
hotbar.

## Fix

1. Keep wrap `flex: 1 1 0%` in the task flex chain.
2. `syncHostToWrap()` sets `host.style.height` to `wrap.clientHeight` px
   before fit and on viewport change.
3. Keep equal-width hotbar keys.
4. Keep ambient fit skip while selection is active (Copy stability).

## Checklist

- [x] syncHostToWrap + selection-skip
- [x] Unit contracts
- [x] Build + focused vitest
- [ ] CI green on PR #529

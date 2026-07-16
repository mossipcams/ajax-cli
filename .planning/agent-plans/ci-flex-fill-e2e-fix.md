# CI fix: mobile terminal flex-fill regressions

## Diagnosis

Web e2e (mobile-webkit) failed on PR #527 after flex-fill:

1. **New output never appears** — host `flex:1; height:auto` broke sticky-host + spacer scroll, so scroll-away stays "live".
2. **Copy overlay detaches** — layout/fit thrash clears selection while clicking Copy.
3. **Resize settle** — unstable host height prevents meaningful viewport→PTY resize.

## Fix

- Keep panel/wrap **flex-fill** (removes dead band).
- Restore `.terminal-host { height: 100% }` (no flex column on interaction wrap).
- Keep proportional hotbar keys + keyboard-open safe-area pad.
- Drop continuous keyboard-open resize refit (band settle on class edges is enough).

## Checklist

- [ ] Adjust TaskTerminal CSS + revert keyboard resize timer
- [ ] Update unit contracts
- [ ] Rebuild dist
- [ ] Focused vitest
- [ ] Push to PR #527

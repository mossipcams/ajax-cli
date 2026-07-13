# Reproduce Web PWA terminal scroll garble

## Scope

Reproduce mid-token wrap / garbled Claude-looking output on Ajax Web PWA
task terminal. No production fix until reproduction is solid.

## Non-goals

- No architecture / terminal-model changes
- No Wide-mode restore
- No live Claude dependency for the first automated attempt

## Delegation decision

`Delegation decision: not delegated because reproduction and diagnosis are
parent-owned per the approved plan; no implementation fix yet.`

## Task checklist

- [x] Task 1 — Playwright mobile-webkit repro with fixed-width MARK lines
- [x] Task 2 — Run repro; record pass/fail + artifacts
- [x] Task 3 — Live PWA smoke only if mocks pass without failure
      (skipped: no live `ajax web` / Claude task available; automated
      softwrap case already reproduces the pasted mid-token wrap)
- [x] Task 4 — Pin root cause (desync / compensation / narrow-fit)

## Validation commands

```bash
rtk npm ci
rtk npx playwright test e2e/terminal-scroll-garble.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit
```

## Execution log

- Added e2e probe gate in `TerminalRawView.svelte` (`__ajaxTerminalProbeEnable`).
- Added `e2e/terminal-scroll-garble.test.ts` with four cases.
- `npm ci` PASS.
- Playwright mobile-webkit: **4 passed**.
  - markers + output while scrolled: PASS (no compensation garble)
  - markers + keyboard resize while scrolled: PASS (no desync garble)
  - CSI redraw while scrolled: PASS
  - long-path softwrap: PASS and **reproduces** mid-token wrap

### Softwrap diagnostic (load-bearing)

```json
{
  "cols": 43,
  "softWrapped": true,
  "sample": [
    "crates/ajax-core/src/registry/sqlite.rs cra",
    "tes/ajax-tui/src/lib.rs crates/ajax-web/src",
    "/runtime.rs",
    "… +16 lines (ctrl+o to expand)"
  ],
  "metrics": {
    "hostClientWidth": 390,
    "canvasClientWidth": 387,
    "cols": 43,
    "rows": 34
  }
}
```

Matches the operator paste shape (`… cra` / `tes/…` mid-token breaks).

Artifacts (gitignored): `crates/ajax-web/web/e2e/artifacts/garble-*.png`

## Root cause pin

**Hypothesis 3 — narrow fit soft wrap.**

On iPhone-width PWA, fit geometry yields ~43 columns (`FIT_TERMINAL_COLS`
floor 40). Long Claude tool lines soft-wrap mid-token. Scrolling into that
history makes the wraps obvious; scroll-follow compensation and keyboard
resize paths did **not** corrupt marker rows in mocked e2e.

Not a scroll-yank / scrollback-compensation bug. Fix (if wanted) is a
geometry / readability product change (wider floor, word-aware wrap is not
available in raw PTY, restore Wide mode, smaller font → more cols, etc.).

## Deviations

- Live Claude smoke skipped: no running Web Cockpit with a live task in this
  environment. Automated softwrap case is sufficient to reproduce the paste.
- Probe is e2e-gated via `window.__ajaxTerminalProbeEnable` so production
  stays clean unless fixtures opt in.

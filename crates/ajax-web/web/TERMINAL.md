# Web Cockpit terminal ownership

## Status

Web Cockpit mounts one xterm.js task terminal from `TaskDetail.tsx` via
`TaskTerminal.tsx`. Geometry math lives in `terminalGeometry.ts`, refit
scheduling in `terminalRefit.ts`, both wired into `TaskTerminal.tsx`; the
mobile-WebKit behavior suite including the viewport-burst case passes as of
2026-07-16.

## Ownership

| Concern | Owner |
| --- | --- |
| Lifecycle, DOM, accessibility, composition | `TaskTerminal.tsx` |
| WebSocket lifecycle / transport | `terminalConnection.ts` |
| Document viewport + keyboard truth | `viewport.ts` |
| Pure grid/scale/row/font persistence math | `terminalGeometry.ts` |
| Frame coalescing, two-frame settling, 100 ms PTY debounce, dimension dedupe, disposal | `terminalRefit.ts` |
| Typed-echo zero-lag overlay (prediction paint + idle/echo clear) | `xtermZeroLag.ts` |
| PTY attach + frame bridge | `ajax-web::adapters::terminal_pty` |
| Task-handle attach planning | `ajax-web::slices::terminal` |
| Protected route `/api/tasks/{handle}/terminal` | `ajax-web::runtime` |

The PTY adapter ownership is unchanged from today.

## Permanent acceptance

- `e2e/terminal-behavior.test.ts` (`mobile-webkit`) — passing as of 2026-07-16.
- `TERMINAL_BEHAVIOR_CONTRACT.md` — behavior inventory (evidence)
- `TERMINAL_REBUILD_ACCEPTANCE.md` — acceptance matrix (evidence)
- `TERMINAL_LEGACY_SURFACE_TESTS.md` — removal hygiene index (evidence)

## Rule

Browser terminal UI does not own task truth or tmux target selection. Behavior
changes require a failing case in the permanent suite first.

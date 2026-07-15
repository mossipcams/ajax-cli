# Web Cockpit terminal ownership

## Status

Web Cockpit mounts one xterm.js task terminal from `TaskDetail.svelte` via
`TaskTerminal.svelte`. The permanent mobile-WebKit behavior suite (27 cases) is
green.

## Ownership

| Concern | Owner |
| --- | --- |
| Task route terminal UI (xterm.js) | `TaskTerminal.svelte` |
| Task-terminal WebSocket lifecycle / reconnect | `terminalConnection.ts` |
| Keyboard / visualViewport / `--app-*` | `viewport.ts` |
| PTY attach + frame bridge | `ajax-web::adapters::terminal_pty` |
| Task-handle attach planning | `ajax-web::slices::terminal` |
| Protected route `/api/tasks/{handle}/terminal` | `ajax-web::runtime` |

## Permanent acceptance

- `e2e/terminal-behavior.test.ts` (`mobile-webkit`, 27 cases) — green
- `TERMINAL_BEHAVIOR_CONTRACT.md` — behavior inventory (evidence)
- `TERMINAL_REBUILD_ACCEPTANCE.md` — acceptance matrix (evidence)
- `TERMINAL_LEGACY_SURFACE_TESTS.md` — removal hygiene index (evidence)

## Rule

Browser terminal UI does not own task truth or tmux target selection. Behavior
changes require a failing case in the permanent suite first.

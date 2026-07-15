# Web Cockpit terminal ownership

## Status (Task 12 complete)

The old Ghostty default surface and experimental xterm frontend are
**removed**. `TaskDetail.svelte` does not mount a browser terminal. There is
**no** shared old/new adapter, placeholder renderer, or deferred terminal chunk.

## Retained boundaries

| Concern | Owner |
| --- | --- |
| Task-terminal WebSocket lifecycle / reconnect | `terminalConnection.ts` |
| Keyboard / visualViewport / `--app-*` | `viewport.ts` |
| PTY attach + frame bridge | `ajax-web::adapters::terminal_pty` |
| Task-handle attach planning | `ajax-web::slices::terminal` |
| Protected route `/api/tasks/{handle}/terminal` | `ajax-web::runtime` |

## Permanent acceptance (intentionally red)

- `e2e/terminal-behavior.test.ts` (`mobile-webkit`) — engine-neutral behavior contract
- `TERMINAL_BEHAVIOR_CONTRACT.md` — pre-removal inventory (evidence)
- `TERMINAL_REBUILD_ACCEPTANCE.md` — acceptance matrix (evidence)
- `TERMINAL_LEGACY_SURFACE_TESTS.md` — removed-surface characterization index (evidence)

The permanent suite stays in the repo and fails until a ground-up controller and
adapter are rebuilt against the retained connection/backend contract.

## Rebuild rule

New terminal UI must not reintroduce browser-owned task truth or tmux target
selection. Behavior changes require a failing case in the permanent suite first.

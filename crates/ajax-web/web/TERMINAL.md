# Web Cockpit terminal ownership

## Product contract
- Raw Ghostty/tmux-first on mobile and desktop
- Do not reintroduce Live/snapshot/composer as default
- Browser modules do not own task truth or tmux target selection

## Ownership table
| Concern | Owner |
| --- | --- |
| Keyboard / visualViewport / --app-* | viewport.ts |
| Fit / font / pan math | terminalGeometry.ts |
| Refit scheduling | terminalRefit.ts |
| Gestures / selection geometry | terminalGestures.ts |
| Scroll-follow / resize validity | terminalOutputPolicy.ts |
| WS connect / backoff / status | terminalConnection.ts |
| Ghostty mount + chrome UI | TerminalRawView.svelte |
| Route scroll / chrome hide | styles.css + App layout |

## Anti-patterns
- Do not add new one-shot `*FlushPending` (or equivalent) booleans in
  TerminalRawView.svelte — put named policy in terminalRefit.ts / geometry
- Do not fix iOS bugs only in CSS/component without a failing Vitest or
  mobile-webkit Playwright case first
- Do not scatter Ghostty private API casts; isolate in one adapter when extracting

## Review rule
Terminal behavior PRs: failing test first; policy change in the owning module.

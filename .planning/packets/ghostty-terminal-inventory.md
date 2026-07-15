# Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: docs-only
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
BLOCKERS: []
```

# Goal

Create the source-backed behavioral inventory for the current Ajax browser
terminal, focused on iOS Safari and classified into Product behavior, Legacy
Ghostty behavior, Bug excluded, or Physical iOS verification.

# Allowed files

- `crates/ajax-web/web/TERMINAL_BEHAVIOR_CONTRACT.md` (new)

# Forbidden changes

- Every production, test, configuration, dependency, lockfile, generated, and
  existing documentation file.
- No xterm/Ghostty refactor, implementation, cleanup, or speculative adapter.

# Context evidence

- Graphify: NOT_REQUIRED because this docs-only task uses `architecture.md` as
  the authoritative boundary map and verifies details directly in source.
- Serena: NOT_REQUIRED because no symbol reuse, semantic edit, or production
  anchor decision is being made; direct source and test names are the evidence.
- ast-grep: NOT_REQUIRED because no code syntax is changed; `rg` anchors are
  sufficient for a documentation inventory.
- Architecture anchors: `architecture.md:488-718`, especially terminal slice
  and PTY adapter ownership at `679-714`.
- Frontend ownership: `crates/ajax-web/web/TERMINAL.md`.
- Mount/lifecycle: `components/TaskDetail.svelte`,
  `TerminalSurfaceSelector.svelte`, `TerminalRawView.svelte`.
- Transport/backend: `terminalConnection.ts`,
  `crates/ajax-web/src/slices/terminal.rs`,
  `crates/ajax-web/src/adapters/terminal_pty.rs`, runtime terminal route.
- Viewport/input/touch/settings: `viewport.ts`, `terminalRefit.ts`,
  `terminalLayoutPolicy.ts`, `terminalGestures.ts`, `terminalClipboard.ts`,
  `terminalGeometry.ts`, `terminalSurfaceSetting.ts`, `SettingsView.svelte`.
- Existing coverage: terminal-related `*.test.ts`, `e2e/*.test.ts`,
  `playwright.config.mts`, and `scripts/ios-terminal-smoke.mjs`.

# Code anchors

- Create a standalone document with sections: classification legend; mount,
  readiness, disposal; PTY/WebSocket I/O; resize/fit and viewport sources;
  focus/keyboard/paste/copy/selection/scroll/touch; reconnect/restoration;
  settings; Ghostty integrations/workarounds; existing tests/infrastructure;
  automation gaps.
- Each behavior row must cite at least one concrete source or test path.

# Test-first instructions

NOT_APPLICABLE: documentation-only inventory; an executable test would be fake
coverage. Verification is source-to-row completeness.

# Edit instructions

- Inspect the named sources and tests fully enough to trace each flow.
- Use compact tables. Classify every discovered behavior with exactly one
  primary label: Product, Legacy Ghostty, Bug excluded, or Physical iOS.
- Distinguish current supported behavior from workarounds and suspected bugs.
- Do not claim theme/cursor/scrollback are settings unless the UI exposes them;
  record fixed defaults separately.
- Explicitly note Playwright mobile WebKit is only a proxy for physical Safari.

# Verification commands

```bash
rg -n "Product|Legacy Ghostty|Bug excluded|Physical iOS" crates/ajax-web/web/TERMINAL_BEHAVIOR_CONTRACT.md
rg -n "mount|readiness|disposal|WebSocket|PTY|resize|visualViewport|orientation|fullscreen|keyboard|focus|paste|copy|selection|scroll|touch|reconnect|restoration|settings|workaround|Playwright" crates/ajax-web/web/TERMINAL_BEHAVIOR_CONTRACT.md
```

# Acceptance criteria

- Every requested inventory category is present and source-backed.
- Known/suspected iOS or Ghostty defects are not phrased as requirements.
- Permanent product outcomes are separated from implementation details.
- Physical-iPhone-only checks are unmistakable.

# Stop conditions

- Required behavior cannot be classified from repository evidence.
- The document would need a production/test/config edit.
- A claim depends only on generated summaries rather than source/tests.

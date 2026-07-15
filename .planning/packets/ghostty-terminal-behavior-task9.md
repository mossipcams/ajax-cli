# Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: docs-only
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
BLOCKERS: []
```

# Goal

Clearly separate permanent iOS-WebKit behavior coverage from removable old
Ghostty and experimental-xterm characterization, and correct inventory rows
that accidentally turn rollout scaffolding or scheduling algorithms into
product requirements.

# Allowed files

- `crates/ajax-web/web/TERMINAL_BEHAVIOR_CONTRACT.md`
- create `crates/ajax-web/web/TERMINAL_LEGACY_SURFACE_TESTS.md`

# Forbidden changes

- Source, tests, other docs, dependencies, or git commands.
- Do not invent current behavior or claim physical iOS proof.

# Required inventory corrections

- Product: a task detail exposes one functioning terminal surface. Legacy:
  which old renderer is selected and the Dev-only Surface V2 flag/error/retry.
- Legacy/test seam: terminal placeholder flag and probe.
- Product output contract: ordered/lossless delivery, responsive application,
  and no scrollback yank. Legacy mechanics: exact 16ms/16KB batching,
  once-per-rAF painting, leading/trailing scheduling, and current listener /
  debounce organization.
- Product resize contract: valid final size, meaningful changes settle,
  duplicate outcomes bounded, no keyboard resize storm. Keep exact thresholds,
  timers, listener sources, fit formula, and implementation dedupe as Legacy.
- Keep the 80-column floor Product only because `architecture.md` explicitly
  calls it a deliberate agent-sized layout; cite that evidence and do not infer
  any other arbitrary floor.
- Product status should describe observable connection semantics without
  renderer-specific label differences.
- Font/pinch persistence is Product. Exact storage key, range, defaults, and
  implementation functions are legacy mechanics; fixed theme/cursor/font
  family/scrollback values are current defaults, not user settings.
- Mark experimental xterm/Ghostty preload selection rows Legacy, not Product.
- State only `mobile-webkit` is in scope for this compatibility work; existing
  Chromium tests are inventory only.

# Legacy-suite index

Create a concise removable-suite map with:

- old Ghostty integration/component/probe/workaround tests;
- old experimental xterm selector/component/settings/preload tests;
- renderer-policy/math tests that may be deleted or rewritten rather than
  required by the future architecture;
- engine-specific Playwright files using canvas, probes, generated DOM, or old
  class names;
- permanent replacement location (`e2e/terminal-behavior.test.ts`) and the
  backend/boundary tests that should remain (`terminalConnection.test.ts`, Rust
  PTY/runtime tests);
- an explicit marker: removable after the ground-up rebuild; not acceptance
  criteria and not a shared adapter contract.

# Verification commands

```bash
rg -n "Product.*(16ms|16KB|animation frame|surfaceV2|Surface V2|ghostty|xterm)" crates/ajax-web/web/TERMINAL_BEHAVIOR_CONTRACT.md
rg -n "ghostty|xterm|canvas|textarea|terminal-host|__ajaxTerminalProbe|data-terminal-engine" crates/ajax-web/web/e2e/terminal-behavior.test.ts
```

# Acceptance criteria

- No Product row freezes old engine choice, rollout flags, DOM, cadence, or
  scheduling organization.
- Legacy index makes deletion boundaries and permanent replacements explicit.
- Physical-only gaps remain labeled as such.

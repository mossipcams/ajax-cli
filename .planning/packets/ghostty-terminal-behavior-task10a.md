# Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: tests-only
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
BLOCKERS: []
```

# Goal

Close two coverage gaps found by the acceptance-matrix audit: supported sticky
Ctrl input at the public toolbar/PTY boundary, and intentional output handling
during delayed initialization and viewport resize.

# Allowed files

- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`

# Forbidden changes

- All other files and production.
- Renderer selectors/state, sleeps, skips, exact fit timing, git commands.

# Required tests

1. **Supported Ctrl combinations:** on one connected terminal, use only the
   public interaction surface and toolbar. Prove the dedicated `⌃C` control
   sends `\x03`; sticky `Ctrl` + left arrow sends `\x1b[1;5D`; sticky `Ctrl`
   + printable `c` from the focused terminal sends `\x03`. Assert the ordered
   input-frame slice exactly and prove the sticky button disarms after each
   consumed key. Do not test Backspace.
2. **Output during delayed initialization:** use `{autoOpen:false}`, observe the
   socket exists, emit a split UTF-8/Unicode/control corpus before socket open,
   then open it. Prove the surface becomes/stays visible, connection settles,
   only one socket is active, and there are no page errors. Transport byte
   exactness remains `terminalConnection.test.ts`; do not inspect rendered
   glyphs.
3. **Output during viewport resize:** on a connected surface, begin a meaningful
   viewport/orientation transition and emit an ordered rapid corpus while the
   transition is in flight. Prove a fresh valid resize outcome eventually
   arrives, the surface remains visible/connected with one socket, and no page
   error occurs. Do not assert a timing algorithm or renderer buffer.

# Verification

```bash
npm run web:smoke -- --project=mobile-webkit crates/ajax-web/web/e2e/terminal-behavior.test.ts
rg -n "ghostty|xterm|canvas|textarea|terminal-host|data-terminal-engine|waitForTimeout|Backspace" crates/ajax-web/web/e2e/terminal-behavior.test.ts
```

# Acceptance criteria

- Three new result-oriented tests pass; prior 24 remain green.
- Exact control data/order is protected.
- Output transition tests claim application stability/continuity only, with
  exact delivery delegated to the existing permanent connection tests.

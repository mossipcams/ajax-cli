# Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: tests-only
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
BLOCKERS: []
```

# Goal

Pin only the externally observable terminal setting Ajax currently exposes to
users on iPhone: pinch-adjusted terminal text density persists across reload.
Do not turn storage keys, fixed renderer defaults, or the experimental xterm
selector into requirements for the ground-up rebuild.

# Allowed files

- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`
- `crates/ajax-web/web/e2e/fixtures.ts` only if a renderer-neutral synthetic
  pinch helper is useful.

# Forbidden changes

- All production and other files; no storage-key assertions or direct seeded
  localStorage values.
- No Surface V2/xterm assertions, Ghostty names, canvas/textarea/private state,
  computed glyph pixels, exact font sizes/ranges, fit formulas, screenshots,
  sleeps, skip/fixme, or git commands.

# Contract decisions

- The Dev-only Surface V2 flag is legacy rollout scaffolding and Task 12 removes
  it; it is not a rebuild acceptance behavior.
- Theme, font family, cursor, and scrollback are fixed current renderer options,
  not user settings, so they get inventory/matrix classifications rather than
  invented permanent tests.
- Invalid persisted-value fallback remains existing legacy characterization
  because no user UI creates an invalid value and asserting the current
  storage key would violate the future implementation/state freedom.
- Missing-preference startup is already covered by the permanent initial valid
  dimension and usable-input tests.

# Required test

Using only `terminal-interaction-surface`, public touch events, PTY resize
frames, surface visibility, and input frames:

1. Open at the standard iPhone viewport and capture the settled valid resize.
2. Dispatch a two-finger outward pinch on the stable interaction surface.
3. Prove the application eventually emits a fresh valid resize outcome that
   differs from the pre-gesture dimensions and remains usable for one exact
   printable PTY input. Do not assert the exact dimensions or font size.
4. Reload the same page/session with the mock boundaries still installed.
5. Prove the terminal boots with the post-gesture dimension outcome (or an
   equivalently stable density outcome), remains visible, and accepts input
   exactly once. This is the persistence contract without naming storage.

If Playwright WebKit cannot synthesize the application pinch path
deterministically through the stable surface, make no test pass by inspecting
renderer/storage internals. Report pinch application/persistence as physical
iPhone and retain the existing legacy unit characterization until Task 12.

# Verification commands

```bash
npm run web:smoke -- --project=mobile-webkit crates/ajax-web/web/e2e/terminal-behavior.test.ts
rg -n "ghostty|xterm|canvas|textarea|localStorage|sessionStorage|fontSize|terminal-host|data-terminal-engine|waitForTimeout" crates/ajax-web/web/e2e/terminal-behavior.test.ts
```

# Acceptance criteria

- Public pinch/persistence is automated only if deterministic and
  implementation-independent.
- Unsupported setting cases are explicitly mapped to legacy/manual coverage,
  not silently claimed.
- Existing permanent tests remain green.

# Stop conditions

- The test needs storage keys, exact renderer font options, or private DOM.
- Synthetic pinch does not yield a deterministic public PTY/application
  outcome after two focused attempts.

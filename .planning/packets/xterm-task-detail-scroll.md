# Xterm integration fix — keep task details reachable

## Status

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
```

## Goal

Mounting xterm must not make the existing desktop Task details summary
unreachable from the route scroll owner. Preserve the single route-scroll
architecture and the existing terminal/mobile behavior.

## Authoritative RED

This existing behavior test fails consistently after xterm mounts:

```bash
npx playwright test crates/ajax-web/web/e2e/actions.test.ts \
  --config crates/ajax-web/web/playwright.config.mts \
  --project=mobile-webkit --workers=1 \
  --grep 'Copy buttons copy branch and worktree path'
```

Failure: `.meta-details summary` is visible in the DOM but outside the viewport;
Playwright cannot scroll it into view and times out. Reproduce RED before edits.

## Allowed files

- `crates/ajax-web/web/src/components/TaskTerminal.svelte`
- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`

Do not edit the existing test, TaskDetail, route-scroll ownership, fixtures, or
global layout.

## Review revision

The normal desktop cap fixes the authoritative action test but is not accepted
yet. The selector excludes `.is-expanded`, while fixed fullscreen layout exists
only in the phone/coarse-short media query. Add a desktop regression that enters
expanded mode and proves terminal height remains bounded and the following
Task-details summary can still be scrolled into the viewport. It must fail
before the implementation revision. Then either retain a desktop cap when
expanded or apply a coherent desktop fullscreen layout; choose the smaller
behavior consistent with the existing component. Remove the redundant identical
`height`/`max-height` declarations so one rule owns the cap.

## Diagnosis requirement

Use a temporary evaluation or Playwright diagnostic, without committing debug
code, to compare bounding boxes and scroll metrics for route-scroll,
terminal-panel, terminal interaction/host/xterm, and the metadata summary.
Identify which terminal dimension prevents route-scroll from exposing the
summary.

## Implementation

Make the smallest terminal-local CSS/layout correction so the normal desktop
terminal has a bounded usable height and the existing route scroll can reach
following content. Keep the existing 38vh phone rule and fullscreen behavior.
Do not add a second vertical page scroll owner or use test-only selectors/code.

## Verification

1. Focused action test above: GREEN.
2. Full `terminal-behavior.test.ts`, Mobile WebKit, one worker: all green.
3. `layout-scroll.test.ts`, Mobile WebKit, one worker: all green.
4. `npm run web:check`.
5. `git diff --check`.

No test edits, plan/packet/generated changes, commits, branches, dependencies,
private xterm APIs, or unrelated refactors.

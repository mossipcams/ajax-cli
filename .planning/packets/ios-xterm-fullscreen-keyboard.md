# TDD Packet: iOS fullscreen band + keyboard chrome + compact keys

## Status

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## Goal

On phone layout:

1. Expanded terminal pins to the visual viewport band
   (`top: var(--app-band-top)`, `height: var(--app-band-height)`), so the
   Expand control stays tappable and the panel sits above the iOS keyboard.
2. `html.keyboard-open` and `html.terminal-expanded` hide cockpit chrome,
   bottom-nav, and task detail chrome so the terminal fills the band; Hide
   keyboard / exit expand restores them (and clears inert).
3. Terminal interaction surface hides its scrollbar chrome (single scroll
   affordance).
4. Control-key buttons are compact (visually smaller than the current 44×44
   tiles) so the key bar wastes less band height.

## Allowed files

- `crates/ajax-web/web/src/components/TaskTerminal.svelte`
- `crates/ajax-web/web/src/styles.css`
- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`
- `crates/ajax-web/web/src/components/App.test.ts` (only if source-contract
  assertions need the restored keyboard-open chrome rules)
- `.planning/agent-plans/ios-xterm-mobile-bugs.md`
- `.planning/packets/ios-xterm-fullscreen-keyboard.md`

## Forbidden changes

- Do not touch Rust, architecture.md, registry, PTY protocol, or auth.
- Do not reintroduce Ghostty / terminalClipboard private APIs.
- Do not implement Copy/selection in this packet (Task 2).
- Do not commit, push, merge, rebase, or change branches.
- Do not weaken unrelated assertions or skip tests.
- Do not add dependencies.

## Context evidence

- Graphify: `NOT_REQUIRED` — confined to Web Cockpit Svelte/CSS already mapped
  by xterm rebuild packets.
- Serena: `NOT_REQUIRED` — exact anchors below from parent inspection.
- ast-grep: `NOT_REQUIRED` — CSS/Svelte string anchors are stable.

## Code anchors

Broken expanded positioning (must become `--app-band-top`):

```1034:1043:crates/ajax-web/web/src/components/TaskTerminal.svelte
    :global(html.terminal-expanded) .terminal-panel.is-expanded {
      position: fixed;
      top: 0;
      right: 0;
      left: 0;
      z-index: 45;
      display: flex;
      flex-direction: column;
      height: var(--app-band-height, 100dvh);
```

Correct band pattern (mirror this):

```17:23:crates/ajax-web/web/src/components/FullscreenLayer.svelte
  .fullscreen-layer {
    position: fixed;
    top: var(--app-band-top, 0px);
    left: 0;
    right: 0;
    height: var(--app-band-height, 100dvh);
```

Lost chrome collapse (restore into `styles.css` mobile media query; from
`cec3c56`):

```css
html.keyboard-open .task-detail .detail-header,
html.keyboard-open .task-detail .interact-panel,
html.keyboard-open .task-detail .meta-details,
html.terminal-expanded .task-detail .detail-header,
html.terminal-expanded .task-detail .interact-panel,
html.terminal-expanded .task-detail .meta-details {
  display: none;
}

html.keyboard-open .cockpit-chrome,
html.keyboard-open .bottom-nav,
html.terminal-expanded .cockpit-chrome,
html.terminal-expanded .bottom-nav {
  display: none;
}
```

Oversized keys:

```989:999:crates/ajax-web/web/src/components/TaskTerminal.svelte
  .terminal-key {
    flex: none;
    min-width: 44px;
    min-height: 44px;
```

Scrollbar hide pattern to mirror on `.terminal-interaction-wrap`:

```378:397:crates/ajax-web/web/src/styles.css
[data-testid="route-scroll"] {
  ...
  scrollbar-width: none;
}
[data-testid="route-scroll"]::-webkit-scrollbar {
  display: none;
  width: 0;
  height: 0;
}
```

Existing expand / inert cases to keep green:

- `phone fullscreen keeps background controls inert until exit`
- `fullscreen enter and exit keep one socket...`
- `terminal controls meet mobile touch target size on phone` — **update** this
  case so compact keys assert a smaller target (e.g. height ≥ 32 and ≤ 40, or
  exact compact size you choose) while Expand / New output / Reconnect /
  Paste-action buttons may remain ≥ 44 if they stay large. Document the
  chosen compact size in the test name/comments.

## RED

Add/update Mobile WebKit cases that fail on current code:

1. After expand, with simulated `--app-top: 40px` / `--app-height: 460px` and
   `html.keyboard-open`, the expand button's bounding box intersects the
   visible band and `aria-pressed` can be toggled back to `false` via click.
2. Under `html.keyboard-open` on a task route, `.cockpit-chrome` and
   `.bottom-nav` compute `display: none`; clearing the class restores them.
3. Under `html.terminal-expanded`, same chrome hide.
4. `.terminal-interaction-wrap` has hidden scrollbar chrome
   (`scrollbar-width: none` / webkit scrollbar display none).
5. `.terminal-keys .terminal-key` heights are compact (not ≥ 44).

Run RED before production edits.

## Implementation

1. In `TaskTerminal.svelte`, change expanded `top: 0` →
   `top: var(--app-band-top, 0px)`. Raise expand-corner z-index if needed so
   it stays above the host (keep ≥ 5, prefer 8+).
2. In `styles.css` mobile media query, restore the keyboard-open /
   terminal-expanded chrome collapse rules (task detail + cockpit + bottom-nav).
3. Hide scrollbars on `.terminal-interaction-wrap`.
4. Compact `.terminal-key` to ~32px min height / tighter min-width; keep
   Expand corner and New output at usable ≥ 44 targets.
5. Update the touch-target e2e accordingly.

## Verification

```bash
rtk npx playwright test crates/ajax-web/web/e2e/terminal-behavior.test.ts \
  --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit --workers=1 \
  --grep 'fullscreen band|keyboard-open hides|terminal-expanded hides|interaction wrap hides scrollbar|compact terminal keys|phone fullscreen keeps background|fullscreen enter and exit keep|terminal controls meet'

rtk npm run web:check
rtk git diff --check
```

Also keep layout-scroll green if chrome CSS overlaps:

```bash
rtk npx playwright test crates/ajax-web/web/e2e/layout-scroll.test.ts \
  --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit --workers=1
```

## Stop conditions

- Patch would exceed ~400 changed lines
- Need Ghostty/private APIs
- Copy/selection work creeps in
- Architecture / Rust changes requested by the failing test

## Return

Exact `DELEGATE_REPORT` schema with RED/GREEN/VERIFY command evidence.

# TDD Implementation Packet — task terminal keyboard band layout

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## Goal

Fix four coupled mobile Web Cockpit terminal layout defects on the task page:

1. **Inline + keyboard**: when the soft keyboard is open and the terminal is **not** fullscreen, the terminal (+ key toolbar) fills the remaining visible band above the keyboard.
2. **Fullscreen + keyboard**: expanded terminal uses the same visual-viewport band as `FullscreenLayer` and must not clip the bottom (keys / last rows).
3. **Default width**: non-expanded terminal is horizontally full-bleed on mobile (same width as fullscreen).
4. **Keep top chrome**: when **not** fullscreen and keyboard is open, `.detail-header` (back) and `.interact-panel` (statuses) stay visible. Only fullscreen (`terminal-expanded`) hides task chrome.

## Allowed files

Production:

- `crates/ajax-web/web/src/styles.css`
- `crates/ajax-web/web/src/components/TaskTerminal.svelte`
- `crates/ajax-web/web/src/components/TaskDetail.svelte` (only if a tiny flex helper class is required for keyboard-open fill; prefer CSS in `styles.css`)

Tests:

- `crates/ajax-web/web/src/components/App.test.ts`
- `crates/ajax-web/web/src/components/TaskDetail.test.ts` (only if existing mobile padding assertions need updating)
- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`

## Forbidden changes

- Do not edit `viewport.ts` hysteresis / keyboard detection unless a test proves `--app-band-*` is wrong (it is not the root cause here).
- Do not change PTY protocol, FitAddon min-col math (`MIN_TERMINAL_COLS`), or Ghostty paths.
- Do not hide `.detail-header` / `.interact-panel` under `html.keyboard-open`.
- Do not remove `html.terminal-expanded` chrome hiding.
- Do not change desktop (`min-width: 768px`) terminal height rules except to keep selectors from colliding.
- No formatting sweeps, renames, or drive-by cleanup.
- Do not hand-edit `crates/ajax-web/web/dist/*` (build scripts may regenerate; do not manually patch).
- Do not commit, push, merge, rebase, or change branches.

## Context evidence

- **Graphify:** `NOT_REQUIRED` — presentation/layout only; core registry/lifecycle untouched.
- **Serena:** `NOT_REQUIRED` — anchors below are exact.
- **ast-grep:** `NOT_REQUIRED` — CSS/layout + e2e contract updates.

## Code anchors

### A. Chrome hide rules (`styles.css` ~404–418)

Today `keyboard-open` and `terminal-expanded` share one block that hides task chrome:

```css
html.keyboard-open .task-detail .detail-header,
html.keyboard-open .task-detail .interact-panel,
html.keyboard-open .task-detail .meta-details,
html.terminal-expanded .task-detail .detail-header,
html.terminal-expanded .task-detail .interact-panel,
html.terminal-expanded .task-detail .meta-details {
  display: none;
}
```

**Required split:**

- `html.keyboard-open` may still hide `.cockpit-chrome` and `.bottom-nav`.
- `html.keyboard-open` may hide `.meta-details` (accordion below terminal) to free vertical space.
- `html.keyboard-open` must **NOT** hide `.detail-header` or `.interact-panel`.
- `html.terminal-expanded` continues to hide task chrome + cockpit chrome + bottom-nav.

### B. Full-bleed width (`styles.css` mobile media query)

`[data-testid="route-scroll"]` still has horizontal `20px + safe-area` padding. Fullscreen terminal is `left:0; right:0`. Add mobile task-route full-bleed:

```css
[data-testid="route-scroll"]:has([data-outlet="task"]) {
  padding-left: 0;
  padding-right: 0;
}
```

TaskDetail already pads `.detail-header` / `.interact-panel` with `12px + safe-area` on mobile — keep that.

### C. Keyboard-open flex fill (mobile)

When `html.keyboard-open` and not expanded, `AppViewport` is already `position: fixed` to `--app-band-*`. Make the task route fill that band and give the terminal the leftover height:

- `route-scroll:has([data-outlet="task"])` → column flex, `min-height: 0`, `overflow: hidden`, reduce/zero bottom padding while keyboard-open.
- `.task-detail` → column flex, `flex: 1`, `min-height: 0`, `height: 100%` under keyboard-open.
- `.terminal-panel` (not expanded) → `flex: 1 1 auto; min-height: 0`.
- Override the capped height:

```css
/* current bug in TaskTerminal.svelte */
.terminal-panel:not(.is-expanded) .terminal-interaction-wrap {
  height: min(38vh, 300px);
}
```

Under `html.keyboard-open`, non-expanded interaction wrap must be `height: auto; flex: 1 1 auto; min-height: 0` (not 38vh capped).

### D. Expanded clip (`TaskTerminal.svelte` ~1260–1288)

```css
:global(html.terminal-expanded) .terminal-panel.is-expanded {
  position: fixed;
  top: var(--app-band-top, 0px);
  height: var(--app-band-height, 100dvh);
  padding: env(safe-area-inset-top) 0 0; /* BUG: FullscreenLayer has no such padding */
  box-sizing: border-box;
  ...
}
```

**Required:** match `FullscreenLayer.svelte` — use `--app-band-top` / `--app-band-height` **without** extra `padding: env(safe-area-inset-top) 0 0` that shrinks the content box and clips the bottom. Ensure:

```css
.terminal-panel.is-expanded .terminal-interaction-wrap {
  flex: 1 1 auto;
  min-height: 0;
  height: auto;
}
.terminal-panel.is-expanded .terminal-host {
  height: 100%;
  min-height: 0;
}
```

so the key toolbar stays inside the band above the keyboard.

### E. Existing e2e to update

`crates/ajax-web/web/e2e/terminal-behavior.test.ts`:

- `"keyboard-open hides cockpit chrome and bottom nav on task route"` — keep asserting cockpit + bottom-nav hide; **add** assertions that `.detail-header` and `.interact-panel` remain **not** `display: none`.
- `"terminal-expanded hides cockpit chrome and bottom nav on task route"` — keep; optionally assert task chrome also hidden.
- `"fullscreen band keeps expand tappable under keyboard-open offset band"` — must stay green.

## Test-first instructions

Add/adjust tests **before** production edits. Prefer source-contract unit tests in `App.test.ts` (fast RED) plus the e2e updates above.

### Unit (App.test.ts) — load `styles.css` the same way existing App tests do

Add tests approximately named:

1. `"zeros horizontal padding on the mobile task route-scroll"`
   - Assert mobile CSS for `[data-testid="route-scroll"]:has([data-outlet="task"])` sets `padding-left: 0` and `padding-right: 0`.

2. `"keyboard-open keeps task header and interact panel visible"`
   - Assert `styles.css` does **not** include selectors that hide `.task-detail .detail-header` or `.interact-panel` under `html.keyboard-open`.
   - Assert `html.terminal-expanded` still hides those.

3. `"keyboard-open still hides bottom nav and cockpit chrome"`
   - Keep/confirm existing contract for those two.

### TaskTerminal source contract (can live in App.test.ts or a small TaskTerminal source read in an existing test file — prefer adding assertions next to other terminal CSS contracts if present; otherwise App.test.ts reading the `.svelte` source is fine)

4. `"expanded terminal panel matches fullscreen band without safe-area top padding"`
   - Assert expanded rule uses `--app-band-top` and `--app-band-height`.
   - Assert the expanded rule block does **not** contain `padding: env(safe-area-inset-top)`.

5. `"keyboard-open non-expanded terminal fills remaining band"`
   - Assert CSS under keyboard-open overrides the `min(38vh, 300px)` cap for non-expanded interaction wrap (flex/auto height).

### E2E

Update `"keyboard-open hides cockpit chrome and bottom nav on task route"` as in anchor E.

### RED command

```bash
npm run web:test -- --run src/components/App.test.ts
```

Must fail on the new assertions before production CSS edits.

Then after GREEN unit:

```bash
npm run web:e2e -- --grep "keyboard-open hides|terminal-expanded hides|fullscreen band keeps expand"
```

(or the project's equivalent playwright script — check `package.json`; use whatever existing e2e runner this repo uses).

## Edit instructions

Smallest CSS-first path:

1. Split chrome-hide rules in `styles.css` per anchor A.
2. Add mobile task full-bleed padding per anchor B.
3. Add keyboard-open flex fill rules for task route / task-detail / terminal-panel per anchor C (in `styles.css` and/or `TaskTerminal.svelte` scoped CSS — prefer keeping terminal height overrides next to the existing `38vh` rule in `TaskTerminal.svelte`).
4. Fix expanded panel padding + flex fill per anchor D in `TaskTerminal.svelte`.
5. Update e2e per anchor E.
6. Do **not** change FitAddon / resize JS unless a focused test proves refit never runs after keyboard-open layout; if needed, only call existing `schedulePostLayoutRef?.(true)` on keyboard class transitions already handled elsewhere — prefer CSS-only.

## Verification commands

```bash
npm run web:test -- --run src/components/App.test.ts src/components/TaskDetail.test.ts
npm run web:check
npm run web:e2e -- --grep "keyboard-open hides|terminal-expanded hides|fullscreen band keeps expand"
```

If the e2e script name differs, use the repo's documented playwright command and report it.

## Acceptance criteria

- Non-fullscreen + keyboard-open: back button and status panel visible; terminal fills leftover band above keyboard; bottom-nav hidden.
- Fullscreen: panel height tracks `--app-band-height` from `--app-top`; bottom keys not clipped by extra safe-area top padding.
- Mobile task terminal width is full-bleed (route-scroll horizontal padding zero).
- RED→GREEN unit evidence; updated e2e green.
- Diff only in Allowed files.

## Stop conditions

- Diff escapes Allowed files.
- Patch exceeds ~400 lines or redesigns the terminal engine / viewport hysteresis.
- Removes `terminal-expanded` chrome hiding.
- Breaks `"fullscreen band keeps expand tappable under keyboard-open offset band"`.
- Attempts to "fix" width by changing `MIN_TERMINAL_COLS` instead of full-bleed padding.

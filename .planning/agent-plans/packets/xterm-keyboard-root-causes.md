# TDD Implementation Packet — xterm keyboard root causes

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## Goal

Port the two Ghostty root causes that the xterm rewrite dropped, plus a real
one-row status panel:

1. Bottom-anchor / soften the xterm helper textarea so iOS places the keyboard
   against the terminal (not page top / offscreen chase).
2. Expand settle: while entering fullscreen, re-fit with `discreteIntent` across
   2 rAFs + `EXPAND_REWRAP_MS` (280) so the grid lands above the keyboard after
   iOS finishes animating `visualViewport`.
3. Mobile `.interact-panel` is one compact row (summary ellipsis + actions in a
   single nowrap horizontal strip), not a tall multi-wrap card.

Keep the already-merged full-bleed width fix untouched.

## Allowed files

- `crates/ajax-web/web/src/components/TaskTerminal.svelte`
- `crates/ajax-web/web/src/components/TaskDetail.svelte`
- `crates/ajax-web/web/src/components/TaskTerminal.test.ts` (**create**)
- `crates/ajax-web/web/src/components/TaskDetail.test.ts`
- `crates/ajax-web/web/e2e/terminal-behavior.test.ts` (only if an existing keyboard
  expand test must be extended for settle; prefer unit tests first)
- `crates/ajax-web/web/dist/*` only via `npm run web:build` at the end

## Forbidden changes

- Do not revert full-bleed `[data-testid="route-scroll"]:has([data-outlet="task"])`
  `padding-left/right: 0`.
- Do not reintroduce `html.keyboard-open` hiding of `.detail-header` / `.interact-panel`.
- Do not change `MIN_TERMINAL_COLS`, FitAddon scale math, or PTY protocol.
- Do not restore Ghostty / TerminalRawView.
- No formatting sweeps, renames, or drive-by cleanup.
- Do not commit / push / change branches.

## Context evidence

- **Graphify:** `NOT_REQUIRED` — presentation + focus geometry only.
- **Serena:** `NOT_REQUIRED` — exact Ghostty port anchors below.
- **ast-grep:** `NOT_REQUIRED` — Svelte/CSS port, not structural Rust rewrite.

## Code anchors

### A. Ghostty hardenMobileTextarea (source of truth — port to xterm)

From pre-removal `TerminalRawView.svelte`:

```js
const hardenMobileTextarea = () => {
  const input = term?.textarea; // xterm: termTextarea() / querySelector textarea.xterm-helper-textarea
  if (!input) return;
  input.setAttribute("autocapitalize", "off");
  input.setAttribute("autocorrect", "off");
  input.setAttribute("autocomplete", "off");
  input.setAttribute("spellcheck", "false");
  input.style.fontSize = "16px";
  input.style.position = "absolute";
  input.style.bottom = "0";
  input.style.height = "44px";
  input.style.width = "100%";
  input.style.opacity = "0.01";
  input.style.setProperty("clip-path", "none");
  input.style.setProperty("-webkit-clip-path", "none");
  input.style.setProperty("clip", "auto");
  input.style.color = "transparent";
  input.style.setProperty("-webkit-text-fill-color", "transparent");
  input.style.caretColor = "transparent";
};
```

CSS companion:

```css
.terminal-host :global(textarea.xterm-helper-textarea) {
  position: absolute !important;
  left: 0 !important;
  top: auto !important;
  bottom: 0 !important;
  height: 44px !important;
  width: 100% !important;
  opacity: 0.01 !important;
  clip-path: none !important;
  -webkit-clip-path: none !important;
  color: transparent;
  -webkit-text-fill-color: transparent;
  caret-color: transparent;
  z-index: 1;
}
```

(`!important` needed to override xterm’s bundled `left:-9999em;top:0` rules.)

Call `hardenMobileTextarea()` immediately after `liveTerm.open(hostEl)` (and
whenever reconnect recreates focus path if needed). Skip Ghostty-only
`seedBackspaceSentinel` unless already present for xterm.

### B. Focus path must reset scroll

Import `resetDocumentScroll` from `../viewport`.

In `onInteractionClick` and before any touch-driven focus of the textarea:

```ts
resetDocumentScroll();
textarea.focus({ preventScroll: true });
```

### C. Expand settle (root cause of fullscreen bottom clip)

Today `toggleExpanded` only calls `schedulePostLayoutRef?.(entering)` once.

Required: when `entering === true`, run a settle sequence that keeps calling
`schedulePostLayoutRef?.(true)` (discreteIntent) on:

1. immediate (existing)
2. next rAF
3. following rAF
4. `setTimeout(..., 280)` (`EXPAND_REWRAP_MS`)

Clear pending timers/frames on dispose and when exiting expand.
While keyboard-open, only `discreteIntent === true` bypasses the fit freeze —
so every settle tick **must** pass `true`.

Constant: `const EXPAND_REWRAP_MS = 280;` at module/script top of TaskTerminal.

### D. Status panel one row (TaskDetail mobile)

In `TaskDetail.svelte` mobile media query, make `.interact-panel` a single row:

- `display: flex; flex-direction: row; align-items: center; gap: 8px;`
- tighter padding (e.g. `8px 12px` + safe-area)
- `.interact-summary` already nowrap/ellipsis; ensure `flex: 1; min-width: 0`
- `.interact-activity` hidden on mobile **or** same single-line slot (prefer hide
  activity when summary exists to keep one row)
- ActionBar row: force nowrap + horizontal scroll (`flex-wrap: nowrap;
  overflow-x: auto`) via `:global(.action-row)` inside the panel on mobile

Do **not** only rely on the previous nowrap CSS — the panel layout itself must
be one row.

## Test-first instructions

Create `crates/ajax-web/web/src/components/TaskTerminal.test.ts` reading
`TaskTerminal.svelte?raw` (same pattern as App.test.ts / legacy TerminalRawView
source contracts).

Add tests (must fail before edits):

1. `"anchors the xterm helper textarea to the host bottom for iOS keyboard placement"`
   - CSS block matches `bottom:\s*0` and overrides `left` away from `-9999`
   - source contains `style.bottom = "0"` (or equivalent harden assignment)

2. `"softens textarea clip/opacity so iOS treats it as an edit target"`
   - `opacity:\s*0\.01`, `clip-path:\s*none`

3. `"resets document scroll before focusing the terminal textarea"`
   - `resetDocumentScroll` imported/used before `focus({ preventScroll: true })`
     on the interaction click/focus path

4. `"re-fits through the expand settle window with discrete intent"`
   - source contains `EXPAND_REWRAP_MS` (280) and a settle path that calls
     `schedulePostLayout` / `schedulePostLayoutRef` with `true` after timeout
   - ideally also shows nested `requestAnimationFrame` settle calls

Update `TaskDetail.test.ts`:

5. `"keeps the mobile interact panel to a single row"`
   - mobile CSS for `.interact-panel` includes `flex-direction:\s*row` (or
     equivalent single-row flex) and summary `min-width:\s*0` / ellipsis contract

RED:

```bash
npm run web:test -- --run src/components/TaskTerminal.test.ts src/components/TaskDetail.test.ts
```

## Edit instructions

1. Implement A–C in `TaskTerminal.svelte` (smallest port; no layout-policy module
   revive unless unavoidable — inline EXPAND_REWRAP_MS + local timers is fine).
2. Implement D in `TaskDetail.svelte`.
3. Make tests green.
4. Run `npm run web:check` and focused e2e:
   `npm run web:smoke -- --grep "keyboard-open expand|fullscreen band keeps expand|keyboard-open hides"`
5. `npm run web:build` so dist matches.

## Verification commands

```bash
npm run web:test -- --run src/components/TaskTerminal.test.ts src/components/TaskDetail.test.ts
npm run web:check
npm run web:smoke -- --grep "keyboard-open expand|fullscreen band keeps expand|keyboard-open hides"
npm run web:build
```

## Acceptance criteria

- Helper textarea is bottom-anchored and softened (not `left:-9999em; top:0`).
- Expand while keyboard-open schedules discreteIntent refits through 280ms settle.
- Focus path calls `resetDocumentScroll` before focus.
- Mobile interact-panel is one row.
- Full-bleed width rules remain.
- RED→GREEN evidence in DELEGATE_REPORT.

## Stop conditions

- Touches files outside Allowed files.
- Reverts full-bleed padding.
- Re-hides task header under `keyboard-open`.
- Exceeds ~400 changed lines or rebuilds terminalLayoutPolicy without need.
- CSS-only flex tweaks without textarea harden + expand settle.

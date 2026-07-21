```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

Tune mobile/iOS PWA hotbar key look (font, spacing, size) and remove the hotbar `⌄` Hide keyboard control. Fullscreen exit remains the corner expand (`⛶`) control only.

## Allowed files

- `crates/ajax-web/web/src/features/task/TaskTerminal.tsx`
- `crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx`
- `crates/ajax-web/web/src/styles.css`
- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`
- `crates/ajax-web/web/dist/app.css` (via `npm run web:build` only)
- `crates/ajax-web/web/dist/app.js` (via `npm run web:build` only)

## Forbidden changes

- Do not remove or change the corner expand/collapse control.
- Do not change CONTROL_KEYS Esc/Tab/⌃C/arrows/Ctrl/Paste behavior.
- Do not change Drop, viewport scroll, or keyboard-band pin geometry rules except hotbar key chrome.
- Do not commit, push, merge, rebase, or change branches.
- Do not hand-edit dist; rebuild only.

## Context evidence

1. **Desired behavior** — User: adjust hotbar keys for iOS PWA WebKit (font, spacing, size); remove the exit-fullscreen-looking key from the hotbar. The only hotbar collapse/exit glyph is `⌄` (`aria-label="Hide keyboard"`) at the end of the toolbar in `TaskTerminal.tsx`.
2. **Source anchors** — Hide keyboard button ~lines 1190–1198 in `TaskTerminal.tsx`. Base `.terminal-key` / `.terminal-keys` and mobile overrides in `styles.css` ~1584–1702. E2E `Hide keyboard focus blur adds no PTY input` in `terminal-behavior.test.ts` clicks that button.
3. **Patterns** — `TaskTerminal.test.tsx` source-contract tests for hotbar flex distribution and aria labels. Extend those; adjust e2e to blur via another path (e.g. tap outside / programmatic blur) or delete only the Hide-keyboard-specific assertion while keeping "toolbar blur adds no PTY input" coverage if still meaningful.
4. **Boundaries** — Frontend chrome only; rebuild dist so embedded assets ship.

## Code anchors

- `TaskTerminal.tsx` — Hide keyboard `<button … aria-label="Hide keyboard">` / `⌄`
- `styles.css` — `.terminal-keys`, `.terminal-key`, mobile `@media` block `.terminal-keys .terminal-key`
- `TaskTerminal.test.tsx` — `distributes hotbar keys…`, `names terminal control keys…`
- `e2e/terminal-behavior.test.ts` — `Hide keyboard focus blur adds no PTY input`

## Test-first instructions

1. In `TaskTerminal.test.tsx`:
   - Assert `taskTerminalSource` does **not** match `Hide keyboard` or the `⌄` hotbar button markup.
   - Assert mobile hotbar CSS contracts for the tuned look (exact values below in Edit instructions): gap, padding, font-size, min-height on `.terminal-keys` / `.terminal-key` inside the mobile media block.
2. Update or rewrite the e2e test that clicks `Hide keyboard` so it no longer depends on that control (prefer deleting that one test if no equivalent control remains; do not leave a failing e2e).

Red command:

```bash
cd crates/ajax-web/web && npx vitest run --config vite.config.mts src/features/task/TaskTerminal.test.tsx
```

## Edit instructions

1. **TaskTerminal.tsx** — Delete the Hide keyboard button (the `⌄` control and its onClick blur handler). Keep Esc/Tab/⌃C/arrows/Ctrl/Paste.

2. **styles.css** — Inside the mobile/coarse media block that already styles `.terminal-keys` / `.terminal-keys .terminal-key`, apply:
   - `.terminal-keys`: `gap: 6px`; horizontal padding `4px 6px` (keep existing keyboard-open / expanded padding-bottom rules).
   - `.terminal-keys .terminal-key`: `min-height: 36px`; `padding: 2px 4px`; `font-size: var(--text-body-sm)`; `font-family: var(--sans)`; `-webkit-text-size-adjust: 100%`; keep `flex: 1 1 0`, `min-width: 0`, `width: 0`.
   Do not weaken the keyboard-open safe-area pad override (`padding-bottom: 6px`).

3. Update unit source contracts to match the new CSS and absent Hide keyboard control.

4. Fix e2e accordingly.

5. Run `npm run web:build` from `crates/ajax-web/web` (or repo `npm run web:build`) so `dist/app.css` / `dist/app.js` update.

## Verification commands

```bash
cd crates/ajax-web/web && npx vitest run --config vite.config.mts src/features/task/TaskTerminal.test.tsx
npm run web:build
```

Optional if practical (may be slow): Playwright e2e for the former Hide keyboard test path — only if the suite is already easy to run; otherwise unit + build is enough and note e2e skipped.

## Acceptance criteria

- No Hide keyboard / `⌄` hotbar control in TaskTerminal.
- Mobile hotbar CSS matches the tuned font/spacing/size contracts.
- Corner expand control still present.
- Dist rebuilt with the CSS changes.
- No edits outside allowed files.

## Stop conditions

- Need to remove corner expand or change CONTROL_KEYS semantics.
- Pixel values disputed beyond the specified tokens — stop and ask.
- False green without production edit.

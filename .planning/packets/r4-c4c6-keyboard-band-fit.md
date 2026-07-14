# Packet R4 — C4/C6: keyboard-open band must show the prompt, not a cropped blank

All paths relative to `crates/ajax-web/web`; run all commands from there.

## 1. Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## 2. Goal

While the soft keyboard is open (layout policy freezes fit; PTY resize stays
withheld — that must NOT change), the terminal canvas must fit the visible band
with the written content (fresh CLI prompt) visible: no ~172px of canvas above
the host, no oversized canvas (fillY ≤ 1.08), no blank band below content when
the host shrinks (reconnect strip, paste fallback, keyboard settle) or in
landscape. On keyboard close the existing unfrozen fit path restores normal
behavior (already works).

## 3. Allowed files

- `src/components/TerminalRawView.svelte`
- `src/terminalLayoutPolicy.ts` — only if a new decision field is genuinely needed
- `src/terminalGeometry.ts` + `src/terminalGeometry.test.ts` — only for a new
  pure helper and its focused unit test

## 4. Forbidden changes

- Any WS/PTY frame or `sendResize` semantics: the frozen keyboard-open path
  must never notify the PTY (no SIGWINCH). `terminalConnection.ts` untouched.
- `terminalRefit.ts` scheduling policy.
- `allowLocalFit === true` behavior (pinch/expand exemptions, normal fit path).
- scrollFollow policy, e2e files, any other component.

## 5. Context evidence

- **Graphify:** NOT_REQUIRED — change is confined to the terminal view's local
  fit/crop math; architecture boundary (browser never owns task truth / PTY
  contract) is respected by the explicit no-SIGWINCH constraint above.
- **Serena:** NOT_REQUIRED — anchors below were collected by direct read;
  no cross-file symbol refactor is involved.
- **ast-grep:** NOT_REQUIRED — anchors are exact functions/lines in one Svelte
  component plus optional pure helpers; text/line anchors below are sufficient.

## 6. Code anchors

`src/components/TerminalRawView.svelte` (line numbers may drift ±10; re-locate
by symbol):

- `fitNow` :657–718. Frozen branch :663–669:
  ```ts
  if (!decision.allowLocalFit) {
    if (decision.cropToBottom && container) {
      container.scrollTop = Math.max(0, container.scrollHeight - container.clientHeight);
    }
    if (scrollFollow.isPinned()) snapScrollbackToBottom();
    return;
  }
  ```
- Same bottom-crop duplicated in `snapVisibleTerminal` :752–760.
- Host resize re-enters `fitNow` via `ResizeObserver(scheduleDebouncedRefit)`
  :795 — so correct frozen-branch math re-runs on every host shrink (C6).
- Primitives available inside `onMount` scope:
  - `terminalInternals(term).getScrollbackLength?.()` (:277)
  - `term.buffer.active` — `.length`, `.getLine(i).translateToString(true)`
    (see probe :1006) for last written row; `term.getViewportY()`
  - cell metrics: `(term as TerminalWithRendererMetrics).renderer?.getMetrics?.()`
    → `{ width, height }` (:698 uses width)
  - applied CSS scale: `terminalFitScale` (set by `applyTerminalScale` :404–434,
    `transform: scale(s)` origin 0 0 on `term.element` when s < 1)
  - `sendResize` is the PTY path — the frozen branch must never call it.
- `src/terminalLayoutPolicy.ts` :40–50 — `allowLocalFit: !keyboardOpen || intent`,
  `cropToBottom: keyboardOpen && !intent`.

Key geometry fact: the failing assertion `fillY = canvasRect.height /
hostRect.height ≤ 1.08` uses `getBoundingClientRect()`, so a pure
`container.scrollTop` crop can never pass — the canvas box itself must end up
band-sized. The intended mechanism is a **local-only row refit**: compute rows
that fit the current host (cell height × `terminalFitScale`), call
`term.resize(currentCols, fitRows)` in the frozen branch **without** any
`sendResize`, then position so the written content (last row = scrollback +
cursor row) is visible — content shorter than the band sits at the top with
crop 0. An equivalent CSS box-clip is acceptable if it makes the canvas rect
fit and keeps the prompt visible, but local row refit is the expected shape.
Guard against thrash: only re-resize when the computed fitRows actually
changed.

## 7. Test-first instructions

Failing e2e already exist — show them red before editing (RED phase), green
after (GREEN phase). All from `crates/ajax-web/web`, `--project=mobile-webkit`:

```bash
npx playwright test e2e/explore-keyboard-blank-jump.test.ts -g "cropped empty band" --project=mobile-webkit   # asserts canvasAboveHost ≤ 24 AND fillY ≤ 1.08
npx playwright test e2e/explore-c4c5-siblings.test.ts --project=mobile-webkit                                  # 3 tests: reconnect, paste fallback, keyboard settle
npx playwright test e2e/explore-terminal-visual.test.ts -g "landscape" --project=mobile-webkit                 # blankBelowCanvas ≤ 12
```

If you add a pure helper to `terminalGeometry.ts`, write its focused failing
vitest in `src/terminalGeometry.test.ts` first and show it red
(`npx vitest run src/terminalGeometry.test.ts`).

## 8. Edit instructions

1. In `fitNow`'s frozen branch (and unify `snapVisibleTerminal`'s duplicate so
   both use one function): replace unconditional bottom-crop with
   content-aware local fit:
   - compute scaled cell height = `metrics.height × terminalFitScale`;
   - compute rows fitting `container.clientHeight`;
   - if different from `term.rows`, `term.resize(term.cols, fitRows)` — local
     only, no `sendResize`;
   - determine last written row (scrollback length + active cursor/last
     non-empty row); scroll the library viewport so that row is visible
     (bottom-pinned only when content exceeds the band; otherwise top);
   - clamp `container.scrollTop` to keep the canvas box aligned with the band
     (target 0 when the box now fits).
2. Keep `pinToBottomOnKeyboardOpen` / `scrollFollow.isPinned()` snap behavior
   for content taller than the band.
3. Do not touch the `allowLocalFit` path, pinch/expand exemptions, or any
   `sendResize` call site.
4. If a decision field must be added to `terminalLayoutPolicy.ts`, extend
   `LayoutDecision` minimally and update its unit test file only as required
   by compilation (`src/terminalLayoutPolicy.test.ts` may gain a focused case;
   do not weaken existing assertions).

## 9. Verification commands

```bash
npx playwright test e2e/explore-keyboard-blank-jump.test.ts -g "cropped empty band" --project=mobile-webkit
npx playwright test e2e/explore-c4c5-siblings.test.ts --project=mobile-webkit
npx playwright test e2e/explore-terminal-visual.test.ts --project=mobile-webkit    # full file: all 11 must pass now (landscape included)
npx playwright test e2e/explore-keyboard-blank-jump.test.ts --project=mobile-webkit # C5 tests stay red (out of scope) but must not get worse
npx vitest run
```

## 10. Acceptance criteria

- RED shown for each target repro before the edit; GREEN after.
- `explore-terminal-visual.test.ts`: 11/11 pass (landscape newly green; the 10
  previously green stay green).
- `explore-c4c5-siblings.test.ts`: 3/3 pass.
- "cropped empty band": pass (canvasAboveHost ≤ 24, fillY ≤ 1.08).
- C5 tests ("must not jump", "new-task handoff") remain the only reds in
  `explore-keyboard-blank-jump.test.ts`, failing on hostTop jump only.
- Full vitest suite passes (no weakened assertions).
- No `sendResize`/WS frame added to the frozen path (grep the diff).

## 11. Stop conditions

- Passing fillY appears to require sending a PTY resize while keyboard-open.
- Anchors differ materially from §6 (e.g. frozen branch already rewritten).
- Any previously green e2e probe or unit test regresses and the fix would
  require touching forbidden files.
- Patch would exceed ~150 changed lines.
- ghostty-web lacks a usable local `term.resize` without side effects on the
  socket (observe: WS frames in `__terminalFrames` during frozen refit).

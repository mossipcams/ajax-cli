# Packet R5 — C5: keyboard dismiss must not jump the terminal down

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

On the mobile task route, dismissing the soft keyboard must not move the
terminal host down by more than 24px. Today dismissal restores
`.detail-header` + `.interact-panel` (hidden under `html.keyboard-open`)
ABOVE the terminal, pushing the host from y≈49 to y≈146. Fix: give the
terminal a stable anchor by reordering the mobile task-route column so the
terminal sits directly under the cockpit chrome and the detail header +
interact strip render BELOW the terminal panel (CSS `order` — no DOM/markup
changes). Desktop (>767px fine pointer) layout must not change.

## 3. Allowed files

- `src/styles.css` — mobile task-route rules (the `@media (max-width: 767px),
  (pointer: coarse) and (max-height: 500px)` block, ~:595–692)
- `src/components/TaskDetail.svelte` — scoped `<style>` mobile rules only

## 4. Forbidden changes

- No markup/DOM-order changes (accessibility reading order stays: header,
  interact, terminal) — CSS `order` only.
- No JS changes (`viewport.ts`, `TerminalRawView.svelte`, layout policy).
- Desktop/tablet layout untouched.
- `html.keyboard-open` / `html.terminal-expanded` collapse rules stay
  (`styles.css` :639–656) — typing still hands the whole band to the terminal.
- No e2e file edits.

## 5. Context evidence

- **Graphify:** NOT_REQUIRED — presentation-order change inside the task
  route; no task-truth or lifecycle surface.
- **Serena:** NOT_REQUIRED — pure CSS; anchors below by direct read.
- **ast-grep:** NOT_REQUIRED — CSS-only edit.

## 6. Code anchors

- `src/styles.css` :595–692 — mobile task-route block. Key facts:
  - `[data-testid="route-scroll"]:has([data-outlet="task"])` is
    `overflow: hidden` flex column (:407–417 area in same file): the task
    route page does NOT scroll, so scroll compensation is impossible — the
    anchor must come from layout order.
  - `html.keyboard-open` hides `.detail-header`, `.interact-panel`,
    `.meta-details`, `.cockpit-chrome`, `.bottom-nav` (:639–656).
- `src/components/TaskDetail.svelte` — component markup order:
  `.detail-header`, then `.interact-panel`, then `.terminal-primary`
  (`data-mobile-primary="terminal"` :67), `.meta-details` (already
  `display:none` on mobile, :404). Scoped mobile block ~:395–415 sets
  `.terminal-primary { display:flex; flex:1 1 auto; min-height:0 }`.
- The task outlet `[data-outlet="task"]` and `.task-detail` are flex columns
  on mobile; `order` on the three visible children is sufficient.
- Terminal panel currently has `margin-top` (TerminalRawView panel CSS sets 0
  on mobile already via its own media block).

Geometry targets from the failing tests (`e2e/explore-keyboard-blank-jump.test.ts`
:128–186): keyboard-open hostTop ≈ 49 (or 41 on new-task handoff);
after dismiss hostTop must be ≤ open + 24. With header+interact ordered below
the terminal, closed hostTop ≈ cockpit-chrome bottom (~48) — passes. The
header/interact must remain visible and hittable below the terminal when the
keyboard is closed: cap the terminal's share so the strip fits above the
bottom-nav (e.g. keep `.terminal-primary` `flex: 1 1 auto; min-height: 0`
and give header/interact `flex: none`).

## 7. Test-first instructions

Existing failing tests are the red (run before editing, expect hostTop-jump
failures at 97/105px):

```bash
npx playwright test e2e/explore-keyboard-blank-jump.test.ts -g "must not jump" --project=mobile-webkit
npx playwright test e2e/explore-keyboard-blank-jump.test.ts -g "new-task handoff" --project=mobile-webkit
```

## 8. Edit instructions

In the mobile task-route media block (styles.css and/or TaskDetail scoped
styles):

1. Order the task column: terminal first, then detail-header, then
   interact-panel (CSS `order` on `.task-detail` children or the outlet's
   flex children; keep DOM untouched).
2. Ensure the reordered header/interact are `flex: none` and visible below
   the panel with the existing bottom-nav clearance; the terminal keeps
   `flex: 1 1 auto; min-height: 0` and its existing mobile min-height floor.
3. Keyboard-open behavior unchanged (those elements are `display:none` then).

## 9. Verification commands

```bash
npx playwright test e2e/explore-keyboard-blank-jump.test.ts --project=mobile-webkit   # all 3 must pass now
npx playwright test e2e/explore-terminal-visual.test.ts e2e/explore-c4c5-siblings.test.ts e2e/explore-webkit-critical.test.ts e2e/fullscreen-refit.test.ts --project=mobile-webkit
npx playwright test --project=mobile-webkit
npx vitest run
```

## 10. Acceptance criteria

- RED shown before edit (97/105px), GREEN after for both C5 tests.
- `explore-keyboard-blank-jump.test.ts` 3/3 green.
- Focused suites in §9 line 2: all green.
- Full mobile-webkit run: only these pre-existing reds remain (other defects,
  out of scope): `jwt-adversarial` (S1), `explore-ui` console/JWT (M1/S1),
  `explore-webkit-qa` "DEFECT pin: viewport meta" (M1), `explore-webkit-qa`
  "deep explore: task actions…" + "settings controls…" + "connection
  unreachable…" (L1 tap targets). Nothing else may fail.
- Full vitest suite passes (509).
- Diff confined to CSS in the two allowed files.

## 11. Stop conditions

- Passing requires JS/markup changes or touching keyboard-open collapse rules.
- A currently green mobile-webkit test regresses and fixing it needs
  forbidden files.
- Patch exceeds ~80 changed lines.

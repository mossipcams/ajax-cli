# Slice 7 — Dialog/Sheet primitive and NewTaskSheet

Master plan: `react-migration-cleanup.md`
Depends on: slice 6 (`83802ac`)

## Measurement before planning

- `NewTaskSheet.tsx` (209 lines) is the **only** modal surface in the app:
  one `role="dialog"` / `aria-modal="true"` (`:130-131`), one `FullscreenLayer`
  consumer (`:126`). Nothing else portals or overlays.
- Modal behaviour is hand-rolled today:
  - Escape → `onClose` via `onKeyDown` on the dialog div (`:136-138`)
  - backdrop click → `handleBackdropClick` target check (`:121-123`)
  - open focus → `sheetRef.current?.focus()` on the dialog div (`:65-67`)
  - drag-to-dismiss → `useSheetDrag(grabRef)` + `dragOffset` transform (`:69-72`, `:144`)
- **No focus trap and no focus restore exist.** Tab walks straight out of the
  sheet into the cockpit behind it, and closing drops focus to `<body>`.
  This is the only real a11y gap on the surface.
- Pixels live in `styles.css:1196-1340`: `.fullscreen-layer`, `#new-task-sheet`,
  `.sheet-card`, `.sheet-grab(ber)`, `.agent-picker`, `.agent-option`,
  `.sheet-actions`, `.sheet-error`, `@keyframes sheet-rise`.
- `.fullscreen-layer` is pinned to the visualViewport band via
  `--app-top` / `--app-height` (`:1198-1201`) — the same pin the terminal
  keyboard band uses.
- Installed already: `@radix-ui/react-slot`, `cva`, `clsx`, `tailwind-merge`,
  `cn` at `src/lib/utils.ts`. `@radix-ui/react-dialog` is **not** installed.
- Existing coverage: `NewTaskSheet.test.tsx` 16 tests (focus-on-open, drag
  dismiss, submit paths, prefs). e2e: `actions.test.ts:151-171` open/cancel/submit,
  `layout-scroll.test.ts:159-162` keyboard band.

## Key decision — Radix Dialog for behaviour, Ajax CSS for pixels

Same shape as slice 6's decision (B): the primitive supplies **structure and
behaviour**, `styles.css` stays the single source of visual truth. `sheet.tsx`
composes `@radix-ui/react-dialog` and renders the existing Ajax class names.

Rejected alternative: a shadcn-shaped `sheet.tsx` that just wraps the current
hand-rolled div. It adds a file and changes nothing — no focus trap, no focus
restore, no dep. That is ceremony, not a slice.

Radix earns its dependency here on exactly three things the hand-rolled version
does not do: **focus trap, focus restore on close, and `aria-hidden` on
background siblings.** Escape and backdrop-dismiss move to Radix as a
consequence, not as the justification.

## Radix defaults that must be overridden — iOS band contract

Three Radix defaults are wrong for this surface and are **not** optional:

1. **Portal target.** `DialogPortal` defaults to `document.body`. The sheet must
   stay inside `FullscreenLayer`, which is pinned to `--app-top`/`--app-height`.
   Portaling to body detaches it from the visualViewport band and breaks the
   keyboard-open geometry. Use `DialogPortal container={…}` pointing at the
   `FullscreenLayer` node (ref), or render `DialogContent` with no portal at all.
2. **`onOpenAutoFocus`.** Radix focuses the first focusable descendant — here the
   repo `<select>`, which pops the iOS picker on open. Today focus lands on the
   dialog container itself and the keyboard stays shut. `preventDefault()` the
   event and focus the content node, preserving `NewTaskSheet.test.tsx:23`.
3. **Scroll lock.** `react-remove-scroll` mutates `document.body`. The sheet
   already sits in a `position:fixed; overflow:hidden` band, so the lock is
   redundant, and body mutation is the historical source of iOS viewport bugs
   in this repo. Verify against the mobile-webkit gate; if it perturbs the band,
   the sheet renders `DialogContent` outside a modal `Root` rather than fighting it.

`useSheetDrag` keeps ownership of drag-to-dismiss — Radix has no gesture layer.
It needs the `DialogContent` ref forwarded to the grabber's sibling, unchanged.

## Round 0 — characterization gap (MANDATED FIRST, before any port)

Two behaviours Radix will take over have **no test**:

- Escape closes the sheet (`NewTaskSheet.tsx:136-138`)
- backdrop click closes it, and a click **inside the card does not**
  (`:121-123` — the `event.target === event.currentTarget` guard)

Add both to `NewTaskSheet.test.tsx`, green against the current hand-rolled
dialog, and **commit before the port**. Without them the port silently
redefines its own baseline.

## Scope

- `src/components/ui/sheet.tsx` — Radix Dialog composition, `data-slot`
  attributes, Ajax class names, no task/API/polling/terminal behaviour.
- `NewTaskSheet.tsx` renders it; all form logic, prefs, submit paths, and
  `useSheetDrag` wiring are untouched.
- `@radix-ui/react-dialog` added to `package.json`; lockfile regenerated.

## Non-goals

- No visual change. `styles.css` is not rewritten and gains no tokens; a diff
  touching `1196-1340` other than to relocate a selector is out of scope.
- No `RadioGroup` — the `.agent-picker` radiogroup is **slice 8**.
- No `NewTaskSheet` behaviour change: same fields, same prefs keys, same
  `startTask` call shape, same `onOpenTask` handle.
- No second modal surface invented to justify the primitive.

## Risks

1. **Portal escaping the visualViewport band** — highest risk, iOS-only,
   invisible to vitest. `layout-scroll.test.ts:159-162` and the iPhone gate are
   the detectors.
2. **Auto-focus popping the iOS select picker** on open — changes perceived
   behaviour on the exact surface the user opens most.
3. **Scroll lock vs. the band pin** — see override 3 above.
4. **Drag-to-dismiss regressing** when the content node becomes a Radix
   component — `NewTaskSheet.test.tsx:77` is the guard.

## Delegation decision

`Delegation decision: delegated via model-router`

## Rounds

- [x] **Round 0 — dismiss characterization** — packet
  `.planning/packets/react-cleanup-s7-round0-dismiss-characterization.md`,
  delegated to pi/`opencode-go/minimax-m3`, ACCEPTED after local placement fix.
- [x] **Round 1 — `ui/sheet.tsx` + `@radix-ui/react-dialog`** — packet
  `.planning/packets/react-cleanup-s7-round1-sheet-primitive.md`, delegated to
  Cursor/`composer-2.5`, ACCEPTED after parent-applied a11y fixes (below).

**S7 code complete.** Remaining: §9 on-device regression (Matt), then PR.

## Baselines (to re-measure at round start)

- Suite: 366 tests / 41 files (slice 6 baseline; s6 landed test changes)
- mobile-webkit e2e: 92 passed / 2 skipped
- `visual.test.ts` asserts computed styles — the visual-regression guard

## Validation gate

```bash
npm run web:check
npm run web:test -- --run
npm run web:lint
npm run web:build:check
npm run web:smoke -- --project=mobile-webkit
cargo nextest run -p ajax-web
npm run verify
```

Dev deploy → https://ajaxdev.mossyhome.net:8788 → iPhone checklist
(open sheet with keyboard closed, focus title, keyboard band, drag-dismiss,
Escape via hardware keyboard, backdrop tap, rotation) → **wait for Matt** → PR.

## Deviations / Validation results

- Round 0: PASS — focused 18/18, full suite 376/376 across 42 files,
  `web:check` exit 0. **Mutation-checked**: stubbing out both
  `NewTaskSheet.tsx` guards fails exactly the two new tests and nothing else,
  so they genuinely pin the behavior rather than passing vacuously.
- Round 0 deviation: the packet's append anchor named line 197 as the close of
  `describe("NewTaskSheet", …)`. The file has **two** describes; 197 closes
  `describe("NewTaskSheet remembered defaults", …)`. The delegate followed the
  literal line anchor and reported the discrepancy; the parent moved both tests
  into the first describe locally rather than spending a revise round.
  Packet defect, not a delegate defect.
- Round 1: PASS — vitest 378/378 across 42 files, `web:check` 0, `web:lint` 0,
  `web:build:check` 0, `cargo nextest -p ajax-web` 159/159, mobile-webkit smoke
  **92 passed / 2 skipped** (exactly the baseline — portal and scroll-lock risks
  did not materialise). Focus-restore test verified RED before the port and
  GREEN after.
- Round 1 **defects caught at the gate and fixed by the parent** (the delegate's
  own report claimed full success on all of these):
  1. **Dangling accessible name.** `<SheetTitle asChild><h2 id="new-task-title">`
     — Radix's `Slot` lets child props win, so the hand-written `id` overrode
     Radix's `titleId` and `aria-labelledby="radix-_r_1_"` resolved to **no
     element**. The dialog lost its accessible name outright. Fixed by dropping
     the hardcoded id (nothing referenced it — grep-verified) and letting Radix
     own it. New test `"labels the dialog with a title that actually exists"`
     guards it; mutation-checked by re-adding the id, which fails exactly that test.
  2. **`aria-modal` dropped.** Present before the port, `null` after. Restored
     explicitly on the content node.
  3. **`onKeyDown={() => {}}`** — a no-op handler added purely to silence
     `jsx-a11y/click-events-have-key-events`. Replaced with a scoped
     eslint-disable plus a comment explaining that Escape is Radix's job, rather
     than lying to the linter.
- Round 1 accepted deviation: **focus restore is hand-rolled, not Radix's.**
  Radix's modal `DialogContent` always `preventDefault`s `onCloseAutoFocus` and
  focuses its `triggerRef`; this sheet has no `Dialog.Trigger`, so `FocusScope`
  would restore nothing. The 4-line effect cleanup stands, with a comment naming
  the ceiling. Consequence to be honest about: Radix is earning its place here on
  **focus trap + background `aria-hidden` only** — the focus-restore test passes
  on our own code and would pass without Radix.
- Round 1 bundle cost: `app.js` 626,710 → 658,557 bytes raw (+31KB), 176,141 →
  186,950 gzipped (**+10.8KB gzip**) for one modal. Data point for slice 11.
- Pre-existing finding (not slice 7): `crates/ajax-web/web/dist` was **stale at
  HEAD** — rebuilding pristine HEAD source produced a different bundle, including
  an 11KB CSS growth. Isolated into its own commit (`83a173a`) so the slice diff
  stays reviewable. Worth understanding why it drifted before the next release.
- Round 0 deviation: the delegate's report used a nonconforming envelope
  (lowercase keys, extra fields), so `run-delegate` recorded `FAILED`. Content
  was independently reverified by the parent — same precedent as react S7
  rounds 1a-i / 1a-ii.

## On-device gate

- **PASS (Matt, 2026-07-19)** — validated on iPhone. S7 cleared for PR.

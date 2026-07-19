# Packet — Slice 7 Round 1: ui/sheet.tsx primitive

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

Introduce `components/ui/sheet.tsx`, a shadcn-shaped primitive over
`@radix-ui/react-dialog`, and rewire `NewTaskSheet` onto it. The primitive
supplies modal *behavior* (focus trap, focus restore, background `aria-hidden`);
`styles.css` remains the sole source of visual truth. The user-visible win is
focus restore and focus containment — today Tab escapes the sheet into the
cockpit behind it and closing drops focus to `<body>`.

Every existing test must keep passing **unmodified**. That is the primary
signal that this is a behavior-preserving port.

## Allowed files

- `crates/ajax-web/web/src/components/ui/sheet.tsx` (new)
- `crates/ajax-web/web/src/components/NewTaskSheet.tsx`
- `crates/ajax-web/web/src/components/NewTaskSheet.test.tsx` (additive only)

## Forbidden changes

- Any edit to `crates/ajax-web/web/src/styles.css`. No CSS is added, removed,
  or reordered. No class name changes.
- Any edit to `package.json` or `package-lock.json` — `@radix-ui/react-dialog`
  is **already installed**. Do not run `npm install`.
- Any edit to files under `crates/ajax-web/web/e2e/`.
- Editing, renaming, reordering, deleting, or weakening any of the **18**
  existing `it(...)` blocks in `NewTaskSheet.test.tsx`. Additions only.
- Adding `@radix-ui/react-dialog`'s `Portal`. See the portal constraint below.
- Enabling React `StrictMode` anywhere.
- Changing any form logic: field names, `localStorage` keys, `startTask` call
  shape, `savePrefs`, `submit`, `initialRepo`, `initialAgent`, or the
  `onCockpit`/`onResult`/`onOpenTask` callbacks.
- Touching `useSheetDrag`, `src/gestures/sheetDrag.ts`, or `FullscreenLayer`.
- Any commit, branch, push, rebase, or branch switch.

## Context evidence

**The DOM contract is pinned by tests outside this packet's scope and cannot
move.** `#new-task-sheet` / `data-testid="new-task-sheet"` is asserted in:
- `e2e/actions.test.ts:152,155,166,171`
- `e2e/layout-scroll.test.ts:68-70,160` (also asserts `.sheet-card`)
- `NewTaskSheet.test.tsx:20,25,163,176`

Therefore `Dialog.Content` **must be** the existing `#new-task-sheet` element
via `asChild`, keeping its `id`, `data-testid`, and flex/backdrop styling. It
must not become a separate `Dialog.Overlay`.

**Current structure** (`NewTaskSheet.tsx:125-208`):

```tsx
<FullscreenLayer zIndex={50}>
  <div id="new-task-sheet" data-testid="new-task-sheet" role="dialog"
       aria-modal="true" aria-labelledby="new-task-title" tabIndex={-1}
       ref={sheetRef} onClick={handleBackdropClick} onKeyDown={…Escape…}>
    <form className={`sheet-card…`} onSubmit={submit} style={{transform:…}}>
      <div className="sheet-grab" ref={grabRef}>…</div>
      <h2 id="new-task-title">New task</h2>
      …fields…
    </form>
  </div>
</FullscreenLayer>
```

**Backdrop dismissal stays local, not Radix's.** Because Content *is* the
backdrop, a backdrop click is *inside* Content, so Radix's
`onPointerDownOutside` never fires for it. The existing `handleBackdropClick`
`event.target === event.currentTarget` guard (`:121-123`) is retained verbatim
as the `onClick` on Content. `NewTaskSheet.test.tsx:167-178` pins this.

**Focus on open** (`:65-67`) currently focuses the dialog container, keeping the
iOS keyboard shut. `NewTaskSheet.test.tsx:23-26` asserts
`document.activeElement === getByTestId("new-task-sheet")`.

**Escape** (`:136-138`) currently a local `onKeyDown`.

**Drag-to-dismiss**: `useSheetDrag(grabRef, …)` (`:69-72`) with `dragOffset`
applied as `style={{transform}}` on the form (`:144`). Pinned by
`NewTaskSheet.test.tsx:77-93`.

**Existing primitive to imitate**: `crates/ajax-web/web/src/components/ui/button.tsx`
— `cn` from `@/lib/utils`, `data-slot` attributes, CVA mapping variants onto
existing Ajax class names rather than emitting Tailwind utilities.

**Dependency**: `@radix-ui/react-dialog` is installed and typechecks
(`npm run web:check` exits 0 at HEAD).

## Code anchors

- `crates/ajax-web/web/src/components/NewTaskSheet.tsx:125-208` — the returned
  JSX; the only production region that changes.
- `crates/ajax-web/web/src/components/NewTaskSheet.tsx:65-67` — open-focus effect.
- `crates/ajax-web/web/src/components/NewTaskSheet.tsx:121-123` — backdrop guard, retained.
- `crates/ajax-web/web/src/components/NewTaskSheet.tsx:136-138` — Escape handler, replaced by Radix.
- `crates/ajax-web/web/src/components/ui/button.tsx:1-6` — import/`cn` conventions to mirror.
- `crates/ajax-web/web/src/components/NewTaskSheet.test.tsx:196` — append point
  for the new test (end of the first `describe("NewTaskSheet", …)` block; note
  the file has **two** `describe` blocks and the second is
  `"NewTaskSheet remembered defaults"` — do not append at end of file).

## Test-first instructions

Add exactly one new test to `NewTaskSheet.test.tsx`, at the append point above,
named exactly:

`"restores focus to the opener when the sheet unmounts"`

- Create and append a `<button>` to `document.body`, focus it, and assert it is
  `document.activeElement`.
- `render(<NewTaskSheet repos={repos} />)` and capture the `unmount` fn.
- Call `unmount()`.
- Assert `document.activeElement` is the opener button again.
- Remove the button in the test to avoid leaking DOM between tests.

Focused red command — this MUST fail before any production change, because the
current hand-rolled dialog has no focus-restore behavior:

```bash
npm run web:test -- --run src/components/NewTaskSheet.test.tsx -t "restores focus to the opener"
```

Record the actual failing assertion output as RED evidence. Then implement, and
record the same command passing as GREEN evidence.

## Edit instructions

**1. Create `crates/ajax-web/web/src/components/ui/sheet.tsx`.**

Thin re-exports over `@radix-ui/react-dialog`, mirroring `button.tsx`
conventions. Export at minimum `Sheet` (Root), `SheetContent`, `SheetTitle`.
Each renders `data-slot="sheet" | "sheet-content" | "sheet-title"` and merges
incoming `className` through `cn(...)`. `SheetContent` must forward `asChild`
and all Radix Content props (notably `onOpenAutoFocus`, `onEscapeKeyDown`,
`aria-describedby`) through to `DialogPrimitive.Content`.

`components/ui` holds **no** task, API, polling, terminal, or gesture behavior.
No `Portal` is exported or used. No CVA is required unless a genuine variant
exists — do not invent one.

**2. Rewire `NewTaskSheet.tsx:125-208`.**

- Wrap in `<Sheet open onOpenChange={(open) => { if (!open) onClose?.(); }}>`,
  keeping `FullscreenLayer` as the outermost element so the sheet stays inside
  the visualViewport band.
- Render `SheetContent` with `asChild`, wrapping the **existing**
  `#new-task-sheet` div. Keep `id`, `data-testid`, `tabIndex={-1}`, `ref`, and
  the `onClick={handleBackdropClick}`. Radix supplies `role="dialog"` and
  `aria-modal`; drop the now-duplicated hand-written `role`/`aria-modal`/
  `aria-labelledby` only if Radix sets an equivalent, and keep the rendered
  result equivalent for assistive tech.
- Delete the local `onKeyDown` Escape handler — Radix's `onEscapeKeyDown` +
  `onOpenChange` replaces it.
- Add `onOpenAutoFocus={(event) => { event.preventDefault(); }}` and keep the
  existing `sheetRef.current?.focus()` effect, so focus lands on the container
  and **not** on the repo `<select>`. Focusing the select would pop the iOS
  picker on open; this is a hard requirement, pinned by
  `NewTaskSheet.test.tsx:23-26`.
- Wrap the `<h2 id="new-task-title">New task</h2>` in `SheetTitle asChild` to
  satisfy Radix's labeling requirement without changing the rendered markup.
- Pass `aria-describedby={undefined}` to Content to suppress Radix's missing
  description warning; do not invent description text.
- The `<form className="sheet-card">`, the grab handle, all fields, the
  `dragOffset` transform, and every handler stay exactly as they are.

Do not add a `Portal`: Content must render in place inside `FullscreenLayer`.
Portaling to `document.body` detaches the sheet from the
`--app-top`/`--app-height` pin (`styles.css:1196-1207`) and breaks iOS
keyboard geometry.

## Verification commands

```bash
npm run web:test -- --run src/components/NewTaskSheet.test.tsx -t "restores focus to the opener"   # RED, then GREEN
npm run web:test -- --run src/components/NewTaskSheet.test.tsx
npm run web:test -- --run
npm run web:check
npm run web:lint
npm run web:build:check
```

Run from the repository root (npm scripts live in the root `package.json`).

## Acceptance criteria

- The new focus-restore test failed before the production change and passes
  after, with both exit codes recorded.
- `NewTaskSheet.test.tsx`: **19** passing, 0 failing, 0 skipped.
- Full suite: **377** passing across 42 files, 0 failing.
- `web:check`, `web:lint`, and `web:build:check` all exit 0.
- `git diff --stat` lists exactly three paths: `ui/sheet.tsx` (new),
  `NewTaskSheet.tsx`, `NewTaskSheet.test.tsx`. No `styles.css`, no
  `package.json`, no `package-lock.json`, no e2e file.
- No existing `it(...)` block was edited, reordered, or removed.
- `grep -c 'Portal' crates/ajax-web/web/src/components/ui/sheet.tsx` returns 0.

## Stop conditions

- The focus-restore test passes *before* the production change — the premise is
  wrong; stop and report rather than inventing a different test.
- Preserving `#new-task-sheet` as `Dialog.Content` proves impossible without
  editing an e2e file or an existing unit test — stop and report; do not edit
  them.
- Any existing test requires modification to pass. This is the central signal
  the port is not behavior-preserving; stop and report which test and why.
- Radix emits a console error or warning during the test run that cannot be
  resolved within the Allowed files.
- Making it work appears to require a `Portal`, `styles.css` change, or a new
  dependency — stop and report.
- The patch exceeds roughly 200 changed lines.

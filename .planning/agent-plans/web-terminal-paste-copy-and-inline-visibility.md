# Web terminal: inline visibility + paste/copy

## Scope

Four related Web Cockpit terminal fixes, in priority order:

0. **Inline (non-fullscreen) terminal visibility** — on open/create of a task
   terminal, the canvas/input row is out of view: bottom chrome visible, typing
   not visible, large blank band below the panel.
1. **Fallback paste** — any `clipboard.readText()` rejection opens the real
   textarea tray; tray gains a Send button that pastes `value` via `term.paste`.
2. **Long-press native paste target** — focus `term.textarea` on `touchstart`
   (`preventScroll: true`); do not `preventDefault` on the long-press path;
   unclip the hidden textarea enough for iOS to treat it as editable.
3. **Explicit Copy overlay** — keep Ajax selection math; stop auto-copy on
   selection end; show a Copy button; on tap `copyText(text)`, and if that
   fails open a readonly selected textarea fallback.

## Non-goals

- No ghostty-web library swap or version bump (try focus/unclip first).
- No change to fullscreen expand/keyboard-open chrome collapse contracts.
- No CLI/TUI clipboard changes.
- No architecture.md changes (UI-only presentation of existing terminal I/O).

## Suspected root cause (Priority 0)

`styles.css` applies to **all** task terminals:

```css
.task-detail .terminal-panel,
.task-detail [data-testid="task-terminal-panel"] {
  max-height: min(58vh, 560px);
}
```

Mobile media rules clear `min-height` but **never clear `max-height`**. Desktop
scoped height in `TerminalRawView.svelte` is already `min(58vh, 560px)` under
`min-width: 768px`. On phones the panel therefore caps at ~58vh inside a
full-height `task-detail`, leaving a large empty paper band — matches “bunch of
blank space.” With the soft keyboard open (non-expanded), chrome collapses but
the panel stays 58vh-capped, so the cursor/input row is easy to lose while the
key bar (bottom row) remains visible.

Likely fix: on mobile (and keyboard-open non-expanded), drop the 58vh cap so the
panel flex-fills `terminal-primary` / the app band; keep desktop 58vh.

## Current paste/copy behavior (to change)

| Area | Today | Target |
| --- | --- | --- |
| `requestPaste` catch | status notice only | open `pasteFallbackOpen` tray |
| Fallback tray | Cancel + native `onpaste` only | + Send → `pasteToTerm(value)`, clear, close |
| Long-press | selection/copy; textarea not focused early | `touchstart` focuses textarea; no PD on long-press idle path |
| Selection end | auto `copyText` + clear | keep selection; show Copy overlay; copy on tap |

## Approval

- User: implement Priority 1–3 paste/copy + fix non-fullscreen visibility.
- Status: planning complete; implementation authorized by “lets implement these.”
- Mode: Behavior Change (TDD). Delegation via model-router (Grok 4.5 High —
  Svelte/viewport/terminal UI).

## Delegation decision

`Delegation decision: delegated via model-router` — one bounded packet per
priority (0 → 1 → 2 → 3), sequential `implement` → review → `resume` as needed.
Do not bundle all four into one delegate prompt.

## Task checklist

### Priority 0 — inline terminal fills mobile band

- [x] **P0-T1 (test):** failing layout/CSS contract + e2e or component assertion
  that mobile non-expanded `[data-testid=task-terminal-panel]` does **not**
  compute `max-height: min(58vh, 560px)` / fills available `terminal-primary`
  height (placeholder mode OK). Update any source-regex tests that currently
  require the global 58vh rule on mobile.
- [x] **P0-T2 (impl):** in `styles.css`, scope the 58vh `max-height` to desktop
  only (mirror `TerminalRawView.svelte` desktop media query). On mobile /
  `html.keyboard-open` non-expanded, `max-height: none` (or band-based height)
  so the panel flex-fills. Rebuild dist if asset snapshots require it.
- [x] **P0-T3 (verify):** focused vitest + `layout-scroll` / fullscreen e2e still
  green; confirm no desktop height regression.

**P0 result:** Accepted (Composer chat `5920f760-2c0d-4e51-8995-d21e4b5edd3e`).
Global 58vh removed; desktop media restores cap; mobile sets `max-height: none`.

### Priority 1 — fallback paste

- [x] **P1-T1 (test):** change
  `surfaces a clipboard read failure instead of silently doing nothing` in
  `TerminalRawView.test.ts` so a rejected `readText()` opens
  `[data-testid=terminal-paste-fallback]` (not only a status string).
- [x] **P1-T2 (test):** Send button reads textarea value, calls `term.paste`,
  clears value, closes tray; empty Send does not paste; Cancel unchanged;
  existing native `onpaste` path still works.
- [x] **P1-T3 (impl):** `requestPaste()` `.catch` → `pasteFallbackOpen = true`
  (same as missing API). Add Send button next to Cancel in the fallback tray.
- [x] **P1-T4 (verify):** `npm run web:test -- --run TerminalRawView.test.ts`

**P1 result:** Accepted (Composer chat `02bc8bd6-f5d0-48bb-8c7b-573fe6e20a78`).

### Priority 2 — long-press editable target

- [x] **P2-T1 (test):** on host `touchstart`, `term.textarea.focus` called with
  `{ preventScroll: true }` before long-press timeout. Assert long-press
  `touchstart` is **not** `defaultPrevented` (pinch/scroll/selection-drag still
  may PD).
- [x] **P2-T2 (test/source):** while paste-mode / always-on mobile harden path,
  textarea is not fully clipped (`opacity:0` + offscreen / zero size alone is
  insufficient). Prefer a small on-canvas or near-cursor hit target during
  paste-capable focus (match rcarmo MenuHandler idea of positioning textarea
  under the finger without waiting for `touchend`).
- [x] **P2-T3 (impl):** in `attachTerminalGestures` host callbacks or
  `TerminalRawView` gesture wiring: on single-finger `touchstart`, call
  `focusTerm()` / `term.textarea?.focus({ preventScroll: true })`. Do **not**
  add `preventDefault` on that path. Only keep existing PD for pinch, scroll
  past threshold, and active selection drag. Soften
  `.terminal-host :global(textarea)` / `hardenMobileTextarea` so iOS can target
  it (e.g. opacity ~0.01, 1× font-size box in-host, not `clip`/`left:-9999`).
- [x] **P2-T4 (verify):** focused gesture + TerminalRawView tests; no keyboard
  pop on mere scroll (focus with preventScroll; blur policy unchanged for ⌄ /
  exit fullscreen).

**P2 result:** Accepted (Grok chat `f5933429-11e0-4802-b971-8ccc4bf903c8`).
`touchBegan` → focus; CSS/harden unclip. Device check still needed for
scroll-only keyboard pop.

### Priority 3 — explicit Copy overlay

- [x] **P3-T1 (test):** rewrite `copies the selection after a long-press drag`
  — after `touchend`, selection remains; **no** immediate `writeText`; a Copy
  control appears (`data-testid=terminal-copy-overlay` or role).
- [x] **P3-T2 (test):** tapping Copy calls `copyText` / `writeText`; on success
  flash “Copied” and clear selection; on failure open readonly textarea with
  selected text and `.select()` (reuse paste-fallback visual pattern or sibling
  copy-fallback tray).
- [x] **P3-T3 (impl):** replace `finishSelectionCopy` auto-copy with
  `endSelection` that keeps selection + sets overlay state. Copy button handler
  uses existing `copyText` from `diagnostics.ts`.
- [x] **P3-T4 (verify):** TerminalRawView tests green; rebuild web bundle.

**P3 result:** Accepted (Grok chat `e854d8f5-d4a8-4e67-af7e-0bf629669514`).

## Parent validation (2026-07-08)

- `npm run web:test -- --run TerminalRawView.test.ts TaskDetail.test.ts` — 136 pass
- `npm run web:check` — 0 errors
- `npx playwright test e2e/layout-scroll.test.ts e2e/fullscreen-refit.test.ts` — 22 pass
- `cargo nextest run -p ajax-web` — 123 pass

## Deviations

- P2: unclip via CSS opacity 0.01 + clip-path none (no finger-following
  reposition yet); device check still needed for scroll-only keyboard pop.
- P3: Copy overlay pinned top-right under expand button; device UX check still
  needed.

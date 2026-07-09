# TDD Packet: Priority 3 — explicit Copy overlay

## 1. Goal

Keep Ajax terminal selection math (long-press / drag). Stop auto-copying on
selection end. After a non-cancelled selection with text, show a small Copy
button overlay. On tap, call `copyText(text)`. If that fails, open a readonly
textarea containing the selected text and `.select()` it so the user can
system-copy.

## 2. Allowed files

**Tests**

- `crates/ajax-web/web/src/components/TerminalRawView.test.ts`

**Production**

- `crates/ajax-web/web/src/components/TerminalRawView.svelte`

**Build**

- `crates/ajax-web/web/dist/*` only via `npm run web:build`

## 3. Forbidden changes

- Do not change Priority 1 paste fallback / Send.
- Do not change Priority 2 `touchBegan` / textarea unclip.
- Do not change selection cell math (`selectionCellAt`, `wordRangeAt`,
  `applySelection`, `orderedSelection`).
- Do not auto-call `copyText` inside `endSelection` anymore.
- Do not edit `diagnostics.ts` (reuse `copyText` as-is).
- Do not edit gestures beyond what TerminalRawView’s `endSelection` callback
  already receives (no terminalGestures.ts changes required for this packet).
- Do not edit Rust / architecture.md / styles.css layout.

## 4. Architecture context

UI-only. Selection still uses ghostty `selectionManager` via Ajax gesture
callbacks. Clipboard write uses existing `copyText` from `diagnostics.ts`
(async clipboard + execCommand fallback). New overlay is presentation only.

## 5. Code anchors

**Current auto-copy on selection end:**

```382:393:crates/ajax-web/web/src/components/TerminalRawView.svelte
    const finishSelectionCopy = (cancelled: boolean) => {
      selectionAnchor = undefined;
      const text = cancelled ? "" : (term?.getSelection() ?? "");
      if (!text) {
        term?.clearSelection();
        return;
      }
      void copyText(text).then((copied) => {
        flashCopyNotice(copied ? "Copied" : "Copy failed — clipboard unavailable");
        term?.clearSelection();
      });
    };
```

Wired as `endSelection: finishSelectionCopy` in `attachTerminalGestures`.

**Reuse:** `flashCopyNotice`, `copyText` import, paste-fallback tray visual
pattern for the copy-failure readonly textarea.

**Existing test to rewrite:**
`copies the selection after a long-press drag` (~1709) — currently expects
immediate `writeText` + clearSelection + “Copied” on touchend.

## 6. Test-first instructions

1. Rewrite `copies the selection after a long-press drag`:
   - After long-press + drag + touchend: selection still present
     (`clearSelection` NOT called yet).
   - `writeText` / `copyText` NOT called yet.
   - Overlay visible: `getByTestId("terminal-copy-overlay")` or
     `getByRole("button", { name: "Copy" })` inside the terminal panel.
   - Then click Copy → expect `writeText` with `"selected text"`, notice
     “Copied”, selection cleared, overlay gone.

2. Add `opens a readonly copy fallback when clipboard write fails`:
   - Mock `navigator.clipboard.writeText` rejected OR delete clipboard and
     stub `copyText` path to fail (prefer real `copyText` with clipboard
     unavailable like diagnostics tests).
   - Make a selection ending in overlay; click Copy.
   - Expect `[data-testid=terminal-copy-fallback]` (or reuse a clearly named
     tray) with a readonly textarea whose value is the selected text.
   - Prefer asserting `.select()` was effectively applied (selectionStart/End
     span the text) when jsdom allows.

3. Add `dismisses copy overlay without copying when selection is cancelled`:
   - If an existing path calls `endSelection(true)` (second finger / cancel),
     overlay must not appear; selection cleared.

4. Bare long-press word select should also show Copy overlay (not auto-copy):
   - Update `selects the word under a bare long-press` if it currently expects
     copy — today it only asserts selection range; after touchend, also assert
     overlay appears (dispatch touchend after timer).

5. Run RED:
   ```bash
   cd crates/ajax-web/web && npm run web:test -- --run TerminalRawView.test.ts
   ```

## 7. Production edit instructions

1. Add state:
   - `let copyOverlayOpen = $state(false);`
   - `let copyOverlayText = $state("");`
   - `let copyFallbackOpen = $state(false);`
   - `let copyFallbackInput = $state<HTMLTextAreaElement | undefined>();`
   - Optional: `$effect` to focus/select copyFallbackInput when open.

2. Replace `finishSelectionCopy` with something like:
   ```ts
   const finishSelection = (cancelled: boolean) => {
     selectionAnchor = undefined;
     if (cancelled) {
       copyOverlayOpen = false;
       copyOverlayText = "";
       term?.clearSelection();
       return;
     }
     const text = term?.getSelection() ?? "";
     if (!text) {
       copyOverlayOpen = false;
       copyOverlayText = "";
       term?.clearSelection();
       return;
     }
     copyOverlayText = text;
     copyOverlayOpen = true;
     // Do NOT copyText here. Do NOT clearSelection yet.
   };
   ```
   Wire `endSelection: finishSelection`.

3. Markup — small Copy button when `copyOverlayOpen` (position absolute near
   top of panel or above bottom controls; keep simple):
   ```svelte
   {#if copyOverlayOpen}
     <button
       type="button"
       class="terminal-copy-overlay"
       data-testid="terminal-copy-overlay"
       onclick={() => void handleCopyOverlay()}>Copy</button>
   {/if}
   ```

4. `handleCopyOverlay`:
   ```ts
   const handleCopyOverlay = async () => {
     const text = copyOverlayText || term?.getSelection() || "";
     copyOverlayOpen = false;
     const ok = text ? await copyText(text) : false;
     if (ok) {
       flashCopyNotice("Copied");
       copyOverlayText = "";
       term?.clearSelection();
       return;
     }
     // failure: open readonly fallback
     copyFallbackOpen = true;
     // keep selection until user dismisses, or clear after opening fallback —
     // prefer keep text in copyOverlayText / copyFallback value
   };
   ```

5. Copy fallback tray (sibling to paste fallback):
   ```svelte
   {#if copyFallbackOpen}
     <div class="terminal-paste-fallback" data-testid="terminal-copy-fallback">
       <textarea readonly rows="3" bind:this={copyFallbackInput} value={copyOverlayText}></textarea>
       <button type="button" class="terminal-key" onclick={() => {
         copyFallbackOpen = false;
         copyOverlayText = "";
         term?.clearSelection();
       }}>Done</button>
     </div>
   {/if}
   ```
   On open, `$effect` → `copyFallbackInput?.focus(); copyFallbackInput?.select();`

6. Minimal CSS for `.terminal-copy-overlay` (absolute, top-right under expand
   button or bottom-center — pick one, keep tiny). Reuse paste-fallback styles
   for the copy fallback tray.

7. When a new selection begins (`beginSelection`), close any open copy overlay
   / fallback from a prior selection.

## 8. Verification commands

```bash
cd crates/ajax-web/web && npm run web:test -- --run TerminalRawView.test.ts
cd crates/ajax-web/web && npm run web:check
cd crates/ajax-web/web && npm run web:build
```

## 9. Acceptance criteria

- Selection end does not auto-copy.
- Copy overlay appears when selection has text.
- Copy tap uses `copyText`; success clears selection + flashes Copied.
- Copy failure opens readonly selected textarea fallback.
- Cancelled selection does not show overlay.
- P1/P2 behaviors unchanged.

## 10. Stop conditions

- Stop if ghostty clears selection on touchend before overlay can read text —
  then capture text into `copyOverlayText` inside `finishSelection` before any
  clear (already in instructions).
- Stop if jsdom cannot assert `.select()` — assert readonly value + presence
  instead and note in report.
- Do not reintroduce auto-copy “for convenience.”

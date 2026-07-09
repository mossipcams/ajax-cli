# TDD Packet: Priority 1 — fallback paste tray + Send

## 1. Goal

When `navigator.clipboard.readText()` rejects (permission denied or any error),
open the existing paste fallback textarea tray instead of only showing a status
notice. Add a Send button that pastes the tray textarea value through
`term.paste`, clears the value, and closes the tray.

## 2. Allowed files

**Tests**

- `crates/ajax-web/web/src/components/TerminalRawView.test.ts`

**Production**

- `crates/ajax-web/web/src/components/TerminalRawView.svelte`

**Build**

- `crates/ajax-web/web/dist/*` only via `npm run web:build` after source is green

## 3. Forbidden changes

- Do not change long-press selection, auto-copy, or gesture `preventDefault`
  policy (Priority 2–3).
- Do not change layout/CSS from Priority 0 except incidental if rebuild touches
  dist.
- Do not remove the native `onpaste` handler on the fallback textarea.
- Do not change `diagnostics.ts` / `copyText`.
- Do not edit Rust crates or architecture.md.

## 4. Architecture context

UI-only. Paste still flows `term.paste(text)` → ghostty `onData` → websocket
input. Fallback tray already exists for missing clipboard API; extend the same
path for `readText` rejection and manual Send.

## 5. Code anchors

**Current requestPaste (catch only sets notice):**

```491:509:crates/ajax-web/web/src/components/TerminalRawView.svelte
    requestPaste = () => {
      const clipboard = navigator.clipboard;
      if (!clipboard || typeof clipboard.readText !== "function") {
        pasteFallbackOpen = true;
        return;
      }
      clipboard
        .readText()
        .then((text) => {
          if (text) term?.paste(text);
          pasteNotice = "";
          term?.focus();
        })
        .catch(() => {
          pasteNotice = "Clipboard read failed — allow paste access and retry";
        });
    };
```

**Fallback tray markup (Cancel only today):**

```962:982:crates/ajax-web/web/src/components/TerminalRawView.svelte
  {#if pasteFallbackOpen}
    <div class="terminal-paste-fallback" data-testid="terminal-paste-fallback">
      <textarea ... bind:this={pasteFallbackInput} onpaste={...}></textarea>
      <button ... Cancel</button>
    </div>
  {/if}
```

**Existing helpers to reuse:** `pasteToTerm(text)` (calls `term?.paste` +
focus). Prefer `pasteToTerm(pasteFallbackInput.value)` on Send.

**Existing tests to update/extend:**

- `surfaces a clipboard read failure instead of silently doing nothing` (~1145)
  — must open fallback tray on reject.
- `opens a paste fallback sheet when the async clipboard API is unavailable`
- `closes the paste fallback sheet without pasting when Cancel is tapped`

## 6. Test-first instructions

1. Update test `surfaces a clipboard read failure instead of silently doing nothing`:
   - Keep rejected `readText`.
   - Assert `[data-testid=terminal-paste-fallback]` appears.
   - Do **not** require the old status string `"Clipboard read failed…"`.
   - Assert `paste` was not called yet (until Send / native paste).

2. Add test `sends paste fallback textarea value through term.paste and closes the tray`:
   - Delete clipboard or reject readText so tray opens.
   - Set textarea value to `"hello from tray"`.
   - Click Send (role/name `"Send"`).
   - Expect `paste` called with that string; tray gone; value cleared if
     element still mounted (or tray unmounted).

3. Add test `does not paste when Send is tapped with an empty fallback value`:
   - Open tray; leave value empty; click Send.
   - Expect `paste` not called; tray still closes (or stays — prefer close
     without paste to match “clear and close” only when there is text; if empty,
     close without calling paste).

4. Run RED:
   ```bash
   cd crates/ajax-web/web && npm run web:test -- --run TerminalRawView.test.ts
   ```

## 7. Production edit instructions

1. In `requestPaste` `.catch`, set `pasteFallbackOpen = true` (same as missing
   API). Optionally clear `pasteNotice` or leave empty — do not rely on the old
   failure string.

2. In the fallback tray markup, add a Send button after the textarea (before or
   after Cancel — prefer: textarea, Send, Cancel):
   ```svelte
   <button type="button" class="terminal-key" onclick={() => {
     const text = pasteFallbackInput?.value ?? "";
     pasteFallbackOpen = false;
     if (pasteFallbackInput) pasteFallbackInput.value = "";
     if (text) pasteToTerm(text);
   }}>Send</button>
   ```

3. Keep Cancel and native `onpaste` behavior unchanged.

## 8. Verification commands

```bash
cd crates/ajax-web/web && npm run web:test -- --run TerminalRawView.test.ts
cd crates/ajax-web/web && npm run web:check
cd crates/ajax-web/web && npm run web:build
```

## 9. Acceptance criteria

- Rejected `readText` opens the fallback tray.
- Send pastes non-empty value via `term.paste`, clears, closes.
- Empty Send does not call paste; closes tray.
- Cancel and native onpaste still work.
- No Priority 2/3 behavior changes.

## 10. Stop conditions

- Stop if Send requires new clipboard APIs beyond `term.paste`.
- Stop if tests cannot mount/bind the textarea value in jsdom — report and ask.
- Do not implement Priority 2 or 3 in this packet.

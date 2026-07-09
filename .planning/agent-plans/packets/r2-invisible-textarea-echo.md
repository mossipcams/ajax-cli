# TDD Packet: R2 — invisible pasteable textarea (no off-terminal echo)

## 1. Goal

Keep the hidden Ghostty textarea pasteable on iOS (opacity ~0.01, no clip-path)
but make its **painted text and caret invisible** so typed characters do not
appear as a duplicate echo off/beside the terminal canvas in inline mode.
Optimistic zero-lag overlay and PTY echo remain the visible feedback.

## 2. Allowed files

**Tests**

- `crates/ajax-web/web/src/components/TerminalRawView.test.ts`

**Production**

- `crates/ajax-web/web/src/components/TerminalRawView.svelte`

**Build**

- `crates/ajax-web/web/dist/*` only via `npm run web:build`

## 3. Forbidden changes

- Do not remove `opacity: 0.01` or restore `clip-path: inset(50%)` — those are
  required for iOS native Paste targeting (P2 long-press paste).
- Do not remove `touchBegan` focus-on-touchstart.
- Do not change zero-lag overlay positioning/logic, expand flush (R1), or
  backspace repeat (R3).
- Do not edit `styles.css`, Rust, package.json, or bump ghostty-web.
- No drive-by refactors.

## 4. Architecture context

Ghostty creates a 1×1 textarea at `left:0; top:0` with `opacity:0` +
`clipPath:inset(50%)`. Ajax softened that in `#393` so iOS treats it as a real
edit target. Softening made the textarea's own text paint visible (faint but
readable) at the host origin — a second echo beside the canvas. Fix paint only:
transparent color / text-fill / caret, while keeping the element focusable and
pasteable.

## 5. Code anchors

```243:258:crates/ajax-web/web/src/components/TerminalRawView.svelte
    const hardenMobileTextarea = () => {
      const input = term?.textarea;
      if (!input) return;
      input.setAttribute("autocapitalize", "off");
      input.setAttribute("autocorrect", "off");
      input.setAttribute("autocomplete", "off");
      input.setAttribute("spellcheck", "false");
      input.style.fontSize = "16px";
      // ghostty clips the textarea to a fully invisible 1px box (opacity:0 +
      // clipPath:inset(50%)). Soften just enough that iOS treats it as a real
      // edit target for native Paste while it still does not paint over the canvas.
      input.style.opacity = "0.01";
      input.style.setProperty("clip-path", "none");
      input.style.setProperty("-webkit-clip-path", "none");
      input.style.setProperty("clip", "auto");
    };
```

```1282:1291:crates/ajax-web/web/src/components/TerminalRawView.svelte
  .terminal-host :global(textarea) {
    user-select: text;
    -webkit-user-select: text;
    opacity: 0.01;
    clip-path: none;
    -webkit-clip-path: none;
  }
```

Existing source-guard test:
`"terminal textarea CSS does not fully clip the edit target"` (~2249) — must
still pass (opacity 0.01 + clip-path none). Extend it or add a sibling.

## 6. Test-first instructions

### T1 — source/CSS contract

Add test named:
`"terminal textarea text and caret paint are transparent"`.

Assert `terminalRawViewSource` matches under `.terminal-host :global(textarea)`:

- `color:\s*transparent` (or `color:\s*rgba\(0,\s*0,\s*0,\s*0\)`)
- `-webkit-text-fill-color:\s*transparent` (Safari needs this; `color` alone is
  not enough for iOS)
- `caret-color:\s*transparent`

Keep existing `"terminal textarea CSS does not fully clip the edit target"`
unchanged and green.

### T2 — hardenMobileTextarea runtime styles

Add test named:
`"hardens the textarea with transparent text paint for iOS paste"`.

Mount terminal (`mountOpenTerminal` or `mountTerminal`), then assert
`lastTextarea` style:

- `opacity` is `"0.01"` (or computed equivalent)
- `color` is `"transparent"` (or empty if only set via CSS — prefer asserting
  the inline style if harden sets it, else assert CSS source only for color)
- Prefer: harden sets inline `color`, `-webkit-text-fill-color`, and
  `caret-color` to `transparent` so Ghostty cannot override via its own
  inline styles later. Assert those three inline properties on `lastTextarea`.

Focused failing command:
```
npm run web:test -- --run src/components/TerminalRawView.test.ts -t "transparent"
```

## 7. Production edit instructions

1. In `hardenMobileTextarea`, after the opacity/clip soften, add:

```ts
input.style.color = "transparent";
input.style.setProperty("-webkit-text-fill-color", "transparent");
input.style.caretColor = "transparent";
```

Update the comment: paste target stays slightly opaque/unclipped; text/caret
must not paint so typed characters do not echo beside the canvas.

2. In `.terminal-host :global(textarea)` CSS, add the same three properties:

```css
color: transparent;
-webkit-text-fill-color: transparent;
caret-color: transparent;
```

Do not change opacity or clip-path rules.

## 8. Verification commands

```
npm run web:test -- --run src/components/TerminalRawView.test.ts -t "transparent"
npm run web:test -- --run src/components/TerminalRawView.test.ts -t "does not fully clip"
npm run web:test -- --run src/components/TerminalRawView.test.ts
npm run web:check
npm run web:build
```

## 9. Acceptance criteria

- New transparent-paint tests fail before impl, pass after.
- Existing clip/opacity paste-target test still passes.
- Full TerminalRawView suite + web:check + web:build green.
- Diff limited to Allowed files.
- Typed characters no longer paint in the host-corner textarea; zero-lag
  overlay and PTY echo remain the visible path.

## 10. Stop conditions

- Making text transparent breaks an existing paste/focus test → stop and report
  (do not remove opacity 0.01).
- Required edit needs files outside Allowed → stop.
- Test passes before production edit → stop and report.
- Unrelated failures → report, do not weaken tests.

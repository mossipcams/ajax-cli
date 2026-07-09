# TDD Packet: keyboard-open band fill (keep touch→keyboard)

## 1. Goal

When `html.keyboard-open`, pin the app shell to the visual viewport band and
remove bottom-nav / route-scroll nav padding so the terminal sits flush above
the keyboard with no massive blank strip. **Keep** terminal touch focusing the
textarea (keyboard must still open on touch).

## 2. Allowed files

**Tests**

- `crates/ajax-web/web/src/components/App.test.ts`
- `crates/ajax-web/web/src/components/TaskDetail.test.ts` (or styles contract there)
- `crates/ajax-web/web/e2e/layout-scroll.test.ts` (optional band assert)

**Production**

- `crates/ajax-web/web/src/components/AppViewport.svelte`
- `crates/ajax-web/web/src/styles.css`

**Build:** `dist/*` only via `npm run web:build`

## 3. Forbidden changes

- Do **not** edit `terminalGestures.ts` or remove `touchBegan` focus in
  `TerminalRawView.svelte`.
- Do not change Paste/Send/Copy overlay behavior in this packet.
- Do not bump ghostty; no Rust; no desktop 58vh changes.

## 4. Architecture context

`viewport.ts` sets `--app-height` / `--app-top` + `keyboard-open`.
`AppViewport` maps those to `--app-band-*`. Expanded terminal already fixed to
the band; keyboard-open non-expanded must match that band contract.

## 5. Code anchors

```20:32:crates/ajax-web/web/src/components/AppViewport.svelte
  .app-viewport {
    --app-band-top: var(--app-top, 0px);
    --app-band-height: var(--app-height, 100dvh);
    height: var(--app-band-height);
    max-height: var(--app-band-height);
    /* missing: fixed + top when keyboard-open */
  }
```

```628:634:crates/ajax-web/web/src/styles.css
  html.terminal-expanded .cockpit-chrome,
  html.terminal-expanded .bottom-nav {
    display: none;
  }
  /* keyboard-open does not hide these today */
```

```382:395:crates/ajax-web/web/src/styles.css
  [data-testid="route-scroll"] {
    padding-bottom: calc(72px + env(safe-area-inset-bottom));
  }
  [data-testid="route-scroll"]:has([data-outlet="task"]) { ... }
```

Existing e2e helper `simulateKeyboardBand` in `layout-scroll.test.ts`.

## 6. Test-first

1. `App.test.ts`: import `AppViewport.svelte?raw` (already used). Assert source
   contains `:global(html.keyboard-open)` (or equivalent) with
   `position:\s*fixed`, `top:\s*var\(--app-band-top`, and
   `height:\s*var\(--app-band-height`.

2. Styles contract (TaskDetail.test.ts `loadStylesSource` or App.test reading
   styles.css via same readFileSync pattern as TaskDetail):
   - `html.keyboard-open .bottom-nav` → `display:\s*none`
   - `html.keyboard-open .cockpit-chrome` → `display:\s*none`
   - `html.keyboard-open [data-testid="route-scroll"]:has([data-outlet="task"])`
     has `padding-bottom:\s*0`

3. Optional e2e on task page + `simulateKeyboardBand`: bottom-nav not visible;
   `app-viewport` getBoundingClientRect top ≈ `--app-top`, height ≈ `--app-height`.

```bash
npm run web:test -- --run App.test.ts TaskDetail.test.ts
```

Confirm RED before CSS edits.

## 7. Production edits

**AppViewport.svelte** — add after `.app-viewport` block:

```css
:global(html.keyboard-open) .app-viewport {
  position: fixed;
  top: var(--app-band-top, 0px);
  left: 0;
  right: 0;
  height: var(--app-band-height, 100dvh);
  max-height: var(--app-band-height, 100dvh);
  z-index: 30;
}
```

**styles.css** mobile media block — add:

```css
html.keyboard-open .cockpit-chrome,
html.keyboard-open .bottom-nav {
  display: none;
}

html.keyboard-open [data-testid="route-scroll"]:has([data-outlet="task"]) {
  padding-bottom: 0;
}
```

Do not touch TerminalRawView / gestures.

## 8. Verification

```bash
npm run web:test -- --run App.test.ts TaskDetail.test.ts
npm run web:check
npm run web:build
npx playwright test --config crates/ajax-web/web/playwright.config.mts e2e/layout-scroll.test.ts e2e/fullscreen-refit.test.ts e2e/smoke.test.ts
```

## 9. Acceptance

- touchBegan / focus-on-touchstart unchanged.
- keyboard-open: app-viewport fixed to band; bottom-nav+chrome hidden; task
  route-scroll padding-bottom 0.
- layout/fullscreen/smoke e2e green.

## 10. Stop conditions

- If fixed AppViewport breaks NewTaskSheet keyboard-band e2e — fix sheet
  layering or ask parent before expanding scope.
- Do not “fix” paste by removing keyboard-on-touch.

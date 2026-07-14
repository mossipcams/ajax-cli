# TDD Packet: Web keyboard band + terminal load speed

## 1. Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## 2. Goal

Two user-visible Web Cockpit fixes on mobile:

1. **Keyboard position** — when opening a new/existing task terminal and
   focusing it, the soft-keyboard typing surface must sit above the keyboard
   (bottom of the visible band), not at the top of the page.
2. **Terminal load speed** — warm Ghostty WASM + `terminal.js` chunk before
   the terminal mounts; after a successful New Task start, navigate straight
   to that task.

## 3. Allowed files

**New**

- `crates/ajax-web/web/src/taskSlug.ts`
- `crates/ajax-web/web/src/taskSlug.test.ts`
- `crates/ajax-web/web/src/terminalPreload.ts`
- `crates/ajax-web/web/src/terminalPreload.test.ts`

**Edit**

- `crates/ajax-web/web/src/viewport.ts`
- `crates/ajax-web/web/src/viewport.test.ts`
- `crates/ajax-web/web/src/components/TerminalRawView.svelte`
- `crates/ajax-web/web/src/components/TerminalRawView.test.ts`
- `crates/ajax-web/web/src/components/NewTaskSheet.svelte`
- `crates/ajax-web/web/src/components/NewTaskSheet.test.ts`
- `crates/ajax-web/web/src/components/App.svelte`
- `crates/ajax-web/web/src/components/App.test.ts`
- `.planning/agent-plans/web-keyboard-position-terminal-load.md`

**Build (only if asset tests require sync)**

- `crates/ajax-web/web/dist/*` via `npm run web:build` only

## 4. Forbidden changes

- No Rust / architecture / registry changes.
- Do not remove `touchBegan` → textarea focus (keyboard must still open).
- Do not change paste/copy/gesture behavior beyond the focus/snap helpers.
- Do not bump ghostty-web.
- No commit / push / branch changes.
- No drive-by refactors outside Allowed files.

## 5. Context evidence

- Graphify: `NOT_REQUIRED` — no project graph; behavior is confined to Web
  Cockpit Svelte/TS already mapped by prior packets
  (`fix-ios-keyboard-band-keep-touch-focus`, terminal preload via Vite
  `manualChunks` → `terminal.js`).
- Serena: `NOT_REQUIRED` — exact symbols live in the Allowed files; parent
  inspected callers.
- ast-grep: `NOT_REQUIRED` — TS/Svelte anchors are line-stable string matches
  already known (see Code anchors).

## 6. Code anchors

**Keyboard open / scroll**

```88:94:crates/ajax-web/web/src/viewport.ts
    if (delta > KEYBOARD_OPEN_DELTA_PX && !keyboardOpen) {
      keyboardOpen = true;
      root.classList.add(KEYBOARD_OPEN_CLASS);
    } else if (delta < KEYBOARD_CLOSE_DELTA_PX && keyboardOpen) {
```

Today `resetDocumentScroll()` runs only on keyboard **close**. Call it on
keyboard **open** too.

**Textarea top-left (bug)**

```303:325:crates/ajax-web/web/src/components/TerminalRawView.svelte
    const hardenMobileTextarea = () => {
      const input = term?.textarea;
      // … opacity / clip soften …
      seedBackspaceSentinel(input);
```

Ghostty parks the hidden textarea at the host top-left; iOS places the
keyboard relative to that box → input accessory appears at page top.
Anchor the textarea to the host bottom (`position:absolute; bottom:0;
height:44px; width:100%`) in both imperative styles and
`.terminal-host :global(textarea)` CSS.

**touchBegan**

```577:579:crates/ajax-web/web/src/components/TerminalRawView.svelte
          touchBegan: () => {
            term?.textarea?.focus({ preventScroll: true });
          },
```

Before focus: `resetDocumentScroll()`, pin scroll-follow, crop host to
bottom, `snapScrollbackToBottom` when pinned.

**Lazy Ghostty load**

```47:83:crates/ajax-web/web/src/components/TerminalRawView.svelte
  const GHOSTTY_WASM_URL = "/ghostty-vt.wasm";
  …
  const loadGhosttyRuntime = () => {
    ghosttyRuntime ??= Ghostty.load(GHOSTTY_WASM_URL);
```

Extract shared `preloadGhosttyRuntime` / `warmTerminalAssets` into
`terminalPreload.ts`. `TerminalRawView` must call the shared loader.
`App.svelte` must idle-warm when `taskOpenHandle` or `sheetOpen` is set.

**Vite chunk already exists**

```77:82:crates/ajax-web/web/vite.config.mts
        manualChunks(id) {
          if (
            id.includes("/node_modules/ghostty-web/") ||
            id.includes("/components/TerminalRawView.svelte") ||
```

`preloadTerminalView()` = `import("./components/TerminalRawView.svelte")`.

**New task does not navigate**

```97:99:crates/ajax-web/web/src/components/NewTaskSheet.svelte
      savePrefs();
      onResult?.("Task started", result.response.output, false);
      onClose?.();
```

Add `onOpenTask?.(startTaskHandle(repo, title))` before `onClose`.
`startTaskHandle` mirrors Rust `start_task_identity` /
`slugify_title` (repo + slug). Wire App: `onOpenTask={(h) => go(taskHash(h))}`.

## 7. Test-first instructions

Run from repo root (worktree has `node_modules` → main symlink; vitest at
`node_modules/.bin/vitest`).

### RED batch (add failing tests first)

1. `viewport.test.ts` — on keyboard-open resize, assert
   `window.scrollTo` was called with `(0, 0)`.
2. `TerminalRawView.test.ts` — source contract:
   - `.terminal-host :global(textarea)` sets `bottom: 0`
   - production source contains `input.style.bottom = "0"`
   - `touchBegan` block calls `resetDocumentScroll`
3. New `taskSlug.test.ts` — `slugifyTaskTitle("Fix Login") === "fix-login"`;
   `startTaskHandle("web", "Fix Login") === "web/fix-login"`.
4. New `terminalPreload.test.ts` — mock `ghostty-web` `Ghostty.load`;
   `preloadGhosttyRuntime` called twice → load once with
   `/ghostty-vt.wasm`; `warmTerminalAssets` invokes load +
   `preloadTerminalView`.
5. `NewTaskSheet.test.ts` — successful start calls `onOpenTask` with
   `"web/fix-login"`.
6. `App.test.ts` — `appSource` matches `/warmTerminalAssets/`.

Focused RED:

```bash
npm run web:test -- --run \
  crates/ajax-web/web/src/viewport.test.ts \
  crates/ajax-web/web/src/taskSlug.test.ts \
  crates/ajax-web/web/src/terminalPreload.test.ts \
  crates/ajax-web/web/src/components/NewTaskSheet.test.ts \
  crates/ajax-web/web/src/components/App.test.ts \
  crates/ajax-web/web/src/components/TerminalRawView.test.ts
```

Confirm nonzero exit and the new assertions fail before production edits.

## 8. Edit instructions

1. **`taskSlug.ts`** — export `slugifyTaskTitle` (mirror
   `ajax_core::commands::new_task::slugify_title`) and
   `startTaskHandle(repo, title)`.
2. **`terminalPreload.ts`** — export `GHOSTTY_WASM_URL`,
   `preloadGhosttyRuntime()`, `preloadTerminalView()`,
   `warmTerminalAssets()`.
3. **`viewport.ts`** — call `resetDocumentScroll()` when transitioning to
   keyboard-open.
4. **`TerminalRawView.svelte`** — use `preloadGhosttyRuntime`; remove local
   Ghostty load cache; bottom-anchor textarea in `hardenMobileTextarea` + CSS;
   expand `touchBegan` as in anchors.
5. **`NewTaskSheet.svelte`** — add `onOpenTask?: (handle: string) => void`;
   on success call `onOpenTask?.(startTaskHandle(repo, title))` before close.
6. **`App.svelte`** — idle `warmTerminalAssets()` when task route or sheet
   open; pass `onOpenTask={(handle) => go(taskHash(handle))}` to sheet.
7. Update plan checklist boxes as tasks complete.

## 9. Verification commands

```bash
npm run web:test -- --run \
  crates/ajax-web/web/src/viewport.test.ts \
  crates/ajax-web/web/src/taskSlug.test.ts \
  crates/ajax-web/web/src/terminalPreload.test.ts \
  crates/ajax-web/web/src/components/NewTaskSheet.test.ts \
  crates/ajax-web/web/src/components/App.test.ts \
  crates/ajax-web/web/src/components/TerminalRawView.test.ts
npm run web:check
```

Optional if dist fingerprints complain:

```bash
npm run web:build
```

## 10. Acceptance criteria

- RED proven, then GREEN on the same focused suite.
- Keyboard-open resets document scroll.
- Hidden textarea anchored to bottom; touchBegan resets scroll / pins bottom
  then focuses with `preventScroll: true`.
- Shared Ghostty preload; App warms on task/sheet.
- Successful New Task navigates to `#/t/<repo>%2F<slug>`.
- No forbidden files changed.

## 11. Stop conditions

- Stop if iOS keyboard position requires Ghostty fork / upstream bump.
- Stop if `startTaskHandle` slug rules diverge from Rust in a way that needs
  a server-returned handle (ask parent — do not change Rust).
- Stop if patch exceeds ~400 changed lines or needs files outside Allowed.
- Stop after two failed verify attempts with the same root cause.

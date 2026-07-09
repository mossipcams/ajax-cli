# Task 1 packet: shared viewport scroll reset

## 1. Goal

Create one shared, safe document-scroll reset helper for Web Cockpit viewport
code and use it from both `initViewport` and expanded terminal snapping.

This reduces duplicated browser cleanup logic without changing terminal,
keyboard, or task behavior.

## 2. Allowed files

Test files:

- `crates/ajax-web/web/src/viewport.test.ts`
- `crates/ajax-web/web/src/components/TerminalRawView.test.ts` only if an
  existing assertion needs a name/import adjustment

Production files:

- `crates/ajax-web/web/src/viewport.ts`
- `crates/ajax-web/web/src/components/TerminalRawView.svelte`

Planning files:

- `.planning/agent-plans/web-viewport-terminal-design-cleanup.md`

## 3. Forbidden changes

- Do not edit files under root `tests/`.
- Do not change `terminalGestures.ts`, `terminalGeometry.ts`,
  `terminalRefit.ts`, backend Rust, generated `dist`, lockfiles, or package
  metadata.
- Do not change keyboard-open thresholds, CSS classes, terminal expanded
  behavior, paste/copy behavior, socket behavior, or Ghostty options.
- Do not weaken or delete existing assertions.
- Do not add dependencies.

## 4. Architecture context

Web Cockpit remains a browser presentation adapter over backend/core task
truth. Browser terminal frontend modules may own viewport/gesture/geometry
presentation behavior, but not task truth or tmux target selection.

Relevant architecture source: `architecture.md` Web Cockpit terminal frontend
section says `viewport.ts`, `terminalGestures.ts`, `terminalGeometry.ts`, and
`terminalRefit.ts` keep mobile scrolling, panning, keyboard-safe fitting, and
refit scheduling local to the browser shell.

## 5. Code anchors

Existing duplicated logic:

- `crates/ajax-web/web/src/viewport.ts`
  - `export function initViewport(): () => void`
  - local `const resetDocumentScroll = () => { ... window.scrollTo(0, 0) ... document.scrollingElement ... }`
  - close branch calls `resetDocumentScroll();`

- `crates/ajax-web/web/src/components/TerminalRawView.svelte`
  - import currently includes `import { isKeyboardOpen } from "../viewport";`
  - function anchor: `const snapVisibleTerminal = () => {`
  - duplicated statements:
    - `document.documentElement.scrollTop = 0;`
    - `document.body.scrollTop = 0;`
    - `const scrollingElement = document.scrollingElement;`
    - `window.scrollTo(0, 0);`

Existing tests:

- `crates/ajax-web/web/src/viewport.test.ts`
  - imports `initViewport, isKeyboardOpen`
  - test `clears document scroll when the keyboard closes`
  - test `no-ops without visualViewport`

- `crates/ajax-web/web/src/components/TerminalRawView.test.ts`
  - existing expanded snap test around lines containing
    `document.documentElement.scrollTop = 120;`
    and expects scroll reset after expand.

AST/text anchors gathered:

- `rtk ast-grep --pattern 'window.scrollTo(0, 0)' --lang ts crates/ajax-web/web/src/viewport.ts`
  matched `viewport.ts:65`.
- `rtk rg -n "resetDocumentScroll|scrollTo\\(0, 0\\)|document\\.documentElement\\.scrollTop|document\\.body\\.scrollTop|document\\.scrollingElement|snapVisibleTerminal|initViewport" ...`
  matched the duplicated anchors above.

## 6. Test-first instructions

Add a failing test in `crates/ajax-web/web/src/viewport.test.ts`:

- Test name: `resetDocumentScroll clears every known document scroll owner safely`.
- Import the new helper name from `./viewport`. Suggested name:
  `resetDocumentScroll`.
- Arrange:
  - `vi.spyOn(window, "scrollTo").mockImplementation(() => {});`
  - set `document.documentElement.scrollTop = 120`;
  - set `document.body.scrollTop = 80`;
  - if `document.scrollingElement` exists, set its `scrollTop = 60`;
- Act: call `resetDocumentScroll()`.
- Assert:
  - `window.scrollTo` called with `(0, 0)`;
  - `document.documentElement.scrollTop === 0`;
  - `document.body.scrollTop === 0`;
  - `document.scrollingElement.scrollTop === 0` when present.
- Also assert the helper does not throw when `window.scrollTo` throws. Keep the
  assertion small; either same test or a second focused test is fine.

Focused command that must fail before implementation:

```bash
rtk npm run web:test -- --run viewport.test.ts
```

Report the expected failure before editing production code. If the test passes
before implementation, stop and report.

## 7. Production edit instructions

In `crates/ajax-web/web/src/viewport.ts`:

- Export `resetDocumentScroll(): void`.
- It should:
  - try `window.scrollTo(0, 0)` and swallow jsdom/browser unsupported errors;
  - set `document.documentElement.scrollTop = 0`;
  - set `document.body.scrollTop = 0`;
  - set `document.scrollingElement.scrollTop = 0` if present.
- Replace the local `resetDocumentScroll` nested function inside `initViewport`
  with the exported helper.

In `crates/ajax-web/web/src/components/TerminalRawView.svelte`:

- Change the viewport import to include the helper:
  `import { isKeyboardOpen, resetDocumentScroll } from "../viewport";`
- In `snapVisibleTerminal`, replace the duplicated document/window scroll reset
  block with `resetDocumentScroll();`.
- Keep the existing keyboard-open container bottom scroll and bottom snap logic.

Do not otherwise refactor `TerminalRawView.svelte`.

## 8. Verification commands

Focused:

```bash
rtk npm run web:test -- --run viewport.test.ts TerminalRawView.test.ts
```

If focused tests pass:

```bash
rtk npm run web:check
```

## 9. Acceptance criteria

- The new viewport test fails before production edits because the helper is not
  exported/implemented.
- Focused tests pass after production edits.
- `TerminalRawView.svelte` no longer contains its own duplicated document/window
  reset block in `snapVisibleTerminal`.
- Existing expanded terminal scroll-reset behavior remains covered.
- No out-of-scope files are changed.

## 10. Stop conditions

- Stop if the new test passes before production edits.
- Stop if satisfying the test requires changing terminal expansion, keyboard
  thresholds, CSS layout, or Ghostty behavior.
- Stop if tests fail for unrelated reasons after implementation.
- Stop if required edits fall outside the allowed files.

# Task 2 packet: remove document-scroll ownership from TaskDetail

## 1. Goal

Make `TaskDetail.svelte` describe task content layout only. App/global layout
CSS should own `html.keyboard-open` / `html.terminal-expanded` scroll and chrome
policy.

This reduces duplicated document-level scroll ownership without changing the
mobile terminal-first behavior.

## 2. Allowed files

Test files:

- `crates/ajax-web/web/src/components/TaskDetail.test.ts`

Production files:

- `crates/ajax-web/web/src/components/TaskDetail.svelte`

Planning files:

- `.planning/agent-plans/web-viewport-terminal-design-cleanup.md`

## 3. Forbidden changes

- Do not edit root `tests/`.
- Do not edit `styles.css` unless the focused tests prove behavior is broken
  after removing the duplicate TaskDetail block; if that happens, stop and
  report before editing it.
- Do not change `TerminalRawView.svelte`, viewport helpers, route wrappers,
  backend Rust, generated `dist`, lockfiles, or package metadata.
- Do not weaken assertions about terminal-first hooks, mobile meta hiding,
  route-scroll ownership, or terminal max-height behavior.
- Do not add dependencies or broad refactors.

## 4. Architecture context

Web Cockpit browser code may own presentation layout, but task detail UI should
not become a document-level policy owner. `RouteScroll` and global app layout
CSS own normal route scrolling; `TaskDetail.svelte` should only render task
content and component-scoped task layout.

`architecture.md` says the browser terminal frontend modules handle mobile
scrolling/keyboard-safe fitting locally to the browser shell and do not own task
truth or tmux target selection.

## 5. Code anchors

Existing duplicate owner in `crates/ajax-web/web/src/components/TaskDetail.svelte`:

- Mobile media block starts near:
  `@media (max-width: 767px), (pointer: coarse) and (max-height: 500px) {`
- Remove only this document-level global block:
  - `:global(html.terminal-expanded),`
  - `:global(html.terminal-expanded body),`
  - `:global(html.keyboard-open),`
  - `:global(html.keyboard-open body) {`
  - `overflow: hidden;`
  - `overscroll-behavior: none;`
  - `}`

Global owner already exists in `crates/ajax-web/web/src/styles.css`:

- Mobile task view block contains:
  - `html.keyboard-open .cockpit-chrome,`
  - `html.keyboard-open .bottom-nav,`
  - `html.terminal-expanded .cockpit-chrome,`
  - `html.terminal-expanded .bottom-nav`
  - `html.keyboard-open [data-testid="route-scroll"]:has([data-outlet="task"])`
  - `html.terminal-expanded [data-testid="task-terminal-panel"].is-expanded`

Existing test to update in `crates/ajax-web/web/src/components/TaskDetail.test.ts`:

- Test name currently: `defines mobile overlay height pins without a fixed task shell`
- It currently expects the TaskDetail mobile CSS to contain the `:global(html...)`
  overflow lock. Change it so the test expects TaskDetail **not** to contain
  `:global(html.keyboard-open` or `:global(html.terminal-expanded`.
- Keep existing positive assertions for:
  - `.task-detail` flex/overflow/min-height content layout;
  - `terminal-inline-spacer`;
  - `class:is-expanded={expanded}`;
  - safe-area padding.

Add or update assertion using `loadStylesSource()` in the same test or a new
small test so global `styles.css` still contains the app-level keyboard/expanded
owners listed above.

Text anchors gathered:

- `rtk rg -n "keyboard-open|terminal-expanded|detail-header|interact-panel|meta-details|route-scroll" crates/ajax-web/web/src/components/TaskDetail.test.ts crates/ajax-web/web/src/components/TaskDetail.svelte crates/ajax-web/web/src/styles.css`

## 6. Test-first instructions

First update `TaskDetail.test.ts` so the expected ownership changes:

- The updated/new test should fail before production edits because
  `TaskDetail.svelte` still contains `:global(html.keyboard-open` /
  `:global(html.terminal-expanded`.
- Focused failing command:

```bash
rtk npm run web:test -- --run TaskDetail.test.ts
```

Report the expected failure before editing production code. If the test passes
before production edits, stop and report.

## 7. Production edit instructions

In `TaskDetail.svelte`:

- Delete only the `:global(html.terminal-expanded/body)` and
  `:global(html.keyboard-open/body)` block from the scoped mobile media query.
- Leave the `.task-detail`, `.detail-header`, `.interact-panel`,
  `.meta-details`, and `.terminal-primary` mobile content layout rules intact.
- Do not move CSS into another file for this task.

Update `.planning/agent-plans/web-viewport-terminal-design-cleanup.md`:

- Mark Task 2 checklist items complete only after verification.
- Record expected failing command and final passing commands/results.

## 8. Verification commands

Focused:

```bash
rtk npm run web:test -- --run TaskDetail.test.ts
```

Behavior/layout smoke:

```bash
rtk npx playwright test e2e/layout-scroll.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit
```

If both pass:

```bash
rtk npm run web:check
```

## 9. Acceptance criteria

- Updated test fails before production edit for the expected reason.
- `TaskDetail.svelte` no longer contains scoped global
  `:global(html.keyboard-open` or `:global(html.terminal-expanded` selectors.
- Global/app stylesheet still owns keyboard/expanded route/terminal policy.
- Focused TaskDetail tests and mobile WebKit layout-scroll test pass.
- No out-of-scope files are changed.

## 10. Stop conditions

- Stop if the updated test passes before production edits.
- Stop if removing the duplicate block changes mobile WebKit layout-scroll
  behavior and fixing it requires edits outside allowed files.
- Stop if you need to edit `styles.css`; report the exact missing owner instead.
- Stop on unrelated test failures.

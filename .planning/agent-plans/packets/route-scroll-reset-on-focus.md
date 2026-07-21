```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

When the operator focuses the inline (non-fullscreen) task terminal on iOS, the detail page back header must remain on-screen. `resetDocumentScroll` must also zero the App `route-scroll` container that owns vertical scroll on the task page.

## Allowed files

- `crates/ajax-web/web/src/shared/lib/viewport.ts`
- `crates/ajax-web/web/src/shared/lib/viewport.test.ts`

## Forbidden changes

- Do not change keyboard band CSS, fullscreen expand geometry, or TaskTerminal focus handlers beyond what already calls `resetDocumentScroll`.
- Do not commit, push, merge, rebase, or change branches.
- Do not touch ActionBar / Drop / hotbar files.

## Context evidence

1. **Desired behavior** — User: when not fullscreen but in the terminal, the top of the page with the back button is off the screen.
2. **Source anchors** — `viewport.ts` `resetDocumentScroll` clears `window` / `documentElement` / `body` / `scrollingElement` only. Task page vertical scroll lives on `[data-testid="route-scroll"]` (`RouteScroll.tsx` + `styles.css`). `TaskTerminal.tsx` already calls `resetDocumentScroll()` before `textarea.focus({ preventScroll: true })` and on keyboard-open class edges.
3. **Patterns** — `viewport.test.ts` `describe("resetDocumentScroll")` already asserts document scroll owners; extend it for route-scroll.
4. **Boundaries** — Shared viewport helper only; TaskTerminal keeps calling the same function.

## Code anchors

- `crates/ajax-web/web/src/shared/lib/viewport.ts` — `export function resetDocumentScroll`
- `crates/ajax-web/web/src/shared/lib/viewport.test.ts` — `resetDocumentScroll clears every known document scroll owner safely`
- `crates/ajax-web/web/src/app/RouteScroll.tsx` — `data-testid="route-scroll"`

## Test-first instructions

In `viewport.test.ts`, extend the `resetDocumentScroll` suite:

1. Create a `div` with `data-testid="route-scroll"`, set `scrollTop` to a non-zero value, append to `document.body`.
2. Call `resetDocumentScroll()`.
3. Assert that element's `scrollTop` is `0`.
4. Clean up the element.

Red command:

```bash
cd crates/ajax-web/web && npx vitest run --config vite.config.mts src/shared/lib/viewport.test.ts
```

Confirm the new assertion fails before production edit.

## Edit instructions

In `resetDocumentScroll` (`viewport.ts`), after existing document scroll clears, also set `scrollTop = 0` on every element matching `[data-testid="route-scroll"]` (querySelectorAll). Keep jsdom safety (no throw). Update the function comment to mention the App route scroller.

## Verification commands

```bash
cd crates/ajax-web/web && npx vitest run --config vite.config.mts src/shared/lib/viewport.test.ts
```

## Acceptance criteria

- `resetDocumentScroll` zeros `[data-testid="route-scroll"]` scrollTop.
- Existing viewport tests still pass.
- No edits outside allowed files.

## Stop conditions

- Need TaskTerminal or CSS layout changes beyond the helper.
- False green without production edit.

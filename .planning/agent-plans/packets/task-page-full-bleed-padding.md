# TDD Implementation Packet — task page full-bleed padding

## Goal

On mobile, the task route's `route-scroll` must drop horizontal padding so the
terminal is truly edge-to-edge (matching `TerminalRawView`'s full-bleed border
rules), while TaskDetail's header/actions keep their own `12px + safe-area`
insets.

## Allowed files

Production:
- `crates/ajax-web/web/src/styles.css`

Tests:
- `crates/ajax-web/web/src/components/App.test.ts`
- `crates/ajax-web/web/src/components/TaskDetail.test.ts` (only if an existing
  assertion must be updated to match the new contract)

## Forbidden changes

- Do not edit `TerminalRawView.svelte`, `TaskDetail.svelte` production markup,
  or any Rust code.
- Do not change desktop (`min-width: 768px`) layout rules.
- Do not remove TaskDetail's mobile header/interact `padding-left/right`
  (`12px + env(safe-area-*)`) — those become the chrome inset once route-scroll
  sides are zero.
- Do not change keyboard-open / terminal-expanded rules beyond what is required
  if a source-string test must stay accurate.
- No formatting sweeps, renames, or drive-by cleanup.
- Do not hand-edit `crates/ajax-web/web/dist/*`.

## Architecture context

Web Cockpit: UI presents task truth; layout lives in `styles.css` + component
scoped CSS. Mobile task route locks scroll in `route-scroll` via
`:has([data-outlet="task"])`. Terminal assumes that band is horizontally
full-bleed on mobile.

## Code anchors

In `crates/ajax-web/web/src/styles.css` inside the mobile media query
`@media (max-width: 767px), (pointer: coarse) and (max-height: 500px)`:

```css
[data-testid="route-scroll"]:has([data-outlet="task"]) {
  display: flex;
  flex-direction: column;
  overflow: hidden;
  overscroll-behavior: none;
  padding-top: 0;
}
```

Contract comment in `TerminalRawView.svelte` (do not edit; behavior target):

```text
Full-bleed: the task page drops its horizontal padding on mobile so the
terminal runs edge to edge
```

Existing test pattern to mirror in `App.test.ts`:

```ts
it("hides chrome and clears task route-scroll padding when keyboard-open", () => {
  ...
});
```

TaskDetail already asserts chrome owns safe-area horizontal padding:

```ts
expect(mobileCss).toMatch(/\.detail-header,\s*\.interact-panel\s*\{[^}]*padding-left:[^;]*env\(safe-area-inset-left\)/);
```

## Test-first instructions

1. In `App.test.ts`, add a test named:
   `"zeros horizontal padding on the mobile task route-scroll"`.
2. Load `styles.css` the same way other App tests do (`loadStylesSource` or
   equivalent existing helper in that file).
3. Assert that within the mobile media block, the rule
   `[data-testid="route-scroll"]:has([data-outlet="task"])` sets:
   - `padding-top: 0` (already true)
   - `padding-left: 0` (new)
   - `padding-right: 0` (new)
4. Keep asserting bottom padding is NOT forced to 0 here (nav clearance stays
   unless keyboard-open).
5. Run and confirm FAIL before production edit:
   ```bash
   rtk npm run web:test -- --run src/components/App.test.ts
   ```
   Expected failure: missing `padding-left: 0` / `padding-right: 0` on the
   task route-scroll rule.

## Production edit instructions

In `styles.css`, update only the mobile task route-scroll rule to:

```css
[data-testid="route-scroll"]:has([data-outlet="task"]) {
  display: flex;
  flex-direction: column;
  overflow: hidden;
  overscroll-behavior: none;
  padding-top: 0;
  padding-left: 0;
  padding-right: 0;
}
```

Optionally add a one-line comment that TaskDetail chrome owns horizontal
safe-area inset and the terminal is full-bleed. Do not touch other padding
rules.

## Verification commands

```bash
rtk npm run web:test -- --run src/components/App.test.ts src/components/TaskDetail.test.ts
rtk npm run web:check
```

## Acceptance criteria

- New App.test fails before the CSS edit for the expected reason.
- After the CSS edit, App + TaskDetail focused tests pass.
- `web:check` passes.
- Diff touches only Allowed files.
- Mobile task route-scroll has zero left/right padding; chrome still has
  TaskDetail's 12px+safe-area padding.

## Stop conditions

- Stop if TaskDetail tests require production edits outside Allowed files.
- Stop if the failing test unexpectedly passes before the CSS change.
- Stop if unrelated tests fail for reasons outside this padding change.
- Stop if the patch would need desktop layout changes.

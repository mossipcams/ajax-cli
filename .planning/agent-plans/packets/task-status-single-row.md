# TDD Implementation Packet — task status single row

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## Goal

On the task detail page, the status text inside `.interact-panel` (`.interact-summary` and `.interact-activity`) must never grow beyond one row. Long copy truncates with ellipsis instead of wrapping into a multi-line block that eats the page.

## Allowed files

- `crates/ajax-web/web/src/components/TaskDetail.svelte`
- `crates/ajax-web/web/src/components/TaskDetail.test.ts`

## Forbidden changes

- Do not edit `styles.css`, `TaskTerminal.svelte`, ActionBar, or any Rust code.
- Do not remove status explanation or activity from the markup entirely.
- Do not change ActionBar wrapping / button layout.
- No formatting sweeps, renames, or drive-by cleanup.
- Do not hand-edit `crates/ajax-web/web/dist/*`.
- Do not commit, push, merge, rebase, or change branches.

## Context evidence

- **Graphify:** `NOT_REQUIRED` — layout-only Web Cockpit presentation; no ownership/registry change.
- **Serena:** `NOT_REQUIRED` — anchors are concrete CSS classes in TaskDetail.
- **ast-grep:** `NOT_REQUIRED` — CSS/HTML clamp, not Rust/TS structural rewrite.

## Code anchors

`crates/ajax-web/web/src/components/TaskDetail.svelte` markup:

```svelte
{#if detail.status_explanation}
  <p class="interact-summary">{detail.status_explanation}</p>
{/if}
{#if activityLine}
  <p class="interact-summary interact-activity" data-testid="agent-activity">{activityLine}</p>
{/if}
```

Current CSS (wraps freely — the bug):

```css
.interact-summary {
  margin: 0 0 12px;
  font-size: 14px;
  line-height: 1.45;
  color: var(--ink-soft);
  overflow-wrap: anywhere;
}
```

Existing tests in `TaskDetail.test.ts` already cover activity visibility; extend with a CSS-contract assertion (same style as other source-contract tests in this crate) or a rendered style assertion that the summary uses single-line clamp.

## Test-first instructions

1. In `TaskDetail.test.ts`, add a focused test named approximately:
   `"clamps status explanation and activity to a single row"`.
2. Assert the TaskDetail `<style>` (or computed contract) for `.interact-summary` includes single-line clamp behavior, e.g. all of:
   - `white-space: nowrap` **or** equivalent `line-clamp: 1` / `-webkit-line-clamp: 1`
   - `overflow: hidden`
   - `text-overflow: ellipsis`
3. Keep existing activity show/hide tests green.
4. RED command (must fail before production edit):

```bash
cd crates/ajax-web/web && npm run test -- --run src/components/TaskDetail.test.ts
```

Expected: new assertion fails because `.interact-summary` still uses free wrapping without clamp/ellipsis.

## Edit instructions

1. In `TaskDetail.svelte` scoped CSS, change `.interact-summary` (and ensure `.interact-activity` inherits) so each status line is exactly one row:
   - Prefer:

```css
.interact-summary {
  margin: 0 0 12px;
  font-size: 14px;
  line-height: 1.45;
  color: var(--ink-soft);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
```

   - Remove `overflow-wrap: anywhere` from `.interact-summary` (conflicts with single-row clamp).
2. Do not clamp `.interact-warning` (observation errors may need multiple lines).
3. Smallest CSS-only change; no new components.

## Verification commands

```bash
cd crates/ajax-web/web && npm run test -- --run src/components/TaskDetail.test.ts
cd crates/ajax-web/web && npm run check
```

If `npm run check` is unavailable/broken in this worktree, run `npx tsc -p tsconfig.check.json --noEmit` and report that.

## Acceptance criteria

- Status explanation and agent activity each render as at most one visual row with ellipsis when overflowing.
- Observation warning can still wrap.
- Focused TaskDetail tests pass with RED→GREEN evidence.
- No files outside Allowed files changed.

## Stop conditions

- Diff touches files outside Allowed files.
- Patch exceeds ~60 changed lines or starts redesigning the interact panel / ActionBar.
- Existing TaskDetail tests fail for unrelated reasons that cannot be fixed within Allowed files.
- Temptation to also fix terminal/keyboard layout (that is Task 2).

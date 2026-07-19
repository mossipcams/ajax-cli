# Slice 8 — RadioGroup primitive

Master plan: `react-migration-cleanup.md`
Depends on: slice 7 (`e8cbdc6`)

## Measurement before planning

- The agent picker (`NewTaskSheet.tsx:208-221`) is the **only** radiogroup in
  the app. A repo-wide sweep for `role="radio"`, `type="radio"`, `role="switch"`,
  `role="tab"`, `type="checkbox"` returns nothing else.
- `SettingsView` and `TestInDevPanel` contain no form controls beyond buttons,
  which slice 6 already moved to `Button`. **"Remaining low-risk primitives"
  from the master plan has no referent** — there is nothing else to convert.
- The one `<select>` (`:181`) and one `<input>` (`:195`) stay **native**. A
  shadcn `Select` would replace the iOS native picker wheel with a custom
  listbox on a mobile-first surface — a downgrade, and out of scope.
- Existing markup is already semantically correct: `role="radiogroup"`,
  `aria-labelledby`, per-item `role="radio"` + `aria-checked`.
- Styling: `.agent-picker` (`styles.css:1296-1301`, a 2-column grid) and
  `.agent-option` / `.agent-option.is-selected` / `:focus-visible`
  (`:1303-1322`).
- Measured cost of `@radix-ui/react-radio-group`, by building a probe against
  the real bundle: **+3.5KB gzip** (186,950 → 190,442).

## The actual gap

Not semantics — those are already right. The gap is **keyboard mechanics**:
all four buttons sit in the tab order, and arrow keys do nothing. The ARIA APG
radiogroup pattern expects one tab stop plus arrow-key traversal. Radix's
`RovingFocusGroup` supplies exactly that.

Decision (Matt, asked explicitly): take the shadcn primitive for consistency
with `Button` (s6) and `Sheet` (s7), accepting the 3.5KB. The hand-rolled
alternative (~12 lines, no dep) was the recommendation and was declined in
favour of a single consistent primitive story.

**That decision was reversed by evidence — see "Radix discarded" below.** The
premise (that the primitive works on this surface) turned out to be false, so
the choice was never actually available.

## Key decision — same as s6/s7: the primitive selects Ajax classes

`radio-group.tsx` maps onto the existing `.agent-picker` / `.agent-option`
classes. `styles.css` is not touched and gains no tokens. Selection styling
keeps using the explicit `is-selected` class rather than switching to
`[data-state=checked]`, so the CSS diff stays empty.

## Carried-forward constraints from slice 7

These are not theoretical; each one is a defect slice 7 actually shipped and
the gate caught:

1. **Never hand-write an `id` on a Radix `asChild` child.** `Slot` lets child
   props win, which silently overrode Radix's generated id and left
   `aria-labelledby` dangling. Verify every aria reference **resolves to a real
   element**, don't assume.
2. **Diff the rendered a11y attributes before and after**, don't trust that a
   primitive preserves them. `aria-modal` vanished in s7 without any test noticing.
3. **No no-op handlers to silence the linter.** A scoped disable with a reason,
   or fix the underlying issue.

## Risks

1. **Radix `BubbleInput`.** Radix `RadioGroupItem` renders a hidden native
   `<input type="radio">` for form participation when inside a `<form>`. This
   picker *is* inside the new-task `<form>`. A stray input must not reach
   `startTask` or alter submit behaviour — the payload is built from React
   state (`NewTaskSheet.tsx:96-101`), so it should be inert, but it must be
   proven, not assumed.
2. **`type="button"`.** Items must not submit the form. Radix sets
   `type="button"`; regression here would make picking an agent start a task.
3. **2-column grid traversal.** `.agent-picker` is a 2×2 grid, so both
   horizontal and vertical arrows should move. Do not set `orientation`.

## Non-goals

- No CSS change, no new tokens, no `@theme` edits.
- No `Select`, `Input`, `Label`, `Checkbox`, or `Switch` primitives — no surface
  needs them.
- No change to agent values, `localStorage` keys, or submit behaviour.

## Delegation decision

`Delegation decision: delegated via model-router`

## Baselines

- Suite: 378 tests / 42 files
- mobile-webkit e2e: 92 passed / 2 skipped
- `app.js` gzip: 186,950 bytes

## Validation gate

```bash
npm run web:check
npm run web:test -- --run
npm run web:lint
npm run web:build:check
npm run web:smoke -- --project=mobile-webkit
cargo nextest run -p ajax-web
```

Dev deploy → iPhone checklist (tap each agent, confirm selection sticks and
survives a start) → **wait for Matt** → PR.

## Radix discarded — the primitive does not work on this surface

The delegated Radix implementation was **DISCARDED** at the review gate. It had
hand-rolled both the roving `tabIndex` and an `agentForArrowKey` handler
*alongside* Radix, which was the tell: the dependency was not doing the job it
was added for. Stripping the hand-rolled code and testing Radix unaided, in
**real Chromium** (jsdom cannot drive roving focus at all):

| Step | Result |
| --- | --- |
| Tab from the title field | focus lands on Codex ✓ |
| ArrowRight | **focus does not move; selection does not change** ✗ |
| Tab again | leaves the group entirely, lands on Cancel ✗ |

Radix parks `tabindex="0"` on the group root and holds every item at `-1`, but
its `RovingFocusGroup` never moved selection inside the slice-7 Dialog's focus
scope. Net effect: **Claude, Cursor and OpenCode became unreachable by
keyboard**, where previously all four were tabbable. A slice whose only purpose
was accessibility would have shipped a hard accessibility regression, at a
measured cost of +3.5KB gzip.

Two things made this catchable, and both are worth keeping:
- the packet demanded the arrow-key behaviour be *proven*, not assumed;
- the proof had to run in a real browser, because jsdom silently passes here.

Also worth recording: my packet's tab-stop test asserted the wrong shape
(one *item* at `tabindex=0`), and the delegate hand-rolled `tabIndex` to satisfy
it. A test encoding the wrong expectation actively produced bad code.

## Final implementation — hand-rolled, 14 lines

`agentForArrowKey` plus a per-item `onKeyDown` and `tabIndex`, on the existing
plain buttons. The key handler sits on the items (not the group), matching the
ARIA APG roving pattern and keeping `jsx-a11y/interactive-supports-focus` happy
without a no-op or a disable.

Cost: **+169 bytes gzip** (186,950 → 187,119), versus +3.5KB for Radix.

## Deviations / Validation results

- **Slice 8 ships without a shadcn primitive.** `components/ui/radio-group.tsx`
  was created and then deleted; `@radix-ui/react-radio-group` was installed and
  then uninstalled. The primitive story from s6/s7 does not extend here, and the
  plan should not pretend otherwise.
- PASS — vitest **380/380** across 42 files, `web:check` 0, `web:lint` 0,
  `web:build:check` 0, `cargo nextest -p ajax-web` 159/159, mobile-webkit smoke
  **93 passed / 2 skipped** (was 92 — the new e2e runs there too).
- Both new jsdom guards mutation-checked: removing `tabIndex` and neutering
  `agentForArrowKey` fails exactly those two tests and nothing else.
- Arrow-key traversal proven in **desktop-chromium and mobile-webkit**, driven
  the way a user reaches it (Tab in from the title field, then arrow). This is
  the test that caught the Radix failure and it stays.
- Test-order pollution found while writing the jsdom guards: `"submits the
  selected opencode agent"` mocks a successful start, so `savePrefs` writes
  `opencode` to `localStorage` and later tests in the same describe no longer
  start on Codex. The two new tests clear `localStorage` explicitly. Pre-existing
  fragility, not introduced here — worth a broader isolation pass someday.

# TDD implementation packet — slice 2b round 1: testing-library remediation (small files)

PACKET_STATUS: READY
TEST_FIRST: NOT_APPLICABLE (this is a test-only refactor; the existing suite is the test)
PRODUCTION_EDIT: FORBIDDEN

## Task contract

Convert testing-library query usage in **seven** test files to accessible,
`screen`-based queries so the six `testing-library/*` rules can be re-enabled as
errors. Behavior of every test must be unchanged.

This is round 1 of 4. Do not touch files belonging to later rounds.

## Allowed files

- `crates/ajax-web/web/src/components/ActionBar.test.tsx` (28 violations)
- `crates/ajax-web/web/src/components/ConnectionStatus.test.tsx` (17)
- `crates/ajax-web/web/src/components/ResultPanel.test.tsx` (13)
- `crates/ajax-web/web/src/components/Skeleton.test.tsx` (9)
- `crates/ajax-web/web/src/react/useSheetDrag.test.tsx` (6)
- `crates/ajax-web/web/src/react/useSwipeReveal.test.tsx` (6)
- `crates/ajax-web/web/src/components/TestInDevPanel.test.tsx` (3)

## Forbidden changes

- **Do not edit any production source.** No `.tsx` or `.ts` file that is not a
  `*.test.tsx` in the Allowed list. If a test cannot be converted without a
  production change (e.g. a control has no accessible name), leave that call
  site alone and report it — do not add an ARIA attribute to make a lint rule
  happy.
- **Do not edit `eslint.config.mjs`.** Re-enabling the rules is the parent's
  final step after all four rounds.
- Do not touch `App.test.tsx`, `TaskList.test.tsx`, `NewTaskSheet.test.tsx`,
  `TaskDetail.test.tsx`, `SettingsView.test.tsx` — later rounds.
- Do not touch anything under `e2e/` (Playwright; testing-library rules do not
  and must not apply there).

## Non-negotiable: assertion preservation

This repo's `AGENTS.md` forbids weakening tests. Concretely, in this task:

- **Do not delete a single test.**
- **Do not rename a single test.** `it(...)` / `describe(...)` strings are
  immutable in this round.
- **Do not delete or relax an assertion.** Every `expect(...)` must survive with
  the same matcher and the same expected value.
- **Do not convert a positive assertion to a negative one** or vice versa.
- **Do not add `.skip`, `.todo`, `.only`, or conditional expectation.**
- Change only *how an element is located*, never *what is asserted about it*.

If a conversion would change what is asserted, leave the call site unconverted
and list it in `REMAINING_RISKS`. An honest partial conversion is the correct
outcome; a complete conversion that alters semantics is a failure.

## Conversion patterns

Apply these mechanical substitutions:

| Rule | From | To |
| --- | --- | --- |
| `prefer-screen-queries` | `const { getByRole } = render(...)`, then `getByRole(...)` | `render(...)`, then `screen.getByRole(...)` |
| `no-container` | `container.querySelector(".pill")` | `screen.getByRole("button", { name: ... })` |
| `no-node-access` | `el.parentElement`, `el.firstChild`, `.children[0]` | an accessible query for the intended element |
| `no-await-sync-events` | `await fireEvent.click(el)` | `fireEvent.click(el)` (drop `await`; it is synchronous) |
| `prefer-presence-queries` | `expect(queryByX(...)).toBeInTheDocument()` | `expect(getByX(...)).toBeInTheDocument()` |
| `no-wait-for-multiple-assertions` | multiple `expect`s inside one `waitFor` | one `expect` in `waitFor`, remaining `expect`s after it |

Import `screen` from `@testing-library/react`.

### Query preference order

Prefer, in order: `getByRole` with an accessible name → `getByLabelText` →
`getByText` → `getByTestId`. Use `data-testid` only when no accessible query
exists; the element's existing `data-testid` is already there in most cases.

### Where a class selector carries meaning

Some assertions intentionally check a **styling contract**, e.g.
`container.querySelector(".pill.is-danger")` proving the destructive variant is
applied. Do **not** convert those into role queries that drop the class check.
Convert the *lookup* to an accessible query and keep the class assertion:

```ts
// before
expect(container.querySelector(".pill.is-danger")).toBeTruthy();
// after
expect(screen.getByRole("button", { name: "Drop" })).toHaveClass("is-danger");
```

If the element has no accessible name, leave it and report it.

## Verification commands

Run all; record exact exit codes and excerpts:

```bash
npm run web:test -- --run
npm run web:check
```

`npm run web:test -- --run` must report **exactly 325 passing tests in 37
files** — the same numbers as before your change. A different count in either
direction is a failure, including a higher one.

Then prove the rules are satisfied for your seven files only:

```bash
npx eslint --config crates/ajax-web/web/eslint.config.mjs \
  --rule '{"testing-library/prefer-screen-queries":"error","testing-library/no-container":"error","testing-library/no-node-access":"error","testing-library/no-await-sync-events":"error","testing-library/prefer-presence-queries":"error","testing-library/no-wait-for-multiple-assertions":"error"}' \
  crates/ajax-web/web/src/components/ActionBar.test.tsx \
  crates/ajax-web/web/src/components/ConnectionStatus.test.tsx \
  crates/ajax-web/web/src/components/ResultPanel.test.tsx \
  crates/ajax-web/web/src/components/Skeleton.test.tsx \
  crates/ajax-web/web/src/react/useSheetDrag.test.tsx \
  crates/ajax-web/web/src/react/useSwipeReveal.test.tsx \
  crates/ajax-web/web/src/components/TestInDevPanel.test.tsx
```

Target: zero errors. If some call sites are legitimately unconvertible per the
rules above, report the exact remaining count and each reason.

## Stop conditions

- A conversion would require editing production source.
- The test count changes from 325.
- Any test name would have to change.
- You cannot convert a site without altering what is asserted (leave it; report it).
- The patch would exceed roughly 400 changed lines.

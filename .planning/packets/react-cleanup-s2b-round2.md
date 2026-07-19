# TDD implementation packet — slice 2b round 2: testing-library remediation (TaskList, SettingsView)

PACKET_STATUS: READY
TEST_FIRST: NOT_APPLICABLE (test-only refactor; the existing suite is the test)
PRODUCTION_EDIT: FORBIDDEN

## Task contract

Convert testing-library query usage in **two** test files to accessible,
`screen`-based queries. Behavior of every test must be unchanged.

Round 2 of 4. 86 violations total.

## Allowed files

- `crates/ajax-web/web/src/components/TaskList.test.tsx` (62 violations)
- `crates/ajax-web/web/src/components/SettingsView.test.tsx` (24 violations)

## Forbidden changes

- **Do not edit any production source.** Only the two test files above.
- **Do not edit `eslint.config.mjs`.**
- Do not touch `App.test.tsx`, `NewTaskSheet.test.tsx`, `TaskDetail.test.tsx` — later rounds.
- Do not touch anything under `e2e/`.
- **Never redirect command output into `/tmp`.** Your sandbox auto-rejects
  writes to `/tmp/*` and the rejected tool call will kill your round. Let
  commands print to stdout, or write inside the repo.

## Non-negotiable: assertion preservation

- Do not delete, rename, skip, or reorder a single test. `it(...)` /
  `describe(...)` strings are immutable.
- Do not delete or relax an assertion. Every `expect(...)` keeps its matcher and
  expected value.
- Do not add `.skip`, `.todo`, `.only`, or conditional expectation.
- Change only *how an element is located*, never *what is asserted about it*.

## Defects found in round 1 — do not repeat these

Round 1 review caught three bad conversions. Each is banned:

1. **Do not drop an attribute contract.** `container.querySelectorAll("button[data-action]")`
   was converted to `screen.getAllByRole("button")`, which no longer proves the
   buttons carry `data-action`. If the original selector asserts an attribute or
   class, the replacement must still assert it:
   `expect(screen.getByRole("button", { name: "Drop" })).toHaveClass("is-danger")`.

2. **Do not write tautological assertions.**
   `expect(screen.getByText("logs here").textContent).toContain("logs here")`
   cannot fail — `getByText` located the element *by* that text. If the original
   asserted text lives inside a specific element, assert that:
   `expect(screen.getByText("logs here")).toHaveClass("result-output")`.

3. **Do not use `getAllByRole("generic", { hidden: true })` filtered by class.**
   That walks every div in the document and is strictly worse than the container
   query it replaces. If an element is `aria-hidden`, roleless, or otherwise not
   in the accessibility tree, **leave the original query alone and report it** —
   a partial conversion is the correct outcome.

**Never add a role, `aria-label`, or `data-testid` to production markup to make
a lint rule pass.** That is a production behavior change disguised as a test fix.

## Conversion patterns

| Rule | From | To |
| --- | --- | --- |
| `prefer-screen-queries` | `const { getByRole } = render(...)` | `render(...)` then `screen.getByRole(...)` |
| `no-container` | `container.querySelector(".pill")` | accessible query, preserving any class/attribute assertion |
| `no-node-access` | `el.parentElement`, `.children[0]` | an accessible query for the intended element |
| `no-await-sync-events` | `await fireEvent.click(el)` | `fireEvent.click(el)` |
| `prefer-presence-queries` | `expect(queryByX(...)).toBeInTheDocument()` | `expect(getByX(...)).toBeInTheDocument()` |
| `no-wait-for-multiple-assertions` | several `expect`s in one `waitFor` | one `expect` inside, the rest after |

Import `screen` from `@testing-library/react`. Query preference: `getByRole`
with an accessible name → `getByLabelText` → `getByText` → `getByTestId`.

## Verification commands

Run these exactly. Do not redirect to `/tmp`.

```bash
npm run web:test -- --run
npm run web:check
npx eslint --config crates/ajax-web/web/eslint.config.mjs --rule '{"testing-library/prefer-screen-queries":"error","testing-library/no-container":"error","testing-library/no-node-access":"error","testing-library/no-await-sync-events":"error","testing-library/prefer-presence-queries":"error","testing-library/no-wait-for-multiple-assertions":"error"}' crates/ajax-web/web/src/components/TaskList.test.tsx crates/ajax-web/web/src/components/SettingsView.test.tsx
```

`npm run web:test -- --run` must report **exactly 325 passing tests in 37 files**
— identical to before your change. Any other number is a failure, including higher.

Target: zero eslint errors. Report any site you legitimately could not convert,
with the reason.

## Stop conditions

- A conversion would require editing production source.
- The test count changes from 325.
- Any test name would have to change.
- You cannot convert a site without altering what is asserted (leave it; report it).
- The patch would exceed roughly 400 changed lines.

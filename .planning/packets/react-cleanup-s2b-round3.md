# TDD implementation packet — slice 2b round 3: TaskDetail

PACKET_STATUS: READY
TEST_FIRST: NOT_APPLICABLE (test-only refactor; the existing suite is the test)
PRODUCTION_EDIT: FORBIDDEN

## Task contract

Convert testing-library query usage in **one** file to accessible, `screen`-based
queries. Behavior of every test must be unchanged.

Round 3 of 4. 33 violations.

## Allowed files

- `crates/ajax-web/web/src/components/TaskDetail.test.tsx`

## Forbidden changes

- **Do not edit any production source.** Only the test file above.
- Do not edit `eslint.config.mjs`.
- Do not touch `NewTaskSheet.test.tsx` — deliberately out of scope; that
  component is being rewritten onto shadcn Dialog in a later slice and its tests
  will be rewritten against the new structure.
- Do not touch `App.test.tsx` — round 4.
- Do not touch anything under `e2e/`.
- **Never redirect command output into `/tmp`.** Writes to `/tmp/*` are
  auto-rejected in this environment and the rejected call will kill your round.

## Non-negotiable: assertion preservation

- Do not delete, rename, skip, or reorder a test. `it(...)` / `describe(...)`
  strings are immutable.
- Do not delete or relax an assertion. Every `expect(...)` keeps its matcher and
  expected value.
- **The `expect(...)` count in this file must not decrease.** It is the primary
  review check and a drop is an automatic discard.
- Change only *how an element is located*, never *what is asserted about it*.

## Semantics rule — read this carefully

The correct principle is **describe what is real; never invent semantics to
satisfy a query.**

- If an assertion checks a class, attribute, or containment, the replacement
  must still check it. Convert the *lookup*, keep the *claim*:
  `expect(screen.getByRole("button", { name: "Drop" })).toHaveClass("is-danger")`.
- Use `within(...)` for containment. It is the canonical idiom for "X is inside
  Y" and is preferred over any CSS descendant selector.
- Check whether the accessible name **already** encodes what you need. Elsewhere
  in this codebase a badge count lives in `aria-label`, making
  `getByRole("button", { name: "web — 2 need attention" })` strictly stronger
  than reading the badge span.
- If an element is genuinely not in the accessibility tree (`aria-hidden`, a
  roleless wrapper), **leave the original query and report it.** A partial
  conversion is the correct outcome.

## Banned conversions — each caused a discard in an earlier round

1. Dropping an attribute contract:
   `querySelectorAll("button[data-action]")` → `getAllByRole("button")` no longer
   proves `data-action` exists.
2. Tautological assertions:
   `expect(screen.getByText("x").textContent).toContain("x")` cannot fail.
3. `getAllByRole("generic", { hidden: true })` filtered by class — walks every
   div in the document; strictly worse than the query it replaces.
4. Replacing a negative assertion with an unrelated positive one. If the original
   asserted something is **absent**, the replacement must still assert absence
   (`queryBy...` + `toBeNull()` / `not.toBeInTheDocument()`).

## Conversion patterns

| Rule | From | To |
| --- | --- | --- |
| `prefer-screen-queries` | `const { getByRole } = render(...)` | `render(...)` then `screen.getByRole(...)` |
| `no-container` | `container.querySelector(".x")` | accessible query, preserving any class/attribute claim |
| `no-node-access` | `el.parentElement`, `.children[0]` | `within(...)` or an accessible query |
| `no-await-sync-events` | `await fireEvent.click(el)` | `fireEvent.click(el)` |
| `prefer-presence-queries` | `expect(queryByX(...)).toBeInTheDocument()` | `expect(getByX(...)).toBeInTheDocument()` |
| `no-wait-for-multiple-assertions` | several `expect`s in one `waitFor` | one inside, the rest after |

Import `screen` and `within` from `@testing-library/react`. Query preference:
`getByRole` with an accessible name → `getByLabelText` → `getByText` → `getByTestId`.

## Verification commands

```bash
npm run web:test -- --run
npm run web:check
npx eslint --config crates/ajax-web/web/eslint.config.mjs --rule '{"testing-library/prefer-screen-queries":"error","testing-library/no-container":"error","testing-library/no-node-access":"error","testing-library/no-await-sync-events":"error","testing-library/prefer-presence-queries":"error","testing-library/no-wait-for-multiple-assertions":"error"}' crates/ajax-web/web/src/components/TaskDetail.test.tsx
```

`npm run web:test -- --run` must report **exactly 326 passing tests in 37 files**.
Any other number is a failure, including higher.

Report any site you could not convert, with the reason.

## Stop conditions

- A conversion would require editing production source.
- The test count changes from 326.
- The `expect(...)` count in `TaskDetail.test.tsx` would decrease.
- Any test name would have to change.
- You cannot convert a site without altering what is asserted (leave it; report it).
- The patch would exceed roughly 400 changed lines.

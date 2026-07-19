# TDD implementation packet — slice 2b round 4: App.test.tsx

PACKET_STATUS: READY
TEST_FIRST: NOT_APPLICABLE (test-only refactor; the existing suite is the test)
PRODUCTION_EDIT: FORBIDDEN

## Task contract

Convert **only the mechanically safe** testing-library violations in
`App.test.tsx`. A specific list of call sites is to be left exactly as-is.

Round 4 of 4. 62 violations, of which ~52 are in scope and ~10 are explicitly
out of scope.

## Allowed files

- `crates/ajax-web/web/src/components/App.test.tsx`

## Forbidden changes

- **Do not edit production source.** Only the test file above.
- Do not edit `eslint.config.mjs`.
- Do not touch any other test file.
- Do not touch anything under `e2e/`.
- **Never redirect command output into `/tmp`** — writes there are auto-rejected
  and the rejected call will kill your round.

## Baselines — these are stop conditions

- `expect(` count in `App.test.tsx` is **107** and must not decrease.
- Whole suite is **326 passing tests in 37 files** and must stay exactly that.
- Every `it(...)` / `describe(...)` string is immutable.

## IN SCOPE — convert these

**1. `prefer-screen-queries` (the bulk).** Destructured queries → `screen`:

```ts
const { getByText } = render(<App />);   →   render(<App />);
getByText("x")                           →   screen.getByText("x")
```

**2. `no-await-sync-events`.** `await fireEvent.click(el)` → `fireEvent.click(el)`.
`fireEvent` is synchronous; the `await` is meaningless.

**3. testid lookups — exactly equivalent, safe.**

```ts
container.querySelector("[data-testid='app-viewport']")
→ screen.getByTestId("app-viewport")

expect(container.querySelector("[data-testid='x']")).toBeInTheDocument()
→ expect(screen.getByTestId("x")).toBeInTheDocument()
```

For the negated form use `queryByTestId`:
```ts
expect(container.querySelector("[data-testid='x']")).toBeNull()
→ expect(screen.queryByTestId("x")).toBeNull()
```

There are 8 such sites. `getByTestId` is testing-library's own API — this
conversion cannot change what is asserted.

## OUT OF SCOPE — leave these EXACTLY as they are

Do **not** convert, reword, or "improve" any of the following. They are
structural contracts with no accessible equivalent, and earlier rounds were
discarded for retargeting exactly this kind of assertion.

```ts
container.querySelector("[data-outlet='dashboard']")
container.querySelector("[data-outlet='settings']")
container.querySelector("[data-bottom-action='new-task']")
container.querySelector<HTMLButtonElement>("[data-bottom-route='#/']")
container.querySelector(".update-banner")
container.querySelector(".empty")
container.querySelector(".connection-status")
container.querySelector(".bottom-nav")
```

Reasons, so you do not "fix" them anyway:

- `data-outlet` / `data-bottom-route` / `data-bottom-action` are **route and
  navigation ownership markers**. The cases asserting them are named for those
  hooks. Replacing them with a role query passes while no longer testing the
  contract.
- **`.update-banner` is a trap.** That element is rendered with the `hidden`
  attribute and the test asserts `banner.hidden === true`. `getByRole` excludes
  hidden elements from the accessibility tree, so a role-query conversion will
  either throw or silently need `{ hidden: true }`, changing the meaning.
- `.empty`, `.connection-status`, `.bottom-nav` are structural/styling
  assertions on roleless wrappers.

Keeping these is **correct and expected**. The parent will exempt them in the
lint config. Do not report them as failures.

## Also leave alone

Assertions that match against source text rather than the DOM — e.g.
`expect(appSource).toMatch(...)`, `expect(stylesSource).toMatch(...)`,
`loadStylesSource()`. These are CSS/source contracts, not queries.

## Absolute rules

- Change only *how* an element is located — never *what* is asserted about it,
  and **never which element is located**. The converted call must resolve to the
  same DOM node.
- Every case must still test what its name claims. If a conversion would make
  the `it(...)` string inaccurate, do not make it.
- Never replace a negative assertion (`toBeNull`, `not.toBeInTheDocument`) with
  a positive one.
- Never write a tautological assertion such as
  `expect(screen.getByText("x").textContent).toContain("x")`.
- Never add a role, `aria-label`, or `data-testid` to production to make a query
  work.

## Verification commands

```bash
npm run web:test -- --run
npm run web:check
```

`npm run web:test -- --run` must report **exactly 326 passing tests in 37 files**.

Then confirm only the out-of-scope sites remain:

```bash
npx eslint --config crates/ajax-web/web/eslint.config.mjs --rule '{"testing-library/prefer-screen-queries":"error","testing-library/no-container":"error","testing-library/no-node-access":"error","testing-library/no-await-sync-events":"error","testing-library/prefer-presence-queries":"error","testing-library/no-wait-for-multiple-assertions":"error"}' crates/ajax-web/web/src/components/App.test.tsx
```

Expect roughly **10 remaining errors**, all on the OUT OF SCOPE list. Zero
`prefer-screen-queries` and zero `no-await-sync-events` should remain. Report
the remaining count and confirm each is on that list.

## Stop conditions

- The `expect(` count in `App.test.tsx` drops below 107.
- The suite count changes from 326.
- Any test name would have to change.
- A conversion would require editing production source.
- You cannot convert a site without changing which element is located.
- The patch would exceed roughly 400 changed lines.

# TDD Implementation Packet: Test in Dev into Task details

PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

For ajax-cli tasks only: show the Test in Dev control inside the Task details disclosure (not as a always-visible page section), and permanently remove the Open Dev pill plus its `window.open` client behavior. Keep deploy/poll/error behavior of Test in Dev unchanged.

## Allowed files

- `crates/ajax-web/web/src/features/task/TaskDetail.tsx`
- `crates/ajax-web/web/src/features/task/TaskDetail.test.tsx`
- `crates/ajax-web/web/src/features/task/TestInDevPanel.tsx`
- `crates/ajax-web/web/src/features/task/TestInDevPanel.test.tsx`
- `crates/ajax-web/web/src/styles.css`

## Forbidden changes

- Backend / Rust (`crates/ajax-web/src/slices/dev_deploy.rs` and related API).
- Removing or renaming the Test in Dev deploy/poll flow.
- Changing ActionBar, TaskTerminal, non-ajax-cli detail rendering.
- Commits, pushes, branch changes, lockfile churn, `web/dist` rebuilds unless a test requires them (prefer not).
- Drive-by refactors, renames, formatting sweeps outside the edit anchors.

## Context evidence

- Desired behavior: User wants Test in Dev moved into the Task details dropdown page; delete Open Dev pill + functionality.
- Placement today: `TaskDetail.tsx` renders `<TestInDevPanel>` between the terminal and `<details className="meta-details">` when `detail.repo === "ajax-cli"`.
- Open Dev today: `TestInDevPanel.tsx` hardcodes `OPEN_URL = "https://ajaxdev.mossyhome.net:8788"` and a second Button `data-testid="open-dev-button"` that calls `window.open`.
- Placement test today (must invert): `TaskDetail.test.tsx` asserts Test in Dev is **not** inside `getByRole("group")` (the details disclosure) and is a sibling region with Task terminal.
- Panel test today: `TestInDevPanel.test.tsx` `"shows ready state and fixed Open Dev URL"` clicks `open-dev-button` and asserts `window.open`.

## Code anchors

- `TaskDetail.tsx` ~L94–96: conditional `TestInDevPanel` before `<details>`.
- `TaskDetail.tsx` ~L98–102: `<details className="meta-details">` / `<summary>Task details</summary>`.
- `TestInDevPanel.tsx` L6: `const OPEN_URL = ...`
- `TestInDevPanel.tsx` L56–58: `function openDev()`
- `TestInDevPanel.tsx` L84–91: Open Dev `<Button data-testid="open-dev-button" ...>`
- `TaskDetail.test.tsx` L253–281: placement test title and assertions.
- `TestInDevPanel.test.tsx` L26–57: Open Dev URL test.
- Optional CSS: `.test-in-dev { margin: 0 0 12px; }` in `styles.css` (~L1713) — reduce bottom margin if nested spacing looks wrong; keep single-button layout working with existing `.test-in-dev .actions`.

## Test-first instructions

1. In `TaskDetail.test.tsx`, rewrite the ajax-cli Test in Dev placement test so that after render:
   - `screen.getByRole("region", { name: "Test in Dev" })` is present
   - it is found **inside** the Task details disclosure: `within(screen.getByRole("group")).getByRole("region", { name: "Test in Dev" })`
   - the top-level regions list no longer requires a sibling `Test in Dev` next to Task terminal (only Task terminal as a top-level region, or assert Test in Dev is not outside the group)
   - Update the test title to say it lives inside Task details (not on the always-visible page).
2. In `TestInDevPanel.test.tsx`:
   - Rename/rewrite `"shows ready state and fixed Open Dev URL"` to assert ready Test in Dev button only.
   - Assert `screen.queryByTestId("open-dev-button")` is null.
   - Remove `window.open` stub usage for Open Dev (ok to drop `vi.stubGlobal("open", ...)` if unused).
3. Red command:

```bash
npm run web:test -- crates/ajax-web/web/src/features/task/TaskDetail.test.tsx crates/ajax-web/web/src/features/task/TestInDevPanel.test.tsx
```

Expect nonzero exit with failures on the new placement / missing Open Dev assertions.

## Edit instructions

1. `TaskDetail.tsx`: remove the standalone `{detail.repo === "ajax-cli" ? <TestInDevPanel .../> : null}` block before `<details>`. Render that same conditional as the first child inside `<details>` after `<summary>Task details</summary>` (before the Branch meta group). Keep `taskHandle` / `onResult` props identical.
2. `TestInDevPanel.tsx`: delete `OPEN_URL`, `openDev`, and the Open Dev `<Button>`. Leave a single Test in Dev button (and error `<pre>`). Simplify markup only if the empty wrapper is silly — prefer keeping `section.test-in-dev` / `data-testid="test-in-dev"` / aria-label.
3. `styles.css`: only if needed for nested spacing (e.g. smaller bottom margin inside details). Do not restyle unrelated chrome.

## Verification commands

```bash
npm run web:test -- crates/ajax-web/web/src/features/task/TaskDetail.test.tsx crates/ajax-web/web/src/features/task/TestInDevPanel.test.tsx
```

## Acceptance criteria

- ajax-cli task detail shows Test in Dev only inside the Task details disclosure.
- Open Dev pill / `open-dev-button` / client `window.open` to the hardcoded URL are gone.
- Non-ajax-cli tasks still have no Test in Dev panel.
- Deploy busy/disabled/error behavior of Test in Dev still works (existing panel test).
- Focused web tests above exit 0.

## Stop conditions

- Need to change Rust/API to complete the request.
- Diff exceeds ~400 lines or touches files outside Allowed files.
- Unrelated test failures dominate; stop and report.
- Ambiguity about whether details must auto-open for Test in Dev (default: leave collapsed; do not force `open`).

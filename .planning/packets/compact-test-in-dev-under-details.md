PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Compact the ajax-cli **Test in Dev** control and move it under the Task details dropdown. Remove the large card, explanatory note, and occupant metadata dump. Keep deploy, Open Dev, phase label, and error behavior.

## Allowed files

- `crates/ajax-web/web/src/components/TestInDevPanel.svelte`
- `crates/ajax-web/web/src/components/TestInDevPanel.test.ts`
- `crates/ajax-web/web/src/components/TaskDetail.svelte`
- `crates/ajax-web/web/src/components/TaskDetail.test.ts`
- `.planning/agent-plans/compact-test-in-dev-under-details.md` (checklist only)

## Forbidden changes

- No React migration / S2 work
- No API, types, deploy backend, or Open Dev URL changes
- No edits to `TaskTerminal.svelte`, `ActionBar.svelte`, frozen modules, e2e assertions, or Rust
- Do not commit, push, merge, rebase, or change branches

## Context evidence

- Desired behavior: Matt — panel is "way too big" / "useless info"; put under Task details dropdown (`meta-details`).
- Placement today: `TaskDetail.svelte` lines 67–69 render `<TestInDevPanel>` inside `.interact-panel` for `detail.repo === "ajax-cli"`.
- Dropdown: `TaskDetail.svelte` lines 74–143 `<details class="meta-details">` with summary `Task details`.
- Useless chrome: `TestInDevPanel.svelte` note (lines 79–82), occupant `<dl>` (102–114), card styles (121–127), large `.pill` min-height 44px (165–175).
- Keep: `test-in-dev-button`, `open-dev-button`, `test-in-dev-phase`, `test-in-dev-error`, poll/deploy logic, `OPEN_URL`.
- Existing tests: `TestInDevPanel.test.ts` asserts phase, button enable/disable, Open Dev URL, startDeploy call. No placement test in `TaskDetail.test.ts` yet.

## Code anchors

- Move: `TaskDetail.svelte` — remove lines 67–69 from interact-panel; insert panel inside `meta-details` after summary (or after Branch group), gated by `detail.repo === "ajax-cli"`, with a compact group label e.g. `Dev`.
- Compact: `TestInDevPanel.svelte` — delete `.note` paragraph and `.occupant` block; drop card padding/border/h2 chrome; size action buttons like `.meta-copy` (min-height ~28px, small uppercase pills). Keep `data-testid="test-in-dev"`, phase, actions, error.
- Tests: update `TestInDevPanel.test.ts` so it does not require note/occupant; add `TaskDetail` test with `repo: "ajax-cli"` + mocked `../api` `fetchDevDeploy` that finds `test-in-dev` inside `.meta-details` and not inside `[data-mobile-chrome='actions']`.

## Test-first instructions

1. Update/add failing tests before production edits:
   - `TestInDevPanel.test.ts`: assert `queryByText(/Shared Ajax Dev slot/)` is null; assert `queryByTestId("test-in-dev-occupant")` is null when occupant is present in mock; keep phase/button/Open Dev/building disable assertions.
   - `TaskDetail.test.ts`: for `detail({ repo: "ajax-cli", qualified_handle: "ajax-cli/demo" })` with `fetchDevDeploy` mocked ready, expect `container.querySelector(".meta-details [data-testid='test-in-dev']")` present and `container.querySelector("[data-mobile-chrome='actions'] [data-testid='test-in-dev']")` null.
2. Red command:
   ```bash
   npm run web:test -- --run crates/ajax-web/web/src/components/TestInDevPanel.test.ts crates/ajax-web/web/src/components/TaskDetail.test.ts
   ```
3. Record RED evidence, then implement.

## Edit instructions

1. `TaskDetail.svelte`: cut `<TestInDevPanel …>` from interact-panel; paste inside `<details class="meta-details">` (after `<summary>Task details</summary>`), wrapped with `{#if detail.repo === "ajax-cli"}` and a `meta-group-label` `Dev`.
2. `TestInDevPanel.svelte`: remove note + occupant markup/styles; remove h2 card head (keep phase via `data-testid="test-in-dev-phase"`); compact CSS — no heavy card; buttons ~meta-copy size; preserve deploy/open/poll/error behavior and testids.
3. Make tests green with smallest diff.

## Verification commands

```bash
npm run web:test -- --run crates/ajax-web/web/src/components/TestInDevPanel.test.ts crates/ajax-web/web/src/components/TaskDetail.test.ts
npm run web:check
```

## Acceptance criteria

- Test in Dev only appears under Task details for ajax-cli tasks.
- No shared-slot note; no occupant metadata dump.
- Phase, Test in Dev button, Open Dev, and error still work; building disables the button.
- Focused tests + `web:check` pass.
- Diff limited to allowed files.

## Stop conditions

- Need to weaken e2e or change deploy API.
- Diff escapes allowed files.
- S1/S2 migration files get touched.

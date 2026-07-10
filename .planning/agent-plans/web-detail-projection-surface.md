# Web: surface the full task-detail projection

## Scope

`BrowserTaskDetail` already ships `runtime_observation_error`, `agent_activity`,
`live_status_summary`, `annotations`, `created_unix_secs`,
`last_activity_unix_secs`, and `agent_attempts` — the browser fetches them and
renders none. Surface them in `TaskDetail.svelte` with small pure helpers in
`state.ts`. Frontend-only; no DTO, API, or Rust changes.

## Non-goals

- No card-level timestamps (would require an `ajax-core` TaskCard change).
- No changes to terminal model, polling, or actions.
- No mobile meta-details reveal (terminal-first mobile layout stays; only the
  observation-error warning is mobile-visible).

## Delegation decision

`Delegation decision: not delegated` — scope emerged from vague discovery
("go crazy"), which is on the do-not-delegate list; the user directly pointed
this session at ajax web and asked it to make the changes.

## Checklist

- [x] Failing tests: `relativeTime` / `formatDuration` in `state.test.ts`
- [x] Failing tests: TaskDetail renders observation error, agent activity,
      activity times, attempts, annotations (`TaskDetail.test.ts`)
- [x] Implement `relativeTime` + `formatDuration` in `state.ts`
- [x] Implement TaskDetail rendering + styles
- [x] `npm run web:check`
- [x] Focused vitest: state + TaskDetail (+ full `npm run web:test -- --run`)
- [x] `npm run web:build` (dist is committed) + affected Rust snapshot tests
      (`cargo test -p ajax-web install`, `cargo nextest -p ajax-cli -E 'test(web_backend)'`)

## Deviations

- Annotations test initially asserted absence in the same test as presence;
  both renders share `document.body`, so the absence check moved to its own test.
- Fresh worktree needed `npm install` before vitest could run.

## Validation

- Failing-first: 10 new tests failed for the expected reasons (missing exports
  / testids), then passed after implementation.
- `npm run web:test -- <state,TaskDetail> --run` — 35 passed
- `npm run web:check` — 0 errors, 0 warnings (164 files)
- `npm run web:test -- --run` — 34 files / 448 tests passed
- `npm run web:build` — dist rebuilt (committed asset)
- `cargo test -p ajax-web install` — 7 passed
- `cargo nextest run -p ajax-cli -E 'test(web_backend)'` — 19 passed
- Not run: `npm run web:smoke` (Playwright) and a live in-browser drive of the
  detail page — rendering is covered by component tests; worth a visual pass
  before merge.

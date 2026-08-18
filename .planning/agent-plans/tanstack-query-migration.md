# TanStack Query Migration

## Approval

- Status: implementation complete.
- Delivery: two sequential implementation phases, with Phase 1 green before Phase 2 starts.

## Current baseline

- Vite already builds and serves the React frontend through
  `crates/ajax-web/web/vite.config.mts`; this plan does not migrate bundlers.
- Hash navigation already lives in `useHashRoute`, `routes.ts`, and `App.tsx`.
- `@tanstack/react-query` is not currently installed.
- The shared API module already owns same-origin transport, session renewal,
  timeouts, response validation, and typed `ApiError` mapping. Query must wrap
  those functions, not replace that boundary.
- `useCockpitResource` has custom generation guards, mutation-projection
  ordering, gesture deferral, adaptive polling, stale-data behavior, and
  connection-state mapping.
- Task detail, Diff Review, model catalogs, version reads, and Test-in-Dev
  status currently implement ordinary read lifecycles with local effects and
  state.
- Task creation, harness swap, and task operations currently call the API
  imperatively. Operation responses can contain the latest authoritative
  Cockpit projection.

## Scope

### Phase 1

- Add TanStack Query to the existing Vite/React application.
- Move ordinary rendered HTTP reads to Query:
  - task detail;
  - task pull-request list and selected/local diff;
  - per-harness session model catalogs;
  - version metadata used by Settings and the update monitor;
  - Test-in-Dev deployment status.
- Retain existing UI contracts such as `RemoteResource`, loading copy, stale
  detail display, typed errors, and connection banners where they prevent
  broad component churn.

### Phase 2

- Move task creation, harness swaps, and `/api/operations` calls to
  `useMutation`.
- Include resume-on-open because it is an `/api/operations` mutation.
- Continue applying mutation-returned Cockpit projections through
  `useCockpitResource.applyCockpit`.
- Invalidate/refetch only affected ordinary Query reads, primarily task detail.

## Non-goals

- No TanStack Router migration.
- No TanStack Start.
- No replacement of the existing hash parser, hash writers, swipe navigation,
  or route-transition telemetry.
- No Query ownership of Cockpit polling or Cockpit projection state.
- No changes to drop confirmation, delayed commit, undo, route-leave latch, or
  operation telemetry.
- No Query ownership of ACP/WebSocket session reducers, transcript cursors,
  prompt outbox, model-setting frames, or permission state.
- No Query ownership of xterm, PTY/WebSocket, speech, viewport, or terminal
  connection state.
- No Query persistence, hydration, devtools, IndexedDB, service worker, offline
  cache, background mutation replay, or optimistic task model.
- No migration of imperative control-flow reads: browser-session renewal,
  push-subscription setup, diagnostics snapshots, health/restart loops, and
  terminal/session handshakes remain ordinary functions.
- No migration of unrelated mutations such as restart, push subscription/test,
  or Test-in-Dev start in Phase 2.

## Design constraints

1. Add one application `QueryClientProvider` at `main.tsx`. Keep one small
   query module for the client and stable key builders; do not add a state
   framework or feature-wide repository layer.
2. Configure behavior-preserving defaults: no automatic retries, no focus or
   reconnect refetch, no persistence, and no mutation retries. Existing shell
   recovery and adaptive timers remain explicit.
3. Keep `api.ts` as the transport/validation boundary. Query functions call the
   existing typed API functions.
4. Keep the Query cache transient and presentation-only. Backend/core
   projections remain authoritative.
5. Do not put `/api/cockpit` in the Query cache. Mutation responses carrying a
   Cockpit projection call `applyCockpit` directly; the custom poll gate remains
   responsible for preventing stale poll overwrite.
6. Do not use optimistic updates. Mutation responses and subsequent focused
   refetches provide the authoritative state.
7. Keep mutation `request_id` generation at invocation time and disable
   retries, so Query cannot replay an operation.
8. Preserve typed non-success mutation results. `MutationResult.ok === false`
   remains a handled domain outcome; do not globally convert it into a thrown
   exception.
9. Key task data by qualified handle and diff data by exact source:
   - task detail: handle;
   - pull requests: handle;
   - diff: handle plus selected PR number or local;
   - model catalog: normalized harness;
   - version and Test-in-Dev status: singleton keys.
10. Never use previous task data as placeholder data for another handle.
11. Keep the version monitor's caller-owned cadence and boot-version pinning.
    Query may deduplicate/cache `fetchVersion`, but the first successful
    `checkVersion` still establishes the boot version.
12. Test-in-Dev may use a Query `refetchInterval` only while the returned
    deployment status is active. Starting a deployment remains imperative in
    this scope and seeds or invalidates that status query.

## Phase 1 checklist — ordinary reads

- [x] 1. Establish the Query boundary.
  - Add the current `@tanstack/react-query` dependency through npm so
    `package.json` and `package-lock.json` stay aligned.
  - Add the configured `QueryClient`, provider wiring, and isolated test-client
    helper.
  - Ensure every test gets a fresh client with retries disabled so cache state
    cannot leak between tests.
  - Preserve `StrictMode` and `ErrorBoundary` placement.

- [x] 2. Migrate task detail reads without changing resume semantics.
  - Replace the manual detail fetch generation/state machinery with a
    handle-keyed query behind `useTaskDetailResource`.
  - Preserve `RemoteResource` mapping: initial loading, ready, stale data after
    refetch failure, and hard error without data.
  - Preserve 404 as a task-detail error rather than a Cockpit disconnect.
  - Preserve connection recovery on successful detail reads.
  - Keep resume-on-open imperative in Phase 1 and at most once per continuously
    open handle; successful resume refetches the current detail.
  - Retain tests for same-handle races, handle A to B races, stale data, retry,
    and referentially stable reload.

- [x] 3. Migrate Diff Review's dependent reads.
  - Query the pull-request list first.
  - After that query settles, load the exact selected PR, first available PR, or
    local diff under a source-specific key.
  - Preserve the two loading phases, local-diff fallback when PR discovery
    fails, the PR warning, judgment validation, and current hash-selected PR.
  - Do not move selection, file expansion, swipe behavior, or hash navigation
    into Query.

- [x] 4. Migrate the remaining ordinary rendered reads.
  - Share the per-harness model catalog query between `ModelPicker` and
    `SessionModelSelect`, preserving current fallback catalogs and `onCatalog`
    behavior.
  - Share version reads between Settings and the update monitor without
    changing boot-version comparison or adaptive scheduling.
  - Move Test-in-Dev status to Query and poll only while `deploy.active` is true;
    preserve transient-error behavior and the existing start latch.
  - Leave VAPID setup, diagnostics, health/restart, Cockpit, ACP, and terminal
    reads outside Query.

- [x] 5. Document the frontend ownership boundary.
  - Update `docs/architecture/web-cockpit.md` to state that TanStack Query owns
    transient in-memory ordinary HTTP read state and selected mutation
    lifecycles only.
  - Record that Cockpit polling/projection ordering, hash navigation,
    ACP/WebSocket session state, and terminal state remain custom.

- [x] 6. Verify Phase 1 before Phase 2.
  - Query-provider bootstrap works in production and isolated component tests.
  - Deep links for dashboard, project, task, diff, session, and settings still
    resolve through the existing hash router.
  - Cockpit cadence, hidden-startup retry, gesture deferral, resume recovery,
    and stale-poll rejection tests pass unchanged except provider setup.
  - No extra Vite chunk violates the fixed `app.js` / `terminal.js` asset
    contract.

## Phase 2 checklist — mutations

- [x] 7. Migrate task creation.
  - Use `useMutation` for `startTask`.
  - Preserve the synchronous submit latch, validation, preferences, late
    unmount handling, typed failure copy, authoritative Cockpit projection
    application, and hash-based open-task navigation.
  - Do not persist or retry the mutation.

- [x] 8. Migrate harness swaps.
  - Use a handle-scoped mutation call for `swapTaskAgent`.
  - Keep refused swaps open with backend-provided copy.
  - On success, preserve the existing outbox-clear callback, invalidate/refetch
    task detail, and let the existing session flow reconnect to the backend-owned
    ACP slot.
  - Do not put ACP session or model-frame state in Query.

- [x] 9. Migrate task operations, including resume-on-open.
  - Use one operation mutation function around `postOperation`; keep component
    click latches and the backend's single-operation lane unchanged.
  - Apply any returned Cockpit projection before presenting success/failure.
  - On successful non-drop operations, invalidate/refetch the affected active
    task detail.
  - Keep resume-on-open at most once per continuously open handle.
  - Inject/call the operation mutation from the existing drop/confirm
    orchestration instead of moving timers or undo state into Query.
  - Preserve exact `branch_adoption` payload forwarding and request IDs.

- [x] 10. Prove custom mutation behavior survived.
  - Same-turn double taps still send one request.
  - Confirmation-required actions do not POST before confirmation.
  - Drop undo sends no request; timeout/manual commit sends exactly one.
  - Leaving a task while Drop resolves cannot navigate back incorrectly.
  - Typed operation failures retain recovery copy and telemetry kinds.
  - A mutation response cannot be overwritten by an older Cockpit poll.
  - Successful create/swap/operate paths refresh only the necessary Query reads
    while Cockpit remains custom.

- [x] 11. Final documentation and cleanup.
  - Remove only superseded effect/state code and obsolete tests; do not retain
    compatibility wrappers with no caller.
  - Update this plan with deviations and exact validation results.

## Test strategy

- Port characterization tests before removing each manual lifecycle.
- Add focused Query integration tests for:
  - isolated test clients and disabled retries;
  - task-key separation and stale same-key race handling;
  - dependent Diff Review query selection/fallback;
  - model-catalog deduplication by harness;
  - active-only Test-in-Dev polling;
  - mutation cache invalidation/refetch without Cockpit cache ownership;
  - zero mutation retry/replay;
  - resume-on-open under `StrictMode`;
  - drop/undo behavior using the Query-backed operation executor.
- Keep existing API transport tests; Query must not duplicate response parsing
  or error classification.

## Validation commands

Run focused tests after each checklist item, then at each phase gate:

```bash
npm run web:check
npm run web:lint
npm run web:sg
npm run web:test -- --run
npm run web:build
npm run web:build:check
npm run web:smoke
npm run verify:slice -- web
git status --short
```

Before a requested PR, run the repository's full local PR verification gate.

## Validation results

- `npm run web:check` — pass
- `npm run web:lint` — pass
- `npm run web:sg` — pass
- `npm run web:test -- --run` — pass (962 passed, 9 skipped)
- `npm run web:build` — pass (`app.js` + `terminal.js` chunk contract preserved)
- `npm run web:build:check` — pass
- `npm run web:smoke` — **skipped/failed**: Playwright `config.webServer` timed out after 60s in this environment
- `npm run verify:slice -- web` — pass (403 Rust tests)

## Deviations

- Task-detail reload uses `invalidateQueries` plus TanStack Query's in-flight
  deduplication; the same-handle race characterization test was updated to prime
  fresh data then assert a cancelled slow refetch cannot overwrite it.
- Mutation `onSuccess` cache invalidation is delegated to existing `onMutated` /
  `reload()` callbacks to avoid duplicate detail fetches (harness-swap regression).
- Test-in-Dev start seeds the deploy query via `setQueryData` after
  `startDevDeploy` so the UI reflects the mutation response before polling resumes.

- Query defaults can silently add retries or refetches. Stop and fix the client
  configuration if request counts change.
- Shared test clients can hide requests or leak stale data. Every test render
  must own or explicitly receive an isolated client.
- Query caching must not show task A under task B or resurrect a missing task.
- `StrictMode` must not duplicate resume, create, swap, or operation mutations.
- Stop if implementation requires moving Cockpit, ACP/WebSocket, terminal,
  drop/undo, or hash-route state into Query.
- Stop if a proposed cache requires persistence, offline replay, optimistic
  task truth, or backend contract changes.
- Stop and split the work if the Vite fixed-chunk contract requires unrelated
  asset-loader changes.

## Risks and stop conditions

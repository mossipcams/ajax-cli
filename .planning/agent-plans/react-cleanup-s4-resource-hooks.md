# Slice 4 — cockpit / version / task-detail resource hooks

Master plan: `react-migration-cleanup.md`
Depends on: slice 3 (`7dd4223`)

## Scope

Extract shell resource ownership out of `App.tsx` into focused hooks, and
replace `detail === null` with a discriminated `RemoteResource<T>` union.

## Non-goals

- No change to `polling.ts` cadences or `cockpitPoll.ts` gate/guard logic — both
  are reused as-is.
- No `RemoteResource` on terminal connection state; it has its own domain state
  machine (explicit program constraint).
- No context. Colocation first; these are one or two props deep.

## Risk assessment — why this is split into three rounds

Not equal risk, so not one dispatch:

| Round | Content | Risk | Owner |
| --- | --- | --- | --- |
| 4a | `useVersionMonitor` | low — self-contained, no shared state | delegate |
| 4b | `RemoteResource<T>` + `useCockpitResource` | medium — connection state feeds chrome | delegate |
| 4c | `useTaskDetailResource` + loading/error render states | **high** | parent |

**4c is not delegated.** It owns the stale-response guard (`taskOpenHandleRef` in
`loadDetail`) that stops a slow response for task A overwriting task B, plus
resume-on-open semantics. `App.test.tsx` covers it with "ignores a stale detail
response after switching tasks", which drives a deliberately unresolved promise
across a route change. Slice 2b showed delegates silently retarget exactly this
kind of subtle contract, and a regression here is a real user-visible data bug,
not a lint statistic.

## Behavior change — this slice is not behavior-preserving

`detail === null` currently conflates four distinct states: initial loading,
route not found, backend disconnected, and stale data. Splitting them changes
what the UI renders per state. That is the point of the slice, but it means:

- characterization tests alone are insufficient; new behaviour needs new tests
- an iPhone pass is required before the PR

```ts
type RemoteResource<T> =
  | { status: "loading"; data: null; error: null }
  | { status: "ready"; data: T; error: null }
  | { status: "stale"; data: T; error: ApiError }
  | { status: "error"; data: null; error: ApiError };
```

## Delegation decision

`Delegation decision: delegated via model-router for 4a and 4b; 4c not delegated
because it owns the stale-response guard and resume-on-open semantics.`

## Baselines

- Suite: **332 tests / 37 files**
- `verify` green at `7dd4223`

## Round 4a — `useVersionMonitor`

Move `bootVersionRef`, `checkVersion`, and `updateAvailable` into
`src/react/useVersionMonitor.ts`. Pure extraction, observable behaviour identical.

Contract to preserve exactly:
- first successful version response **pins** the boot version, does not banner
- a later differing version sets `updateAvailable` permanently
- fetch failure is swallowed (offline keeps the pinned version)
- the caller still drives cadence; the hook does not own its own interval

## Round 4b — `RemoteResource<T>` + `useCockpitResource`

Owns cockpit polling, the apply gate, the in-flight guard, and connection state.

## Round 4c — `useTaskDetailResource` (parent)

Owns detail loading, cancellation, route identity, resume-on-open, and the
`TaskSkeleton` / `TaskLoadError` render states.

## Validation

Per round: focused tests → full suite → `web:lint` → `web:check` →
mobile-webkit e2e → `verify`.

## Results

### 4a — `useVersionMonitor` (clean, `7315a58`)

Verbatim extraction; `checkVersion` stayed `useCallback`-stable; no interval
inside the hook. `App.test.tsx` unmodified. 332 → 338 tests.

### 4b — `RemoteResource` + `useCockpitResource` (`1e4a34a`) — corrected

**The most serious defect this program produced, and all 352 tests passed with
it in place.**

`loadDetail` originally ended `setConnection("connected"); setConnectionDetail(null)`.
With connection state moved into the hook, the delegate reached for
`applyCockpit(cockpit.data)` — re-applying the existing projection purely for
its side effect — and added `cockpit.data` to the dependency array.

That made `loadDetail` referentially unstable. It is a dependency of the detail
effect (`[taskOpenHandle, loadDetail, resumeOnOpen]`), so **every cockpit poll
that changed the projection re-ran the effect**, firing `setDetail(null)`
(skeleton flicker) and **another resume mutation** — every 5s on a task route,
indefinitely. The brief requires exactly one resume per open.

Tests could not catch it: the fixture is static, the apply gate suppresses
unchanged projections, so `data` identity never churns under test. In production
it changes constantly.

**Lesson: a static fixture makes referential-stability bugs structurally
invisible.** Caught by reading the dependency array against how the callback is
consumed — not by the suite.

Fix: `markConnected()` on the hook clears connection state without touching the
projection; `loadDetail` depends only on `[applyConnectionError, markConnected]`.
New regression test drives a genuinely changing projection across 15s of
task-route cadence and pins resume at 1. Verified it fails on the old code.

### 4c — `useTaskDetailResource` (clean)

Delegated at Matt's direction despite being planned as parent-owned. Mitigated
by pre-specifying **mechanisms**, not just contracts — 4b failed precisely where
the delegate had to invent one:

- the `depsRef` pattern was dictated ("this is the mechanism; do not invent another")
- `TaskLoadError` markup was supplied verbatim, reusing existing `empty`/`pill`
- five existing tests named as the proof, with "a failure means you broke the
  code, do not edit the test"

Result: stale guard present and checked after **every** await including the
resume-triggered reload; all callbacks stable; five named tests pass unmodified;
`App.test.tsx` additions only. **Guard proven** — deleting the stale check fails
"ignores a stale detail response after switching tasks"; restoring passes.

Accepted deviation from the packet status table: network errors keep the
skeleton rather than showing `TaskLoadError`, documented in-code. Correct call —
a transient blip should not replace the task view when the connection banner
already reports it. `TaskLoadError` therefore appears for HTTP failures only.
Consequence: with the backend fully down a task route still shows a skeleton
indefinitely, as before. Not a regression, but the union has not fixed that case.

## Repo anomaly during 4c — resurrected Svelte files

`TaskDetail.svelte` and `TestInDevPanel.svelte` reappeared on disk with unmerged
index entries (`DU`, stages 1 and 3) despite no merge or rebase in progress. They
were the genuine files deleted in `dd0af7e`, importing a non-existent
`ActionBar.svelte`, referenced by nothing live, and inert to tests, typecheck,
and build. Cause never established — reflog showed only this branch's commits and
the delegate log contained no git commands.

Surfaced rather than silently deleted (not parent-created), then removed on
Matt's instruction. Verified afterwards: zero `.svelte` files, zero unmerged
entries.

## Validation (4c)

| Command | Result |
| --- | --- |
| `web:test` | 40 files / **362 tests** |
| hook tests | 9 passed |
| `web:lint` / `web:check` | 0 / 0 |
| `web:smoke` mobile-webkit | 92 passed / 0 failed |

## Deviations

- 4c delegated rather than parent-owned (Matt's direction).
- Network-error status mapping keeps the skeleton (see 4c above).

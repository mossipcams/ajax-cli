# TDD implementation packet — slice 4b: RemoteResource + useCockpitResource

PACKET_STATUS: READY
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED

## Task contract

Introduce the `RemoteResource<T>` union and extract cockpit ownership out of
`App.tsx` into `useCockpitResource`.

**This round must be behaviour-preserving.** Model the states; do not change
what the user sees. The richer loading/error UI lands in round 4c against task
detail, which is where the program brief asks for it.

## Allowed files

- `crates/ajax-web/web/src/types.ts` (append `RemoteResource<T>` only)
- `crates/ajax-web/web/src/react/useCockpitResource.ts` (new)
- `crates/ajax-web/web/src/react/useCockpitResource.test.tsx` (new)
- `crates/ajax-web/web/src/components/App.tsx`

## Forbidden changes

- **No new UI, no new copy, no visual change.** Do not invent an error panel or
  empty state. Ajax's visual identity is fixed.
- Do not modify `cockpitPoll.ts`, `polling.ts`, or `api.ts` — reuse
  `createCockpitApplyGate` and `createInFlightGuard` exactly as they are.
- Do not change polling cadence or the interval effect's dependency array.
- Do not touch `ConnectionStatus.tsx`.
- Do not modify `App.test.tsx`. If you believe an existing test must change,
  **stop and report** — that means behaviour changed and this round must not.
- Do not add `eslint-disable` anywhere.
- **Never redirect command output into `/tmp`** — auto-rejected, kills the round.

## The type

Append to `types.ts`:

```ts
export type RemoteResource<T> =
  | { status: "loading"; data: null; error: null }
  | { status: "ready"; data: T; error: null }
  | { status: "stale"; data: T; error: ApiError }
  | { status: "error"; data: null; error: ApiError };
```

Import `ApiError` from `./api`. If that creates an import cycle
(`import-x/no-cycle` is an error), put the type in a new
`src/remoteResource.ts` instead and say so in your report.

## Critical coupling — read before designing the hook

`applyConnectionError` is called by **two** paths today:

1. `loadCockpit` — cockpit poll failures
2. `loadDetail` — task-detail failures (still owned by `App.tsx` this round)

The hook owns connection state, so it **must expose `applyConnectionError`** for
`loadDetail` to keep calling. If detail failures stop reaching connection state,
the "reports reachable detail HTTP failures as disconnected" and "clears detail
failure text after a later successful detail load" tests in `App.test.tsx` will
fail — that is the signal you broke it, not a reason to edit those tests.

## Target shape

```ts
export type CockpitResource = {
  cockpit: RemoteResource<BrowserCockpitView>;
  connection: ConnectionState;
  connectionDetail: string | null;
  loadCockpit: () => Promise<void>;
  applyCockpit: (next: BrowserCockpitView) => void;
  applyConnectionError: (error: unknown) => void;
};

export function useCockpitResource(): CockpitResource;
```

`loadCockpit`, `applyCockpit`, and `applyConnectionError` must all be
referentially stable (`useCallback`). `loadCockpit` is a dependency of the
interval effect in `App.tsx`; an unstable identity would recreate both intervals
every render.

## Status mapping — preserve current behaviour exactly

| Condition | status | Renders today | Must still render |
| --- | --- | --- | --- |
| no data yet, no error | `loading` | dashboard skeleton | dashboard skeleton |
| data, last poll ok | `ready` | `TaskList` | `TaskList` |
| data, last poll failed | `stale` | `TaskList` (+ error in chrome) | same |
| no data, first load failed | `error` | dashboard skeleton | **dashboard skeleton** |

The last row is deliberate. Today a failed first load shows the skeleton
indefinitely; that is a real weakness, but fixing it is a visible change and is
**out of scope for this round**. Model the state, render as before.

In `App.tsx` the dashboard branch stays equivalent to
`cockpit.data ? <TaskList … cockpit={cockpit.data} /> : <Skeleton … />`.

## Behaviour contract — preserve exactly

1. `loadCockpit` returns immediately when `document.hidden` is true.
2. Concurrent calls are collapsed by the in-flight guard (no overlap).
3. The apply gate suppresses `setCockpit` when the projection is unchanged.
4. A successful poll sets connection `"connected"` and clears the detail string.
5. `ApiError` maps: `network` → `"backend unreachable"`, `stale-session` →
   `"stale session"`, otherwise `"disconnected"`; the message becomes the detail.
6. A non-`ApiError` throw maps to `"backend unreachable"` with its message.

## Task 1 — RED

Create `useCockpitResource.test.tsx` covering contract points 1–6 and the four
status-mapping rows. Prove it fails first:

```bash
npm run web:test -- --run src/react/useCockpitResource.test.tsx
```

Record the nonzero exit as RED evidence.

## Task 2 — GREEN

Implement the hook, then rewire `App.tsx` to consume it. Remove the now-dead
`cockpit`/`connection`/`connectionDetail` state, the two refs, and the three
callbacks from `App.tsx`.

## Verification commands

```bash
npm run web:test -- --run src/react/useCockpitResource.test.tsx
npm run web:test -- --run
npm run web:check
npm run web:lint
```

- Full suite: **38+ files**, **at least 338 tests**, zero failures.
- `App.test.tsx` must pass **unmodified** — it is the behaviour-preservation
  proof for this round.
- `web:lint` exits 0 with no new suppressions.

## Stop conditions

- Any `App.test.tsx` test would need editing.
- You cannot keep the three callbacks referentially stable.
- The type creates an import cycle you cannot resolve as described.
- Total tests drop below 338, or any test fails.
- You find yourself adding UI, copy, or CSS.
- The patch would exceed roughly 400 changed lines.

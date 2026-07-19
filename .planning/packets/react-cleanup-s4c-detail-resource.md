# TDD implementation packet — slice 4c: useTaskDetailResource

PACKET_STATUS: READY
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED

## Task contract

Extract task-detail ownership from `App.tsx` into `useTaskDetailResource`, and
render the detail route from a `RemoteResource` union instead of
`detail === null`.

**This is the highest-risk round in the slice.** It owns the stale-response
guard. Read the whole packet before editing.

## Allowed files

- `crates/ajax-web/web/src/react/useTaskDetailResource.ts` (new)
- `crates/ajax-web/web/src/react/useTaskDetailResource.test.tsx` (new)
- `crates/ajax-web/web/src/components/TaskLoadError.tsx` (new)
- `crates/ajax-web/web/src/components/App.tsx`
- `crates/ajax-web/web/src/components/App.test.tsx` — **additions only**

## Forbidden changes

- Do not modify or delete any existing `App.test.tsx` test. You may **append**
  new ones. Editing an existing test means you changed behaviour that must not
  change — stop and report instead.
- Do not touch `useCockpitResource.ts`, `TaskDetail.tsx`, `api.ts`, `polling.ts`.
- Do not add new CSS or new class names. Reuse `empty` and `pill` (both exist).
- Do not add `eslint-disable` anywhere.
- **Never redirect command output into `/tmp`** — auto-rejected, kills the round.

## Current implementation — the exact code being replaced

```ts
const [detail, setDetail] = useState<BrowserTaskDetail | null>(null);
const taskOpenHandleRef = useRef(taskOpenHandle);
taskOpenHandleRef.current = taskOpenHandle;          // assigned during render

const loadDetail = useCallback(async (handle: string) => {
  try {
    const next = await fetchDetail(handle);
    if (taskOpenHandleRef.current !== handle) return;   // ← STALE GUARD
    setDetail(next);
    markConnected();
  } catch (error) {
    if (error instanceof ApiError) applyConnectionError(error);
  }
}, [applyConnectionError, markConnected]);

const resumeOnOpen = useCallback(async (handle: string): Promise<boolean> => {
  try {
    const opResult = await postOperation({
      task_handle: handle, action: "resume", request_id: requestId(),
    });
    if (opResult.ok && opResult.response.cockpit) applyCockpit(opResult.response.cockpit);
    return opResult.ok;
  } catch { return false; }
}, [applyCockpit]);

useEffect(() => {
  const handle = taskOpenHandle;
  if (!handle) { setDetail(null); return; }
  setDetail(null);
  void loadDetail(handle);
  void resumeOnOpen(handle).then((mutated) => {
    if (mutated) void loadDetail(handle);
  });
}, [taskOpenHandle, loadDetail, resumeOnOpen]);
```

## THE STALE GUARD — do not lose this

`if (taskOpenHandleRef.current !== handle) return;` stops a slow response for
task A from overwriting task B after the user navigates. Without it the user
sees another task's data under the current task's header. This is a data-
correctness bug, not a cosmetic one.

Keep the identical mechanism inside the hook: a ref holding the currently
requested handle, compared **after every await** and before any state write.
Apply the same guard to the resume-triggered reload.

`App.test.tsx` covers this with "ignores a stale detail response after switching
tasks" — it holds an unresolved promise across a route change. That test must
pass **unmodified**.

## Referential stability — this is how round 4b broke

`loadDetail` was made to depend on `cockpit.data`, which made it unstable. It is
a dependency of the detail effect, so **every cockpit poll re-ran the effect and
fired another resume mutation.**

Rules:
- Every callback the hook returns must be `useCallback`-stable.
- The hook must **not** depend on cockpit data, connection state, or the route
  object — only on the handle string and the three stable callbacks passed in.
- The effect that loads on handle change must depend on the **handle string**
  and stable callbacks only.

The regression test "does not re-resume an open task when the cockpit projection
changes" must pass **unmodified**.

## Target shape — implement exactly this

```ts
export type TaskDetailResourceDeps = {
  applyCockpit: (next: BrowserCockpitView) => void;
  applyConnectionError: (error: unknown) => void;
  markConnected: () => void;
};

export function useTaskDetailResource(
  handle: string | null,
  deps: TaskDetailResourceDeps,
): {
  detail: RemoteResource<BrowserTaskDetail>;
  reload: () => void;
};
```

`deps` is an object rebuilt every render, so **do not put `deps` in a dependency
array**. Hold it in a ref updated on every render and read `depsRef.current`
inside callbacks. This is the mechanism; do not invent another.

### Status mapping

| Situation | status | data | error |
| --- | --- | --- | --- |
| no handle (not a task route) | `loading` | null | null |
| handle set, first load in flight | `loading` | null | null |
| load succeeded | `ready` | detail | null |
| load failed, no previous detail for this handle | `error` | null | ApiError |
| load failed, detail already shown for this handle | `stale` | previous | ApiError |

Switching handles resets to `loading` — never show the previous task's data
under a new handle.

## Render in App.tsx

Replace `{detail ? <TaskDetail …/> : <Skeleton …/>}` with:

```tsx
{detail.status === "loading" ? (
  <Skeleton testid="task-skeleton" rows={6} />
) : detail.data ? (
  <TaskDetail detail={detail.data} … />
) : (
  <TaskLoadError message={detail.error.message} onRetry={reload} />
)}
```

Keep every existing `TaskDetail` prop exactly as it is today, with `onMutated`
calling `reload`.

### TaskLoadError — exact markup, no invention

```tsx
export default function TaskLoadError({
  message,
  onRetry,
}: {
  message: string;
  onRetry: () => void;
}) {
  return (
    <div data-testid="task-load-error">
      <p className="empty">Could not load this task — {message}</p>
      <button type="button" className="pill" onClick={onRetry}>
        Retry
      </button>
    </div>
  );
}
```

No CSS file changes. No other classes.

## Task 1 — RED

Create `useTaskDetailResource.test.tsx` covering:

1. all five status-mapping rows
2. a slow response for handle A is discarded after switching to handle B
3. switching handles resets to `loading` (never shows A's data under B)
4. `reload` refetches the current handle
5. every returned callback is referentially stable across re-renders

Prove failure first:

```bash
npm run web:test -- --run src/react/useTaskDetailResource.test.tsx
```

Record the nonzero exit as RED evidence.

## Task 2 — GREEN

Implement the hook, `TaskLoadError`, and rewire `App.tsx`.

Append one `App.test.tsx` test: a task route whose detail fetch rejects renders
`task-load-error`, and tapping Retry refetches.

## Verification commands

```bash
npm run web:test -- --run src/react/useTaskDetailResource.test.tsx
npm run web:test -- --run
npm run web:check
npm run web:lint
```

- Full suite: **39+ files**, **at least 352 tests**, zero failures.
- These must pass unmodified:
  - "ignores a stale detail response after switching tasks"
  - "does not re-resume an open task when the cockpit projection changes"
  - "resumes the task once when its route is entered, and re-resumes a different handle"
  - "renders task detail while the resume operation is still in flight"
  - "shows a task skeleton while a task detail is loading"

## Stop conditions

- Any existing test would need editing.
- You cannot keep the returned callbacks stable.
- You need the hook to read cockpit data, connection state, or the route object.
- Total tests drop below 352, or any test fails.
- You need new CSS or class names.
- The patch would exceed roughly 400 changed lines.

# TDD implementation packet — slice 4a: useVersionMonitor

PACKET_STATUS: READY
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED

## Task contract

Extract version-update detection out of `App.tsx` into a focused hook,
`src/react/useVersionMonitor.ts`. **Observable behaviour must be identical.**

One bounded outcome: `App.tsx` no longer owns `bootVersionRef`, `checkVersion`,
or `updateAvailable`; it consumes them from the hook.

## Allowed files

- `crates/ajax-web/web/src/react/useVersionMonitor.ts` (new)
- `crates/ajax-web/web/src/react/useVersionMonitor.test.tsx` (new)
- `crates/ajax-web/web/src/components/App.tsx`

## Forbidden changes

- Do not touch `polling.ts`, `api.ts`, `cockpitPoll.ts`, or any other component.
- Do not change polling cadences or when the caller invokes the check.
- **Do not give the hook its own `setInterval`.** The caller owns cadence; the
  hook exposes a callback the caller schedules. Moving the interval inside would
  change the adaptive route/visibility cadence behaviour.
- Do not add `eslint-disable` anywhere.
- Do not touch `App.test.tsx` beyond what a compile error forces; if you think
  an existing App test must change, **stop and report** instead.
- **Never redirect command output into `/tmp`** — writes there are auto-rejected
  and will kill your round.

## Current implementation (App.tsx — the exact code to move)

```ts
const [updateAvailable, setUpdateAvailable] = useState(false);
const bootVersionRef = useRef<string | null>(null);

const checkVersion = useCallback(async () => {
  try {
    const { version } = await fetchVersion();
    if (!version) return;
    if (bootVersionRef.current === null) bootVersionRef.current = version;
    else if (version !== bootVersionRef.current) setUpdateAvailable(true);
  } catch {
    // Offline: keep the pinned version and retry later.
  }
}, []);
```

Consumers in `App.tsx`:
- `checkVersion` is called from `onShellMount`, `onShellResume`,
  `onShellVisibilityChange` (all `useEffectEvent`), and from the interval effect
  where it is a dependency.
- `updateAvailable` drives `hidden={!updateAvailable}` on the update banner.

## Target shape

```ts
export type VersionMonitor = {
  updateAvailable: boolean;
  checkVersion: () => Promise<void>;
};

export function useVersionMonitor(): VersionMonitor;
```

`checkVersion` **must be referentially stable** (`useCallback` with `[]`). The
interval effect in `App.tsx` lists it as a dependency; an unstable identity
would tear down and recreate both intervals on every render.

## Behaviour contract — preserve exactly

1. The **first** successful version response pins the boot version and does
   **not** raise the banner.
2. A later response with a **different** version sets `updateAvailable` true.
3. Once true, it stays true (no later response clears it).
4. An empty/missing `version` field is ignored entirely.
5. A rejected fetch is swallowed — no throw, no state change.

## Task 1 — RED

Create `useVersionMonitor.test.tsx` covering all five contract points above,
using `renderHook` from `@testing-library/react` and a stubbed `fetchVersion`
(stub `fetch`, matching the existing `App.test.tsx` `jsonResponse` style).

Run it and prove it fails before the hook exists:

```bash
npm run web:test -- --run src/react/useVersionMonitor.test.tsx
```

Record the nonzero exit as RED evidence.

## Task 2 — GREEN

1. Implement the hook.
2. Rewire `App.tsx`: delete `updateAvailable` state, `bootVersionRef`, and the
   `checkVersion` callback; call `const { updateAvailable, checkVersion } = useVersionMonitor();`.
3. Leave every call site and the interval dependency array otherwise unchanged.

## Verification commands

```bash
npm run web:test -- --run src/react/useVersionMonitor.test.tsx
npm run web:test -- --run
npm run web:check
npm run web:lint
```

- Full suite must report **37 files** and **at least 332 tests**, with **zero
  failures**. Your new tests raise the count; nothing may be removed.
- `web:lint` must exit 0 with no new suppressions.
- The existing App tests "defers the version check until the browser is idle"
  and "surfaces an update banner when the API version changes" must pass
  **unmodified**.

## Stop conditions

- Any existing `App.test.tsx` test would need editing.
- `checkVersion` cannot be made referentially stable.
- You need an interval inside the hook.
- Total tests drop below 332 or any test fails.
- The patch would exceed roughly 400 changed lines.

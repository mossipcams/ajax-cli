# TDD Packet: Change-only cockpit + overlapping poll guard (PR5)

## 1. Goal

Skip Svelte cockpit assignment when the fetched view is unchanged, and never start a second cockpit fetch while one is in flight.

## 2. Allowed files

- `crates/ajax-web/web/src/state.ts` (or new small helper colocated — prefer extending existing state helpers if present)
- `crates/ajax-web/web/src/state.test.ts`
- `crates/ajax-web/web/src/components/App.svelte`
- `crates/ajax-web/web/src/components/App.test.ts` (only if needed)
- `.planning/agent-plans/web-mobile-power-optimizations.md` (optional)

If `state.ts` is unrelated, put pure helpers in a new `crates/ajax-web/web/src/cockpitPoll.ts` + `cockpitPoll.test.ts` instead — then those two files replace state.ts in Allowed files.

## 3. Forbidden changes

- Do not change fetch API contracts, cache headers, or server.
- Do not change adaptive polling intervals (PR1).
- Do not hash/skip connection-error paths — errors must still update connection state.
- Mutation/operation responses that call `applyCockpit` with a fresh cockpit from POST may still apply; prefer hashing there too (same helper) so identical payloads no-op.
- No Rust. No terminal. No drive-by refactors.

## 4. Architecture context

Web Cockpit polls server-authoritative projections. Re-assigning identical JSON into Svelte still dirties the UI. Overlapping polls on slow mobile networks waste work and risk stale overwrites.

## 5. Code anchors

```ts
// App.svelte
function applyCockpit(next: BrowserCockpitView) {
  cockpit = next;
  connection = "connected";
  connectionDetail = null;
}

async function loadCockpit() {
  if (document.hidden) return;
  try {
    applyCockpit(await fetchCockpit());
  } catch (error) {
    applyConnectionError(error);
  }
}
```

Inspect `state.ts` first; reuse if it already has serialization helpers.

## 6. Test-first instructions

Create/extend unit tests for:

1. `stableCockpitHash` (or `stableHash`) is stable for deep-equal objects / same JSON shape
2. `stableCockpitHash` differs when a field changes
3. `createCockpitApplyGate` / `applyCockpitIfChanged` returns false when hash matches, true when changed
4. `createCockpitPollGuard` / in-flight: second `run` while first pending does not invoke fetch; after settle, next run does

Suggested API:

```ts
export function stableCockpitHash(view: BrowserCockpitView): string;
// Implement via JSON.stringify with sorted keys OR JSON.stringify if fixture order is stable.
// Prefer: JSON.stringify(view) if BrowserCockpitView is already deterministic from the API.
// Document choice in a one-line comment.

export function createCockpitApplyGate(): {
  applyIfChanged(next: BrowserCockpitView): boolean; // true if applied
  reset(): void;
};

export function createInFlightGuard(): {
  run<T>(fn: () => Promise<T>): Promise<T | undefined>; // undefined if skipped
};
```

**Fail first:**

```bash
npm run web:test -- crates/ajax-web/web/src/cockpitPoll.test.ts --run
# or state.test.ts if colocated there
```

## 7. Production edit instructions

1. Add helpers in `cockpitPoll.ts` (preferred new file) unless `state.ts` already fits.
2. In `App.svelte`:
   - Keep a module-local/gate instance for apply.
   - `applyCockpit` uses gate: if unchanged, still set `connection = "connected"` and clear detail (connectivity recovered) but do **not** reassign `cockpit = next` when hash matches.
   - Wrap `loadCockpit` body with in-flight guard so overlapping interval ticks no-op.
3. Manual retry / pull-to-refresh / mutation `loadCockpit()` also go through the same guard (skip if in flight is OK).
4. Do not block `applyCockpit` from POST operation responses behind the in-flight fetch guard — only the poll fetch. Apply-gate hashing still applies.

## 8. Verification commands

```bash
npm run web:test -- crates/ajax-web/web/src/cockpitPoll.test.ts crates/ajax-web/web/src/state.test.ts crates/ajax-web/web/src/components/App.test.ts --run
```

(Adjust paths to whatever files you created.)

## 9. Acceptance criteria

- Unchanged cockpit payload does not reassign `cockpit` state.
- Overlapping `loadCockpit` does not start a second fetch.
- Connection still becomes `"connected"` on successful poll even when hash matches.
- App tests pass.

## 10. Stop conditions

- Need server ETag support.
- Hashing BrowserCockpitView requires unstable key order and breaks tests — stop and report with fixture evidence.

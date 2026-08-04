PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior

## Task

Add durable persistence for **explicit** telemetry events: write each event to IndexedDB before/at capture time, batch-upload to PostHog Cloud, retry failures with exponential backoff, and delete local records only after successful delivery. Wire this through the existing `@/shared/lib/telemetry` `track` path without changing call sites. Expose queue status helpers for the future Settings diagnostic screen (packet 3), but do not edit Settings UI or architecture docs here.

## Scope

### Allowed

- `crates/ajax-web/web/src/shared/lib/telemetryStore.ts` (new IndexedDB store + in-memory test backend)
- `crates/ajax-web/web/src/shared/lib/telemetryStore.test.ts` (new)
- `crates/ajax-web/web/src/shared/lib/telemetryUpload.ts` (new batch/retry uploader)
- `crates/ajax-web/web/src/shared/lib/telemetryUpload.test.ts` (new)
- `crates/ajax-web/web/src/shared/lib/telemetry.ts` (wire `track` → enqueue + flush; export queue status)
- `crates/ajax-web/web/src/shared/lib/telemetry.test.ts` (update for durable path)
- `crates/ajax-web/web/src/shared/lib/telemetryContext.ts` (only if a tiny shared id helper is needed)
- `crates/ajax-web/web/src/shared/lib/telemetryFilter.ts` (only if needed for shared types)

### Forbidden

- Settings UI / diagnostic button / architecture.md / web-cockpit.md (packet 3)
- Swipe distance/velocity/cancel/settle, route visible, PWA launch/resume (packet 3)
- New npm dependencies (no `fake-indexeddb`; inject an in-memory store for tests)
- Reverse proxy, session replay, capturing terminal/prompt/token/source
- Commits, pushes, branch changes
- Changing Vite chunk contract

## Acceptance

1. Explicit `track`/`captureEvent` / interaction / swipe helpers persist a durable record (event name + properties including context fields) before relying on PostHog SDK in-memory flush.
2. Uploader sends events in batches (default batch size ≤ 20) via `posthog.capture` (or equivalent), and **deletes** IndexedDB records only after successful delivery of that batch/item.
3. Failed uploads increment attempt count, schedule `next_attempt_at` with exponential backoff (capped), and keep the record.
4. On `initTelemetry` success, pending ready events are flushed.
5. When `VITE_POSTHOG_KEY` is missing / telemetry uninitialized, events are not queued (telemetry off = no-op), matching current soft-disable behavior.
6. `getTelemetryQueueStatus()` (or equivalent) returns at least `{ pending: number; initialized: boolean }` for packet 3 UI.
7. Tests cover: persistence across a simulated restart (re-open store), batching, retry/backoff on failure, and delete-after-success. Use an injectable in-memory store — do not add npm deps.
8. Sensitive filtering still applies before persistence (no terminal/prompt/token/source props stored).

## Constraints

- IndexedDB database name should be namespaced, e.g. `ajax-telemetry`, store `events`, keyPath `event_id`.
- This is a **narrow storage exception** for telemetry delivery durability only — do not store task/API/offline mutation state.
- Keep files under ~600 LOC; soft-fail storage errors with `console.warn`.
- Do not block the UI thread on flush; fire-and-forget with serialized flush lock is fine.
- Preserve public API from packet 1; additive exports only.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: npm run web:test -- --run crates/ajax-web/web/src/shared/lib/telemetry
      expected: exit 0; persistence, batching, retry tests pass
    - type: typecheck
      command: npm run web:check
      expected: exit 0
  broader_checks:
    - npm run web:build
  reason: Unit tests with injectable store lock queue semantics; tsc/build guard wiring.
```

## Stop if

- Scope grows into Settings UI, docs, or swipe/route/launch instrumentation.
- Diff would exceed ~400 changed lines.
- Tempted to add `fake-indexeddb` or other dependencies.
- Vite emits a third chunk.

## Code anchors

- Wrapper: `crates/ajax-web/web/src/shared/lib/telemetry.ts` (`track`, `initTelemetry`)
- Context/filter: `telemetryContext.ts`, `telemetryFilter.ts`
- Plan: `.planning/agent-plans/posthog-backed-telemetry.md`

## Edit instructions

1. Implement `TelemetryStore` interface + `createMemoryTelemetryStore()` + `openIndexedDbTelemetryStore()`.
2. Implement `flushTelemetryQueue(store, captureFn, opts)` with batch size, backoff, delete-on-success.
3. In `track`: sanitize → enrich → `store.put` → schedule flush using `posthog.capture`.
4. On `initTelemetry` success, kick a flush of pending events.
5. Export `getTelemetryQueueStatus` and a test seam to inject the store.
6. Write focused tests with the memory store; mock capture success/failure.

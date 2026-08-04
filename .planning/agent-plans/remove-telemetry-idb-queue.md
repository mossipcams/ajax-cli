# Remove IndexedDB telemetry queue

**Mode:** Refactor / Cleanup  
**Delegation decision:** not delegated because parent is deleting an overbuilt path just reviewed; smallest safe change is in-place removal  
**Approval:** user — do not persist to a local DB at all

## Scope

- Remove IndexedDB store/uploader modules and all queue wiring
- Keep typed wrapper, context, filter, events, Settings diagnostic, default key
- Update architecture docs to drop the IndexedDB exception

## Checklist

- [x] Delete `telemetryStore*` / `telemetryUpload*`
- [x] Simplify `track()` to direct `posthog.capture`
- [x] Simplify Settings queue status (no pending IDB count)
- [x] Docs + tests
- [x] `web:test` / `web:check` (44 passed)

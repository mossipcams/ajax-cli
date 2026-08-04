PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior

## Task

Instrument remaining explicit Safari PWA telemetry events and Settings diagnostic UI: enrich swipe metrics; add route-visible, PWA launch/resume, and diagnostic capture helpers; wire App + Settings. Do **not** edit architecture docs in this packet (packet 3b).

## Scope

### Allowed

- `crates/ajax-web/web/src/shared/lib/telemetry.ts`
- `crates/ajax-web/web/src/shared/lib/telemetry.test.ts`
- `crates/ajax-web/web/src/shared/hooks/useSwipePageTransition.ts`
- `crates/ajax-web/web/src/shared/hooks/useSwipePageTransition.test.tsx`
- `crates/ajax-web/web/src/app/App.tsx`
- `crates/ajax-web/web/src/app/main.tsx`
- `crates/ajax-web/web/src/features/settings/SettingsView.tsx`
- `crates/ajax-web/web/src/features/settings/SettingsView.test.tsx`

### Forbidden

- `architecture.md` / `docs/architecture/web-cockpit.md` (packet 3b)
- Rewriting durable queue/filter/context beyond tiny exports
- New npm deps, reverse proxy, session replay, terminal/prompt capture
- Commits/pushes/branch changes; do not modify `dist/`

## Acceptance

1. `ajax_swipe` includes `duration_ms`, `distance_px`, `velocity_px_per_ms`, `completed`, `cancelled`, `settle_ms`, `direction` (+ route fields when known). Cancelled snap-backs are recorded.
2. `ajax_route_visible` with `duration_ms` from nav start to visible content.
3. `ajax_pwa_launch` once per cold boot with `duration_ms`; `ajax_pwa_resume` on hidden→visible.
4. Settings Diagnostics shows telemetry status (initialized, standalone, pending queue, app version) + button emitting `ajax_telemetry_diagnostic`.
5. Focused tests cover richer swipe (completed vs cancelled), diagnostic emission, and helper behavior where practical.

## Constraints

- Soft-fail; observational standalone only; keep existing common context props via `track`.
- Keep files under ~600 LOC; additive API on telemetry wrapper.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: npm run web:test -- --run crates/ajax-web/web/src/shared/lib/telemetry
      expected: exit 0
    - type: test
      command: npm run web:test -- --run crates/ajax-web/web/src/shared/hooks/useSwipePageTransition
      expected: exit 0
    - type: test
      command: npm run web:test -- --run crates/ajax-web/web/src/features/settings/SettingsView
      expected: exit 0
    - type: typecheck
      command: npm run web:check
      expected: exit 0
  broader_checks:
    - npm run web:build
  reason: Focused tests lock instrumentation and Settings wiring.
```

## Stop if

- Docs edited; diff > ~400 lines; third Vite chunk; dist modified.

## Code anchors

- `useSwipePageTransition.ts`, `App.tsx` visibility/route, `SettingsView.tsx`, `getTelemetryQueueStatus` / `isStandaloneDisplay` / `track` in `telemetry.ts`

## Edit instructions

1. Extend `captureSwipe`; emit cancel path; measure settle in `animateTo`.
2. Add route/launch/resume/diagnostic helpers on telemetry wrapper.
3. Wire App + Settings; add tests; do not touch docs or dist.

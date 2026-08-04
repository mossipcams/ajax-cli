PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior

## Task

Complete Safari PWA telemetry instrumentation and documentation: enrich swipe metrics; add route-visible, PWA launch/resume, and diagnostic events; add a Settings telemetry status section; document event names and property schemas in root `architecture.md` and update `docs/architecture/web-cockpit.md` for the IndexedDB telemetry-queue exception.

## Scope

### Allowed

- `crates/ajax-web/web/src/shared/lib/telemetry.ts` (new helpers: route visible, launch/resume, diagnostic, richer swipe)
- `crates/ajax-web/web/src/shared/lib/telemetry.test.ts`
- `crates/ajax-web/web/src/shared/hooks/useSwipePageTransition.ts` (distance, velocity, completed/cancelled, settle_ms)
- `crates/ajax-web/web/src/shared/hooks/useSwipePageTransition.test.tsx` (if present) or create/update sibling test
- `crates/ajax-web/web/src/app/App.tsx` (route-visible + launch/resume wiring)
- `crates/ajax-web/web/src/app/main.tsx` (optional launch mark only if needed)
- `crates/ajax-web/web/src/features/settings/SettingsView.tsx`
- `crates/ajax-web/web/src/features/settings/SettingsView.test.tsx`
- `architecture.md` (event schema section + pointer)
- `docs/architecture/web-cockpit.md` (PostHog section: env init, IndexedDB queue exception, full event schemas)

### Forbidden

- Rewriting the durable queue / filter / context modules beyond tiny export additions
- New npm dependencies
- Reverse proxy, session replay, capturing terminal/prompt/token/source
- Requiring Home Screen PWA / service-worker offline mutation model
- Commits, pushes, branch changes
- Touching Rust crates or Vite chunk contract beyond docs

## Acceptance

1. `ajax_swipe` includes `duration_ms`, `distance_px`, `velocity_px_per_ms`, `completed` (bool), `cancelled` (bool), `settle_ms`, plus `direction` and existing route fields when known. Cancelled (snap-back) swipes are recorded; committed swipes set `completed: true`.
2. `ajax_route_visible` fires when a hash route’s primary content becomes visible, with `duration_ms` from navigation start (or hash change) to visible paint/content.
3. `ajax_pwa_launch` fires once per cold boot with `duration_ms` from navigation/page start to first interactive shell visibility; includes `standalone`.
4. `ajax_pwa_resume` fires when returning from `document.hidden` → visible with `duration_ms` hidden interval (or time-to-visible after resume).
5. Settings Diagnostics shows a Telemetry status subsection (initialized, standalone, pending queue count, app version) and a button that emits `ajax_telemetry_diagnostic`.
6. Root `architecture.md` documents event names + property schemas (or a concise table with required common props) and points to `docs/architecture/web-cockpit.md` for detail.
7. `docs/architecture/web-cockpit.md` updates PostHog section: env-gated key/host, TTFB, IndexedDB **telemetry queue only** exception to the browser-storage ban, durable batch/retry semantics, and full event schemas.
8. Focused tests cover richer swipe props (at least completed vs cancelled), diagnostic emission, and launch/resume helpers where practical.

## Constraints

- Prefer small helpers on the telemetry wrapper; avoid new frameworks.
- Soft-fail; never block Cockpit.
- Keep autocapture ignorelists; do not enable session replay.
- Observational standalone detection only — do not add a PWA manifest/service worker requirement.
- Common explicit-event props remain: `event_id`, `session_id`, `install_id`, `sequence`, `app_version`, `route`, `ios_version`, `viewport_w`/`viewport_h`, `standalone`.

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
  reason: Focused tests lock new events/UI; docs are reviewed in gate; tsc/build guard wiring.
```

## Stop if

- Diff exceeds ~400 changed lines (split further).
- Architecture edits expand beyond telemetry/storage exception wording.
- Vite emits a third JS chunk.

## Code anchors

- Swipe: `useSwipePageTransition.ts` (`captureSwipe`, `animateTo`, cancel path at engaged snap-back)
- App visibility/route: `App.tsx` (`documentVisibility`, `useHashRoute`)
- Settings: `SettingsView.tsx` diagnostics section
- Docs: `architecture.md` Hard Invariants / Navigation; `docs/architecture/web-cockpit.md` PostHog Cloud telemetry
- Queue status: `getTelemetryQueueStatus` in `telemetry.ts`

## Edit instructions

1. Extend `captureSwipe` props; emit cancelled swipes from the snap-back path; measure settle via `animateTo` timing.
2. Add `markNavigationStart` / `captureRouteVisible`, `capturePwaLaunch`, `capturePwaResume`, `captureTelemetryDiagnostic` on the wrapper.
3. Wire App for route + launch/resume; wire Settings status + diagnostic button.
4. Document schemas in `architecture.md` + expand web-cockpit PostHog section (IndexedDB exception).
5. Add/adjust focused tests; do not modify `dist/`.

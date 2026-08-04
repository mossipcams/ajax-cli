PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior

## Task

Replace the thin `posthog.ts` helper with one typed `@/shared/lib/telemetry` wrapper that env-gates PostHog Cloud init, attaches standalone/context metadata to every explicit event, filters sensitive properties, enables Web Vitals including TTFB, and keeps session replay off. Migrate existing callers off direct `posthog` imports. Do **not** implement IndexedDB persistence, Settings diagnostic UI, or route/launch/resume instrumentation in this packet (those are packets 2–3).

## Scope

### Allowed

- `crates/ajax-web/web/src/shared/lib/telemetry.ts` (new public wrapper)
- `crates/ajax-web/web/src/shared/lib/telemetryContext.ts` (new: standalone, ids, sequence, ios, viewport, route helpers)
- `crates/ajax-web/web/src/shared/lib/telemetryFilter.ts` (new: sensitive-property filter)
- `crates/ajax-web/web/src/shared/lib/telemetry.test.ts` (new)
- `crates/ajax-web/web/src/shared/lib/telemetryContext.test.ts` (new)
- `crates/ajax-web/web/src/shared/lib/telemetryFilter.test.ts` (new)
- `crates/ajax-web/web/src/shared/lib/posthog.ts` (thin re-export shim OR delete after migrating callers)
- `crates/ajax-web/web/src/shared/lib/posthog.test.ts` (update or delete to match shim)
- `crates/ajax-web/web/src/app/main.tsx` (init via telemetry)
- `crates/ajax-web/web/src/app/App.tsx` (import from telemetry)
- `crates/ajax-web/web/src/features/task/ActionBar.tsx` (import from telemetry)
- `crates/ajax-web/web/src/shared/hooks/useSwipePageTransition.ts` (import from telemetry; keep current swipe props shape for now)
- `crates/ajax-web/web/src/vite-env.d.ts` (`ImportMetaEnv` for `VITE_POSTHOG_KEY` / `VITE_POSTHOG_HOST`)

### Forbidden

- IndexedDB / upload queue / retry logic (packet 2)
- Settings UI telemetry status / diagnostic button (packet 3)
- `ajax_route_visible` / `ajax_pwa_launch` / `ajax_pwa_resume` instrumentation (packet 3)
- Expanding swipe to distance/velocity/cancel/settle (packet 3)
- Editing `architecture.md` or `docs/architecture/web-cockpit.md` (packet 3)
- Adding reverse proxy, session replay, or new npm dependencies beyond existing `posthog-js`
- Capturing terminal/PTY/prompt/token/source contents
- Commits, pushes, branch changes
- Touching Rust crates or Vite chunk contract (`app.js` + `terminal.js` only)

## Acceptance

1. `initTelemetry()` reads `import.meta.env.VITE_POSTHOG_KEY`; if absent/empty, remains uninitialized and all capture APIs no-op without throwing.
2. When key is present, initializes `posthog-js` against `VITE_POSTHOG_HOST` or default `https://us.i.posthog.com`, with `defaults: '2026-05-30'`, `disable_session_recording: true`, `capture_exceptions: false`, terminal/sensitive autocapture ignorelist preserved, and Web Vitals including **TTFB** plus LCP/CLS/FCP/INP.
3. Hardcoded `phc_…` project key is removed from source.
4. Public API is typed and exported from `@/shared/lib/telemetry` only: at least `initTelemetry`, `track`/`captureEvent`, `beginInteraction`, `endTapToFeedback`, `endTapToOperationComplete`, `cancelInteraction`, `captureSwipe`, plus test reset helpers. Application code does not import `posthog-js` directly.
5. Every explicit event payload includes: `event_id`, `session_id`, `install_id`, `sequence`, `app_version` (when known), `route`, `ios_version` (when parseable), `viewport_w`, `viewport_h`, `standalone` (boolean).
6. `isStandaloneDisplay()` detects installed PWA vs browser tab via `(display-mode: standalone)` and/or `navigator.standalone`.
7. Sensitive filter strips/rejects property keys/values that look like terminal contents, commands, prompts, tokens, or source code before capture.
8. Existing call sites (`main.tsx`, `App.tsx`, `ActionBar.tsx`, `useSwipePageTransition.ts`) compile and use the new wrapper.
9. Tests cover standalone detection, sequencing (monotonic sequence per install), sensitive-data filtering, and env-gated init.

## Constraints

- Prefer small concrete modules over new frameworks; keep each new `.ts` file well under ~600 LOC.
- Soft-fail on PostHog errors (`console.warn`); never break Cockpit boot.
- Preserve current event names `ajax_tap_to_feedback`, `ajax_tap_to_operation_complete`, `ajax_swipe`.
- `install_id` / `session_id` may use `localStorage` for identity (already allowed for PostHog SDK persistence); do not introduce IndexedDB in this packet.
- Sequence counter must survive within a session at minimum; prefer persisting last sequence with install id in localStorage.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: npm run web:test -- --run crates/ajax-web/web/src/shared/lib/telemetry
      expected: exit 0; standalone, sequencing, filter, and init tests pass
    - type: test
      command: npm run web:test -- --run crates/ajax-web/web/src/shared/lib/posthog
      expected: exit 0 if shim tests retained; or file removed cleanly
    - type: typecheck
      command: npm run web:check
      expected: exit 0
  broader_checks:
    - npm run web:build  # still emits only app.js + terminal.js
  reason: Unit tests lock context/filter/init behavior; tsc + build guard the public API migration and Vite chunk contract.
```

## Stop if

- Packet scope grows into IndexedDB, Settings diagnostic UI, or architecture doc edits.
- Diff would exceed ~400 changed lines.
- Verification fails and cannot be fixed within Allowed files.
- Vite build emits a third JS chunk.

## Code anchors

- Current init/capture: `crates/ajax-web/web/src/shared/lib/posthog.ts`
- Tests: `crates/ajax-web/web/src/shared/lib/posthog.test.ts`
- Boot: `crates/ajax-web/web/src/app/main.tsx` (`initPostHog()`)
- Callers: `App.tsx`, `ActionBar.tsx`, `useSwipePageTransition.ts`
- Plan ledger: `.planning/agent-plans/posthog-backed-telemetry.md`

## Edit instructions

1. Add `telemetryContext.ts` with `isStandaloneDisplay`, install/session id getters, `nextSequence()`, viewport/ios/route readers, and `buildEventContext()`.
2. Add `telemetryFilter.ts` with `sanitizeTelemetryProps(props)` that drops sensitive keys and suspicious string values.
3. Add `telemetry.ts` implementing env-gated `initTelemetry`, enriching `track` via context+filter, and moving interaction/swipe helpers from `posthog.ts`.
4. Migrate callers to `@/shared/lib/telemetry`; leave a one-line re-export shim in `posthog.ts` only if needed for a soft landing, otherwise delete `posthog.ts` + its test.
5. Extend `vite-env.d.ts` for the two `VITE_POSTHOG_*` keys.
6. Write focused vitest coverage with mocked `posthog-js` and fake `matchMedia` / `localStorage`.

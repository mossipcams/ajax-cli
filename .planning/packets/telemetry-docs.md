PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: docs-only

## Task

Document PostHog-backed Safari PWA telemetry event names and property schemas in root `architecture.md`, and update `docs/architecture/web-cockpit.md` PostHog section for env-gated init, TTFB vitals, IndexedDB telemetry-queue exception, durable batch/retry, and full event schemas. No production code changes.

## Scope

### Allowed

- `architecture.md`
- `docs/architecture/web-cockpit.md`

### Forbidden

- Any `crates/**` or other source changes
- Adding reverse proxy / requiring PWA install / enabling session replay
- Commits, pushes, branch changes

## Acceptance

1. Root `architecture.md` includes a concise telemetry section (or subsection) listing event names, common properties on every explicit event, and a pointer to `docs/architecture/web-cockpit.md`.
2. `docs/architecture/web-cockpit.md` PostHog section is updated to describe:
   - `VITE_POSTHOG_KEY` / `VITE_POSTHOG_HOST` (no hardcoded key; soft no-op when missing)
   - Web Vitals LCP/CLS/FCP/INP/TTFB; session replay off
   - Standalone vs browser-tab context
   - Narrow IndexedDB exception for **explicit telemetry event queue only** (not task/API/offline mutation)
   - Batch upload, backoff retry, delete-after-success
   - Full event name + property schemas for: `ajax_tap_to_feedback`, `ajax_tap_to_operation_complete`, `ajax_swipe`, `ajax_route_visible`, `ajax_pwa_launch`, `ajax_pwa_resume`, `ajax_telemetry_diagnostic`
3. Browser-storage ban wording is clarified so the telemetry queue exception is explicit and does not weaken the ban on operational/offline mutation storage.

## Constraints

- Keep docs concise and accurate to the implemented wrapper in `crates/ajax-web/web/src/shared/lib/telemetry*.ts`.
- Do not invent events that are not implemented.

## Verification

```yaml
verification:
  methods:
    - type: other
      command: rg -n "ajax_swipe|ajax_route_visible|ajax_pwa_launch|IndexedDB|VITE_POSTHOG" architecture.md docs/architecture/web-cockpit.md
      expected: matches present for event names, env vars, and IndexedDB exception in both docs
    - type: other
      steps: "Read the new architecture.md section and web-cockpit PostHog section; confirm schemas match telemetry.ts exports"
      expected: docs align with implementation
  reason: Docs-only change; grep + readback verify schema coverage without code edits.
```

## Stop if

- Any non-doc file is edited.
- Docs claim a reverse proxy or required PWA install.

## Code anchors

- Implementation truth: `telemetry.ts`, `telemetryContext.ts`, `telemetryStore.ts`, `telemetryUpload.ts`
- Existing PostHog section: `docs/architecture/web-cockpit.md` (~line 380)
- Plan: `.planning/agent-plans/posthog-backed-telemetry.md`

## Edit instructions

1. Add a short "Web Cockpit telemetry" section to `architecture.md` with common props + event table + link.
2. Rewrite/expand the PostHog Cloud telemetry subsection in web-cockpit.md per Acceptance.
3. Clarify IndexedDB ban vs telemetry-queue exception in the same doc near the storage ban.

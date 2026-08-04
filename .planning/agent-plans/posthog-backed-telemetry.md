# PostHog-backed Safari PWA telemetry

**Mode:** Behavior Change (architecture doc + browser-storage exception)  
**Approval status:** approved by user request — implement now  
**Delegation decision:** delegated via model-router (sequential packets)

## Locked decisions

| Decision | Value |
| --- | --- |
| Sink | PostHog Cloud direct (no reverse proxy) |
| Init | `VITE_POSTHOG_KEY` + optional `VITE_POSTHOG_HOST` (default `https://us.i.posthog.com`); missing key → soft no-op |
| Hardcoded key | Remove from source (migrate off PR #755 inline key) |
| Public API | One typed wrapper: `@/shared/lib/telemetry` (callers must not import `posthog-js`) |
| Context on every explicit event | `event_id`, `session_id`, `install_id`, `sequence`, `app_version`, `route`, `ios_version`, `viewport_w`/`viewport_h`, `standalone` |
| Standalone | `display-mode: standalone` / `navigator.standalone` — observational only; no PWA requirement |
| Web Vitals | LCP, CLS, FCP, INP, TTFB via PostHog `capture_performance` |
| Replay | off; exception autocapture off |
| Autocapture | limited ignorelist for terminal/sensitive surfaces; prefer named events |
| Persistence | IndexedDB queue for **explicit** events only; batch + backoff; delete after successful delivery |
| Storage exception | Narrow carve-out under browser-storage ban: telemetry event queue only (not task/API/offline mutation) |
| Diagnostic | Settings Diagnostics → telemetry status + `ajax_telemetry_diagnostic` |
| Docs | Event names + property schemas in root `architecture.md` (summary + pointer) and full detail in `docs/architecture/web-cockpit.md` |

## Non-goals

- Reverse proxy / same-origin ingest
- Session replay / heatmaps-as-primary
- Requiring Home Screen install or service-worker offline mutation
- Capturing prompts, PTY, terminal buffer, tokens, source code
- Task lifecycle / registry / action policy changes

## Event names

| Event | Purpose |
| --- | --- |
| `ajax_tap_to_feedback` | Button tap → visible feedback |
| `ajax_tap_to_operation_complete` | Button tap → completed operation |
| `ajax_swipe` | Swipe metrics (duration, distance, velocity, completed/cancelled, settle_ms) |
| `ajax_route_visible` | Route/navigation time to visible content |
| `ajax_pwa_launch` | Cold launch timing |
| `ajax_pwa_resume` | Resume from background timing |
| `ajax_telemetry_diagnostic` | Manual diagnostic from Settings |

## Implementation slices

### Packet 1 — typed wrapper foundation

- [x] Env-gated PostHog init + TTFB vitals + replay off
- [x] Standalone / context helpers + sequence / install+session IDs
- [x] Sensitive-property filter
- [x] Public `telemetry` wrapper; migrate existing `posthog` callers
- [x] Tests: standalone, sequencing, sensitive filter, init
- [x] Parent Review Gate: session_id → sessionStorage; context wins over caller props

### Packet 2 — durable queue

- [x] IndexedDB store for explicit events
- [x] Batch upload + exponential backoff retry
- [x] Delete only after successful PostHog delivery
- [x] Tests: persistence, batching, retries
- [x] Parent fix: direct-capture fallback when IndexedDB unavailable; restore accidental `dist/app.js`

### Packet 3a — instrumentation

- [x] Enrich swipe (distance/velocity/cancel/settle); route visible; launch/resume
- [x] Settings telemetry status + diagnostic trigger
- [x] Focused tests + `web:check` / `web:test`

### Packet 3b — docs

- [x] Update `architecture.md` + `web-cockpit.md`
- [x] MiniMax hit OpenCode Go usage limit → escalated to Cursor

## Deviations

- Packet 1 first dispatch via `pi-delegate`/`opencode-go/glm-5.2` failed: OpenCode Go monthly usage limit (429). Escalated to `cursor-delegate`/`composer-2.5`.
- Snapshot directory moved to `/tmp/...` so in-worktree snapshot objects do not pollute deltas.
- Cursor often wrapped `DELEGATE_REPORT` in a yaml fence → schema extractor FAILED; parent reviewed delta + re-ran verification.
- Parent Review Gate corrections: `session_id` → `sessionStorage`; context wins over caller props; direct-capture fallback when IndexedDB unavailable; restore accidental `dist/app.js`.
- Packet 3 split into 3a instrumentation + 3b docs after R-SIZE-SPLIT risk on combined estimate.
- Packet 3b MiniMax also hit OpenCode Go usage limit → Cursor.

## Validation results

```bash
npm run web:test -- --run .../telemetry .../useSwipePageTransition .../SettingsView
# EXIT 0 — telemetry + swipe + settings suites green

npm run web:check   # EXIT 0
npm run web:build   # EXIT 0 — app.js + terminal.js only
```

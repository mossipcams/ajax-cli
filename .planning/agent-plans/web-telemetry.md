# Web telemetry — PostHog JS performance baselines

**Mode:** Behavior Change — implementing  
**Approval status:** approved — PostHog Cloud US, identify, persistence, implement now  
**Delegation decision:** delegated via model-router (sequential packets)

## Decisions (locked)

| Decision | Value |
| --- | --- |
| Sink | PostHog Cloud |
| Region | US — `https://us.i.posthog.com` |
| Identify | yes — `posthog.identify` keyed by host/operator surface |
| Persistence | yes — SDK default localStorage/cookie distinct id |
| Session Replay | off |
| Key | Project key from operator init (`phc_…`); client-side PostHog project token |
| SDK defaults | `'2026-05-30'` |
| api_host | `https://us.i.posthog.com` |

## Product event stack

```text
PostHog JS
├── automatic button/tap events          ($autocapture)
├── automatic Web Vitals                 ($web_vitals via capture_performance)
├── custom swipe event                   (ajax_swipe)
├── custom tap-to-feedback event         (ajax_tap_to_feedback)
└── custom tap-to-operation-complete     (ajax_tap_to_operation_complete)
```

## Scope

- Add `posthog-js` to root `package.json` (web deps live there)
- Init once at app boot with the PostHog Cloud US project token
- Autocapture + Web Vitals (LCP, CLS, FCP, INP)
- Three custom timed events
- Update `docs/architecture/web-cockpit.md` (outbound PostHog Cloud; denylist;
  Replay/exception autocapture off; analytics persistence vs operational storage)
- Keep Vite emit contract: only `app.js` + `terminal.js` (bundle PostHog into
  `app.js`; do not add a third chunk without updating `adapters/assets.rs`)

## Non-goals

- Session Replay / heatmaps-as-primary / exception autocapture
- Self-hosted PostHog or same-origin ingest proxy
- Per-keystroke / PTY / prompt capture
- Task lifecycle / registry / action policy changes
- CI baseline gates

## Implementation slices

### Slice A+B — foundation (packet 1)

- [x] `docs/architecture/web-cockpit.md` PostHog Cloud note (after arch split on main)
- [x] Add `posthog-js`; `shared/lib/posthog.ts` + tests
- [x] Init: US host, project key, `defaults: '2026-05-30'`, identify, persistence,
      autocapture ignorelists, vitals, Replay off
- [x] Wire from `main.tsx`
- [x] Terminal/sensitive autocapture ignorelist

### Slice C — custom timed events (packet 2)

- [x] `ajax_swipe` from swipe commit
- [x] `ajax_tap_to_feedback` (ActionBar confirm/busy/banner; open_task nav)
- [x] `ajax_tap_to_operation_complete` (ActionBar mutation settle)
- [x] Focused tests + `web:check` / `web:build`

## Validation results

```bash
npm run web:test -- --run crates/ajax-web/web/src/shared/lib/posthog.test.ts
# EXIT 0 — 9 passed

npm run web:check
# EXIT 0

npm run web:build
# EXIT 0 — app.js + terminal.js only (PostHog bundled into app.js ~561KB)
```

## Deviations

- Used operator-provided project key + `defaults: '2026-05-30'` instead of
  `VITE_POSTHOG_KEY` env gate (explicit user init snippet).
- Parent finished foundation after interrupted cursor-delegate and implemented
  custom events in the same pass (delegate tool interrupted mid-transaction).
- Architecture review (Composer): MOSTLY_ALIGNED → applied MEDIUM fixes:
  private-egress clarification, storage-exception wording, `capture_exceptions: false`.

## PR prep checklist

- [x] Apply MEDIUM architecture-review fixes
- [x] Focused tests + `web:check` / `web:build`
- [x] Local verify gate (`npm run verify` — EXIT 0; known jsdom canvas noise)
- [ ] Commit + push + `gh pr create`

## Validation results

```bash
npm run web:test -- --run .../posthog.test.ts  # EXIT 0 — 9 passed
npm run web:check                              # EXIT 0
npm run web:build                              # EXIT 0 — app.js+terminal.js only
npm run verify                                 # EXIT 0 — 1811 Rust tests + web suite
```

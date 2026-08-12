# Fix push suppress + Cursor default model

Mode: Behavior Change. Issue: #793

## Scope

1. Declarative Web Push tick must not treat background/Simulator cockpit
   **data** polls as “operator is using Ajax.”
2. Keep poll rate/latency unchanged: same `/api/cockpit` request; add
   `X-Ajax-Foreground: 1` only when `document.visibilityState === "visible"`.
3. Ajax Web / CLI task create for Cursor must launch
   `cursor agent --model cursor-grok-4.5-high` (not Fast / auto).

## Non-goals

- Extra presence round-trips
- Per-device subscription matching
- Mid-session model switching UI

## Tasks

- [x] Open defect #793
- [x] Pin Cursor default model in `agent_launch_spec` + tests
- [x] Foreground-header presence on cockpit polls + restore tick gate
- [x] Client sends `X-Ajax-Foreground` only when visible
- [x] Focused verify

## Validation

```bash
cargo nextest run -p ajax-web -E 'test(foreground) | test(push_tick_logic)'
npm run web:test -- --run crates/ajax-web/web/src/shared/lib/api.test.ts
cargo fmt --check
```

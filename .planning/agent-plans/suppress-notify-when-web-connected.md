# Suppress attention webhooks while Web Cockpit is connected

Mode: Behavior Change.
Delegation decision: delegated via model-router

## Scope

When a browser is actively using Web Cockpit (`GET /api/cockpit`), do **not**
fire attention webhook notifications. Background notify ticks resume delivery
only after the browser has been quiet long enough to count as disconnected.

## Non-goals

- Visibility-aware signaling from the SPA (hidden vs focused)
- Suppressing CLI/TUI cockpit notifications
- Changing rising-edge / dwell / acknowledge semantics in `attention.rs`
- New config knobs

## Design

1. Track `last_browser_cockpit_at` on `WebAppState` (shared `Arc<Mutex<Option<Instant>>>`).
2. Every `GET /api/cockpit` (including cache hits) marks the browser seen.
3. `browser_connected()` is true when last seen elapsed < `BROWSER_CONNECTED_TTL`
   (90s — covers hidden 60s poll + grace).
4. `RuntimeBridge::refresh_cockpit` gains `deliver_notifications: bool`.
   - Browser-driven refresh: `false` (UI update only; no stamp/delivery).
   - Background tick: skip entirely when `browser_connected()`; otherwise
     refresh with `true`.
5. Catch-up: stamps are not written while connected, so the first tick after
   disconnect delivers for still-Waiting/Error tasks.

## Tasks

- [x] T1: Unit tests for `browser_connected` / mark / TTL expiry
- [x] T2: Browser `/api/cockpit` refresh passes `deliver_notifications=false`
- [x] T3: Tick skips when connected; delivers when not
- [x] T4: Wire production code; update `CliRuntimeBridge` + call sites
- [x] T5: Docs (README + architecture.md one-liners)
- [x] Validate focused tests + clippy/fmt

## Validation

```bash
cargo nextest run -p ajax-web browser_connected axum_cockpit_marks_browser refresh_cockpit_and_cache
# parent: 4 passed
cargo nextest run -p ajax-cli web_refresh_cockpit notify
# parent: 6 passed
cargo fmt --check  # exit 0
cargo clippy -p ajax-web -p ajax-cli --all-targets -- -D warnings  # exit 0
```

## Deviations

- Codex packet-critique unavailable (`reasoning.effort=max` rejected by API).
  Parent reviewed packet locally as PASS.
- GLM (`opencode-go/glm-5.2`) exited immediately with empty output → escalated to
  Cursor `composer-2.5`.

## Results

- Review Gate: ACCEPT (2026-07-16). Scope confined to allowed files.
- Behavior: `/api/cockpit` marks presence (90s TTL); browser refreshes never
  deliver webhooks; background tick only delivers when `browser_connected()` is
  false (no stamp while suppressed → catch-up after disconnect).

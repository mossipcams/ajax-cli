# TDD Packet: Suppress attention webhooks while Web Cockpit is connected

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## 1. Status and task contract

READY behavior change. Test first, then smallest production edit.

## 2. Goal

Phone/webhook attention notifications must not fire while a browser is actively
polling Web Cockpit. Background `ajax web` notify ticks deliver only when no
browser has hit `GET /api/cockpit` within `BROWSER_CONNECTED_TTL` (90s).

## 3. Allowed files

- `crates/ajax-web/src/runtime.rs` — presence tracking, tick gate, refresh flag,
  tests (embedded `#[cfg(test)]`)
- `crates/ajax-cli/src/web_backend.rs` — honor `deliver_notifications` in
  `CliRuntimeBridge::refresh_cockpit`; update call sites in its tests
- `README.md` — one sentence that webhooks stay quiet while a browser is connected
- `architecture.md` — one sentence on the browser-presence gate for the notify tick

Do NOT edit any other file. Do NOT edit files under `tests/` or `crates/ajax-web/web/`.

## 4. Forbidden changes

- No changes to `attention.rs`, dwell/acknowledge, or CLI/TUI notify wiring in
  `cockpit_backend.rs` / `notify.rs` delivery helpers (except call-site signature
  updates already listed in web_backend tests).
- No new config fields, dependencies, routes, or frontend changes.
- Do not weaken, delete, or rewrite unrelated tests.
- No formatting sweeps outside touched lines.

## 5. Context evidence

- **Graphify:** NOT_REQUIRED — single vertical path (web runtime ↔ CLI bridge
  notify side-effect); architecture.md already documents the notify tick.
- **Serena:** NOT_REQUIRED — anchors verified via ripgrep + direct file reads.
- **ast-grep:** NOT_REQUIRED — trait signature change is a single known method;
  call sites enumerated by `rg '\.refresh_cockpit\('`.

Anchors (verified 2026-07-16):

- `WebAppState` fields + `Clone` + `new` / `load_or_create`: `runtime.rs` ~51–187
- `spawn_notify_tick` always calls `refresh_cockpit_and_cache` (ponytail comment
  admits redundant refresh while browser polls): ~382–406
- `axum_cockpit` → cache hit or `refresh_cockpit_and_cache`: ~562–571
- `refresh_cockpit_and_cache` → `handle_refreshed_cockpit_request`: ~579–621
- `RuntimeBridge::refresh_cockpit(context, runner, tier)`: ~900–906
- `handle_refreshed_cockpit_request` calls refresh with Live tier: ~931–941
- `CliRuntimeBridge::refresh_cockpit` always runs
  `notify_attention_transitions`: `web_backend.rs` ~222–237
- TestBridge + web_backend tests call the 3-arg form: `runtime.rs` ~1117–1130,
  `web_backend.rs` ~674,679,716,768
- Hidden cockpit poll is 60s (`REFRESH_INTERVAL_HIDDEN_MS`); TTL must exceed that

## 6. Code anchors

```rust
// runtime.rs — add near DEFAULT_NOTIFY_POLL_SECONDS
const BROWSER_CONNECTED_TTL: Duration = Duration::from_secs(90);

// WebAppState gains:
last_browser_cockpit_at: Arc<Mutex<Option<Instant>>>,
// (std::sync::Mutex already imported; Instant already used)

// methods on WebAppState:
fn mark_browser_cockpit_seen(&self) { ... Instant::now() ... }
fn browser_connected(&self) -> bool {
  // Some(at) if at.elapsed() < BROWSER_CONNECTED_TTL
}

// axum_cockpit: first line mark_browser_cockpit_seen(); then existing logic;
// refresh_cockpit_and_cache(&state, false)

// refresh_cockpit_and_cache(state, deliver_notifications: bool)
// handle_refreshed_cockpit_request(..., deliver_notifications)
// bridge.refresh_cockpit(..., deliver_notifications)

// spawn_notify_tick loop:
if tick_state.browser_connected() { continue; }
let _ = refresh_cockpit_and_cache(&tick_state, true).await;

// CliRuntimeBridge:
let notified = if deliver_notifications {
    crate::notify::notify_attention_transitions(context, runner)
} else {
    false
};
```

## 7. Test-first instructions

Order; each must fail before its production edit (compile failure counts for
new APIs / new param):

1. **`browser_connected_is_false_until_marked_and_expires_after_ttl`** in
   `runtime.rs` tests:
   - Fresh `WebAppState` via existing `app_with` / `WebAppState::new` fixture →
     `browser_connected()` is false.
   - `mark_browser_cockpit_seen()` → true.
   - Expose a test-only `mark_browser_cockpit_seen_at(Instant)` **or** set the
     mutex directly in the test module to an `Instant` older than
     `BROWSER_CONNECTED_TTL` → `browser_connected()` false.
   - Prefer a `pub(super)` / `#[cfg(test)]` helper over making the field public.
   - Command: `cargo nextest run -p ajax-web browser_connected`

2. **`axum_cockpit_marks_browser_connected_even_on_cache_hit`**:
   - Call `GET /api/cockpit` twice (second is cache hit per existing TTL test
     pattern).
   - Assert `state.browser_connected()` is true after the cache-hit response.
   - Command: same filter or `axum_cockpit_marks_browser`

3. **`refresh_cockpit_and_cache_passes_deliver_notifications_flag`**:
   - Extend `TestBridge` with `last_deliver_notifications: Option<bool>`
     (or a `Vec<bool>` of flags seen).
   - Call `refresh_cockpit_and_cache(&state, false)` then `(&state, true)` after
     TTL expiry (or clear cache) so both refresh.
   - Assert bridge recorded `false` then `true`.
   - This fails until the new parameter exists end-to-end.

4. **`CliRuntimeBridge` notify gate** in `web_backend.rs` tests (add one focused
   test near existing refresh tests):
   - Build context with a waiting task + `[notify]` webhook config (reuse
     patterns from `notify.rs` tests / existing web_backend fixtures).
   - `refresh_cockpit(..., deliver_notifications: false)` must **not** invoke
     curl / must not change notify stamps (or use a recording runner and assert
     zero webhook specs).
   - `refresh_cockpit(..., true)` must fire once.
   - Command: `cargo nextest run -p ajax-cli web_refresh_cockpit_notify` (name
     the test accordingly).

Update existing `.refresh_cockpit(... Full)` call sites to pass `true` (preserve
prior notify behavior in those tests).

Do not add a flaky real-timer test for the tick loop itself; unit-test
`browser_connected` + the tick's `if browser_connected() { continue; }` is
covered by the presence tests + documenting the gate in `spawn_notify_tick`.

## 8. Edit instructions

1. Add `BROWSER_CONNECTED_TTL`, field, Clone/`new`/`load_or_create` wiring,
   `mark_browser_cockpit_seen`, `browser_connected`, and test helper for aged
   timestamps.
2. `axum_cockpit`: mark seen; pass `deliver_notifications: false` into refresh.
3. `refresh_cockpit_and_cache` / `handle_refreshed_cockpit_request` /
   `RuntimeBridge` / `TestBridge`: plumb the bool.
4. `spawn_notify_tick`: skip when `browser_connected()`; else refresh with `true`.
   Update the doc comment to say webhooks stay quiet while a browser is connected.
5. `CliRuntimeBridge::refresh_cockpit`: gate `notify_attention_transitions` on
   the flag; update all call sites/tests.
6. README + architecture.md: one sentence each.

## 9. Verification commands

```bash
cargo nextest run -p ajax-web browser_connected
cargo nextest run -p ajax-web axum_cockpit_marks_browser
cargo nextest run -p ajax-web refresh_cockpit_and_cache
cargo nextest run -p ajax-cli web_refresh_cockpit
cargo nextest run -p ajax-web
cargo nextest run -p ajax-cli notify
cargo fmt --check
cargo clippy -p ajax-web -p ajax-cli --all-targets -- -D warnings
```

## 10. Acceptance criteria

- New tests failed before production edits (RED evidence), pass after (GREEN).
- Browser `/api/cockpit` path never delivers webhooks.
- Tick path delivers only when `browser_connected()` is false.
- Existing ajax-web / ajax-cli notify tests still pass.
- Diff confined to allowed files.

## 11. Stop conditions

- Plumbing forces a semantic change to cache/lock/revision → stop and report.
- `notify_attention_transitions` cannot be gated without editing `notify.rs`
  logic → stop (should not happen; gate is at the call site).
- A new test passes before its production edit → stop.
- Required edit outside Allowed files → stop.

# TDD Implementation Packet B — background notify tick (`[notify] poll_seconds` + web-server tick)

## 1. Goal

`ajax web` fires attention webhooks with no browser attached: an optional tokio tick inside the existing web runtime reuses the exact `/api/cockpit` refresh path (same lock, cache TTL, revision-checked commit), at an interval from a new optional `[notify] poll_seconds` config field.

## 2. Allowed files

Production (embedded `#[cfg(test)]` modules in the same files are where tests go):
- `crates/ajax-core/src/config.rs` — add `poll_seconds` to `NotifyConfig` (+ parse tests)
- `crates/ajax-cli/src/notify.rs` — ONLY the `NotifyConfig` struct literal in its test (~line 158) gains the new field; no logic changes
- `crates/ajax-web/src/runtime.rs` — extract helper, add `notify_poll_interval`, spawn tick (+ tests)

Do NOT edit any file outside this list. Do NOT edit files under any `tests/` directory.

## 3. Forbidden changes

- No changes to `notify_attention_transitions`, `webhook_command`, `attention.rs`, or any ajax-core status/live code.
- No changes to `CliRuntimeBridge::refresh_cockpit` (web_backend.rs) — the tick reuses it transitively, unchanged.
- No changes to routes, session/auth middleware, TLS setup, frontend assets, polling.ts, or any web contract.
- No new dependencies. No changes to existing cache TTL, lock, or revision semantics.
- Do not weaken, delete, or rewrite any existing test or assertion.
- No formatting sweeps or drive-by cleanup.

## 4. Architecture context

- `GET /api/cockpit` (`axum_cockpit`, runtime.rs ~:531-577) is today the ONLY web refresh trigger: cache check → `cockpit_refresh_lock` (tokio Mutex, single-flight) → double-check cache → clone context/runner/bridge from `state.shared()` → `handle_refreshed_cockpit_request` → revision-checked commit back into shared state + cockpit cache. Notify + SQLite persistence happen inside `handle_refreshed_cockpit_request` → `bridge.refresh_cockpit` (web_backend.rs:236-252). The tick must reuse this body verbatim so serialization and dedup come for free.
- `serve_axum_web` (runtime.rs ~:338-372) builds a multi-thread tokio runtime and `runtime.block_on(async move { ... axum::serve(...).await })`. The tick is a `tokio::spawn` inside that async block, before `axum::serve`, using a clone of `state` taken before `axum_app(state)` consumes it.
- Config: `NotifyConfig` (crates/ajax-core/src/config.rs:264-267) `{ webhook_url: String }`, `#[serde(deny_unknown_fields)]`; `Config.notify: Option<NotifyConfig>` with `#[serde(default)]`. Cross-process notification dedup already persists via task metadata — no double-notify with CLI cockpit running concurrently.

## 5. Code anchors

- `config.rs:264-267` `pub struct NotifyConfig` — add field here. Existing struct literals to update: config test (~config.rs:496) and `crates/ajax-cli/src/notify.rs:158`.
- `runtime.rs` `async fn axum_cockpit<C, B>(State(state): ...)` (~:531): everything from the `let _refresh_guard = state.cockpit_refresh_lock.lock().await;` line through the final `match result` moves into the new helper; the handler keeps its leading `state.cached_cockpit_response()` fast-path, then calls the helper.
- New helper signature: `async fn refresh_cockpit_and_cache<C, B>(state: &WebAppState<C, B>) -> AxumResponse` (or `Result<WebResponse, WebError>` if that keeps the handler smaller — pick whichever needs the least code motion; the helper must contain the lock, the double cache check, the refresh, and the revision-checked commit).
- New pure fn: `fn notify_poll_interval(notify: Option<&NotifyConfig>) -> Option<std::time::Duration>` — `None` → `None`; `Some` with `poll_seconds: None` → 30s default; `Some(0)` → `None`; `Some(n)` → n secs. `const DEFAULT_NOTIFY_POLL_SECONDS: u64 = 30;`
- Tick site: inside `runtime.block_on(async move { ... })` in `serve_axum_web`, before `axum::serve`. Read the notify config from `state` (shared context config — follow how other code reads `state.shared().context` and drop the guard before awaiting). Spawn:
  ```rust
  if let Some(period) = notify_poll_interval(...) {
      let tick_state = state.clone();
      tokio::spawn(async move {
          let mut interval = tokio::time::interval(period);
          interval.tick().await; // skip immediate tick
          loop {
              interval.tick().await;
              // ponytail: refreshes even while a browser polls — one redundant
              // refresh per period, cheap; gate on cache age if it ever matters.
              let _ = refresh_cockpit_and_cache(&tick_state).await;
          }
      });
  }
  ```
- Test fixtures to reuse: existing `TestBridge` (`impl RuntimeBridge` ~runtime.rs:1071) and `WebAppState<OkRunner, TestBridge>` construction (~:1231-1234); the cache-TTL test `axum_cockpit_serves_cached_projection_within_refresh_ttl` (~:1718) shows the established pattern for calling the handler and counting bridge refreshes.

## 6. Test-first instructions

Order; each must fail before its production edit (compile failure counts for new APIs):

1. `config.rs` tests, `notify_poll_seconds_parses_and_defaults`: TOML `[notify]` with `poll_seconds = 60` → `Some(60)`; without it → `None`; a config WITHOUT `[notify]` still parses. Command: `cargo nextest run -p ajax-core config`.
2. `runtime.rs` tests, `notify_poll_interval_maps_config`: the four cases (no config → None; default → 30s; 0 → None; 90 → 90s).
3. `runtime.rs` tests (`#[tokio::test]`), `refresh_cockpit_and_cache_refreshes_once_and_caches`: build the `TestBridge` state fixture; call `refresh_cockpit_and_cache(&state)` → bridge refresh count 1 and cockpit cache populated; call again within TTL → handler-visible cached path means count stays 1 (mirror the TTL test's assertions).

The tick loop itself is glue over `interval` + the tested helper — no timer test required (existing TTL test at ~:1718 must keep passing to prove the handler extraction preserved behavior).

## 7. Production edit instructions

1. `config.rs`: add `#[serde(default)] pub poll_seconds: Option<u64>,` to `NotifyConfig`. Update the two struct literals (config.rs test ~:496, notify.rs ~:158) with `poll_seconds: None`.
2. `runtime.rs`: extract the helper from `axum_cockpit` (pure code motion — no semantic change); add `notify_poll_interval` + `DEFAULT_NOTIFY_POLL_SECONDS`; spawn the tick in `serve_axum_web` as anchored.

## 8. Verification commands

```bash
cargo nextest run -p ajax-core config
cargo nextest run -p ajax-web
cargo nextest run -p ajax-cli notify
cargo fmt --check
cargo clippy -p ajax-web -p ajax-core --all-targets -- -D warnings
```

## 9. Acceptance criteria

- New tests failed before their edits, pass after (report before/after per test).
- `axum_cockpit_serves_cached_projection_within_refresh_ttl` and all other pre-existing ajax-web tests pass UNCHANGED.
- Existing configs without `poll_seconds` and without `[notify]` still parse (proven by existing config tests passing).
- Diff confined to the three allowed files; well under 400 lines.

## 10. Stop conditions

- Extraction forces a semantic change to cache/lock/revision behavior → stop and report.
- `WebAppState` is not `Clone` or config is not reachable from `state` → stop, report what you found instead.
- Any pre-existing test fails and fixing it would mean editing it → stop.
- A new test passes before its production edit → stop.
- Required edit outside Allowed files → stop.
